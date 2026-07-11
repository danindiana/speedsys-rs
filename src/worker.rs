/// Worker orchestration: unified disk test function for both CLI test-mode and TUI.
use crate::bench::{disk, BenchMsg, DiskBenchResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

/// Run disk benchmarks (linear read + random seek) on a device.
/// Used by both --test CLI mode and TUI DiskTest screen.
pub fn run_disk_test(
    device_path: &str,
    device_name: &str,
    samples: usize,
    sample_size_mb: usize,
    cancel: &AtomicBool,
    tx: &mpsc::Sender<BenchMsg>,
) {
    let test_start = Instant::now();
    let mut result = DiskBenchResult {
        device: device_name.to_string(),
        ..Default::default()
    };

    let _ = tx.send(BenchMsg::Status(format!("Linear read on {}...", device_name)));

    // Linear read benchmark
    match disk::bench_linear_read(device_path, samples, sample_size_mb, cancel, Some(tx), test_start) {
        Ok(linear_result) => {
            result.linear_speed_mbs = linear_result.speeds;
            result.avg_linear_mbs = linear_result.avg;
            result.min_linear_mbs = linear_result.min;
            result.max_linear_mbs = linear_result.max;
            result.read_errors = linear_result.errors;
            result.cache_bypass_mode = linear_result.cache_bypass_mode;
            let _ = tx.send(BenchMsg::DiskUpdate(result.clone()));
        }
        Err(e) => {
            let _ = tx.send(BenchMsg::Status(format!("Linear read error: {}", e)));
            return;
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return;
    }

    let _ = tx.send(BenchMsg::Status(format!("Random seek on {}...", device_name)));

    // Random seek benchmark
    let seek_samples = 200;
    match disk::bench_random_seek(device_path, seek_samples, cancel, Some(tx), test_start) {
        Ok(seek_result) => {
            result.seek_times_ms = seek_result.latencies;
            result.avg_seek_ms = seek_result.avg;
            result.max_seek_ms = seek_result.max;
            result.read_errors.extend(seek_result.errors.into_iter().map(|e| (0.0, e)));
        }
        Err(e) => {
            let _ = tx.send(BenchMsg::Status(format!("Seek test error: {}", e)));
            return;
        }
    }

    let _ = tx.send(BenchMsg::DiskUpdate(result));
    let _ = tx.send(BenchMsg::Status("✓ Test complete".into()));
}
