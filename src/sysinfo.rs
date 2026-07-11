use std::fs;

pub fn read(p: &str) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

pub fn cpuinfo_field(key: &str) -> Option<String> {
    let txt = fs::read_to_string("/proc/cpuinfo").ok()?;
    txt.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split(':').nth(1))
        .map(|v| v.trim().to_string())
}

pub fn meminfo_kb(key: &str) -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_whitespace().nth(1).map(str::to_string))
        })
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

#[derive(Clone, Debug)]
pub struct SysInfo {
    pub cpu_model: String,
    pub cpu_id: String,
    pub cores: usize,
    pub mhz: String,
    pub mem_total_mb: u64,
    pub caches: Vec<String>,
    pub drives: Vec<String>,
    pub bios: String,
    pub board: String,
    pub os: String,
}

pub fn gather() -> SysInfo {
    let caches = (0..8)
        .filter_map(|i| {
            let base = format!("/sys/devices/system/cpu/cpu0/cache/index{i}");
            let level = read(&format!("{base}/level"))?;
            let ty = read(&format!("{base}/type"))?;
            let size = read(&format!("{base}/size"))?;
            Some(format!("L{level} {ty:<12} {size}"))
        })
        .collect();

    let drives = fs::read_dir("/sys/block")
        .map(|rd| {
            rd.filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().into_string().ok()?;
                if name.starts_with("loop") || name.starts_with("ram") {
                    return None;
                }
                let sectors: u64 = read(&format!("/sys/block/{name}/size"))?.parse().ok()?;
                if sectors < 1_000_000 {
                    return None;
                }
                let model = read(&format!("/sys/block/{name}/device/model"))
                    .unwrap_or_else(|| "unknown".into());
                Some(format!(
                    "{name}: {model} {:.1} GB",
                    sectors as f64 * 512.0 / 1e9
                ))
            })
            .collect()
        })
        .unwrap_or_default();

    let os = read("/etc/os-release")
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("PRETTY_NAME="))
                .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "Linux".into());

    SysInfo {
        cpu_model: cpuinfo_field("model name").unwrap_or_else(|| "unknown CPU".into()),
        cpu_id: format!(
            "family {} model {} stepping {}",
            cpuinfo_field("cpu family").unwrap_or_default(),
            cpuinfo_field("model\t").or(cpuinfo_field("model")).unwrap_or_default(),
            cpuinfo_field("stepping").unwrap_or_default()
        ),
        cores: fs::read_to_string("/proc/cpuinfo")
            .map(|t| t.matches("processor\t").count().max(1))
            .unwrap_or(1),
        mhz: cpuinfo_field("cpu MHz").unwrap_or_else(|| "?".into()),
        mem_total_mb: meminfo_kb("MemTotal") / 1024,
        caches,
        drives,
        bios: format!(
            "{} {} ({})",
            read("/sys/class/dmi/id/bios_vendor").unwrap_or_default(),
            read("/sys/class/dmi/id/bios_version").unwrap_or_default(),
            read("/sys/class/dmi/id/bios_date").unwrap_or_default()
        ),
        board: format!(
            "{} {}",
            read("/sys/class/dmi/id/board_vendor").unwrap_or_default(),
            read("/sys/class/dmi/id/board_name").unwrap_or_default()
        ),
        os,
    }
}
