use std::alloc::Layout;
use std::hint::black_box;
use std::io::{Read, Seek, SeekFrom, Error as IoError, ErrorKind};
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Instant;

static DEVICE_CACHE: OnceLock<Vec<DiskDevice>> = OnceLock::new();

/// RAII wrapper for 4096-byte aligned buffer (required for O_DIRECT).
struct AlignedBuf {
    ptr: *mut u8,
    size: usize,
}

impl AlignedBuf {
    fn new(size: usize) -> Result<Self, IoError> {
        // Round up to nearest multiple of 4096
        let aligned_size = ((size + 4095) / 4096) * 4096;
        let layout = Layout::from_size_align(aligned_size, 4096)
            .map_err(|_| IoError::new(ErrorKind::Other, "Failed to create aligned layout"))?;

        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            return Err(IoError::new(ErrorKind::Other, "Failed to allocate aligned memory"));
        }
        Ok(AlignedBuf {
            ptr,
            size: aligned_size,
        })
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }

    fn layout(&self) -> Layout {
        Layout::from_size_align(self.size, 4096).unwrap()
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.ptr, self.layout());
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiskDevice {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub model: String,
    pub is_rotational: bool,
}

// Linear read benchmark results including error tracking
#[derive(Debug)]
pub struct LinearReadResult {
    pub speeds: Vec<(f64, f64)>,                   // (position %, MB/s)
    pub errors: Vec<(f64, String)>,                // (position %, error description)
    pub avg: f64,
    pub min: f64,
    pub max: f64,
    pub cache_bypass_mode: String,                 // "O_DIRECT" or "buffered (FADV_DONTNEED)"
}

/// List all block devices from /sys/block (cached after first call).
/// Devices don't hotplug during a benchmark session, so caching is safe.
pub fn scan_disks() -> Vec<DiskDevice> {
    DEVICE_CACHE
        .get_or_init(scan_disks_impl)
        .clone()
}

/// Internal implementation: scan /sys/block for block devices.
fn scan_disks_impl() -> Vec<DiskDevice> {
    let mut devices = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/sys/block") {
        for entry in dir.filter_map(|e| e.ok()) {
            if let Ok(name) = entry.file_name().into_string() {
                if name.starts_with("loop")
                    || name.starts_with("ram")
                    || name.starts_with("zram")
                {
                    continue;
                }

                let size_path = format!("/sys/block/{}/size", name);
                if let Ok(size_str) = std::fs::read_to_string(&size_path) {
                    if let Ok(sectors) = size_str.trim().parse::<u64>() {
                        let bytes = sectors * 512;
                        if bytes < 1_000_000 {
                            continue;
                        }

                        let model = if name.starts_with("md") {
                            // RAID: get level and member count
                            let level_path = format!("/sys/block/{}/md/level", name);
                            let level = std::fs::read_to_string(&level_path)
                                .ok()
                                .map(|s| s.trim().to_uppercase())
                                .unwrap_or_else(|| "unknown".to_string());
                            let members = std::fs::read_dir(format!("/sys/block/{}/md", name))
                                .map(|d| d.filter(|e| {
                                    e.as_ref()
                                        .ok()
                                        .map(|en| en.file_name().to_string_lossy().starts_with("rd"))
                                        .unwrap_or(false)
                                }).count())
                                .unwrap_or(0);
                            format!("{} ({} members)", level, members)
                        } else {
                            std::fs::read_to_string(format!("/sys/block/{}/device/model", name))
                                .ok()
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        };

                        let rot_path = format!("/sys/block/{}/queue/rotational", name);
                        let is_rotational = std::fs::read_to_string(&rot_path)
                            .ok()
                            .and_then(|s| s.trim().parse::<u8>().ok())
                            .map(|v| v != 0)
                            .unwrap_or(false);

                        devices.push(DiskDevice {
                            name: name.clone(),
                            path: format!("/dev/{}", name),
                            size_bytes: bytes,
                            model,
                            is_rotational,
                        });
                    }
                }
            }
        }
    }

    // Natural sort: split into numeric/alpha runs
    devices.sort_by(|a, b| {
        let a_parts = split_device_name(&a.name);
        let b_parts = split_device_name(&b.name);
        a_parts.cmp(&b_parts)
    });

    devices
}

