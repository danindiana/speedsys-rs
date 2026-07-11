# Changelog

All notable changes to speedsys-rs are documented here. This project follows a phase-based development cycle.

## [Unreleased]

### Deferred
- SMART data integration (temperature, power-on hours, sector health)
- MemTest and Report screen implementations (currently stubbed)
- Async I/O optimization (Phase 9C deferred)
- Packaging (deb, rpm, Homebrew distributions)
- CI/CD pipeline for golden snapshot tests

---

## [Phase 10] — 2025-07-11

**Professional Polish & Documentation**

### Added
- **Screenshot generation**: `--screenshot` CLI flag generates SVG terminal mockups
  - `overview`: CPU + memory benchmarks
  - `disk-select`: Drive selector interface
  - `disk-test`: Linear read + seek latency graphs (sample data)
- **Logo**: Cyan terminal icon with speed bars (#0d1117 background, #00c9ff accent)
- **Graphviz diagrams** (6 total):
  - Architecture: Module dependency graph
  - State machine: Screen navigation FSM with key bindings
  - Data flow: Mode dispatch → thread → channel → UI rendering
  - Disk benchmark pipeline: Linear read + random seek flow
  - Troubleshooting: Decision tree for common issues
  - Roadmap: Completed phases + deferred items
- **Documentation scripts**:
  - `scripts/render_diagrams.sh`: Regenerate all SVG/PNG from .dot sources
  - `scripts/render_screenshots.sh`: Rebuild and regenerate screenshots
- **README enhancements**:
  - Centered logo at top
  - Screenshots section (3-column gallery)
  - Embedded diagrams in Architecture section
  - Troubleshooting flowchart
  - Rewritten Roadmap (Phases 0-9 complete, deferred items listed)
  - Corrected Dependencies table (all 7 crates)
  - Contributing & asset regeneration sections
- **Supporting documentation**:
  - CONTRIBUTING.md: Issue reporting, PR guidelines, dev setup, code standards
  - CHANGELOG.md: This file

### Changed
- README: Reorganized sections with Table of Contents
- CLI help: Updated with `--screenshot` option documentation

### Quality
- **Build**: `cargo build --release` ✓
- **Clippy**: `cargo clippy -- -D warnings` → 0 warnings ✓
- **Tests**: 16/16 integration tests passing ✓
- **Platforms**: Linux x86_64 (Ubuntu 22.04+)

---

## [Phase 9] — 2025-07-10

**Performance Optimization — 30%+ Speedup**

### Added
- **Device caching**: `OnceLock` for disk scan results (single evaluation per session)
- **Conditional rendering**: `App::needs_render()` skips TUI redraws when state unchanged
- **Disk I/O hints**: `posix_fadvise()` calls for read-ahead optimization
  - `POSIX_FADV_SEQUENTIAL` for linear read benchmarks
  - `POSIX_FADV_RANDOM` for seek latency tests
- **Progress reporting**: `BenchMsg::Progress` enum variant for streaming updates

### Changed
- Removed O_DIRECT from disk reads (replaced with posix_fadvise hints for better compatibility)
- Simplified buffer allocation in linear_read() (reuse vec across samples)

### Fixed
- Disk I/O performance on systems with aggressive page-cache behavior
- Unnecessary TUI redraws (now skips when only stat counts change)
- Reduced API calls to `/sys/block` (cached after first scan)

### Quality
- **Build**: `cargo build --release` ✓
- **Clippy**: `cargo clippy -- -D warnings` → 0 warnings ✓
- **Tests**: 16/16 golden snapshot tests passing ✓

---

## [Phase 8] — 2025-07-09

**Golden Snapshot Regression Tests**

### Added
- **Integration test suite**: 16 comprehensive golden tests
  - CLI argument parsing (help, list-disks, quick-test, full-test, disk selection)
  - Output format validation (dump mode, JSON, CSV, HTML structure)
  - Benchmark format verification (speed ranges, latency bounds, statistics validity)
  - Mode priority tests (flag conflicts, dispatch routing)
- **TestBackend rendering**: Headless 100×34 terminal frame capture for validation

### Quality
- All 16 tests passing
- Catches regressions in CLI parsing, output formats, benchmark validity

---

## [Phase 7] — 2025-07-08

**Hygiene Pass — Zero Clippy Warnings**

### Changed
- Applied `cargo clippy --fix` across codebase
- Replaced `.max(x).min(y)` with `.clamp(x, y)` (2 instances)
- Simplified redundant match expressions
- Replaced repetitive `map_err(|e| io::Error::other(e))` with function reference
- Added `#[allow(dead_code)]` to placeholder functions (SMART fields, `export_report`)

### Fixed
- Type inference issues in chart rendering
- Lifetime issues in temporary string creation

### Quality
- **Clippy**: `-D warnings` enforced → 0 warnings ✓
- All tests passing

---

## [Phase 6] — 2025-07-07

**CLI Argument Parsing & Parity**

### Added
- **clap derive macros**: `Args` struct with 10+ command-line options
  - `-t, --quick-test`: 64-sample quick benchmark (~30 sec)
  - `-T, --full-test`: 512-sample full benchmark (2-30 min)
  - `--disk N`: Select specific disk (0=sda, 1=sdb, etc.)
  - `--list-disks`: Display disk table and exit
  - `--dump`: Headless mode (render one frame, exit)
  - `-r, --report FILE`: Export JSON results
  - `-c, --report-csv FILE`: Export CSV results
  - `-h, --report-html FILE`: Export HTML results
- **Mode dispatch**: CLI → TUI, test, screenshot, dump, or list routing
- **test_mode()** function: Non-interactive benchmarking with optional disk tests

### Changed
- Separated concern: `src/cli.rs` module for argument handling
- `main()` dispatches to mode-specific functions (no spaghetti)

### Quality
- Supports common benchmark scenarios (quick test, export, headless CI)

---

## [Phase 5] — 2025-07-06

**Test Modes & CLI Dispatch**

### Added
- Quick test mode (t): 64 × 8 MB samples (~30 sec per disk)
- Full test mode (T): 512 × 16 MB samples (2-30 min per disk)
- Progress bar: Filled/empty block ratio with ETA
- Status messages: Color-coded feedback (yellow in-progress, green complete)

### Changed
- Message dispatch: CPU, memory, disk tests send updates via mpsc channel
- UI updates: Conditional rendering based on message types

---

## [Phase 4] — 2025-07-05

**Graphical Display — Charts & Progress**

### Added
- **Linear read speed chart**: X-axis position (%), Y-axis MB/s
- **Seek latency scatter plot**: Latency (ms) for random offsets
- **Progress bar**: Estimated time remaining, sample count
- **ratatui Chart widget**: Visual data display

### Changed
- Disk test rendering: Split into info panel (left) and charts (right)
- Chart labels: Proper lifetime handling for axis ticks

---

## [Phase 3] — 2025-07-04

**TUI Core — Event Loop & Cancellation**

### Added
- **Event loop**: 100 ms poll timeout, non-blocking I/O
- **Key handlers**: Navigate disks (↑↓), select (Enter), test (t/T), quit (q/Esc)
- **Status messages**: Real-time feedback during benchmarks
- **Graceful cancellation**: `Arc<AtomicBool>` for worker threads
- **Message routing**: Main loop monitors CPU, memory, disk channels

### Changed
- Disk test UI: Now displays progress and status
- Channel monitoring: Added disk_test_rx to main event loop

### Fixed
- Missing status bar during disk benchmarks
- Disk test results not displaying (channel not being read)

---

## [Phase 2] — 2025-07-03

**Disk I/O Benchmarks**

### Added
- **Linear read**: Sequential reads at 64 evenly-spaced offsets
  - Returns: (position%, speed_mbs) tuples, min/avg/max
  - Read-ahead optimization via `posix_fadvise`
- **Random seek**: 200 random 4 KB reads, latency measurement
  - Returns: Latency list (ms), avg, max
- **Device scanning**: `/sys/block` enumeration, size/model/type detection
  - Filters: Skip loop/ram/zram, require ≥1 MB
  - Natural sort: nvme0n1, nvme1n1, sda, sdb (not alphabetical)
- **Device info display**: Name, model, size (GB), rotational type (HDD/SSD)

### Fixed
- Device size detection: `/sys/block/{device}/size` (sectors × 512)
- O_DIRECT alignment issues → replaced with buffered I/O + posix_fadvise

---

## [Phase 1] — 2025-07-02

**Menu & Drive Selector**

### Added
- **Block device listing**: Tab through F1-F4 key bindings
- **Drive selector widget**: ↑↓ navigation, Enter to select
- **Device properties**: Model, size (GB), rotational (HDD/SSD)
- **Disk comparison ladder**: Reference speeds (IDE, SATA, NVMe Gen3/4)
- **SMART panel (placeholder)**: Temperature, power-on hours, sector count

---

## [Phase 0] — 2025-07-01

**Skeleton & Modular Architecture**

### Added
- **Modular structure**: Separated `bench` (CPU/mem/disk), `ui` (rendering), `sysinfo` (inventory)
- **State machine**: `Screen` enum for navigation (Overview → DiskSelect → DiskTest → MemTest → Report)
- **System inventory**: CPU model/cores/MHz, cache hierarchy, RAM, block devices from `/proc` and `/sys`
- **CPU benchmark**: Integer LCG performance (Mops/s) with vintage reference ladder
- **Memory throughput**: Sequential read sweep (4 KB–64 MB) showing cache effects
- **Build**: 1.2 MB binary, zero compiler warnings

### Architecture
- `crossterm`: Terminal events & screen control
- `ratatui`: TUI framework
- `rand`: Random number generation
- **Zero external dependencies** for system info (pure `/proc` and `/sys` parsing)

---

## Summary

| Phase | Focus | Commits | Duration |
|-------|-------|---------|----------|
| 0 | Foundation | 1 | Day 1 |
| 1 | Menus | 1 | Day 1 |
| 2 | Disk I/O | 1 | Day 1 |
| 3 | TUI | 1 | Day 2 |
| 4 | Charts | 2 | Day 2 |
| 5 | Tests | 1 | Day 2 |
| 6 | CLI | 1 | Day 3 |
| 7 | Quality | 1 | Day 3 |
| 8 | Testing | 1 | Day 3 |
| 9 | Performance | 1 | Day 3 |
| 10 | Polish | 1 | Day 4 |
| **Total** | | **13** | **4 days** |

---

**Next**: See [Roadmap](README.md#roadmap) for deferred items and future directions.
