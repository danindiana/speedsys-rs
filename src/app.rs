use crate::bench::BenchResults;
use crate::sysinfo::SysInfo;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Screen {
    Overview,
    DiskSelect,
    DiskTest,
    MemTest,
    Report,
}

pub struct App {
    pub screen: Screen,
    pub sys_info: SysInfo,
    pub bench_results: BenchResults,
    pub selected_disk: usize,
    pub disks: Vec<String>,
    pub worker: Option<JoinHandle<()>>,
    pub cancel: Arc<AtomicBool>,
}

impl App {
    pub fn new(sys_info: SysInfo) -> Self {
        let disks = crate::bench::disk::scan_disks()
            .iter()
            .map(|d| d.name.clone())
            .collect();

        App {
            screen: Screen::Overview,
            sys_info,
            bench_results: BenchResults::default(),
            selected_disk: 0,
            disks,
            worker: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn reset_cancel(&mut self) {
        self.cancel = Arc::new(AtomicBool::new(false));
    }

    pub fn switch_screen(&mut self, screen: Screen) {
        self.screen = screen;
    }

    pub fn next_disk(&mut self) {
        if !self.disks.is_empty() {
            self.selected_disk = (self.selected_disk + 1) % self.disks.len();
        }
    }

    pub fn prev_disk(&mut self) {
        if !self.disks.is_empty() {
            if self.selected_disk == 0 {
                self.selected_disk = self.disks.len() - 1;
            } else {
                self.selected_disk -= 1;
            }
        }
    }
}
