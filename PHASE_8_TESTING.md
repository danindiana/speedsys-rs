# Phase 8: Golden Snapshot Tests

**Goal:** Establish regression testing framework to catch unintended changes in output formats and benchmark results.

---

## Overview

Golden snapshot testing captures reference outputs ("snapshots") from correct behavior, then validates that future runs produce identical or compatible output.

---

## Test Categories

### 1. **CLI Argument Parsing Tests** ✅

**File:** `tests/integration_tests.rs`

Tests that all CLI flags are recognized and properly parsed:
- `--help`, `--dump`, `--list-disks`
- `-t1` / `--quick-test`, `-T` / `--full-test`
- `--disk N`, `-r` / `--report`, `-c` / `--report-csv`, `-h` / `--report-html`
- Flag combinations (e.g., `-t1 --disk 0 --report /tmp/out.json`)

**Validation:** Arguments parse without errors; help text displays correctly.

### 2. **Output Format Tests** ✅

**Golden Snapshots:**

#### 2a. System Info (--dump mode)
```
Expected format:
  Hostname: [value]
  Kernel: [value]
  CPU Model: [value]
  CPU Count: [number]
  RAM: [number] GB
  BIOS: [value]
  Board: [value]
  OS: [value]
  Uptime: [number] seconds
  ...
```

**Validation Rules:**
- All expected fields present
- Numeric values are valid floats/ints
- String values are non-empty
- No missing sections

#### 2b. JSON Report Format
```json
{
  "timestamp": "RFC3339 format",
  "system": {
    "cpu_model": "string",
    "cores": number,
    "mhz": "string",
    "os": "string",
    "mem_mb": number
  },
  "benchmarks": {
    "cpu_mops": number or null,
    "memory_sweep": [
      { "log2_kb": number, "mb_s": number },
      ...
    ],
    "disks": {
      "device_name": {
        "linear_speed_mbs": { "avg": number, "min": number, "max": number, "samples": number },
        "seek_latency_ms": { "avg": number, "max": number, "samples": number },
        "temperature_c": number or null
      }
    }
  }
}
```

**Validation Rules:**
- JSON is valid and well-formed
- All required fields present
- Numeric values are positive
- Benchmarks are mutually consistent (min ≤ avg ≤ max)

#### 2c. CSV Report Format
```
Device,Test Type,Metric,Value,Unit
CPU,Benchmark,Performance,1250.5,MOPS
Memory,Bandwidth,4KB,8000.0,MB/s
sda,Linear Read,Average,150.5,MB/s
...
```

**Validation Rules:**
- CSV is valid (proper escaping, no newlines in fields)
- Header row matches spec
- All data rows have correct column count
- Numeric values in Value column are valid

#### 2d. HTML Report Format
```html
<!DOCTYPE html>
<html>
  <head>
    <title>System Benchmark Report</title>
    <!-- styles embedded -->
  </head>
  <body>
    <div class="container">
      <h1>System Benchmark Report</h1>
      <h2>System Information</h2>
      <div class="system-info">
        <!-- info items -->
      </div>
      <h2>Benchmark Results</h2>
      <table>
        <!-- results table -->
      </table>
      <div class="timestamp">Report generated: ...</div>
    </div>
  </body>
</html>
```

**Validation Rules:**
- HTML is valid (proper tag closure)
- All major sections present
- No external dependencies (CSS/JS inline)
- Timestamp present and valid

### 3. **Benchmark Result Structure Tests** ✅

**Linear Read Benchmark:**
- Results: Vec<(position_pct: f64, speed_mbs: f64)>
- Position: 0.0 to 100.0 (device location percentage)
- Speed: Positive float (MB/s)
- Invariant: min_speed ≤ avg_speed ≤ max_speed

**Random Seek Benchmark:**
- Results: Vec<latency_ms: f64>
- Latency: Positive float (milliseconds)
- Typical range: 0.1ms (NVMe) to 10ms (HDD)
- Invariant: avg_latency ≤ max_latency

**Memory Bandwidth Test:**
- Results: Vec<(log2_kb: f64, mb_s: f64)>
- Log2_kb: 2-16 (4KB to 64MB)
- Speed: Positive float (MB/s)
- Monotonic: Speed generally increases with cache size (until L3 limit)

### 4. **Mode Priority Tests** ✅

**CLI modes prioritized in order:**
1. `--list-disks` → print disks and exit
2. `--dump` → system info and exit
3. `-t1` / `-T` → run tests and exit
4. (default) → interactive TUI

**Test:** Verify that when multiple modes are specified, the first in priority order executes.

---

## Running Tests

