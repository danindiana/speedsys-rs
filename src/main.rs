// speedsys-rs — a Rust/ratatui homage to SYSTEM SPEED TEST 4.78 (V. Afanasiev)
// Reimplements the spirit of the DOS original on Linux: system inventory,
// a CPU benchmark ladder, and the classic cache/memory throughput staircase.
//
// Keys:  r = rerun benchmarks   q / Esc = quit
// Flags: --dump  render one frame as plain text and exit (for CI / screenshots)

use std::{
    fs,
    hint::black_box,
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::TestBackend, prelude::*, widgets::*};

// ── system inventory (all from /proc and /sys, no external commands) ──

fn read(p: &str) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn cpuinfo_field(key: &str) -> Option<String> {
    let txt = fs::read_to_string("/proc/cpuinfo").ok()?;
    txt.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split(':').nth(1))
        .map(|v| v.trim().to_string())
}

fn meminfo_kb(key: &str) -> u64 {
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

struct SysInfo {
    cpu_model: String,
    cpu_id: String,
    cores: usize,
    mhz: String,
    mem_total_mb: u64,
    caches: Vec<String>,
    drives: Vec<String>,
    bios: String,
    board: String,
    os: String,
}

fn gather() -> SysInfo {
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

// ── benchmarks ────────────────────────────────────────────────────────

/// Integer ALU benchmark: LCG updates, returns Mops/s.
fn cpu_bench() -> f64 {
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

/// Sequential read over a buffer of `bytes`; returns MB/s.
/// Small buffers stay resident in cache -> the classic staircase.
fn mem_read_speed(bytes: usize) -> f64 {
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

#[derive(Clone, Default)]
struct BenchResults {
    cpu_mops: Option<f64>,
    sweep: Vec<(f64, f64)>, // (log2 KB, MB/s)
    status: String,
}

fn run_benchmarks(tx: mpsc::Sender<BenchResults>) {
    let mut r = BenchResults {
        status: "Testing processor...".into(),
        ..Default::default()
    };
    let _ = tx.send(r.clone());

    r.cpu_mops = Some(cpu_bench());
    r.status = "Testing memory throughput...".into();
    let _ = tx.send(r.clone());

    for kb in [4usize, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536] {
        let mbs = mem_read_speed(kb * 1024);
        r.sweep.push(((kb as f64).log2(), mbs));
        r.status = format!("Testing extended memory... {kb} KB");
        let _ = tx.send(r.clone());
    }
    r.status = "PASSED".into();
    let _ = tx.send(r);
}

// ── UI ────────────────────────────────────────────────────────────────

const REFERENCE_LADDER: &[(&str, f64)] = &[
    // (label, Mops/s of the LCG loop — rough vintage-flavoured anchors)
    ("i486DX2-66", 8.0),
    ("Pentium-133", 60.0),
    ("PentiumII-400", 350.0),
    ("Athlon-600", 550.0),
    ("Core2-2.4GHz", 1400.0),
    ("Modern x86-64", 3000.0),
];

fn ui(f: &mut Frame, sys: &SysInfo, res: &BenchResults) {
    let cyan = Style::default().fg(Color::Cyan);
    let block = |t: &'static str| {
        Block::default()
            .borders(Borders::ALL)
            .border_style(cyan)
            .title(Span::styled(t, Style::default().fg(Color::White).bold()))
    };

    let cols = Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(f.size());
    let left = Layout::vertical([
        Constraint::Min(14),
        Constraint::Length(REFERENCE_LADDER.len() as u16 + 4),
    ])
    .split(cols[0]);
    let right = Layout::vertical([Constraint::Min(10), Constraint::Length(3)]).split(cols[1]);

    // — left/top: inventory —
    let y = Style::default().fg(Color::Yellow);
    let w = Style::default().fg(Color::White);
    let mut lines = vec![
        Line::from(vec![Span::styled("Processor : ", w), Span::styled(sys.cpu_model.clone(), y)]),
        Line::from(vec![Span::styled("CPUID     : ", w), Span::styled(sys.cpu_id.clone(), y)]),
        Line::from(vec![
            Span::styled("Cores     : ", w),
            Span::styled(format!("{}  @ {} MHz", sys.cores, sys.mhz), y),
        ]),
        Line::from(vec![
            Span::styled("Memory    : ", w),
            Span::styled(format!("{} MB", sys.mem_total_mb), y),
        ]),
    ];
    for c in &sys.caches {
        lines.push(Line::from(Span::styled(format!("  {c}"), Style::default().fg(Color::Green))));
    }
    for d in &sys.drives {
        lines.push(Line::from(vec![Span::styled("Drive     : ", w), Span::styled(d.clone(), y)]));
    }
    lines.push(Line::from(vec![Span::styled("Board     : ", w), Span::styled(sys.board.clone(), y)]));
    lines.push(Line::from(vec![Span::styled("BIOS      : ", w), Span::styled(sys.bios.clone(), y)]));
    lines.push(Line::from(vec![Span::styled("OS        : ", w), Span::styled(sys.os.clone(), y)]));
    f.render_widget(
        Paragraph::new(lines).block(block(" System Speed Test  Ver 4.78-rs ")),
        left[0],
    );

    // — left/bottom: processor benchmark ladder —
    let score = res.cpu_mops.unwrap_or(0.0);
    let max = REFERENCE_LADDER.last().unwrap().1.max(score);
    let mut rows: Vec<Line> = REFERENCE_LADDER
        .iter()
        .map(|(name, v)| bar_line(name, *v, max, Color::Gray))
        .collect();
    rows.push(bar_line("THIS MACHINE", score, max, Color::LightRed));
    rows.push(Line::from(Span::styled(
        format!("  {score:.0} Mops/s   Processor benchmark"),
        Style::default().fg(Color::LightCyan).bold(),
    )));
    f.render_widget(Paragraph::new(rows).block(block(" Processor benchmark ")), left[1]);

    // — right/top: the staircase —
    let data: Vec<(f64, f64)> = res.sweep.clone();
    let ymax = data.iter().map(|p| p.1).fold(1.0, f64::max) * 1.1;
    let ds = Dataset::default()
        .name("Read MB/s")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Green))
        .data(&data);
    let chart = Chart::new(vec![ds])
        .block(block(" Cache / memory throughput  (block size KB) "))
        .x_axis(
            Axis::default()
                .bounds([2.0, 16.0])
                .labels(
                    ["4", "16", "64", "256", "1M", "4M", "16M", "64M"]
                        .iter()
                        .map(|s| Span::styled(*s, cyan))
                        .collect::<Vec<_>>(),
                )
                .style(cyan),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, ymax])
                .labels(vec![
                    Span::styled("0".to_string(), cyan),
                    Span::styled(format!("{:.0}", ymax / 2.0), cyan),
                    Span::styled(format!("{ymax:.0}"), cyan),
                ])
                .style(cyan),
        );
    f.render_widget(chart, right[0]);

    // — right/bottom: status line, SPEEDSYS-style —
    let passed = res.status == "PASSED";
    let msg = if passed {
        "Testing extended memory... \u{2593}\u{2593}\u{2593}\u{2593} PASSED".to_string()
    } else {
        res.status.clone()
    };
    let color = if passed { Color::LightGreen } else { Color::LightMagenta };
    f.render_widget(
        Paragraph::new(Span::styled(msg, Style::default().fg(color)))
            .block(block(" Status   [r] rerun  [q] quit ")),
        right[1],
    );
}

