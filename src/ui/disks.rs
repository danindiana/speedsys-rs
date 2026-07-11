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
    let cyan_style = Style::default().fg(Color::Cyan);

    let device_name = app.disks.get(app.selected_disk).cloned().unwrap_or_default();
    let all_devices = crate::bench::disk::scan_disks();
    let device = all_devices.iter().find(|d| d.name == device_name);

    // Full layout
    let main_area = f.size();

    // Render left panel (device info + stats)
    let mut left_lines = vec![];

    if let Some(dev) = device {
        let size_gb = dev.size_bytes as f64 / 1e9;
        left_lines.push(Line::from(vec![
            Span::styled("Device    : ", w),
            Span::styled(dev.name.clone(), y),
        ]));
        left_lines.push(Line::from(vec![
            Span::styled("Model     : ", w),
            Span::styled(dev.model.clone(), y),
        ]));
        left_lines.push(Line::from(vec![
            Span::styled("Size      : ", w),
            Span::styled(format!("{:.1} GB", size_gb), y),
        ]));
        left_lines.push(Line::from(vec![
            Span::styled("Type      : ", w),
            Span::styled(if dev.is_rotational { "Rotational (HDD)" } else { "Non-rotational (SSD)" }, y),
        ]));
    }

    left_lines.push(Line::from(""));

    // Show disk test results if available
    if let Some(result) = app.disk_results.get(&device_name) {
        left_lines.push(Line::from(Span::styled(" ═══ Linear Read ═══", g)));
        left_lines.push(Line::from(vec![
            Span::styled("Avg : ", w),
            Span::styled(format!("{:.1} MB/s", result.avg_linear_mbs), y),
        ]));
        left_lines.push(Line::from(vec![
            Span::styled("Min : ", w),
            Span::styled(format!("{:.1} MB/s", result.min_linear_mbs), y),
        ]));
        left_lines.push(Line::from(vec![
            Span::styled("Max : ", w),
            Span::styled(format!("{:.1} MB/s", result.max_linear_mbs), y),
        ]));

        left_lines.push(Line::from(""));
        left_lines.push(Line::from(Span::styled(" ═══ Random Seek ═══", g)));
        left_lines.push(Line::from(vec![
            Span::styled("Avg : ", w),
            Span::styled(format!("{:.2} ms", result.avg_seek_ms), y),
        ]));
        left_lines.push(Line::from(vec![
            Span::styled("Max : ", w),
            Span::styled(format!("{:.2} ms", result.max_seek_ms), y),
        ]));

        if let Some(temp) = result.smart_temp {
            left_lines.push(Line::from(""));
            left_lines.push(Line::from(vec![
                Span::styled("Temperature : ", w),
                Span::styled(format!("{:.0}°C", temp), y),
            ]));
        }
    } else {
        left_lines.push(Line::from(Span::styled(
            "  [Press t for quick test]",
            Style::default().fg(Color::DarkGray),
        )));
        left_lines.push(Line::from(Span::styled(
            "  [Press T for full test]",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Add progress bar if test is running
    if let Some((curr, total, elapsed)) = app.current_progress {
        left_lines.push(Line::from(""));

        let pct = (curr as f64 / total as f64) * 100.0;
        let bar_width: usize = 30;
        let filled = ((curr as f64 / total as f64) * bar_width as f64) as usize;
        let empty = bar_width.saturating_sub(filled);

        let bar = format!("{}{}",
            "█".repeat(filled),
            "░".repeat(empty)
        );

        // Estimate time remaining
        let est_total_time = if curr > 0 {
            elapsed / (curr as f64 / total as f64)
        } else {
            0.0
        };
        let remaining_time = (est_total_time - elapsed).max(0.0);

        left_lines.push(Line::from(Span::styled(
            format!("{} {:.0}%", bar, pct),
            cyan_style,
        )));
        left_lines.push(Line::from(Span::styled(
            format!("~{:.0}s left", remaining_time),
            cyan_style,
        )));
    }

    // Add status bar at the bottom of left panel
    left_lines.push(Line::from(""));
    let status_color = if app.bench_results.status.contains("error") || app.bench_results.status.contains("Error") {
        Color::Red
    } else if app.bench_results.status == "PASSED" || app.bench_results.status == "✓ Test complete" {
        Color::Green
    } else {
        Color::Yellow
    };
    left_lines.push(Line::from(Span::styled(
        format!("Status: {}", app.bench_results.status),
        Style::default().fg(status_color),
    )));

    // Split layout: left (info), right (charts)
    let h_chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(35),
            ratatui::layout::Constraint::Percentage(65),
        ])
        .split(main_area);

    // Render left panel
    let left_para = Paragraph::new(left_lines)
        .block(cyan_block(" Device Info "))
        .scroll((0, 0));
    f.render_widget(left_para, h_chunks[0]);

    // Split right panel into two charts vertically
    let v_chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Percentage(50),
            ratatui::layout::Constraint::Percentage(50),
        ])
        .split(h_chunks[1]);

    // Render right-top panel: linear read chart
    if let Some(result) = app.disk_results.get(&device_name) {
        if !result.linear_speed_mbs.is_empty() {
            let max_speed = result.linear_speed_mbs.iter()
                .map(|(_, speed)| speed)
                .cloned()
                .fold(0.0, f64::max)
                .max(1.0);

            let dataset = Dataset::default()
                .name("Read Speed (MB/s)")
                .marker(Marker::Dot)
                .style(Style::default().fg(Color::Cyan))
                .data(&result.linear_speed_mbs);

            // Create y-axis labels with proper lifetime
            let y_labels_str = vec![
                format!("{:.0}", 0.0),
                format!("{:.0}", max_speed * 0.5),
                format!("{:.0}", max_speed),
            ];
            let y_labels = y_labels_str.iter()
                .map(|s| Span::raw(s.clone()))
                .collect::<Vec<_>>();

            let chart = Chart::new(vec![dataset])
                .block(Block::default()
                    .title("Linear Read Speed")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)))
                .x_axis(Axis::default()
                    .title("Position %")
                    .bounds([0.0, 100.0])
                    .labels(vec![
                        Span::raw("0%"),
                        Span::raw("50%"),
                        Span::raw("100%"),
                    ]))
                .y_axis(Axis::default()
                    .title("MB/s")
                    .bounds([0.0, max_speed * 1.1])
                    .labels(y_labels));

            f.render_widget(chart, v_chunks[0]);
        } else {
            // Show placeholder if no data yet
            let placeholder = Paragraph::new("Waiting for data...")
                .block(Block::default()
                    .title("Linear Read Speed")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)))
                .alignment(Alignment::Center);
            f.render_widget(placeholder, v_chunks[0]);
        }
    } else {
        // Show hint if no test started
        let hint = Paragraph::new("Select test (t/T)")
            .block(Block::default()
                .title("Linear Read Speed")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center);
        f.render_widget(hint, v_chunks[0]);
    }

    // Render right-bottom panel: seek latency chart
    if let Some(result) = app.disk_results.get(&device_name) {
        if !result.seek_times_ms.is_empty() {
            let max_seek = result.seek_times_ms.iter()
                .cloned()
                .fold(0.0, f64::max)
                .max(0.1);

            // Convert seek times to (index, latency) tuples for scatter plot
            let seek_data: Vec<(f64, f64)> = result.seek_times_ms.iter()
                .enumerate()
                .map(|(i, &lat)| (i as f64, lat))
                .collect();

            let dataset = Dataset::default()
                .name("Seek Latency (ms)")
                .marker(Marker::Dot)
                .style(Style::default().fg(Color::Yellow))
                .data(&seek_data);

            // Create y-axis labels
            let y_labels_str = vec![
                format!("{:.2}", 0.0),
                format!("{:.2}", max_seek * 0.5),
                format!("{:.2}", max_seek),
            ];
            let y_labels = y_labels_str.iter()
                .map(|s| Span::raw(s.clone()))
                .collect::<Vec<_>>();

            let chart = Chart::new(vec![dataset])
                .block(Block::default()
                    .title("Random Seek Latency")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)))
                .x_axis(Axis::default()
                    .title("Seek #")
                    .bounds([0.0, result.seek_times_ms.len() as f64]))
                .y_axis(Axis::default()
                    .title("Latency (ms)")
                    .bounds([0.0, max_seek * 1.1])
                    .labels(y_labels));

            f.render_widget(chart, v_chunks[1]);
        } else {
            // Show placeholder if no seek data yet
            let placeholder = Paragraph::new("Waiting for seeks...")
                .block(Block::default()
                    .title("Random Seek Latency")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)))
                .alignment(Alignment::Center);
            f.render_widget(placeholder, v_chunks[1]);
        }
    } else {
        // Show hint
        let hint = Paragraph::new("Run test for results")
            .block(Block::default()
                .title("Random Seek Latency")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center);
        f.render_widget(hint, v_chunks[1]);
    }
}