```bash
# Run all tests
cargo test --release

# Run only integration tests
cargo test --test integration_tests --release

# Run specific test
cargo test test_cli_args_quick_test --release

# Run tests with output
cargo test --release -- --nocapture

# Run golden snapshot tests
cargo test golden_snapshots --release
```

---

## Adding New Snapshot Tests

### Step 1: Generate Golden Snapshot
```bash
# Run the command and save output
sudo ./target/release/speedsys-rs --dump > snapshots/sysinfo_golden.txt
sudo ./target/release/speedsys-rs -t1 --report /tmp/bench.json
cat /tmp/bench.json > snapshots/benchmark_golden.json
```

### Step 2: Create Test Function
```rust
#[test]
fn snapshot_name_matches_golden() {
    let output = run_command();
    let golden = include_str!("../snapshots/snapshot_name_golden.txt");
    assert_eq!(output, golden, "Output should match golden snapshot");
}
```

### Step 3: Update Snapshot When Expected
```bash
# If changes are intentional, regenerate snapshot
sudo ./target/release/speedsys-rs --dump > snapshots/sysinfo_golden.txt
cargo test snapshot_name_matches_golden
```

---

## Snapshot Strategy

### ✅ **Exact Match** (Recommended)
- For format validation tests
- For unit test outputs
- For architecture/structure tests

### ✅ **Pattern Match** (For System-Specific Values)
- For CPU model (varies by hardware)
- For RAM amount (varies by system)
- For device names (varies by system)
- Use regex or partial matching

### ✅ **Numeric Range Check** (For Benchmark Values)
- For performance metrics
- Verify: positive, reasonable range, consistency
- Example: "Speed should be between 100-5000 MB/s"

### ❌ **Avoid Brittle Tests**
- Don't hardcode absolute performance numbers
- Don't assume specific device names
- Don't expect exact CPU model names
- Don't compare floating-point values with == (use epsilon comparison)

---

## Regression Test Coverage

| Category | Tests | Status |
|----------|-------|--------|
| CLI parsing | 7+ | ✅ Implemented |
| Output format | 4 (dump, JSON, CSV, HTML) | ✅ Implemented |
| Benchmark structure | 3 | ✅ Implemented |
| Mode priority | 1 | ✅ Implemented |
| **Total** | **15+** | **✅ Complete** |

---

## CI/CD Integration

### Local
```bash
# Before committing
cargo test --release
cargo clippy --release -- -D warnings
```

### GitHub Actions (if configured)
```yaml
- name: Run tests
  run: cargo test --release

- name: Clippy check
  run: cargo clippy --release -- -D warnings
```

---

## Known Limitations

### System-Specific Values
- CPU model, count, MHz vary by hardware
- RAM amount varies by system
- Disk device names vary by system
- Kernel version varies by OS

**Solution:** Use pattern matching or numeric range validation for these fields.

### Floating-Point Precision
- Benchmark results may vary slightly between runs
- Cache effects can cause variation

**Solution:** Use epsilon comparison or check ranges rather than exact equality.

### Performance Variability
- Test duration varies with system load
- Thermal throttling affects speeds
- Background processes affect results

**Solution:** Don't assert specific performance values; check ranges and consistency.

---

## Future Test Additions

### Phase 9 (Optional)
- [ ] Benchmark consistency tests (repeated runs should be similar)
- [ ] Performance regression detection (warn if speed drops >10%)
- [ ] CSV parsing validation (ensure CSV is RFC 4180 compliant)
- [ ] HTML accessibility validation (ensure semantic HTML)
- [ ] JSON schema validation (assert against formal schema)

### Phase 10 (Polish)
- [ ] Snapshot update automation (`./scripts/update-snapshots.sh`)
- [ ] Snapshot versioning (when format changes)
- [ ] Snapshot diff reporting (show what changed)

---

## Test Maintenance

### When Tests Fail

1. **Intentional Format Change?**
   - Regenerate snapshot: `cargo test -- --nocapture > new_snapshot.txt`
   - Review changes carefully
   - Commit updated snapshot with explanation

2. **Regression Bug?**
   - Investigate root cause
   - Fix the bug
   - Verify test passes with fix

3. **System-Specific Value?**
   - Update test to use pattern matching
   - Add comment explaining why

### Keeping Tests Current

- Review snapshots when making format changes
- Update docstrings when adding new fields
- Run full test suite before pushing: `cargo test --release && cargo clippy --release -- -D warnings`

---

## References

- Test file: `tests/integration_tests.rs`
- Documentation: This file (`PHASE_8_TESTING.md`)
- Snapshots directory: `tests/snapshots/` (created as needed)

---

**Status:** ✅ Phase 8 (Snapshot tests) documented and implemented

**Next:** Phase 9 (optional) - Advanced test scenarios
