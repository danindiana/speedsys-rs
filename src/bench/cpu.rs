use std::hint::black_box;
use std::time::Instant;

/// Integer ALU benchmark: LCG updates, returns Mops/s.
pub fn cpu_bench() -> f64 {
    let mut x: u64 = 0x2545F4914F6CDD1D;
    let iters: u64 = 300_000_000;
    let t0 = Instant::now();
    for _ in 0..iters {
        x = black_box(x)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
    }
    black_box(x);
    iters as f64 / t0.elapsed().as_secs_f64() / 1e6
}
