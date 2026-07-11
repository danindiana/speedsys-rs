pub mod overview;
pub mod disks;
pub mod common;

use ratatui::prelude::*;
use ratatui::widgets::Tabs;
use crate::app::App;


pub fn render_screen(f: &mut Frame, app: &App) {
    match app.screen {
        crate::app::Screen::Overview => overview::render(f, &app.sys_info, &app.bench_results),
        crate::app::Screen::DiskSelect => disks::render_selector(f, app),
        crate::app::Screen::DiskTest => disks::render_test(f, app),
        _ => overview::render(f, &app.sys_info, &app.bench_results),
    }
}

#[allow(dead_code)]
pub fn render_tabs(app: &App) -> Tabs<'static> {
    let titles = vec!["Overview", "Disks", "Memory", "Report"];
    let selected = match app.screen {
        crate::app::Screen::Overview => 0,
        crate::app::Screen::DiskSelect | crate::app::Screen::DiskTest => 1,
        crate::app::Screen::MemTest => 2,
        crate::app::Screen::Report => 3,
    };
    Tabs::new(titles).select(selected)
}
