use ratatui::prelude::*;
use ratatui::symbols;
use ratatui::widgets::*;
use crate::bench::BenchResults;
use crate::sysinfo::SysInfo;
use super::common::*;

const REFERENCE_LADDER: &[(&str, f64)] = &[
    ("i486DX2-66", 8.0),
    ("Pentium-133", 60.0),
    ("PentiumII-400", 350.0),
    ("Athlon-600", 550.0),
    ("Core2-2.4GHz", 1400.0),
    ("Modern x86-64", 3000.0),
];

pub fn render(f: &mut Frame, sys: &SysInfo, res: &BenchResults) {
    let cyan = Style::default().fg(Color::Cyan);

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
        Paragraph::new(lines).block(cyan_block(" System Speed Test  Ver 4.78-rs ")),
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

    // Add thermal info if available
    if let (Some(temp_before), Some(temp_after)) = (res.cpu_temp_before_c, res.cpu_temp_after_c) {
        let (freq_before, freq_after, max_freq) = (
            res.cpu_freq_before_mhz.unwrap_or(0),
            res.cpu_freq_after_mhz.unwrap_or(0),
            res.cpu_max_freq_mhz.unwrap_or(0),
        );
        let thermal_line = if res.throttle_detected {
            Span::styled(
                format!("  Temp: {:.0}°C → {:.0}°C   Freq: {} → {}MHz / {}MHz   [THROTTLED]",
                    temp_before, temp_after, freq_before, freq_after, max_freq),
                Style::default().fg(Color::Red).bold(),
            )
        } else {
            Span::styled(
                format!("  Temp: {:.0}°C → {:.0}°C   Freq: {} → {}MHz / {}MHz",
                    temp_before, temp_after, freq_before, freq_after, max_freq),
                Style::default().fg(Color::Green),
            )
        };
        rows.push(Line::from(thermal_line));
    }

    f.render_widget(Paragraph::new(rows).block(cyan_block(" Processor benchmark ")), left[1]);

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
        .block(cyan_block(" Cache / memory throughput  (block size KB) "))
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
        "Testing extended memory... ▓▓▓▓ PASSED".to_string()
    } else {
        res.status.clone()
    };
    let color = if passed { Color::LightGreen } else { Color::LightMagenta };
    f.render_widget(
        Paragraph::new(Span::styled(msg, Style::default().fg(color)))
            .block(cyan_block(" Status   [F1] Overview  [F2] Disks  [q] quit ")),
        right[1],
    );
}
