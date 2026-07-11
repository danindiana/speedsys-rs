use std::hint::black_box;
use std::time::{Duration, Instant};

/// Sequential read over a buffer of `bytes`; returns MB/s.
/// Small buffers stay resident in cache -> the classic staircase.
pub fn mem_read_speed(bytes: usize) -> f64 {
    let buf: Vec<u64> = vec![1; bytes / 8];
    let mut sum: u64 = 0;
    let mut done: usize = 0;
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(120) {
        for chunk in buf.chunks(4096 / 8) {
            for v in chunk {
                sum = sum.wrapping_add(*v);
            }
        }
        done += bytes;
    }
    black_box(sum);
    done as f64 / t0.elapsed().as_secs_f64() / 1e6
}
