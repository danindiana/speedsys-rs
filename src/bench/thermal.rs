use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct ThermalSample {
    pub temp_c: Option<f64>,
    pub freq_mhz: Option<u64>,
    pub max_freq_mhz: Option<u64>,
}

/// Find the hwmon directory that corresponds to the CPU temperature sensor.
/// Looks for known CPU driver names: k10temp (AMD), coretemp (Intel), zenpower (AMD alternative).
fn find_cpu_hwmon() -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name_file = path.join("name");

            if let Ok(name) = fs::read_to_string(&name_file) {
                let name = name.trim();
                if name == "k10temp" || name == "coretemp" || name == "zenpower" {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Read CPU temperature from hwmon sensor. Returns the maximum of all temp*_input
/// files (since some drivers like k10temp expose multiple CCD temperatures).
fn read_cpu_temp(hwmon_dir: &Path) -> Option<f64> {
    let mut max_temp = None;

    if let Ok(entries) = fs::read_dir(hwmon_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let filename = path.file_name()?;
            let filename_str = filename.to_str()?;

            if filename_str.starts_with("temp") && filename_str.ends_with("_input") {
                if let Ok(contents) = fs::read_to_string(&path) {
                    if let Ok(temp_millidegrees) = contents.trim().parse::<u64>() {
                        let temp_c = temp_millidegrees as f64 / 1000.0;
                        max_temp = Some(match max_temp {
                            None => temp_c,
                            Some(prev) => if temp_c > prev { temp_c } else { prev },
                        });
                    }
                }
            }
        }
    }

    max_temp
}

/// Read CPU frequency (scaling_cur_freq) and maximum frequency (scaling_max_freq or cpuinfo_max_freq).
/// Returns (current_freq_mhz, max_freq_mhz), converting from kHz to MHz.
fn read_cpu_freq() -> (Option<u64>, Option<u64>) {
    let cpufreq_base = "/sys/devices/system/cpu/cpu0/cpufreq";

    let cur_freq = fs::read_to_string(format!("{}/scaling_cur_freq", cpufreq_base))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|khz| khz / 1000);

    let max_freq = fs::read_to_string(format!("{}/scaling_max_freq", cpufreq_base))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .or_else(|| {
            fs::read_to_string(format!("{}/cpuinfo_max_freq", cpufreq_base))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
        })
        .map(|khz| khz / 1000);

    (cur_freq, max_freq)
}

/// Sample CPU thermal state: temperature and frequency (current/max).
pub fn sample() -> ThermalSample {
    let temp_c = find_cpu_hwmon()
        .and_then(|hwmon_path| read_cpu_temp(&hwmon_path));

    let (freq_mhz, max_freq_mhz) = read_cpu_freq();

    ThermalSample {
        temp_c,
        freq_mhz,
        max_freq_mhz,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_millidegrees() {
        // Test converting millidegrees to Celsius
        let millidegrees = 71125u64;
        let celsius = millidegrees as f64 / 1000.0;
        assert!((celsius - 71.125).abs() < 0.001);
    }

    #[test]
    fn test_throttle_detection_below_threshold() {
        let cur_freq = 2723;  // MHz
        let max_freq = 3400;  // MHz
        let percent = (cur_freq as f64 / max_freq as f64) * 100.0;
        assert!(percent < 90.0);  // Should be throttled
    }

    #[test]
    fn test_throttle_detection_above_threshold() {
        let cur_freq = 3200;  // MHz
        let max_freq = 3400;  // MHz
        let percent = (cur_freq as f64 / max_freq as f64) * 100.0;
        assert!(percent >= 90.0);  // Should NOT be throttled
    }

    #[test]
    fn test_thermal_sample_creation() {
        let sample = ThermalSample {
            temp_c: Some(71.125),
            freq_mhz: Some(2723),
            max_freq_mhz: Some(3400),
        };
        assert_eq!(sample.temp_c, Some(71.125));
        assert_eq!(sample.freq_mhz, Some(2723));
        assert_eq!(sample.max_freq_mhz, Some(3400));
    }
}