fn split_device_name(name: &str) -> Vec<DevicePart> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut is_digit = false;

    for ch in name.chars() {
        if ch.is_ascii_digit() {
            if !is_digit && !current.is_empty() {
                parts.push(DevicePart::Alpha(current.clone()));
                current.clear();
            }
            is_digit = true;
            current.push(ch);
        } else {
            if is_digit && !current.is_empty() {
                if let Ok(n) = current.parse::<u64>() {
                    parts.push(DevicePart::Num(n));
                }
                current.clear();
            }
            is_digit = false;
            current.push(ch);
        }
    }

    if !current.is_empty() {
        if is_digit {
            if let Ok(n) = current.parse::<u64>() {
                parts.push(DevicePart::Num(n));
            }
        } else {
            parts.push(DevicePart::Alpha(current));
        }
    }

    parts
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DevicePart {
    Alpha(String),
    Num(u64),
}

/// Linear read speed: sample K evenly spaced offsets with O_DIRECT cache bypass + error tracking.
/// Falls back to buffered I/O with POSIX_FADV_DONTNEED if O_DIRECT unavailable (e.g., md/dm devices).
pub fn bench_linear_read(
    device_path: &str,
    samples: usize,
    sample_size_mb: usize,
    cancel: &AtomicBool,
    tx: Option<&mpsc::Sender<crate::bench::BenchMsg>>,
    start_time: std::time::Instant,
) -> Result<LinearReadResult, String> {
    let sample_bytes = sample_size_mb * 1024 * 1024;

    // Read device size from /sys/block
    let file_size = if let Some(dev_name) = device_path.strip_prefix("/dev/") {
        let size_path = format!("/sys/block/{}/size", dev_name);
        std::fs::read_to_string(&size_path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|sectors| sectors * 512)
            .unwrap_or(0)
    } else {
        0
    };

    if file_size == 0 || file_size < sample_bytes as u64 {
        return Err(format!(
            "Device too small or unreadable (size: {} bytes, need: {} bytes)",
            file_size, sample_bytes
        ));
    }

    // Try O_DIRECT first for true cache bypass
    match try_open_direct(device_path) {
        Ok(file) => bench_linear_read_direct(file, device_path, samples, sample_size_mb, sample_bytes, file_size, cancel, tx, start_time),
        Err(_) => {
            // Fallback to buffered I/O with POSIX_FADV_DONTNEED
            bench_linear_read_buffered(device_path, samples, sample_size_mb, sample_bytes, file_size, cancel, tx, start_time)
        }
    }
}

/// Open file with O_DIRECT flag.
fn try_open_direct(path: &str) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
}

