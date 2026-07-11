use ratatui::prelude::*;
use ratatui::widgets::*;
use crate::app::App;
use super::common::*;

pub fn render_selector(f: &mut Frame, app: &App) {
    let block = cyan_block(" Disk/Device Selector [↑/↓] [Enter] Select [t] Quick Test [T] Full Test [q] Back ");

    let all_devices = crate::bench::disk::scan_disks();
    let mut items = Vec::new();
    for (idx, dev) in all_devices.iter().enumerate() {
        let size_gb = dev.size_bytes as f64 / 1e9;
        let kind = if dev.is_rotational { "HDD" } else { "SSD" };
        let label = format!(" {}  {}  {:.1} GB  ({})", dev.name, dev.model, size_gb, kind);
        items.push((idx, label));
    }

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected_disk % items.len().max(1)));

    let list = List::new(items.iter().map(|(_, label)| label.clone()))
        .block(block)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightCyan),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, f.size(), &mut list_state);
}

pub fn render_test(f: &mut Frame, app: &App) {
    let y = Style::default().fg(Color::Yellow);
    let w = Style::default().fg(Color::White);
    let g = Style::default().fg(Color::Green);

    let device_name = app.disks.get(app.selected_disk).cloned().unwrap_or_default();
    let all_devices = crate::bench::disk::scan_disks();
    let device = all_devices.iter().find(|d| d.name == device_name);

    let mut lines = vec![];
    if let Some(dev) = device {
        let size_gb = dev.size_bytes as f64 / 1e9;
        lines.push(Line::from(vec![
            Span::styled("Device    : ", w),
            Span::styled(dev.name.clone(), y),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Model     : ", w),
            Span::styled(dev.model.clone(), y),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Size      : ", w),
            Span::styled(format!("{:.1} GB", size_gb), y),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Type      : ", w),
            Span::styled(if dev.is_rotational { "Rotational (HDD)" } else { "Non-rotational (SSD)" }, y),
        ]));
    }

    lines.push(Line::from(""));

    // Show disk test results if available
    if let Some(result) = app.disk_results.get(&device_name) {
        lines.push(Line::from(Span::styled(" ═══ Linear Read Performance ═══", g)));
        lines.push(Line::from(vec![
            Span::styled("Avg Speed     : ", w),
            Span::styled(format!("{:.1} MB/s", result.avg_linear_mbs), y),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Min Speed     : ", w),
            Span::styled(format!("{:.1} MB/s", result.min_linear_mbs), y),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Max Speed     : ", w),
            Span::styled(format!("{:.1} MB/s", result.max_linear_mbs), y),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(" ═══ Random Access Performance ═══", g)));
        lines.push(Line::from(vec![
            Span::styled("Avg Seek      : ", w),
            Span::styled(format!("{:.2} ms", result.avg_seek_ms), y),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Max Seek      : ", w),
            Span::styled(format!("{:.2} ms", result.max_seek_ms), y),
        ]));

        // Draw a mini scatter plot of seek times
        if !result.seek_times_ms.is_empty() {
            lines.push(Line::from(""));
            let min_seek = result.seek_times_ms.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_seek = result.seek_times_ms.iter().cloned().fold(0.0, f64::max);
            let range = (max_seek - min_seek).max(0.1);

            let width = 50;
            let mut plot = vec![' '; width];
            for &lat_ms in &result.seek_times_ms {
                let normalized = (lat_ms - min_seek) / range;
                let x = ((normalized * (width - 1) as f64).round() as usize).min(width - 1);
                plot[x] = '·';
            }
            lines.push(Line::from(Span::styled(
                format!("  {}", plot.iter().collect::<String>()),
                Style::default().fg(Color::Cyan),
            )));
        }

        if let Some(temp) = result.smart_temp {
            lines.push(Line::from(vec![
                Span::styled("Temperature   : ", w),
                Span::styled(format!("{:.0}°C", temp), y),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  [Press t for quick test, T for full test]",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Add status bar at the bottom
    lines.push(Line::from(""));
    let status_color = if app.bench_results.status.contains("error") || app.bench_results.status.contains("Error") {
        Color::Red
    } else if app.bench_results.status == "PASSED" {
        Color::Green
    } else {
        Color::Yellow
    };
    lines.push(Line::from(Span::styled(
        format!("Status: {}", app.bench_results.status),
        Style::default().fg(status_color),
    )));

    let para = Paragraph::new(lines)
        .block(cyan_block(" Disk Test Results [t] Quick [T] Full [q] Back "))
        .scroll((0, 0));

    f.render_widget(para, f.size());
}