fn bar_line(name: &str, v: f64, max: f64, color: Color) -> Line<'static> {
    let width = 22usize;
    let n = ((v / max) * width as f64).round() as usize;
    Line::from(vec![
        Span::styled(format!("{name:<14}"), Style::default().fg(Color::White)),
        Span::styled("\u{2588}".repeat(n.max(1)), Style::default().fg(color)),
        Span::styled(format!(" {v:.0}"), Style::default().fg(Color::DarkGray)),
    ])
}

// ── main loop ─────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let sys = gather();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || run_benchmarks(tx));

    if std::env::args().any(|a| a == "--dump") {
        // headless single-frame render for verification
        let mut res = BenchResults::default();
        while let Ok(r) = rx.recv() {
            res = r;
            if res.status == "PASSED" {
                break;
            }
        }
        let backend = TestBackend::new(100, 34);
        let mut term = Terminal::new(backend)?;
        term.draw(|f| ui(f, &sys, &res))?;
        let buf = term.backend().buffer().clone();
        for yy in 0..buf.area.height {
            let mut line = String::new();
            for xx in 0..buf.area.width {
                line.push_str(buf.get(xx, yy).symbol());
            }
            println!("{}", line.trim_end());
        }
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut res = BenchResults::default();
    let mut rx = rx;
    loop {
        while let Ok(r) = rx.try_recv() {
            res = r;
        }
        term.draw(|f| ui(f, &sys, &res))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => {
                        let (tx2, rx2) = mpsc::channel();
                        rx = rx2;
                        res = BenchResults::default();
                        thread::spawn(move || run_benchmarks(tx2));
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