/// O_DIRECT version: true cache bypass with aligned buffers.
fn bench_linear_read_direct(
    mut file: std::fs::File,
    device_path: &str,
    samples: usize,
    sample_size_mb: usize,
    sample_bytes: usize,
    file_size: u64,
    cancel: &AtomicBool,
    tx: Option<&mpsc::Sender<crate::bench::BenchMsg>>,
    start_time: std::time::Instant,
) -> Result<LinearReadResult, String> {
    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut total_speed: f64 = 0.0;
    let mut min_speed: f64 = f64::INFINITY;
    let mut max_speed: f64 = 0.0;
    let progress_interval = (samples / 10).clamp(1, 50);

    // Allocate aligned buffer for O_DIRECT
    let mut buf = AlignedBuf::new(sample_bytes)
        .map_err(|e| format!("Failed to allocate aligned buffer: {}", e))?;

    for i in 0..samples {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let pos = ((i as u64) * file_size) / samples as u64;
        let position_pct = (pos as f64 / file_size as f64) * 100.0;

        if let Err(e) = file.seek(SeekFrom::Start(pos)) {
            errors.push((position_pct, format!("Seek: {}", e)));
            continue;
        }

        let read_start = Instant::now();
        match file.read(buf.as_mut_slice()) {
            Ok(bytes_read) => {
                black_box(buf.as_mut_slice());
                let elapsed = read_start.elapsed().as_secs_f64();

                if bytes_read == sample_bytes && elapsed > 0.0 {
                    let speed_mbs = (sample_bytes as f64) / elapsed / 1e6;
                    total_speed += speed_mbs;
                    min_speed = min_speed.min(speed_mbs);
                    max_speed = max_speed.max(speed_mbs);
                    results.push((position_pct, speed_mbs));
                } else if bytes_read < sample_bytes {
                    errors.push((position_pct, format!("Short read: {} of {} bytes", bytes_read, sample_bytes)));
                }
            }
            Err(e) => {
                errors.push((position_pct, format!("Read error: {}", e)));
            }
        }

        if (i + 1) % progress_interval == 0 {
            if let Some(tx) = tx {
                let elapsed_secs = start_time.elapsed().as_secs_f64();
                let _ = tx.send(crate::bench::BenchMsg::Progress(i + 1, samples, elapsed_secs));
            }
        }
    }

    let count = results.len().max(1) as f64;
    Ok(LinearReadResult {
        speeds: results,
        errors,
        avg: total_speed / count,
        min: min_speed,
        max: max_speed,
        cache_bypass_mode: "O_DIRECT".to_string(),
    })
}

/// Buffered version with POSIX_FADV_DONTNEED for each read.
fn bench_linear_read_buffered(
    device_path: &str,
    samples: usize,
    sample_size_mb: usize,
    sample_bytes: usize,
    file_size: u64,
    cancel: &AtomicBool,
    tx: Option<&mpsc::Sender<crate::bench::BenchMsg>>,
    start_time: std::time::Instant,
) -> Result<LinearReadResult, String> {
    let mut file = std::fs::File::open(device_path)
        .map_err(|e| format!("Failed to open {}: {}", device_path, e))?;

    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut total_speed: f64 = 0.0;
    let mut min_speed: f64 = f64::INFINITY;
    let mut max_speed: f64 = 0.0;
    let progress_interval = (samples / 10).clamp(1, 50);

    let mut buf = vec![0u8; sample_bytes];

    for i in 0..samples {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let pos = ((i as u64) * file_size) / samples as u64;
        let position_pct = (pos as f64 / file_size as f64) * 100.0;

        if let Err(e) = file.seek(SeekFrom::Start(pos)) {
            errors.push((position_pct, format!("Seek: {}", e)));
            continue;
        }

        // Discard from cache immediately after read to prevent cache effects
        unsafe {
            let _ = libc::posix_fadvise(
                file.as_raw_fd(),
                pos as i64,
                sample_bytes as i64,
                libc::POSIX_FADV_DONTNEED,
            );
        }

        let read_start = Instant::now();
        match file.read(&mut buf) {
            Ok(bytes_read) => {
                black_box(&buf);
                let elapsed = read_start.elapsed().as_secs_f64();

                if bytes_read == sample_bytes && elapsed > 0.0 {
                    let speed_mbs = (sample_bytes as f64) / elapsed / 1e6;
                    total_speed += speed_mbs;
                    min_speed = min_speed.min(speed_mbs);
                    max_speed = max_speed.max(speed_mbs);
                    results.push((position_pct, speed_mbs));
                } else if bytes_read < sample_bytes {
                    errors.push((position_pct, format!("Short read: {} of {} bytes", bytes_read, sample_bytes)));
                }
            }
            Err(e) => {
                errors.push((position_pct, format!("Read error: {}", e)));
            }
        }

        if (i + 1) % progress_interval == 0 {
            if let Some(tx) = tx {
                let elapsed_secs = start_time.elapsed().as_secs_f64();
                let _ = tx.send(crate::bench::BenchMsg::Progress(i + 1, samples, elapsed_secs));
            }
        }
    }

    let count = results.len().max(1) as f64;
    Ok(LinearReadResult {
        speeds: results,
        errors,
        avg: total_speed / count,
        min: min_speed,
        max: max_speed,
        cache_bypass_mode: "buffered (FADV_DONTNEED)".to_string(),
    })
}

