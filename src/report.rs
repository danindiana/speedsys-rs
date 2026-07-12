use crate::bench::{BenchResults, DiskBenchResult};
use crate::sysinfo::SysInfo;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

pub struct Report {
    pub sys_info: SysInfo,
    pub bench_results: BenchResults,
    pub disk_results: HashMap<String, DiskBenchResult>,
}

impl Report {
    pub fn new(sys_info: SysInfo, bench_results: BenchResults, disk_results: HashMap<String, DiskBenchResult>) -> Self {
        Report {
            sys_info,
            bench_results,
            disk_results,
        }
    }

    /// Export to JSON format
    pub fn export_json(&self, path: &str) -> Result<(), String> {
        let json = serde_json::json!({
            "timestamp": chrono::Local::now().to_rfc3339(),
            "system": {
                "cpu_model": &self.sys_info.cpu_model,
                "cores": self.sys_info.cores,
                "mhz": &self.sys_info.mhz,
                "os": &self.sys_info.os,
                "mem_mb": self.sys_info.mem_total_mb,
            },
            "benchmarks": {
                "cpu_mops": self.bench_results.cpu_mops,
                "memory_sweep": self.bench_results.sweep.iter()
                    .map(|(log2kb, mbs)| {
                        serde_json::json!({
                            "log2_kb": log2kb,
                            "mb_s": mbs
                        })
                    })
                    .collect::<Vec<_>>(),
                "disks": self.disk_results.iter()
                    .map(|(name, result)| {
                        (name.clone(), serde_json::json!({
                            "linear_speed_mbs": {
                                "avg": result.avg_linear_mbs,
                                "min": result.min_linear_mbs,
                                "max": result.max_linear_mbs,
                                "samples": result.linear_speed_mbs.len(),
                            },
                            "seek_latency_ms": {
                                "avg": result.avg_seek_ms,
                                "max": result.max_seek_ms,
                                "samples": result.seek_times_ms.len(),
                            },
                            "smart": {
                                "temperature_c": result.smart_temp,
                                "power_on_hours": result.smart_hours,
                                "defect_sectors": result.smart_sectors,
                            },
                            "raid": {
                                "level": &result.raid_level,
                                "members": result.raid_members,
                                "state": &result.raid_state,
                            }
                        }))
                    })
                    .collect::<HashMap<_, _>>(),
            },
        });

        let mut file = File::create(path).map_err(|e| e.to_string())?;
        file.write_all(serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?.as_bytes())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Export to CSV format
    pub fn export_csv(&self, path: &str) -> Result<(), String> {
        let mut file = File::create(path).map_err(|e| e.to_string())?;

        // Header
        writeln!(file, "Device,Test Type,Metric,Value,Unit").map_err(|e| e.to_string())?;

        // CPU result
        if let Some(mops) = self.bench_results.cpu_mops {
            writeln!(file, "CPU,Benchmark,Performance,{:.1},MOPS", mops).map_err(|e| e.to_string())?;
        }

        // Memory sweep
        for (log2kb, mbs) in &self.bench_results.sweep {
            let kb = 2f64.powf(*log2kb);
            writeln!(file, "Memory,Bandwidth,{:.0}KB,{:.1},MB/s", kb, mbs).map_err(|e| e.to_string())?;
        }

        // Disk results
        for (device, result) in &self.disk_results {
            writeln!(file, "{},Linear Read,Average,{:.1},MB/s", device, result.avg_linear_mbs).map_err(|e| e.to_string())?;
            writeln!(file, "{},Linear Read,Min,{:.1},MB/s", device, result.min_linear_mbs).map_err(|e| e.to_string())?;
            writeln!(file, "{},Linear Read,Max,{:.1},MB/s", device, result.max_linear_mbs).map_err(|e| e.to_string())?;
            writeln!(file, "{},Seek Latency,Average,{:.2},ms", device, result.avg_seek_ms).map_err(|e| e.to_string())?;
            writeln!(file, "{},Seek Latency,Max,{:.2},ms", device, result.max_seek_ms).map_err(|e| e.to_string())?;
            if let Some(temp) = result.smart_temp {
                writeln!(file, "{},SMART,Temperature,{:.0},°C", device, temp).map_err(|e| e.to_string())?;
            }
            if let Some(hours) = result.smart_hours {
                writeln!(file, "{},SMART,Power-On Hours,{},hrs", device, hours).map_err(|e| e.to_string())?;
            }
            if let Some(sectors) = result.smart_sectors {
                writeln!(file, "{},SMART,Defect Sectors,{},sectors", device, sectors).map_err(|e| e.to_string())?;
            }
            if let Some(level) = &result.raid_level {
                writeln!(file, "{},RAID,Level,{},—", device, level).map_err(|e| e.to_string())?;
            }
            if let Some(members) = result.raid_members {
                writeln!(file, "{},RAID,Members,{},—", device, members).map_err(|e| e.to_string())?;
            }
            if let Some(state) = &result.raid_state {
                writeln!(file, "{},RAID,State,{},—", device, state).map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }

    /// Export to HTML format with embedded data
    pub fn export_html(&self, path: &str) -> Result<(), String> {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let device_rows = self.disk_results.iter()
            .map(|(name, result)| {
                let temp_cell = result.smart_temp
                    .map(|t| format!("{:.0}°C", t))
                    .unwrap_or_else(|| "—".to_string());
                let hours_cell = result.smart_hours
                    .map(|h| format!("{}", h))
                    .unwrap_or_else(|| "—".to_string());
                let sectors_cell = result.smart_sectors
                    .map(|s| {
                        if s > 0 {
                            format!("<span class=\"defect\">{}</span>", s)
                        } else {
                            format!("{}", s)
                        }
                    })
                    .unwrap_or_else(|| "—".to_string());
                let raid_level_cell = result.raid_level
                    .as_ref()
                    .map(|l| l.clone())
                    .unwrap_or_else(|| "—".to_string());
                let raid_members_cell = result.raid_members
                    .map(|m| format!("{}", m))
                    .unwrap_or_else(|| "—".to_string());
                let raid_state_cell = result.raid_state
                    .as_ref()
                    .map(|s| {
                        let class = match s.as_str() {
                            "clean" => "raid-clean",
                            "degraded" | "failed" => "raid-alert",
                            _ => "raid-warning",
                        };
                        format!("<span class=\"{}\">{}</span>", class, s)
                    })
                    .unwrap_or_else(|| "—".to_string());
                format!(
                    "<tr><td>{}</td><td>{:.1}</td><td>{:.1}</td><td>{:.1}</td><td>{:.2}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    name, result.avg_linear_mbs, result.min_linear_mbs, result.max_linear_mbs, result.avg_seek_ms,
                    temp_cell, hours_cell, sectors_cell, raid_level_cell, raid_members_cell, raid_state_cell
                )
            })
            .collect::<Vec<_>>()
            .join("\n    ");

        let cpu_row = if let Some(mops) = self.bench_results.cpu_mops {
            format!("<tr><td>CPU</td><td colspan=\"3\">{:.1} MOPS</td><td>—</td></tr>", mops)
        } else {
            String::new()
        };

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>System Benchmark Report</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            margin: 2rem;
            background: #f5f5f5;
        }}
        .container {{
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            padding: 2rem;
            border-radius: 8px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.1);
        }}
        h1 {{
            color: #333;
            border-bottom: 3px solid #00c9ff;
            padding-bottom: 1rem;
        }}
        h2 {{
            color: #555;
            margin-top: 2rem;
            font-size: 1.3rem;
        }}
        .system-info {{
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 1rem;
            margin: 1rem 0;
            font-size: 0.95rem;
        }}
        .info-item {{
            padding: 0.5rem;
            background: #f9f9f9;
            border-left: 3px solid #00c9ff;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin: 1rem 0;
        }}
        th {{
            background: #00c9ff;
            color: white;
            padding: 0.75rem;
            text-align: left;
            font-weight: 600;
        }}
        td {{
            padding: 0.75rem;
            border-bottom: 1px solid #eee;
        }}
        tr:hover {{
            background: #f9f9f9;
        }}
        .timestamp {{
            color: #888;
            font-size: 0.9rem;
            margin-top: 2rem;
            text-align: right;
        }}
        .defect {{
            color: #ff4444;
            font-weight: bold;
        }}
        .raid-clean {{
            color: #22c55e;
            font-weight: bold;
        }}
        .raid-warning {{
            color: #eab308;
            font-weight: bold;
        }}
        .raid-alert {{
            color: #ff4444;
            font-weight: bold;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>System Benchmark Report</h1>

        <h2>System Information</h2>
        <div class="system-info">
            <div class="info-item"><strong>CPU Model:</strong> {}</div>
            <div class="info-item"><strong>CPU Cores:</strong> {}</div>
            <div class="info-item"><strong>OS:</strong> {}</div>
            <div class="info-item"><strong>RAM:</strong> {:.1} GB</div>
        </div>

        <h2>Benchmark Results</h2>
        <table>
            <thead>
                <tr>
                    <th>Device</th>
                    <th>Linear Read (MB/s)</th>
                    <th>Min Speed</th>
                    <th>Max Speed</th>
                    <th>Seek Latency (ms)</th>
                    <th>Temperature (°C)</th>
                    <th>Power-On Hrs</th>
                    <th>Defect Sectors</th>
                    <th>RAID Level</th>
                    <th>Members</th>
                    <th>Array State</th>
                </tr>
            </thead>
            <tbody>
                {}
                {}
            </tbody>
        </table>

        <div class="timestamp">Report generated: {}</div>
    </div>
</body>
</html>
"#,
            &self.sys_info.cpu_model,
            self.sys_info.cores,
            &self.sys_info.os,
            self.sys_info.mem_total_mb as f64 / 1024.0,
            device_rows,
            cpu_row,
            timestamp
        );

        let mut file = File::create(path).map_err(|e| e.to_string())?;
        file.write_all(html.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }
}
