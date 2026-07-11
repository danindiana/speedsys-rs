use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

pub fn cyan_block(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(title, Style::default().fg(Color::White).bold()))
}

pub fn bar_line(name: &str, v: f64, max: f64, color: Color) -> Line<'static> {
    let width = 22usize;
    let n = ((v / max) * width as f64).round() as usize;
    Line::from(vec![
        Span::styled(format!("{name:<14}"), Style::default().fg(Color::White)),
        Span::styled("█".repeat(n.max(1)), Style::default().fg(color)),
        Span::styled(format!(" {v:.0}"), Style::default().fg(Color::DarkGray)),
    ])
}

#[allow(dead_code)]
pub fn label_value(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<14}"), Style::default().fg(Color::White)),
        Span::styled(value.to_string(), Style::default().fg(Color::Yellow)),
    ])
}
