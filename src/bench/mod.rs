pub mod cpu;
pub mod mem;
pub mod disk;

pub use cpu::cpu_bench;
pub use mem::mem_read_speed;

#[derive(Clone, Debug, Default)]
pub struct BenchResults {
    pub cpu_mops: Option<f64>,
    pub sweep: Vec<(f64, f64)>, // (log2 KB, MB/s)
    pub disk_results: Vec<DiskBenchResult>,
    pub status: String,
}

#[derive(Clone, Debug, Default)]
pub struct DiskBenchResult {
    pub device: String,
    pub linear_speed_mbs: Vec<(f64, f64)>, // (position %, MB/s)
    pub avg_linear_mbs: f64,
    pub min_linear_mbs: f64,
    pub max_linear_mbs: f64,
    pub seek_times_ms: Vec<f64>,
    pub avg_seek_ms: f64,
    pub max_seek_ms: f64,
    pub smart_temp: Option<f64>,
    pub smart_hours: Option<u64>,
    pub smart_sectors: Option<u64>,
}
