# Phase 6: CLI Parity & Arguments

**Goal:** Add command-line argument support for non-interactive operation, matching original SPEEDSYS flags.

---

## Target CLI Interface

```bash
# Interactive (current default)
sudo speedsys-rs

# Quick test all disks
sudo speedsys-rs -t1

# Full test all disks  
sudo speedsys-rs -t2

# Test specific disk
sudo speedsys-rs -hd0 -t1        # Quick test /dev/sda
sudo speedsys-rs -hd1 -t2        # Full test /dev/sdb

# List disks
sudo speedsys-rs --list-disks

# Combined with report export
sudo speedsys-rs -t1 -hd0 --report /tmp/hd0-quick.json
sudo speedsys-rs -t2 --report-html /tmp/full-bench.html

# Help
sudo speedsys-rs --help
```

---

## Argument Mapping

| Flag | Long Form | Meaning | Value |
|------|-----------|---------|-------|
| `-t1` | `--quick-test` | Quick benchmark (64 samples) | Boolean |
| `-t2` | `--full-test` | Extended benchmark (512 samples) | Boolean |
| `-hdN` | `--disk N` | Select disk (0=sda, 1=sdb, etc.) | usize |
| `-l` | `--list-disks` | List available disks | Boolean |
| `-s` / `-sm` | `--smart` | Include SMART data | Boolean |
| `-r FILE` | `--report FILE` | JSON export | String |
| `-c FILE` | `--report-csv FILE` | CSV export | String |
| `-h FILE` | `--report-html FILE` | HTML export | String |
| `--dump` | `--dump` | Text system info | Boolean |
| `--help` | `--help` | Show help | Boolean |

---

## Implementation Strategy

### Option A: Simple Manual Parser (No Dependencies)
- Parse `std::env::args()` manually
- Minimal code, no external deps
- Pros: Lightweight, works everywhere
- Cons: Verbose, error-prone

### Option B: clap Crate (Recommended)
- Industry-standard Rust CLI parser
- Auto-generated help, error handling
- Pros: Professional, maintainable, extensible
- Cons: ~5KB binary size overhead
- **RECOMMENDATION: Use clap 4.x**

---

## Implementation Steps

### Step 1: Add clap dependency
```toml
[dependencies]
clap = { version = "4.4", features = ["derive"] }
```

### Step 2: Define CLI struct
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "speedsys-rs")]
#[command(about = "System performance benchmarking tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Quick test (64 samples, 8MB per sample)
    #[arg(short = 't', long, value_name = "1")]
    quick_test: Option<String>,

    /// Full test (512 samples, 16MB per sample)
    #[arg(short = 't', long, value_name = "2")]
    full_test: Option<String>,

    /// Select disk by index (0=sda, 1=sdb, etc.)
    #[arg(long = "disk", value_name = "N")]
    disk: Option<usize>,

    /// List available disks and exit
    #[arg(long)]
    list_disks: bool,

    /// JSON report output
    #[arg(short = 'r', long = "report", value_name = "FILE")]
    report_json: Option<String>,

    /// CSV report output
    #[arg(short = 'c', long = "report-csv", value_name = "FILE")]
    report_csv: Option<String>,

    /// HTML report output
    #[arg(short = 'h', long = "report-html", value_name = "FILE")]
    report_html: Option<String>,

    /// System info dump (text output)
    #[arg(long)]
    dump: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run interactive TUI
    Interactive,
    /// Run benchmarks in test mode
    Test { /* args */ },
}
```

### Step 3: Implement non-interactive test mode
- Run CPU + memory benchmarks
- Optional disk tests (if `-hdN` specified)
- Export results (if `--report*` specified)
- Exit when complete (no TUI)

### Step 4: Implement list-disks mode
- Scan /sys/block
- Print formatted table
- Exit

---

## Execution Flow by Command

### `speedsys-rs` (no args)
→ Interactive TUI

### `speedsys-rs --list-disks`
→ Print disk table → Exit

### `speedsys-rs -t1`
→ Run quick CPU/memory/all-disks → Export (if requested) → Exit

### `speedsys-rs -t1 -hd0 --report /tmp/out.json`
→ Run quick CPU/memory/disk0 → Export JSON → Exit

### `speedsys-rs --dump`
→ Print system info → Exit

---

## File Changes Required

1. **Cargo.toml**
   - Add `clap = "4.4"` with derive feature

2. **src/main.rs**
   - Import clap
   - Add Cli struct
   - Refactor main() to parse args
   - Route to: tui_mode() | test_mode() | list_mode() | dump_mode()

3. **src/cli.rs** (new)
   - `fn list_disks()` → print disk table
   - `fn test_mode(args)` → run benchmarks non-interactively
   - Handle disk selection for tests

---

## Testing Checklist

- [ ] `speedsys-rs` → Shows TUI
- [ ] `speedsys-rs --help` → Shows help text
- [ ] `speedsys-rs --list-disks` → Shows disk table
- [ ] `speedsys-rs -t1` → Quick test, CPU + all disks, exit
- [ ] `speedsys-rs -t2 -hd0` → Full test disk 0 only
- [ ] `speedsys-rs --dump` → System info only
- [ ] `speedsys-rs -t1 --report /tmp/out.json` → Quick test + JSON export
- [ ] `speedsys-rs -t1 --report-html /tmp/out.html` → Quick test + HTML export
- [ ] `speedsys-rs -t2 -hd1 --report-csv /tmp/out.csv` → Full disk1 test + CSV export

---

## Backward Compatibility

- Original `--dump` mode still works
- Original `--report FILE` still works
- New flags are additive, don't break existing commands
- Default (no args) still launches interactive TUI

---

## Estimated Effort

- Add clap: 10 min (dependencies)
- Implement Cli struct: 15 min
- Refactor main routing: 20 min
- Implement test_mode: 30 min (reuse export logic)
- Implement list_mode: 10 min
- Testing: 20 min
- **Total: ~105 minutes (1.75 hours)**

---

## Next Steps After Phase 6

- **Phase 7:** Hygiene pass (`cargo clippy -D warnings`)
- **Phase 8:** Golden snapshot tests for --dump output
- **Phase 9:** Performance optimization (SIMD, threading)
- **Phase 10:** Package management (Homebrew, AUR, deb/rpm)
