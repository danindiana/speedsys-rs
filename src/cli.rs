use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "speedsys-rs")]
#[command(about = "System performance benchmarking tool", long_about = "
Comprehensive system benchmark suite for Linux systems.

Measures CPU performance, memory throughput, and disk I/O characteristics.
")]
#[command(version)]
pub struct Args {
    /// Quick test mode (64 samples per metric)
    #[arg(short = 't', long, value_name = "1")]
    pub quick_test: bool,

    /// Full test mode (512 samples per metric)
    #[arg(short = 'T', long, value_name = "2")]
    pub full_test: bool,

    /// Select specific disk by index (0=sda, 1=sdb, etc.)
    #[arg(long, value_name = "N")]
    pub disk: Option<usize>,

    /// List available disks
    #[arg(long)]
    pub list_disks: bool,

    /// Include SMART data in tests
    #[arg(short = 's', long)]
    pub smart: bool,

    /// Export results to JSON file
    #[arg(short = 'r', long, value_name = "FILE")]
    pub report: Option<String>,

    /// Export results to CSV file
    #[arg(short = 'c', long, value_name = "FILE")]
    pub report_csv: Option<String>,

    /// Export results to HTML file
    #[arg(short = 'h', long, value_name = "FILE")]
    pub report_html: Option<String>,

    /// Print system information only
    #[arg(long)]
    pub dump: bool,

    /// Non-interactive test mode (run and exit, no TUI)
    #[arg(long)]
    pub test: bool,

    /// Generate screenshot of a specific screen (overview, disk-select, disk-test)
    #[arg(long, value_name = "SCREEN")]
    pub screenshot: Option<String>,

    /// Output path for screenshot SVG
    #[arg(long, value_name = "FILE")]
    pub screenshot_out: Option<String>,
}

impl Args {
    pub fn parse_cli() -> Self {
        Args::parse()
    }

    /// Determine if we should show TUI (interactive mode)
    pub fn should_show_tui(&self) -> bool {
        !self.dump
            && !self.list_disks
            && !self.quick_test
            && !self.full_test
            && !self.test
            && self.report.is_none()
            && self.report_csv.is_none()
            && self.report_html.is_none()
    }

    /// Get test mode (quick=true, full=false, none=None)
    #[allow(dead_code)]
    pub fn test_mode(&self) -> Option<bool> {
        if self.quick_test {
            Some(true)
        } else if self.full_test {
            Some(false)
        } else {
            None
        }
    }

    /// Check if any export format is requested
    pub fn export_path(&self) -> Option<(&str, &str)> {
        if let Some(path) = &self.report {
            Some((path, "json"))
        } else if let Some(path) = &self.report_csv {
            Some((path, "csv"))
        } else if let Some(path) = &self.report_html {
            Some((path, "html"))
        } else {
            None
        }
    }
}

/// Print available disks in a formatted table
pub fn print_disks() {
    let disks = crate::bench::disk::scan_disks();

    if disks.is_empty() {
        println!("No disks found (requires root/sudo)");
        return;
    }

    println!("\n{:<6} {:<20} {:<30} {:<10} Type",
        "Index", "Device", "Model", "Size");
    println!("{}", "─".repeat(80));

    for (idx, disk) in disks.iter().enumerate() {
        let size_gb = disk.size_bytes as f64 / 1e9;
        let disk_type = if disk.is_rotational { "HDD" } else { "SSD" };
        println!("{:<6} {:<20} {:<30} {:<10.1}GB {}",
            idx,
            format!("/dev/{}", disk.name),
            disk.model,
            size_gb,
            disk_type
        );
    }
    println!();
}
