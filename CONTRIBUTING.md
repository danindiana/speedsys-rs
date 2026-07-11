# Contributing to speedsys-rs

Thanks for your interest in contributing! This document outlines how to report issues, submit pull requests, and set up a development environment.

## Reporting Issues

### Bug Reports

Found a bug? Please create a GitHub issue with:
1. **Title**: Brief, descriptive summary (e.g., "Disk test hangs on NVMe with -T flag")
2. **Environment**: Your OS, Linux distro, kernel version, and hardware (e.g., "Ubuntu 22.04, RTX 5080, NVMe")
3. **Steps to Reproduce**: Exact commands and sequence to reproduce the issue
4. **Expected Behavior**: What should happen
5. **Actual Behavior**: What actually happened
6. **Error Output**: Full error message or stderr output

### Feature Requests

Have an idea for a feature? Open an issue with:
- **Title**: What you want to add (e.g., "Add SMART data dashboard")
- **Description**: Why this feature would be valuable
- **Implementation Ideas**: Any thoughts on how to build it (optional)

### Questions

Unsure how to use the tool? Open a GitHub Discussion or check:
- [README.md](README.md) — Quick start, usage, troubleshooting
- [CHANGELOG.md](CHANGELOG.md) — Version history and what changed

## Development Setup

### Prerequisites
- **Rust** 1.74+ ([install rustup](https://rustup.rs/))
- **Linux** (Ubuntu 22.04+, or any distro with `/sys/block` and `/proc`)
- **Graphviz** (for diagram rendering): `apt install graphviz`
- **librsvg2** (for PNG conversion): `apt install librsvg2-bin`

### Clone and Build
```bash
git clone https://github.com/danindiana/speedsys-rs
cd speedsys-rs
cargo build --release
./target/release/speedsys-rs --help
```

### Running Tests
```bash
# All tests (unit + integration)
cargo test

# Integration tests only (the main test suite)
cargo test --test integration_tests

# With output
cargo test -- --nocapture
```

### Code Quality Checks
```bash
# Clippy linter (must pass with -D warnings)
cargo clippy -- -D warnings

# Format check (optional, but encouraged)
rustfmt --check src/**/*.rs
```

### Regenerating Documentation
If you modify diagrams or add new screenshots:
```bash
./scripts/render_diagrams.sh    # Regenerate docs/diagrams/*.{svg,png}
./scripts/render_screenshots.sh # Rebuild and generate docs/screenshots/*.{svg,png}
```

## Submitting Pull Requests

### Before You Start
1. **Check for existing issues**: Is this already reported or in progress?
2. **Open an issue first** for larger features (so we can discuss approach)
3. **Fork and branch**: Work on a feature branch, not `master`
   ```bash
   git checkout -b fix/disk-test-hang
   # or
   git checkout -b feat/smart-integration
   ```

### Code Standards
- **Clippy**: All code must pass `cargo clippy -- -D warnings`
- **Tests**: Add tests for new functionality (see `tests/integration_tests.rs` for examples)
- **Comments**: Only comment non-obvious WHY (not WHAT) — code should be self-documenting
- **Modules**: Keep module responsibilities single and clear (see `src/` structure)

### Commit Message Format
```
<type>: <subject>

<body (optional)>

<footer (optional)>
```

**Type**: fix, feat, refactor, docs, test, perf  
**Subject**: Imperative, present tense, lowercase, no period

Example:
```
fix: prevent disk test hang on 512-sector HDDs

Apply posix_fadvise(POSIX_FADV_SEQUENTIAL) to linear_read() to reduce
kernel page-cache churn during full test (-T) benchmark.

Fixes #42
```

### Submitting
1. **Push your branch**: `git push origin fix/disk-test-hang`
2. **Open a PR** with:
   - Clear title and description
   - Reference to related issue (e.g., "Fixes #42")
   - Test results: `cargo test --release` output
   - Clippy status: `cargo clippy -- -D warnings` (must pass)
3. **Respond to review**: Address feedback promptly

## Code Structure Overview

```
src/
├── main.rs              # Entry point, mode dispatch, event loop
├── cli.rs               # Argument parsing (clap)
├── app.rs               # Application state machine (Screen enum)
├── sysinfo.rs           # System inventory (/proc, /sys parsing)
├── bench/
│   ├── mod.rs           # BenchMsg enum, result types
│   ├── cpu.rs           # CPU LCG benchmark
│   ├── mem.rs           # Memory throughput sweep
│   └── disk.rs          # Disk linear read & random seek
└── ui/
    ├── mod.rs           # Screen router
    ├── overview.rs      # System info + CPU/mem display
    ├── disks.rs         # Disk selector + test results
    └── common.rs        # Shared widgets
tests/
├── integration_tests.rs # 16 golden snapshot tests
```

### Key Patterns
- **State Machine**: `Screen` enum drives UI navigation
- **Channels**: `mpsc::Sender<BenchMsg>` for thread → UI communication
- **Graceful Shutdown**: `Arc<AtomicBool>` in benchmark workers
- **Caching**: `OnceLock` for device scanning (Phase 9 optimization)
- **Conditional Rendering**: `App::needs_render()` skips redraws (Phase 9 optimization)

## Performance Considerations

Before submitting performance improvements:
1. **Measure**: Time the old and new code with `time ./target/release/speedsys-rs`
2. **Profile**: Use `perf` or `flamegraph` if making significant changes
3. **Test**: Run full benchmark suite across NVMe, SSD, and HDD
4. **Document**: Note the improvement in commit message (e.g., "30% faster device scan")

## Phases & Project History

This project uses **Phase-based development**:
- **Phase 0-5**: Core benchmarks, TUI, disk tests (2024)
- **Phase 6-7**: CLI parity, code quality (2025)
- **Phase 8-9**: Testing, optimization (2025)
- **Phase 10**: Documentation & polish (2025)
- **Deferred**: SMART integration, async I/O, packaging

See [CHANGELOG.md](CHANGELOG.md) and [README Roadmap](#roadmap) for current status.

## Questions?

- **Issue not clear?** Open a GitHub Discussion
- **Need help?** Check the [Troubleshooting](README.md#troubleshooting) section
- **Found typo?** Submit a PR — no issue needed for doc fixes!

Thanks for contributing to speedsys-rs! 🚀
