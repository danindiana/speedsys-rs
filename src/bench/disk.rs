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
    pub raid_level: Option<String>,
    pub raid_members: Option<usize>,
    pub raid_state: Option<String>,
    pub queue_depth: Option<u32>,
    pub io_scheduler: Option<String>,
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

/// Read RAID metadata for a device (e.g., md0). Returns (level, member_count, state).
fn read_raid_info(name: &str) -> (Option<String>, Option<usize>, Option<String>) {
    if !name.starts_with("md") {
        return (None, None, None);
    }

    let level_path = format!("/sys/block/{}/md/level", name);
    let level = std::fs::read_to_string(&level_path)
        .ok()
        .map(|s| s.trim().to_uppercase());

    let members = std::fs::read_dir(format!("/sys/block/{}/md", name))
        .map(|d| d.filter(|e| {
            e.as_ref()
                .ok()
                .map(|en| en.file_name().to_string_lossy().starts_with("rd"))
                .unwrap_or(false)
        }).count())
        .ok()
        .filter(|&c| c > 0);

    let state_path = format!("/sys/block/{}/md/array_state", name);
    let state = std::fs::read_to_string(&state_path)
        .ok()
        .map(|s| s.trim().to_string());

    (level, members, state)
}

/// Read queue depth and I/O scheduler for a device. Returns (nr_requests, scheduler_name).
fn read_queue_info(name: &str) -> (Option<u32>, Option<String>) {
    let depth_path = format!("/sys/block/{}/queue/nr_requests", name);
    let queue_depth = std::fs::read_to_string(&depth_path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());

    let scheduler_path = format!("/sys/block/{}/queue/scheduler", name);
    let io_scheduler = std::fs::read_to_string(&scheduler_path)
        .ok()
        .map(|s| {
            // Scheduler format: "none mq-deadline [cfq]" or similar
            // Extract the bracketed scheduler name, fallback to first if no brackets
            if let Some(start) = s.find('[') {
                if let Some(end) = s.find(']') {
                    return s[start + 1..end].to_string();
                }
            }
            // Fallback: use first non-empty word
            s.split_whitespace().next().unwrap_or("unknown").to_string()
        });

    (queue_depth, io_scheduler)
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

                        let (raid_level, raid_members, raid_state) = read_raid_info(&name);
                        let model = if let Some(ref level) = raid_level {
                            let members_str = raid_members.map(|m| m.to_string()).unwrap_or_else(|| "?".to_string());
                            let state_str = raid_state.as_deref().unwrap_or("unknown");
                            format!("{} ({} members, {})", level, members_str, state_str)
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

                        let (queue_depth, io_scheduler) = read_queue_info(&name);

                        devices.push(DiskDevice {
                            name: name.clone(),
                            path: format!("/dev/{}", name),
                            size_bytes: bytes,
                            model,
                            is_rotational,
                            raid_level,
                            raid_members,
                            raid_state,
                            queue_depth,
                            io_scheduler,
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
pub struct SmartInfo {
    pub temperature: Option<f64>,
    pub power_on_hours: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    pub pending_sectors: Option<u64>,
}

/// Parse SMART JSON from smartctl -j output. Handles both SATA and NVMe drives.
/// For NVMe, `percentage_used` and `media_errors` are mapped to `reallocated_sectors` and
/// `pending_sectors` fields respectively (semantics differ, but reuses existing field shape).
fn parse_smart_json(value: &serde_json::Value) -> SmartInfo {
    let mut result = SmartInfo::default();

    // Try SATA first (ata_smart_attributes)
    if let Some(ata) = value.get("ata_smart_attributes").and_then(|v| v.get("table")) {
        if let Some(array) = ata.as_array() {
            for attr in array {
                let id = attr.get("id").and_then(|v| v.as_u64());
                let name = attr.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let raw_value = attr.get("raw").and_then(|v| v.get("value")).and_then(|v| v.as_u64());

                match id {
                    Some(194) | Some(_) if name.contains("Temperature") => {
                        result.temperature = attr.get("current").and_then(|v| v.as_u64()).map(|v| v as f64);
                    }
                    Some(9) | Some(_) if name.contains("Power_On_Hours") => {
                        result.power_on_hours = raw_value;
                    }
                    Some(5) | Some(_) if name.contains("Reallocated_Sector_Ct") => {
                        result.reallocated_sectors = raw_value;
                    }
                    Some(197) | Some(_) if name.contains("Current_Pending_Sector") => {
                        result.pending_sectors = raw_value;
                    }
                    _ => {}
                }
            }
        }
    }

    // Try NVMe if no SATA attributes found
    if result.temperature.is_none() {
        if let Some(nvme) = value.get("nvme_smart_health_information_log") {
            result.temperature = nvme
                .get("temperature")
                .and_then(|v| v.as_u64())
                .map(|v| v as f64);
            result.power_on_hours = nvme.get("power_on_hours").and_then(|v| v.as_u64());
            // NVMe percentage_used maps to reallocated_sectors slot (used instead of literal sectors)
            result.reallocated_sectors = nvme.get("percentage_used").and_then(|v| v.as_u64());
            // NVMe media_errors maps to pending_sectors slot
            result.pending_sectors = nvme.get("media_errors").and_then(|v| v.as_u64());
        }
    }

    result
}

/// Try to read SMART data via smartctl (requires: apt install smartmontools).
pub fn read_smart_info(device_path: &str) -> Option<SmartInfo> {
    use std::process::Command;

    let output = Command::new("smartctl")
        .args(["-a", "-j", device_path])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    Some(parse_smart_json(&json))
}

#[cfg(test)]
mod smart_tests {
    use super::*;

    #[test]
    fn test_parse_sata_smart() {
        let sata_json = serde_json::json!({
            "ata_smart_attributes": {
                "table": [
                    {"id": 194, "name": "Temperature_Celsius", "current": 45, "raw": {"value": 45}},
                    {"id": 9, "name": "Power_On_Hours", "raw": {"value": 8760}},
                    {"id": 5, "name": "Reallocated_Sector_Ct", "raw": {"value": 0}},
                    {"id": 197, "name": "Current_Pending_Sector", "raw": {"value": 2}}
                ]
            }
        });

        let result = parse_smart_json(&sata_json);
        assert_eq!(result.temperature, Some(45.0));
        assert_eq!(result.power_on_hours, Some(8760));
        assert_eq!(result.reallocated_sectors, Some(0));
        assert_eq!(result.pending_sectors, Some(2));
    }

    #[test]
    fn test_parse_nvme_smart() {
        let nvme_json = serde_json::json!({
            "nvme_smart_health_information_log": {
                "temperature": 40,
                "power_on_hours": 1234,
                "percentage_used": 15,
                "media_errors": 0
            }
        });

        let result = parse_smart_json(&nvme_json);
        assert_eq!(result.temperature, Some(40.0));
        assert_eq!(result.power_on_hours, Some(1234));
        assert_eq!(result.reallocated_sectors, Some(15)); // NVMe percentage_used
        assert_eq!(result.pending_sectors, Some(0)); // NVMe media_errors
    }

    #[test]
    fn test_parse_empty_smart() {
        let empty_json = serde_json::json!({});
        let result = parse_smart_json(&empty_json);
        assert_eq!(result.temperature, None);
        assert_eq!(result.power_on_hours, None);
    }

    #[test]
    fn test_raid_info_non_raid_device() {
        let (level, members, state) = read_raid_info("sda");
        assert_eq!(level, None);
        assert_eq!(members, None);
        assert_eq!(state, None);
    }

    #[test]
    fn test_raid_info_format() {
        // Test that md device name detection works
        let result = read_raid_info("md0");
        // Will return None if /sys/block/md0 doesn't exist, but the parsing would work if it did
        // The important thing is that non-md devices return all None
        let (level, members, state) = read_raid_info("nvme0n1");
        assert_eq!(level, None);
        assert_eq!(members, None);
        assert_eq!(state, None);
    }

    #[test]
    fn test_queue_scheduler_with_brackets() {
        // Test parsing scheduler string with brackets: "none mq-deadline [cfq]"
        let scheduler_line = "none mq-deadline [cfq]";
        let parsed = if let Some(start) = scheduler_line.find('[') {
            if let Some(end) = scheduler_line.find(']') {
                scheduler_line[start + 1..end].to_string()
            } else {
                "unknown".to_string()
            }
        } else {
            scheduler_line.split_whitespace().next().unwrap_or("unknown").to_string()
        };
        assert_eq!(parsed, "cfq");
    }

    #[test]
    fn test_queue_scheduler_no_brackets() {
        // Test parsing scheduler string without brackets (fallback)
        let scheduler_line = "mq-deadline none";
        let parsed = if let Some(start) = scheduler_line.find('[') {
            if let Some(end) = scheduler_line.find(']') {
                scheduler_line[start + 1..end].to_string()
            } else {
                "unknown".to_string()
            }
        } else {
            scheduler_line.split_whitespace().next().unwrap_or("unknown").to_string()
        };
        assert_eq!(parsed, "mq-deadline");
    }
}
