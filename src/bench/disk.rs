use std::fs::{File, OpenOptions};
use std::hint::black_box;
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

pub const O_DIRECT: i32 = 0o40000; // Linux-specific; libc::O_DIRECT on some systems

#[derive(Clone, Debug)]
pub struct DiskDevice {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub model: String,
    pub is_rotational: bool,
}

/// List all block devices from /sys/block (skip loop, ram, zram; >= 1MB).
pub fn scan_disks() -> Vec<DiskDevice> {
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
                            continue; // Skip < 1 MB
                        }

                        let model_path = format!("/sys/block/{}/device/model", name);
                        let model = std::fs::read_to_string(&model_path)
                            .ok()
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "unknown".to_string());

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
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    devices
}

/// Linear read speed: sample K evenly spaced offsets, return (position%, MB/s) tuples.
pub fn bench_linear_read(
    device_path: &str,
    samples: usize,
    sample_size_mb: usize,
) -> Result<(Vec<(f64, f64)>, f64, f64, f64), String> {
    let sample_bytes = sample_size_mb * 1024 * 1024;
    let file = OpenOptions::new()
        .read(true)
        .open(device_path)
        .map_err(|e| format!("Failed to open {}: {}", device_path, e))?;

    // For device files, metadata().len() returns 0, so read size from /sys/block
    let file_size = if let Some(dev_name) = device_path.strip_prefix("/dev/") {
        let size_path = format!("/sys/block/{}/size", dev_name);
        std::fs::read_to_string(&size_path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|sectors| sectors * 512)
            .unwrap_or(0)
    } else {
        file.metadata()
            .map_err(|e| format!("Failed to get metadata: {}", e))?
            .len()
    };

    if file_size == 0 || file_size < sample_bytes as u64 {
        return Err(format!("Device too small or unreadable (size: {} bytes, need: {} bytes)", file_size, sample_bytes));
    }

    let mut results = Vec::new();
    let mut total_speed: f64 = 0.0;
    let mut min_speed: f64 = f64::INFINITY;
    let mut max_speed: f64 = 0.0;

    for i in 0..samples {
        let pos = ((i as u64) * file_size) / samples as u64;
        let position_pct = (pos as f64 / file_size as f64) * 100.0;
        let speed_mbs = read_at_position(&file, pos, sample_bytes)?;

        total_speed += speed_mbs;
        min_speed = min_speed.min(speed_mbs);
        max_speed = max_speed.max(speed_mbs);
        results.push((position_pct, speed_mbs));

        // Log progress every 50 samples
        if (i + 1) % 50 == 0 {
            eprintln!("[WORKER] Linear read progress: {}/{} samples ({:.1}% done)",
                     i + 1, samples, ((i + 1) as f64 / samples as f64) * 100.0);
        }
    }

    let avg_speed = total_speed / samples as f64;
    Ok((results, avg_speed, min_speed, max_speed))
}

fn read_at_position(file: &File, offset: u64, size: usize) -> Result<f64, String> {
    let mut file_clone = file.try_clone()
        .map_err(|e| format!("Failed to clone file handle: {}", e))?;

    file_clone
        .seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Seek failed: {}", e))?;

    let mut buf = vec![0u8; size];
    let start = Instant::now();
    let read_bytes = file_clone
        .read(&mut buf)
        .map_err(|e| format!("Read failed: {}", e))?;
    let elapsed = start.elapsed();

    black_box(&buf);
    let mbs = (read_bytes as f64) / elapsed.as_secs_f64() / 1e6;
    Ok(mbs)
}

/// Random seek/access time: K random 4KB reads, return latencies in ms.
pub fn bench_random_seek(
    device_path: &str,
    num_seeks: usize,
) -> Result<(Vec<f64>, f64, f64), String> {
    let file = OpenOptions::new()
        .read(true)
        .open(device_path)
        .map_err(|e| format!("Failed to open {}: {}", device_path, e))?;

    // For device files, metadata().len() returns 0, so read size from /sys/block
    let file_size = if let Some(dev_name) = device_path.strip_prefix("/dev/") {
        let size_path = format!("/sys/block/{}/size", dev_name);
        std::fs::read_to_string(&size_path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|sectors| sectors * 512)
            .unwrap_or(0)
    } else {
        file.metadata()
            .map_err(|e| format!("Failed to get metadata: {}", e))?
            .len()
    };

    if file_size == 0 {
        return Err(format!("Cannot determine device size"));
    }

    let mut latencies = Vec::new();
    let mut total: f64 = 0.0;
    let mut max_latency: f64 = 0.0;

    use rand::Rng;
    let mut rng = rand::thread_rng();

    for _ in 0..num_seeks {
        let offset = (rng.gen::<u64>() % (file_size - 4096)) & !0xFFF; // Align to 4KB
        let latency_ms = seek_latency(&file, offset, 4096)?;
        total += latency_ms;
        max_latency = max_latency.max(latency_ms);
        latencies.push(latency_ms);
    }

    let avg_latency = total / num_seeks as f64;
    Ok((latencies, avg_latency, max_latency))
}

fn seek_latency(file: &File, offset: u64, size: usize) -> Result<f64, String> {
    let mut file_clone = file.try_clone()
        .map_err(|e| format!("Failed to clone file: {}", e))?;

    let start = Instant::now();
    file_clone
        .seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Seek failed: {}", e))?;

    let mut buf = vec![0u8; size];
    file_clone
        .read_exact(&mut buf)
        .map_err(|e| format!("Read failed: {}", e))?;

    black_box(&buf);
    Ok(start.elapsed().as_secs_f64() * 1000.0) // Convert to ms
}

/// Try to read SMART data via smartctl (requires: apt install smartmontools).
pub fn read_smart_info(_device_path: &str) -> Option<SmartInfo> {
    // For now, return None (smartctl integration can be added later)
    // This would require spawning a process, parsing JSON, etc.
    None
}

#[derive(Clone, Debug, Default)]
pub struct SmartInfo {
    pub temperature: Option<f64>,
    pub power_on_hours: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    pub pending_sectors: Option<u64>,
}