/// Random seek/access time: K random 4KB reads with error tracking.
pub fn bench_random_seek(
    device_path: &str,
    num_seeks: usize,
    cancel: &AtomicBool,
    tx: Option<&mpsc::Sender<crate::bench::BenchMsg>>,
    start_time: std::time::Instant,
) -> Result<RandomSeekResult, String> {
    // Try O_DIRECT first
    match try_open_direct(device_path) {
        Ok(file) => bench_random_seek_direct(file, device_path, num_seeks, cancel, tx, start_time),
        Err(_) => bench_random_seek_buffered(device_path, num_seeks, cancel, tx, start_time),
    }
}

#[derive(Debug)]
pub struct RandomSeekResult {
    pub latencies: Vec<f64>,
    pub errors: Vec<String>,
    pub avg: f64,
    pub max: f64,
    pub cache_bypass_mode: String,
}

/// O_DIRECT version for random seek.
fn bench_random_seek_direct(
    mut file: std::fs::File,
    device_path: &str,
    num_seeks: usize,
    cancel: &AtomicBool,
    tx: Option<&mpsc::Sender<crate::bench::BenchMsg>>,
    start_time: std::time::Instant,
) -> Result<RandomSeekResult, String> {
    // Read device size from /sys/block
    let file_size = if let Some(dev_name) = device_path.strip_prefix("/dev/") {
        let size_path = format!("/sys/block/{}/size", dev_name);
        std::fs::read_to_string(&size_path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|sectors| sectors * 512)
            .unwrap_or(0)
    } else {
        0
    };

    if file_size == 0 || file_size < 4096 {
        return Err("Device too small for seek test".to_string());
    }

    let mut latencies = Vec::new();
    let mut errors = Vec::new();
    let mut total: f64 = 0.0;
    let mut max_latency: f64 = 0.0;
    let progress_interval = (num_seeks / 10).clamp(1, 20);

    let mut buf = AlignedBuf::new(4096)
        .map_err(|e| format!("Failed to allocate aligned buffer: {}", e))?;

    use rand::Rng;
    let mut rng = rand::thread_rng();

    for i in 0..num_seeks {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let max_offset = file_size.saturating_sub(4096);
        let offset = (rng.gen::<u64>() % (max_offset + 1)) & !0xFFF;

        if let Err(e) = file.seek(SeekFrom::Start(offset)) {
            errors.push(format!("Seek error: {}", e));
            continue;
        }

        let read_start = Instant::now();
        match file.read(buf.as_mut_slice()) {
            Ok(bytes_read) => {
                black_box(buf.as_mut_slice());
                let latency_ms = read_start.elapsed().as_secs_f64() * 1000.0;

                if bytes_read == 4096 {
                    total += latency_ms;
                    max_latency = max_latency.max(latency_ms);
                    latencies.push(latency_ms);
                } else {
                    errors.push(format!("Short read: {} bytes", bytes_read));
                }
            }
            Err(e) => {
                errors.push(format!("Read error: {}", e));
            }
        }

        if (i + 1) % progress_interval == 0 {
            if let Some(tx) = tx {
                let elapsed_secs = start_time.elapsed().as_secs_f64();
                let _ = tx.send(crate::bench::BenchMsg::Progress(i + 1, num_seeks, elapsed_secs));
            }
        }
    }

    let avg_latency = if latencies.is_empty() { 0.0 } else { total / latencies.len() as f64 };
    Ok(RandomSeekResult {
        latencies,
        errors,
        avg: avg_latency,
        max: max_latency,
        cache_bypass_mode: "O_DIRECT".to_string(),
    })
}

