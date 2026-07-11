mod sysinfo;
mod bench;
mod app;
mod ui;
mod report;
mod cli;

use app::{App, Screen};
use bench::{BenchMsg, DiskBenchResult};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() -> io::Result<()> {
    let sys = sysinfo::gather();
    let args = cli::Args::parse_cli();

    // Handle --list-disks mode
    if args.list_disks {
        cli::print_disks();
        return Ok(());
    }

    // Handle --dump mode
    if args.dump {
        return dump_mode(&sys);
    }

    // Handle --quick-test or --full-test modes (non-interactive)
    if args.quick_test || args.full_test {
        let is_quick = args.quick_test;
        let (samples, sample_size) = if is_quick { (64, 8) } else { (512, 16) };
        return test_mode(&sys, samples, sample_size, args.disk, &args);
    }

    // Handle interactive TUI mode (default)
    if args.should_show_tui() {
        return tui_mode(&sys);
    }

    // Fallback: show TUI
    tui_mode(&sys)
}

fn tui_mode(sys: &sysinfo::SysInfo) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut app = App::new(sys.clone());

    // Start background CPU/memory benchmarks
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || run_benchmarks(tx));

    let mut rx = rx;
    loop {
        // Collect all pending messages
        while let Ok(msg) = rx.try_recv() {
            match msg {
                BenchMsg::Status(s) => app.bench_results.status = s,
                BenchMsg::CpuDone(mops) => app.bench_results.cpu_mops = Some(mops),
                BenchMsg::SweepPoint(log2kb, mbs) => app.bench_results.sweep.push((log2kb, mbs)),
                _ => {}
            }
        }

        // Collect disk test messages
        if let Some(ref disk_rx) = app.disk_test_rx {
            while let Ok(msg) = disk_rx.try_recv() {
                match msg {
                    BenchMsg::Status(s) => app.bench_results.status = s,
                    BenchMsg::DiskUpdate(result) => {
                        app.disk_results.insert(result.device.clone(), result);
                    }
                    BenchMsg::Progress(curr, total, elapsed) => {
                        app.current_progress = Some((curr, total, elapsed));
                    }
                    _ => {}
                }
            }
        }

        // Draw current screen
        term.draw(|f| {
            ui::render_screen(f, &app);
        })?;

        // Handle input with screen-contextual semantics
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') => {
                        app.request_cancel();
                        app.join_worker();
                        break;
                    }
                    KeyCode::Esc => {
                        // Esc: cancel running test, else back, else quit
                        if let Some(ref disk_rx) = app.disk_test_rx {
                            if disk_rx.try_recv().is_err() {
                                // No more messages, test is done
                                app.disk_test_rx = None;
                            }
                        }
                        if app.disk_test_rx.is_some() {
                            // Test is running, cancel it
                            app.request_cancel();
                        } else {
                            // Test not running, go back if not on Overview
                            match app.screen {
                                Screen::Overview => {
                                    app.join_worker();
                                    break;
                                }
                                Screen::DiskSelect | Screen::DiskTest => {
                                    app.switch_screen(Screen::Overview);
                                }
                                _ => {
                                    app.switch_screen(Screen::Overview);
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        let (tx2, rx2) = mpsc::channel();
                        rx = rx2;
                        app.bench_results = Default::default();
                        thread::spawn(move || run_benchmarks(tx2));
                    }
                    KeyCode::F(1) | KeyCode::Char('1') => {
                        app.switch_screen(Screen::Overview);
                    }
                    KeyCode::F(2) | KeyCode::Char('2') => {
                        app.switch_screen(Screen::DiskSelect);
                    }
                    KeyCode::F(3) | KeyCode::Char('3') => {
                        app.switch_screen(Screen::MemTest);
                    }
                    KeyCode::F(4) | KeyCode::Char('4') => {
                        app.switch_screen(Screen::Report);
                    }
                    KeyCode::Tab => {
                        match app.screen {
                            Screen::Overview => app.switch_screen(Screen::DiskSelect),
                            Screen::DiskSelect => app.switch_screen(Screen::MemTest),
                            Screen::MemTest => app.switch_screen(Screen::Report),
                            _ => app.switch_screen(Screen::Overview),
                        }
                    }
                    KeyCode::BackTab => {
                        match app.screen {
                            Screen::Overview => app.switch_screen(Screen::Report),
                            Screen::DiskSelect => app.switch_screen(Screen::Overview),
                            Screen::MemTest => app.switch_screen(Screen::DiskSelect),
                            Screen::Report => app.switch_screen(Screen::MemTest),
                            _ => {}
                        }
                    }
                    KeyCode::Up => {
                        if app.screen == Screen::DiskSelect {
                            app.prev_disk();
                        }
                    }
                    KeyCode::Down => {
                        if app.screen == Screen::DiskSelect {
                            app.next_disk();
                        }
                    }
                    KeyCode::Enter => {
                        if app.screen == Screen::DiskSelect {
                            app.switch_screen(Screen::DiskTest);
                        }
                    }
                    KeyCode::Char('t') => {
                        if app.screen == Screen::DiskSelect || app.screen == Screen::DiskTest {
                            start_disk_test(&mut app, 64, 8);
                        }
                    }
                    KeyCode::Char('T') => {
                        if app.screen == Screen::DiskSelect || app.screen == Screen::DiskTest {
                            start_disk_test(&mut app, 512, 16);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn dump_mode(sys: &sysinfo::SysInfo) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || run_benchmarks(tx));

    let mut res = bench::BenchResults::default();
    while let Ok(msg) = rx.recv() {
        match msg {
            BenchMsg::Status(s) => res.status = s,
            BenchMsg::CpuDone(mops) => res.cpu_mops = Some(mops),
            BenchMsg::SweepPoint(log2kb, mbs) => res.sweep.push((log2kb, mbs)),
            _ => {}
        }
        if res.status == "PASSED" {
            break;
        }
    }

    let backend = ratatui::backend::TestBackend::new(100, 34);
    let mut term = Terminal::new(backend)?;
    term.draw(|f| {
        ui::overview::render(f, sys, &res);
    })?;

    let buf = term.backend().buffer().clone();
    for yy in 0..buf.area.height {
        let mut line = String::new();
        for xx in 0..buf.area.width {
            line.push_str(buf.get(xx, yy).symbol());
        }
        println!("{}", line.trim_end());
    }
    Ok(())
}

fn test_mode(sys: &sysinfo::SysInfo, samples: usize, sample_size_mb: usize, selected_disk: Option<usize>, args: &cli::Args) -> io::Result<()> {
    println!("Running benchmarks...\n");

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || run_benchmarks(tx));

    let mut bench_results = bench::BenchResults::default();
    let mut disk_results = std::collections::HashMap::new();

    // Collect CPU/memory benchmarks
    println!("CPU & Memory:");
    while let Ok(msg) = rx.recv() {
        match msg {
            BenchMsg::Status(s) => {
                bench_results.status = s.clone();
                print!("  {}\r", s);
                std::io::stdout().flush().ok();
            }
            BenchMsg::CpuDone(mops) => bench_results.cpu_mops = Some(mops),
            BenchMsg::SweepPoint(log2kb, mbs) => bench_results.sweep.push((log2kb, mbs)),
            _ => {}
        }
        if bench_results.status == "PASSED" {
            println!("  ✓ CPU/Memory benchmarks completed");
            break;
        }
    }

    // Optional: run disk benchmarks if selected or all if in quick/full test mode
    if !args.quick_test && !args.full_test {
        println!("\nNo disk tests requested. Use -t1 or -t2 with --disk N for disk benchmarks.");
    } else if let Some(disk_idx) = selected_disk {
        // Test specific disk
        let disks = bench::disk::scan_disks();
        if disk_idx < disks.len() {
            let disk = &disks[disk_idx];
            println!("\nDisk Test ({}):", disk.name);
            let (disk_tx, disk_rx) = mpsc::channel();
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let disk_path = disk.path.clone();
            let disk_name = disk.name.clone();
            let cancel_clone = cancel.clone();

            thread::spawn(move || {
                let mut result = bench::DiskBenchResult {
                    device: disk_name,
                    ..Default::default()
                };

                let test_start = std::time::Instant::now();
                let _ = disk_tx.send(BenchMsg::Status(format!("Testing disk {}...", disk_path)));

                match bench::disk::bench_linear_read(&disk_path, samples, sample_size_mb, &cancel_clone, Some(&disk_tx), test_start) {
                    Ok((data, avg, min, max)) => {
                        result.linear_speed_mbs = data;
                        result.avg_linear_mbs = avg;
                        result.min_linear_mbs = min;
                        result.max_linear_mbs = max;
                        let _ = disk_tx.send(BenchMsg::DiskUpdate(result.clone()));
                    }
                    Err(e) => {
                        let _ = disk_tx.send(BenchMsg::Status(format!("Error: {}", e)));
                        return;
                    }
                }

                match bench::disk::bench_random_seek(&disk_path, 200, &cancel_clone, Some(&disk_tx), test_start) {
                    Ok((latencies, avg, max)) => {
                        result.seek_times_ms = latencies;
                        result.avg_seek_ms = avg;
                        result.max_seek_ms = max;
                    }
                    Err(e) => {
                        let _ = disk_tx.send(BenchMsg::Status(format!("Seek error: {}", e)));
                        return;
                    }
                }

                let _ = disk_tx.send(BenchMsg::DiskUpdate(result));
                let _ = disk_tx.send(BenchMsg::Status("✓ Test complete".into()));
            });

            // Collect disk test results
            while let Ok(msg) = disk_rx.recv() {
                match msg {
                    BenchMsg::Status(s) => {
                        println!("  {}", s);
                    }
                    BenchMsg::DiskUpdate(result) => {
                        disk_results.insert(result.device.clone(), result);
                    }
                    BenchMsg::Progress(curr, total, _elapsed) => {
                        print!("  Progress: {}/{}\r", curr, total);
                        std::io::stdout().flush().ok();
                    }
                    _ => {}
                }
            }
            println!();
        } else {
            eprintln!("Error: Invalid disk index {}", disk_idx);
            return Ok(());
        }
    }

    // Export results if requested
    if let Some((path, format)) = args.export_path() {
        let rep = report::Report::new(sys.clone(), bench_results, disk_results);
        match format {
            "json" => {
                rep.export_json(path).map_err(io::Error::other)?;
                println!("✓ Results exported to {} (JSON)", path);
            }
            "csv" => {
                rep.export_csv(path).map_err(io::Error::other)?;
                println!("✓ Results exported to {} (CSV)", path);
            }
            "html" => {
                rep.export_html(path).map_err(io::Error::other)?;
                println!("✓ Results exported to {} (HTML)", path);
            }
            _ => {}
        }
    } else {
        // Print results to console
        if let Some(mops) = bench_results.cpu_mops {
            println!("\nResults:");
            println!("  CPU: {:.1} MOPS", mops);
        }
        println!("  Memory: {} points measured", bench_results.sweep.len());
        for (disk_name, result) in disk_results {
            println!("  {}: {:.1} MB/s avg (linear), {:.2} ms avg (seek)",
                disk_name, result.avg_linear_mbs, result.avg_seek_ms);
        }
    }

    Ok(())
}

fn run_benchmarks(tx: mpsc::Sender<BenchMsg>) {
    let _ = tx.send(BenchMsg::Status("Testing processor...".into()));

    let mops = bench::cpu_bench();
    let _ = tx.send(BenchMsg::CpuDone(mops));
    let _ = tx.send(BenchMsg::Status("Testing memory throughput...".into()));

    for kb in [
        4usize, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536,
    ] {
        let mbs = bench::mem_read_speed(kb * 1024);
        let _ = tx.send(BenchMsg::SweepPoint((kb as f64).log2(), mbs));
        let _ = tx.send(BenchMsg::Status(format!("Testing memory... {kb} KB")));
    }
    let _ = tx.send(BenchMsg::Status("PASSED".into()));
}

fn start_disk_test(app: &mut App, samples: usize, sample_size_mb: usize) {
    // Safety check: if no disks in app, something went wrong
    if app.disks.is_empty() {
        app.bench_results.status = "No disks detected. Check permissions.".into();
        return;
    }

    let device_name = app.disks.get(app.selected_disk).cloned().unwrap_or_default();
    if device_name.is_empty() {
        app.bench_results.status = "Invalid disk selection.".into();
        return;
    }

    let all_devices = bench::disk::scan_disks();
    let device = match all_devices.iter().find(|d| d.name == device_name) {
        Some(d) => d.clone(),
        None => {
            app.bench_results.status = format!("Disk {} not found in scan.", device_name);
            return;
        }
    };

    let (tx, rx) = mpsc::channel();
    let cancel = app.cancel.clone();
    app.reset_cancel();

    let handle = thread::spawn(move || {
        let mut result = DiskBenchResult {
            device: device.name.clone(),
            ..Default::default()
        };

        let test_start = std::time::Instant::now();
        let _ = tx.send(BenchMsg::Status(format!("Linear read on {}...", device.name)));

        // Linear read with cancellation and progress
        match bench::disk::bench_linear_read(&device.path, samples, sample_size_mb, &cancel, Some(&tx), test_start) {
            Ok((data, avg, min, max)) => {
                result.linear_speed_mbs = data;
                result.avg_linear_mbs = avg;
                result.min_linear_mbs = min;
                result.max_linear_mbs = max;
                let _ = tx.send(BenchMsg::DiskUpdate(result.clone()));
            }
            Err(e) => {
                let _ = tx.send(BenchMsg::Status(format!("Linear read error: {}", e)));
                return;
            }
        }

        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        let _ = tx.send(BenchMsg::Status(format!("Random seek on {}...", device.name)));

        // Random seek with cancellation and progress
        let seek_samples = 200; // Quick test default
        match bench::disk::bench_random_seek(&device.path, seek_samples, &cancel, Some(&tx), test_start) {
            Ok((latencies, avg, max)) => {
                result.seek_times_ms = latencies;
                result.avg_seek_ms = avg;
                result.max_seek_ms = max;
            }
            Err(e) => {
                let _ = tx.send(BenchMsg::Status(format!("Seek test error: {}", e)));
                return;
            }
        }

        let _ = tx.send(BenchMsg::DiskUpdate(result));
        let _ = tx.send(BenchMsg::Status("✓ Test complete".into()));
    });

    app.worker = Some(handle);
    app.disk_test_rx = Some(rx);
}

#[allow(dead_code)]
fn export_report(sys: &sysinfo::SysInfo, path: &str, format: &str) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || run_benchmarks(tx));

    let mut bench_results = bench::BenchResults::default();
    let disk_results = std::collections::HashMap::new();

    // Collect benchmark results
    while let Ok(msg) = rx.recv() {
        match msg {
            BenchMsg::Status(s) => bench_results.status = s,
            BenchMsg::CpuDone(mops) => bench_results.cpu_mops = Some(mops),
            BenchMsg::SweepPoint(log2kb, mbs) => bench_results.sweep.push((log2kb, mbs)),
            _ => {}
        }
        if bench_results.status == "PASSED" {
            break;
        }
    }

    let rep = report::Report::new(sys.clone(), bench_results, disk_results);

    match format {
        "json" => {
            rep.export_json(path).map_err(io::Error::other)?;
            println!("✓ Report exported to {}", path);
        }
        "html" => {
            rep.export_html(path).map_err(io::Error::other)?;
            println!("✓ Report exported to {}", path);
        }
        "csv" => {
            rep.export_csv(path).map_err(io::Error::other)?;
            println!("✓ Report exported to {}", path);
        }
        _ => eprintln!("Unknown format: {}", format),
    }

    Ok(())
}
