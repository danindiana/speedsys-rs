# speedsys-rs — Rust/ratatui Reimplementation of SYSTEM SPEED TEST 4.78

A modern Rust port of the classic DOS benchmark **SYSTEM SPEED TEST 4.78** by Vladimir Afanasiev, featuring a TUI interface, disk benchmarking, and modular architecture.

[![GitHub](https://img.shields.io/badge/GitHub-danindiana%2Fspeedsys--rs-blue)](https://github.com/danindiana/speedsys-rs)
[![Rust](https://img.shields.io/badge/Rust-1.74%2B-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

---

## Features

### Phase 0: Modular Architecture ✅
- **Separated concerns**: System inventory, CPU benchmarks, memory benchmarks, disk benchmarks, UI rendering
- **State machine**: Screen navigation (Overview → Disk Selector → Test Results → Report)
- **Clean event loop**: Non-blocking I/O, graceful cancellation with `Arc<AtomicBool>`
- **Build**: `cargo build --release` produces 1.2 MB binary with zero compiler errors

### Phase 1: Menu & Drive Selector ✅
- **Tab navigation**: F1–F4 keys switch between screens
- **Drive selector widget**: List all block devices (skip loop/ram/zram, ≥1 MB)
- **Device info**: Name, model, size (GB), rotational type (HDD/SSD)
- **Arrow key navigation**: Select disk with ↑/↓, confirm with Enter
- **Test modes**: Quick (T1: 64 × 8MB) and Full (T2: 512 × 16MB)

### Phase 2: Disk Benchmarks ✅
- **Linear read speed graph**: Samples K evenly-spaced positions across device
  - Quick: 64 samples, ~30 seconds
  - Full: 512 samples, 2–30 minutes (depending on drive speed)
  - Plots MB/s vs position (0–100%)
  - Reports min/avg/max speeds
  
- **Random seek/access time scatter**: K random 4 KB reads at random offsets
  - Quick: 200 seeks
  - Full: 1000 seeks
  - Reports avg/max latency (ms)
  - Visual scatter plot of all seeks
  
- **Drive comparison ladder**: Benchmark your drive against reference speeds
  - IDE 1998 (~10 MB/s)
  - SATA HDD (~180 MB/s)
  - SATA SSD (~550 MB/s)
  - NVMe Gen3 (~3500 MB/s)
  - NVMe Gen4 (~7000 MB/s)
  
- **SMART health panel**: Device temperature, power-on hours, sector health

### Core Features
- **System inventory**: CPU model, cores, MHz, L1/L2/L3 caches, RAM, block devices, motherboard, BIOS, OS (all from `/proc` and `/sys`, no external commands)
- **CPU benchmark ladder**: Integer LCG performance (Mops/s) vs vintage reference speeds
- **Memory throughput staircase**: Sequential read speed (4 KB–64 MB) showing cache hierarchy effects
- **Headless mode**: `--dump` renders one frame as ASCII/ANSI for CI/screenshots
- **Read-only**: All disk I/O uses `O_DIRECT + O_RDONLY` — no write benchmarks on raw devices
- **Graceful errors**: Permission denied → shows hint; O_DIRECT unavailable → falls back to `posix_fadvise`

---

## Quick Start

### Installation
```bash
# Clone and build
git clone https://github.com/danindiana/speedsys-rs
cd speedsys-rs
cargo build --release

# Run (requires sudo for raw device access)
sudo ./target/release/speedsys-rs
```

### Requirements
- **Linux** (Ubuntu 22.04+, any distro with `/sys/block` and `/proc`)
- **Rust** 1.74+ (install via [rustup](https://rustup.rs/))
- **Sudo** or membership in `disk` group for raw device reads

---

## Usage

### Interactive TUI Mode
```bash
sudo ./target/release/speedsys-rs
```

**Key Bindings:**

| Key | Action |
|-----|--------|
| **F1–F4** or **1–4** | Switch screens (Overview, Disks, Memory, Report) |
| **Tab** / **Shift-Tab** | Next/Previous screen |
| **↑** / **↓** | Navigate disk list |
| **Enter** | Select disk / open menu |
| **t** | Quick test (64 × 8 MB, ~30 sec) |
| **T** | Full test (512 × 16 MB, 2–30 min) |
| **r** | Rerun CPU/memory benchmarks |
| **q** / **Esc** | Quit |

### Headless Mode
```bash
sudo ./target/release/speedsys-rs --dump
```
Renders one frame as ASCII/ANSI and exits (useful for CI, screenshots, automation).

---

## Disk Benchmarking

### Quick Test (t) vs Full Test (T)

| Aspect | Quick (t) | Full (T) |
|--------|-----------|----------|
| **Samples** | 64 × 8 MB | 512 × 16 MB |
| **Total I/O** | ~500 MB | ~8 GB |
| **NVMe** | 30 sec | 2–3 min |
| **SATA SSD** | 1–2 min | 4–8 min |
| **HDD** | 2–3 min | 15–30+ min |

### What Gets Measured

**Linear Read Speed:**
- Sequential read speed at different disk positions
- HDD: Shows decline (outer → inner tracks), 50–150 MB/s
- SSD/NVMe: Flat line (uniform speed), 400+ MB/s

**Random Access Time:**
- Latency for 4 KB reads after random seeks
- HDD: 5–20 ms (mechanical movement)
- NVMe: <0.5 ms (electronic access)

**Drive Comparison Ladder:**
- Your drive's avg speed vs reference benchmarks
- Visual bar chart for context

---

## Architecture

### Modules
```
src/
├── main.rs              # Event loop, screen routing, test launcher
├── sysinfo.rs           # System inventory from /proc and /sys
├── app.rs               # State machine, navigation, disk list
├── bench/
│   ├── cpu.rs           # LCG integer benchmark
│   ├── mem.rs           # Memory throughput sweep
│   ├── disk.rs          # Linear read, random seek, device scanning
│   └── mod.rs           # Shared types
└── ui/
    ├── overview.rs      # System info + CPU/memory display
    ├── disks.rs         # Drive selector + test results
    ├── common.rs        # Shared widgets
    └── mod.rs           # Screen router
```

### Design Patterns
- **State Machine**: `Screen` enum for navigation
- **Event Loop**: Non-blocking I/O with 100 ms poll timeout
- **Channels**: mpsc for background thread → UI communication
- **Graceful Shutdown**: `Arc<AtomicBool>` for worker cancellation
- **No External Deps**: System info from `/proc` and `/sys` only

---

## Troubleshooting

### Permission Denied Errors
```bash
# Option 1: Run with sudo
sudo ./target/release/speedsys-rs

# Option 2: Add user to disk group (permanent)
sudo usermod -aG disk $USER
newgrp disk  # Apply immediately
```

### Full Test Hangs
**It's likely still running** (especially on mechanical HDDs). Full test reads 8 GB across the drive surface:
- NVMe: 2–3 minutes
- HDD: 15–30+ minutes

Watch stderr for `[WORKER] Linear read progress: X/512` messages.

### No Disks Detected
Check that block devices exist:
```bash
sudo ls -l /sys/block/
sudo ls -l /dev/nvme* /dev/sd*
```

---

## Performance Notes

### Why Disk Tests are Slow
- **Linear read**: Samples entire drive surface (0–100% positions)
- **Random seek**: Seeks across full address space, no caching advantage
- **Mechanical HDD**: Each seek + rotation takes 5–20 ms; 512 seeks = 2.5–10+ seconds just for I/O

### Best Practices
1. Start with **quick test** (t) for immediate feedback
2. Use **full test** (T) for detailed graphs and documentation
3. Test **NVMe first** to see fast execution (2–3 min)
4. Run **HDD tests overnight** if patience is limited

---

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `crossterm` | 0.27 | Terminal events & control |
| `ratatui` | 0.26 | TUI framework |
| `rand` | 0.8 | Random offset generation |

---

## Roadmap

**Phase 3** (planned): Memory improvements, multi-core CPU variant, memory error test
**Phase 4** (planned): Report export (text, ANSI, HTML)
**Phase 5** (stretch): CLI argument parity with original SPEEDSYS

---

## License

MIT

---

## Credits

- **Original**: SYSTEM SPEED TEST 4.78 by Vladimir Afanasiev
- **Rewrite**: Rust + ratatui implementation
- **Inspiration**: Retro DOS benchmark aesthetic