/// Buffered version for random seek.
fn bench_random_seek_buffered(
    device_path: &str,
    num_seeks: usize,
    cancel: &AtomicBool,
    tx: Option<&mpsc::Sender<crate::bench::BenchMsg>>,
    start_time: std::time::Instant,
) -> Result<RandomSeekResult, String> {
    let mut file = std::fs::File::open(device_path)
        .map_err(|e| format!("Failed to open {}: {}", device_path, e))?;

    // Read device size from /sys/block
    let file_size = if let Some(dev_name) = device_path.strip_prefix("/dev/") {
        let size_path = format!("/sys/block/{}/size", dev_name);
        std::fs::read_to_string(&size_path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|sectors| sectors * 512)
            .unwrap_or(0)
    } else {
        0
    };

    if file_size == 0 || file_size < 4096 {
        return Err("Device too small for seek test".to_string());
    }

    let mut latencies = Vec::new();
    let mut errors = Vec::new();
    let mut total: f64 = 0.0;
    let mut max_latency: f64 = 0.0;
    let progress_interval = (num_seeks / 10).clamp(1, 20);

    let mut buf = [0u8; 4096];

    use rand::Rng;
    let mut rng = rand::thread_rng();

    for i in 0..num_seeks {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let max_offset = file_size.saturating_sub(4096);
        let offset = (rng.gen::<u64>() % (max_offset + 1)) & !0xFFF;

        if let Err(e) = file.seek(SeekFrom::Start(offset)) {
            errors.push(format!("Seek error: {}", e));
            continue;
        }

        // Discard cache for this seek
        unsafe {
            let _ = libc::posix_fadvise(
                file.as_raw_fd(),
                offset as i64,
                4096,
                libc::POSIX_FADV_DONTNEED,
            );
        }

        let read_start = Instant::now();
        match file.read(&mut buf) {
            Ok(bytes_read) => {
                black_box(&buf);
                let latency_ms = read_start.elapsed().as_secs_f64() * 1000.0;

                if bytes_read == 4096 {
                    total += latency_ms;
                    max_latency = max_latency.max(latency_ms);
                    latencies.push(latency_ms);
                } else {
                    errors.push(format!("Short read: {} bytes", bytes_read));
                }
            }
            Err(e) => {
                errors.push(format!("Read error: {}", e));
            }
        }

        if (i + 1) % progress_interval == 0 {
            if let Some(tx) = tx {
                let elapsed_secs = start_time.elapsed().as_secs_f64();
                let _ = tx.send(crate::bench::BenchMsg::Progress(i + 1, num_seeks, elapsed_secs));
            }
        }
    }

    let avg_latency = if latencies.is_empty() { 0.0 } else { total / latencies.len() as f64 };
    Ok(RandomSeekResult {
        latencies,
        errors,
        avg: avg_latency,
        max: max_latency,
        cache_bypass_mode: "buffered (FADV_DONTNEED)".to_string(),
    })
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct SmartInfo {
    pub temperature: Option<f64>,
    #[allow(dead_code)]
    pub power_on_hours: Option<u64>,
    #[allow(dead_code)]
    pub reallocated_sectors: Option<u64>,
    #[allow(dead_code)]
    pub pending_sectors: Option<u64>,
}

/// Try to read SMART data via smartctl (requires: apt install smartmontools).
#[allow(dead_code)]
pub fn read_smart_info(_device_path: &str) -> Option<SmartInfo> {
    // Placeholder for Phase 4: smartctl -a -j integration
    None
}
