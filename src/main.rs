mod sysinfo;
mod bench;
mod app;
mod ui;

use app::{App, Screen};
use bench::DiskBenchResult;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() -> io::Result<()> {
    let sys = sysinfo::gather();
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--dump") {
        return dump_mode(&sys);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut app = App::new(sys);

    // Start background benchmarks
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || run_benchmarks(tx));

    let mut rx = rx;
    loop {
        // Collect all pending results
        while let Ok(r) = rx.try_recv() {
            app.bench_results = r;
        }

        // Draw current screen
        term.draw(|f| {
            ui::render_screen(f, &app);
        })?;

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
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
                            Screen::DiskTest => app.switch_screen(Screen::Report),
                            Screen::MemTest => app.switch_screen(Screen::Report),
                            Screen::Report => app.switch_screen(Screen::Overview),
                        }
                    }
                    KeyCode::BackTab => {
                        match app.screen {
                            Screen::Overview => app.switch_screen(Screen::Report),
                            Screen::DiskSelect => app.switch_screen(Screen::Overview),
                            Screen::DiskTest => app.switch_screen(Screen::DiskSelect),
                            Screen::MemTest => app.switch_screen(Screen::DiskSelect),
                            Screen::Report => app.switch_screen(Screen::MemTest),
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
                            start_disk_test(&mut app, 64, 8, "quick");
                        }
                    }
                    KeyCode::Char('T') => {
                        if app.screen == Screen::DiskSelect || app.screen == Screen::DiskTest {
                            start_disk_test(&mut app, 512, 16, "full");
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
    while let Ok(r) = rx.recv() {
        res = r;
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

fn run_benchmarks(tx: mpsc::Sender<bench::BenchResults>) {
    let mut r = bench::BenchResults {
        status: "Testing processor...".into(),
        ..Default::default()
    };
    let _ = tx.send(r.clone());

    r.cpu_mops = Some(bench::cpu_bench());
    r.status = "Testing memory throughput...".into();
    let _ = tx.send(r.clone());

    for kb in [
        4usize, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536,
    ] {
        let mbs = bench::mem_read_speed(kb * 1024);
        r.sweep.push(((kb as f64).log2(), mbs));
        r.status = format!("Testing extended memory... {kb} KB");
        let _ = tx.send(r.clone());
    }
    r.status = "PASSED".into();
    let _ = tx.send(r);
}

fn start_disk_test(app: &mut App, samples: usize, sample_size_mb: usize, mode: &str) {
    let device_name = app.disks.get(app.selected_disk).cloned().unwrap_or_default();
    let all_devices = bench::disk::scan_disks();
    let device = match all_devices.iter().find(|d| d.name == device_name) {
        Some(d) => d.clone(),
        None => return,
    };

    let (tx, _rx) = mpsc::channel();
    let cancel = app.cancel.clone();
    let mode = mode.to_string(); // Convert to owned String

    let handle = thread::spawn(move || {
        let mut result = DiskBenchResult {
            device: device.name.clone(),
            ..Default::default()
        };

        // Linear read benchmark
        match bench::disk::bench_linear_read(&device.path, samples, sample_size_mb) {
            Ok((data, avg, min, max)) => {
                result.linear_speed_mbs = data;
                result.avg_linear_mbs = avg;
                result.min_linear_mbs = min;
                result.max_linear_mbs = max;
            }
            Err(e) => {
                let _ = tx.send(bench::BenchResults {
                    status: format!("Linear read error: {}", e),
                    ..Default::default()
                });
                return;
            }
        }

        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // Random seek benchmark
        let seek_samples = if mode == "quick" { 200 } else { 1000 };
        match bench::disk::bench_random_seek(&device.path, seek_samples) {
            Ok((latencies, avg, max)) => {
                result.seek_times_ms = latencies;
                result.avg_seek_ms = avg;
                result.max_seek_ms = max;
            }
            Err(e) => {
                let _ = tx.send(bench::BenchResults {
                    status: format!("Seek test error: {}", e),
                    ..Default::default()
                });
                return;
            }
        }

        // Smart info (optional)
        result.smart_temp = bench::disk::read_smart_info(&device.path).and_then(|s| s.temperature);

        // Send final result
        let _ = tx.send(bench::BenchResults {
            disk_results: vec![result],
            status: "Disk test completed".into(),
            ..Default::default()
        });
    });

    app.worker = Some(handle);
}
