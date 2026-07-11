# Frequently Asked Questions

## Installation & Setup

### Q: Why do I get "Permission denied" errors?
**A:** Raw device access requires elevated privileges. Either:
1. Run with `sudo`: `sudo ./target/release/speedsys-rs`
2. Add your user to the `disk` group (permanent):
   ```bash
   sudo usermod -aG disk $USER
   newgrp disk  # Apply immediately
   ```

### Q: My Linux distro doesn't have Rust. How do I install it?
**A:** Visit [rustup.rs](https://rustup.rs/) and run the installer:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Q: Can I run this on Windows or macOS?
**A:** Not currently. speedsys-rs reads `/proc` and `/sys`, which are Linux-only. A macOS port would require rewriting the system inventory module to use `sysctl`, Mach kernel APIs, and IOKit for device enumeration.

## Usage

### Q: What's the difference between quick test (-t) and full test (-T)?
**A:** 
- **Quick** (-t): 64 samples × 8 MB per disk (~30 sec per NVMe, 2-3 min per HDD)
- **Full** (-T): 512 samples × 16 MB per disk (~2-3 min per NVMe, 15-30+ min per HDD)

Full test gives more granular data (especially on mechanical drives where performance varies with disk position), but takes much longer.

### Q: Why does the full test take so long?
**A:** Full test reads 8 GB across the entire disk surface to measure performance at different positions. Mechanical HDDs must physically seek for each sample, adding significant latency. This is expected and necessary for accurate benchmarking.

### Q: Can I benchmark external USB drives?
**A:** Yes, they'll appear in the disk list and work with both quick and full tests. Performance will depend on the drive's interface and the USB hub.

### Q: What if I interrupt a test (Ctrl+C)?
**A:** The benchmark stops immediately, and results are discarded. The TUI returns to the overview screen.

### Q: Can I run tests in the background?
**A:** No, but you can use headless mode:
```bash
./target/release/speedsys-rs -t --report results.json
```
This runs quick test non-interactively and exports JSON. Useful for scripts and CI.

## Performance & Results

### Q: My SSD shows weird results (very high or very low speeds). Is the benchmark broken?
**A:** A few possibilities:
1. **Thermal throttling**: SSDs may reduce speed if overheating. Check `/proc/thermal` or SMART data.
2. **Write cache effects**: Some SSDs have very fast write-back cache; read-through can be slower.
3. **File system caching**: If running with insufficient permissions, OS caching may skew results. Use `sudo`.
4. **Background I/O**: Other processes reading/writing the disk will interfere. Kill them and retry.

### Q: My NVMe shows 3500 MB/s but the spec says 7000 MB/s. What's wrong?
**A:** Several factors:
1. **Sequential vs. 4K random**: Specs list sequential throughput. speedsys-rs measures both but they differ.
2. **Not a benchmark flaw**: Actual real-world throughput rarely reaches theoretical maximums.
3. **Throttling**: Check `/sys/class/nvme/nvme0/throttling_events`.
4. **Driver**: Older NVMe drivers have lower performance. Update your kernel/drivers.

### Q: Can I compare results with online benchmarks?
**A:** Roughly, yes, but:
- **Tools differ**: CrystalDiskInfo, Fio, and speedsys-rs use different patterns
- **Hardware context matters**: Speed depends on drive, controller, motherboard, CPU, RAM
- **Thermal state**: Sustained tests show thermal throttling; quick bursts do not

For reliable comparisons, **run speedsys-rs multiple times** on the same drive.

## Reports & Export

### Q: What format should I use for exporting results?
**A:**
- **JSON** (`--report results.json`): Machine-readable, import into scripts
- **CSV** (`--report-csv results.csv`): Excel/Sheets compatible, easy pivot tables
- **HTML** (`--report-html results.html`): Standalone report with charts, email-friendly

### Q: How do I share results with someone else?
**A:** Export to JSON, HTML, or take screenshots:
```bash
# JSON export
./target/release/speedsys-rs -t --report my_drive.json

# HTML report (standalone, includes embedded data)
./target/release/speedsys-rs -t --report-html my_drive.html

# Screenshot for comparison
./target/release/speedsys-rs --screenshot overview --screenshot-out overview.svg
```

## Development & Contributions

### Q: How do I add a new benchmark (e.g., 4K random write)?
**A:** 
1. Add a new function in `src/bench/disk.rs` (e.g., `bench_random_write()`)
2. Add a `BenchMsg` variant in `src/bench/mod.rs` (e.g., `WriteDone(f64)`)
3. Spawn a thread in `src/main.rs` and send updates via the channel
4. Display results in `src/ui/disks.rs`
5. Add tests in `tests/integration_tests.rs`
6. Run `cargo clippy -- -D warnings` and `cargo test`

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

### Q: How do I regenerate screenshots or diagrams?
**A:**
```bash
# Regenerate diagrams (requires graphviz)
./scripts/render_diagrams.sh

# Regenerate screenshots (requires rsvg-convert)
./scripts/render_screenshots.sh

# Or use the Makefile
make diagrams
make screenshot
```

### Q: Can I contribute without committing to the project?
**A:** Absolutely! Open issues, suggest features, review PRs, improve documentation. Every contribution counts.

## Troubleshooting

### Q: The UI looks garbled or has weird characters.
**A:** 
1. Ensure your terminal supports UTF-8: `echo $LANG` should show `utf-8` or `UTF-8`
2. Try a different terminal (GNOME Terminal, Konsole, iTerm2 on SSH)
3. Make sure `TERM` is set correctly: `echo $TERM` should be `xterm-256color` or similar

### Q: The tool crashes with "SIGSEGV" or segmentation fault.
**A:** This is a bug! Please [report it](https://github.com/danindiana/speedsys-rs/issues) with:
- Kernel version (`uname -r`)
- Hardware (`lsblk`, `lscpu`)
- Full error output and stack trace

### Q: Disk test reports zero MB/s or hangs forever.
**A:**
1. **Check permissions**: `ls -la /dev/nvme0n1` or `/dev/sda` (should be readable)
2. **Try with sudo**: `sudo ./target/release/speedsys-rs`
3. **Check disk health**: `sudo smartctl -a /dev/sda` (if smartmontools installed)
4. **Verify device size**: `sudo blockdev --getsize /dev/sda` (should be > 0)

### Q: Build fails with "could not find `sysinfo` in the dependency tree."
**A:** Make sure you're in the project directory:
```bash
cd speedsys-rs
cargo build --release
```

## Performance Tuning

### Q: How can I get the most accurate results?
**A:**
1. **Close background apps**: Stop browsers, editors, file managers
2. **Disable power management**: Disable CPU freq scaling temporarily (`sudo cpupower frequency-set -g performance`)
3. **Run multiple times**: Average results across 2-3 runs
4. **Use full test**: `-T` provides more granular data than `-t`
5. **Check thermals**: Monitor CPU/disk temperature during test

### Q: My results vary wildly between runs. Is the benchmark flaky?
**A:** Likely due to:
1. **Background I/O**: Other processes using the disk
2. **Thermal throttling**: Drive is overheating, reducing speed
3. **Write-back cache**: Not fully flushed; state carries from previous test
4. **Multiple runs help**: Average several runs for more stable numbers

## License & Legal

### Q: Can I use speedsys-rs in a commercial product?
**A:** Yes, under the MIT license. You must include the original copyright notice, but you can modify and distribute it freely. See [LICENSE](LICENSE) for details.

### Q: Who wrote speedsys-rs?
**A:** Original concept from Vladimir Afanasiev's SYSTEM SPEED TEST 4.78 (DOS, 1998). Modern Rust reimplementation by the speedsys-rs contributors. See [README credits](README.md#credits).

---

**Still have questions?** Open a [GitHub Discussion](https://github.com/danindiana/speedsys-rs/discussions) or check [CONTRIBUTING.md](CONTRIBUTING.md#questions).
