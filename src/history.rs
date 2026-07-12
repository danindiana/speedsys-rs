use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub timestamp: String,
    pub device: String,
    pub avg_linear_mbs: f64,
    pub min_linear_mbs: f64,
    pub max_linear_mbs: f64,
    pub avg_seek_ms: f64,
    pub max_seek_ms: f64,
}

impl HistoryRecord {
    pub fn from_bench_result(result: &crate::bench::DiskBenchResult) -> Self {
        Self {
            timestamp: chrono::Local::now().to_rfc3339(),
            device: result.device.clone(),
            avg_linear_mbs: result.avg_linear_mbs,
            min_linear_mbs: result.min_linear_mbs,
            max_linear_mbs: result.max_linear_mbs,
            avg_seek_ms: result.avg_seek_ms,
            max_seek_ms: result.max_seek_ms,
        }
    }
}

pub struct HistoryStats {
    pub linear_min: f64,
    pub linear_avg: f64,
    pub linear_max: f64,
    pub seek_min: f64,
    pub seek_avg: f64,
    pub seek_max: f64,
    pub record_count: usize,
    pub oldest: String,
    pub newest: String,
}

fn history_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".speedsys-rs/history")
}

pub fn save_result(result: &crate::bench::DiskBenchResult) -> Result<(), String> {
    let record = HistoryRecord::from_bench_result(result);
    let dir = history_dir();

    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create history dir: {}", e))?;

    // Filename: device_name-timestamp.json (sanitize device name)
    let device_safe = result.device.replace("/", "_");
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("{}-{}.json", device_safe, timestamp);
    let path = dir.join(filename);

    let json = serde_json::to_string_pretty(&record)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;

    fs::write(&path, json)
        .map_err(|e| format!("Failed to write history file: {}", e))?;

    Ok(())
}

pub fn load_history(device: &str, limit: usize) -> Result<Vec<HistoryRecord>, String> {
    let dir = history_dir();

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let device_safe = device.replace("/", "_");
    let mut records = Vec::new();

    let entries = fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read history dir: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Match files like "dev_sda-20250711_195634.json"
        if !filename.starts_with(&device_safe) || !filename.ends_with(".json") {
            continue;
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read history file: {}", e))?;

        if let Ok(record) = serde_json::from_str::<HistoryRecord>(&content) {
            records.push(record);
        }
    }

    // Sort by timestamp (newest first)
    records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    records.truncate(limit);

    Ok(records)
}

pub fn calculate_stats(records: &[HistoryRecord]) -> Option<HistoryStats> {
    if records.is_empty() {
        return None;
    }

    let linear_speeds: Vec<f64> = records.iter().map(|r| r.avg_linear_mbs).collect();
    let seek_times: Vec<f64> = records.iter().map(|r| r.avg_seek_ms).collect();

    let linear_avg = linear_speeds.iter().sum::<f64>() / linear_speeds.len() as f64;
    let linear_min = linear_speeds.iter().cloned().fold(f64::INFINITY, f64::min);
    let linear_max = linear_speeds.iter().cloned().fold(0.0, f64::max);

    let seek_avg = seek_times.iter().sum::<f64>() / seek_times.len() as f64;
    let seek_min = seek_times.iter().cloned().fold(f64::INFINITY, f64::min);
    let seek_max = seek_times.iter().cloned().fold(0.0, f64::max);

    Some(HistoryStats {
        linear_min,
        linear_avg,
        linear_max,
        seek_min,
        seek_avg,
        seek_max,
        record_count: records.len(),
        oldest: records.last().map(|r| r.timestamp.clone()).unwrap_or_default(),
        newest: records.first().map(|r| r.timestamp.clone()).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_record_creation() {
        let record = HistoryRecord {
            timestamp: "2025-07-11T19:00:00+00:00".to_string(),
            device: "/dev/sda".to_string(),
            avg_linear_mbs: 100.0,
            min_linear_mbs: 95.0,
            max_linear_mbs: 105.0,
            avg_seek_ms: 0.5,
            max_seek_ms: 1.0,
        };
        assert_eq!(record.device, "/dev/sda");
        assert_eq!(record.avg_linear_mbs, 100.0);
    }

    #[test]
    fn test_calculate_stats() {
        let records = vec![
            HistoryRecord {
                timestamp: "2025-07-11T19:00:00+00:00".to_string(),
                device: "/dev/sda".to_string(),
                avg_linear_mbs: 100.0,
                min_linear_mbs: 95.0,
                max_linear_mbs: 105.0,
                avg_seek_ms: 0.5,
                max_seek_ms: 1.0,
            },
            HistoryRecord {
                timestamp: "2025-07-11T20:00:00+00:00".to_string(),
                device: "/dev/sda".to_string(),
                avg_linear_mbs: 110.0,
                min_linear_mbs: 105.0,
                max_linear_mbs: 115.0,
                avg_seek_ms: 0.6,
                max_seek_ms: 1.2,
            },
        ];

        let stats = calculate_stats(&records).unwrap();
        assert_eq!(stats.record_count, 2);
        assert_eq!(stats.linear_min, 100.0);
        assert_eq!(stats.linear_max, 110.0);
        assert!((stats.linear_avg - 105.0).abs() < 0.01);
        assert_eq!(stats.seek_min, 0.5);
        assert_eq!(stats.seek_max, 0.6);
    }

    #[test]
    fn test_calculate_stats_empty() {
        let records: Vec<HistoryRecord> = Vec::new();
        assert!(calculate_stats(&records).is_none());
    }
}
