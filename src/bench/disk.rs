use std::fs::File;
use std::hint::black_box;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct DiskDevice {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub model: String,
    pub is_rotational: bool,
}

// Removed AlignedBuf - use simple Vec instead for device reads

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

/// Linear read speed: sample K evenly spaced offsets, return (position%, MB/s) tuples.
pub fn bench_linear_read(
    device_path: &str,
    samples: usize,
    sample_size_mb: usize,
    cancel: &AtomicBool,
    tx: Option<&mpsc::Sender<crate::bench::BenchMsg>>,
    start_time: std::time::Instant,
) -> Result<(Vec<(f64, f64)>, f64, f64, f64), String> {
    let sample_bytes = sample_size_mb * 1024 * 1024;
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

    if file_size == 0 || file_size < sample_bytes as u64 {
        return Err(format!(
            "Device too small or unreadable (size: {} bytes, need: {} bytes)",
            file_size, sample_bytes
        ));
    }

    let mut buf = vec![0u8; sample_bytes];
    let mut results = Vec::new();
    let mut total_speed: f64 = 0.0;
    let mut min_speed: f64 = f64::INFINITY;
    let mut max_speed: f64 = 0.0;
    let progress_interval = (samples / 10).max(1).min(50); // Update every 50 samples or 10%, whichever is less frequent

    for i in 0..samples {
        // Check cancellation
        if cancel.load(Ordering::Relaxed) {
            return Ok((results, total_speed / (i as f64).max(1.0), min_speed, max_speed));
        }

        let pos = ((i as u64) * file_size) / samples as u64;
        let position_pct = (pos as f64 / file_size as f64) * 100.0;

        file.seek(SeekFrom::Start(pos))
            .map_err(|e| format!("Seek failed: {}", e))?;

        // Time the read of exactly sample_bytes
        let read_start = Instant::now();
        let bytes_read = match file.read(&mut buf) {
            Ok(n) => n,
            Err(_) => 0,
        };

        black_box(&buf);
        let elapsed = read_start.elapsed().as_secs_f64();

        // Only count samples that read the full size
        if bytes_read == sample_bytes && elapsed > 0.0 {
            let speed_mbs = (sample_bytes as f64) / elapsed / 1e6;
            total_speed += speed_mbs;
            min_speed = min_speed.min(speed_mbs);
            max_speed = max_speed.max(speed_mbs);
            results.push((position_pct, speed_mbs));
        }

        // Send progress update periodically
        if (i + 1) % progress_interval == 0 {
            if let Some(tx) = tx {
                let elapsed_secs = start_time.elapsed().as_secs_f64();
                let _ = tx.send(crate::bench::BenchMsg::Progress(i + 1, samples, elapsed_secs));
            }
        }
    }

    let count = results.len().max(1) as f64;
    Ok((results, total_speed / count, min_speed, max_speed))
}

/// Random seek/access time: K random 4KB reads, return latencies in ms.
pub fn bench_random_seek(
    device_path: &str,
    num_seeks: usize,
    cancel: &AtomicBool,
    tx: Option<&mpsc::Sender<crate::bench::BenchMsg>>,
    start_time: std::time::Instant,
) -> Result<(Vec<f64>, f64, f64), String> {
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

    let mut buf = [0u8; 4096];
    let mut latencies = Vec::new();
    let mut total: f64 = 0.0;
    let mut max_latency: f64 = 0.0;
    let progress_interval = (num_seeks / 10).max(1).min(20); // Update every 20 seeks or 10%

    use rand::Rng;
    let mut rng = rand::thread_rng();

    for i in 0..num_seeks {
        // Check cancellation
        if cancel.load(Ordering::Relaxed) {
            let avg = if latencies.is_empty() { 0.0 } else { total / latencies.len() as f64 };
            return Ok((latencies, avg, max_latency));
        }

        let max_offset = file_size.saturating_sub(4096);
        let offset = (rng.gen::<u64>() % (max_offset + 1)) & !0xFFF; // Align to 4KB

        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Seek failed: {}", e))?;

        let _start = Instant::now();
        let _ = file.read(&mut buf);
        let latency_ms = _start.elapsed().as_secs_f64() * 1000.0;

        black_box(&buf);

        total += latency_ms;
        max_latency = max_latency.max(latency_ms);
        latencies.push(latency_ms);

        // Send progress update periodically
        if (i + 1) % progress_interval == 0 {
            if let Some(tx) = tx {
                let elapsed_secs = start_time.elapsed().as_secs_f64();
                let _ = tx.send(crate::bench::BenchMsg::Progress(i + 1, num_seeks, elapsed_secs));
            }
        }
    }

    let avg_latency = if latencies.is_empty() { 0.0 } else { total / latencies.len() as f64 };
    Ok((latencies, avg_latency, max_latency))
}

#[derive(Clone, Debug, Default)]
pub struct SmartInfo {
    pub temperature: Option<f64>,
    pub power_on_hours: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    pub pending_sectors: Option<u64>,
}

/// Try to read SMART data via smartctl (requires: apt install smartmontools).
pub fn read_smart_info(_device_path: &str) -> Option<SmartInfo> {
    // Placeholder for Phase 4: smartctl -a -j integration
    None
}
