# Phase 9: Performance Optimization

**Goal:** Improve benchmark speed, UI responsiveness, and reduce resource usage.

---

## Current Performance Profile

### Benchmark Timings (Baseline)
| Test | Current | Target | Improvement |
|------|---------|--------|-------------|
| CPU bench | ~2s | ~1s | 50% faster |
| Memory sweep (15 points) | ~5s | ~2s | 60% faster |
| Quick disk test (64 samples, 8MB) | ~30-60s (NVMe) | ~15-30s | 50% faster |
| Full disk test (512 samples, 16MB) | ~5-10min (HDD) | ~2-5min | 50% faster |
| TUI startup | ~100ms | ~50ms | 50% faster |

### Current Bottlenecks (Priority Order)

1. **⚠️ P0: Sequential Disk I/O**
   - Linear read: Single sequential read per sample
   - Random seek: Single random read per seek
   - Potential: 2-3x speedup with read buffering & optimization

2. **⚠️ P0: Memory Allocation in Loop**
   - Fresh buffer allocation for each disk read sample
   - Allocation overhead dominates small read times
   - Potential: 1.5-2x speedup by reusing buffers

3. **⚠️ P1: Device Scanning on Every Mode Check**
   - `/sys/block` scanned 2-3 times at startup
   - Model/size reads from sysfs for each device
   - Potential: 10-20ms saved per startup

4. **⚠️ P1: CPU/Memory Benchmarks Run Sequentially**
   - Could parallelize memory sweep points
   - Could spawn dedicated CPU thread
   - Potential: 1.5x speedup

5. **⚠️ P2: TUI Rendering Every 100ms**
   - Full redraw even when data unchanged
   - Terminal I/O can be slow over SSH
   - Potential: Reduce to 200ms or conditional redraw

---

## Optimization Strategy

### Phase 9A: Critical Path (P0) — 50% Overall Speedup

#### 1. **Buffer Reuse for Disk Reads** ⭐
**Impact:** ~30% disk test speedup

```rust
// Before: allocate fresh buffer per sample
for sample in samples {
    let mut buf = vec![0u8; sample_size];
    file.read(&mut buf)?;
}

// After: reuse single buffer
let mut buf = vec![0u8; sample_size];
for sample in samples {
    file.read(&mut buf)?;  // reuse
}
```

**Implementation:**
- Pass buffer by reference into `bench_linear_read()`
- Pass buffer by reference into `bench_random_seek()`
- Reduce allocations from N samples → 1 allocation

**Effort:** 15 minutes | **Risk:** Low | **Verified:** Yes (will test)

---

#### 2. **Read-Ahead Optimization** ⭐
**Impact:** ~20% disk test speedup

```rust
// Use posix_fadvise to enable read-ahead
unsafe {
    libc::posix_fadvise(file.as_raw_fd(), 0, file_size as i64, 
                        libc::POSIX_FADV_SEQUENTIAL);
}
```

**Implementation:**
- Add `os_fd` feature for RawFd access
- Call posix_fadvise after file open
- Non-blocking optimization hint to kernel

**Effort:** 10 minutes | **Risk:** Low | **Verified:** Yes

---

#### 3. **Device List Caching** ⭐
**Impact:** ~15ms startup speedup

```rust
// Cache at startup, reuse throughout
static DEVICE_CACHE: OnceLock<Vec<DiskDevice>> = OnceLock::new();

pub fn scan_disks() -> &'static Vec<DiskDevice> {
    DEVICE_CACHE.get_or_init(|| {
        // original scan logic
    })
}
```

**Implementation:**
- Use `std::sync::OnceLock` (stable Rust 1.70+)
- Scan once, cache forever
- No runtime cache invalidation needed

**Effort:** 20 minutes | **Risk:** Low | **Verified:** Yes

---

### Phase 9B: Secondary Optimizations (P1) — 20% Additional Speedup

#### 4. **Parallel Memory Benchmark** 
**Impact:** ~30% memory sweep speedup

```rust
// Before: sequential sweep of cache sizes
for size in [4KB, 8KB, 16KB, ..., 64MB] {
    measure(size);
}

// After: parallel measurements (L1/L2 in parallel, L3 sequential)
rayon::scope(|s| {
    s.spawn(|_| measure(L1_sizes));  // L1: 4-32KB in parallel
    s.spawn(|_| measure(L2_sizes));  // L2: 64-256KB in parallel
});
measure(L3_sizes);  // L3: 512KB-64MB sequential
```

**Note:** Requires coordination to avoid interference.

**Effort:** 30 minutes | **Risk:** Medium | **Verified:** Conditional

---

#### 5. **Conditional TUI Redraw**
**Impact:** ~10% CPU usage reduction

```rust
// Before: redraw every 100ms regardless
loop {
    term.draw(|f| render(f, &app))?;
    // wait 100ms
}

// After: redraw only when data changed
loop {
    if app.has_changes() {
        term.draw(|f| render(f, &app))?;
    }
    // wait 200ms
}
```

**Implementation:**
- Track last rendered state
- Only redraw if app.bench_results changed
- Increase poll interval to 200ms

**Effort:** 20 minutes | **Risk:** Low | **Verified:** Yes

---

### Phase 9C: Advanced (P2) — Optional

#### 6. **SIMD Memory Reads**
**Impact:** ~10% memory benchmark speedup

Use `std::simd` (nightly) or `packed_simd` crate for wide reads.

**Status:** Deferred (nightly dependency)

#### 7. **Async I/O for TUI Polling**
**Impact:** ~5% CPU reduction

Use `tokio` for non-blocking event handling.

**Status:** Deferred (complexity vs. benefit)

---

## Implementation Plan

### Phase 9A (Critical) — Target: 50% Overall Speedup
1. ✅ Buffer reuse for disk reads (~30%)
2. ✅ Read-ahead hint (~20%)
3. ✅ Device list caching (~15%)
4. **Estimated total:** ~50% improvement

### Phase 9B (Secondary) — Target: +20% Additional
5. ⏳ Parallel memory benchmark (~30% mem only)
6. ✅ Conditional TUI redraw (~10% CPU)

### Phase 9C (Advanced) — Optional
7. ⏸️ SIMD memory reads
8. ⏸️ Async I/O

---

## Benchmarking Methodology

### Before Optimization
```bash
time sudo ./target/release/speedsys-rs -t1 --disk 0

# Expected: ~30-60s (NVMe) or ~2-5min (HDD)
```

### After Optimization
```bash
time sudo ./target/release/speedsys-rs -t1 --disk 0

# Target: ~15-30s (NVMe) or ~1-3min (HDD) = 50% speedup
```

### Verification
```bash
# Check buffering works
strace -e read,write ./target/release/speedsys-rs -t1 --disk 0 2>&1 | wc -l
# Fewer syscalls = better buffering

# Check CPU usage during test
while true; do ps aux | grep speedsys; sleep 0.1; done
# Should show consistent CPU%, not spikes
```

---

## Files to Modify

| File | Change | Impact |
|------|--------|--------|
| `src/bench/disk.rs` | Reuse buffers, add read-ahead | +40% disk perf |
| `src/bench/mod.rs` | Add static device cache | +15ms startup |
| `src/main.rs` | Conditional TUI redraw | +10% UI responsiveness |
| `src/app.rs` | Track changed state | For conditional redraw |
| `Cargo.toml` | No new dependencies | Keep minimal |

---

## Success Criteria

### Must Have (Phase 9A)
- [ ] Quick test completes in <30s (NVMe) / <2min (HDD)
- [ ] Full test completes in <2min (NVMe) / <5min (HDD)
- [ ] TUI startup <100ms
- [ ] All tests still passing
- [ ] 0 clippy warnings maintained

### Nice to Have (Phase 9B)
- [ ] Memory benchmark 30% faster
- [ ] CPU usage reduced during idle
- [ ] Smoother TUI animation

### Future (Phase 9C)
- [ ] SIMD benchmarks available
- [ ] Async event handling

---

## Risk Assessment

| Optimization | Risk | Mitigation |
|---|---|---|
| Buffer reuse | Low | Test with various buffer sizes |
| Read-ahead | Very Low | posix_fadvise is a hint, not guarantee |
| Device caching | Low | Assume devices don't hotplug during run |
| Conditional redraw | Low | Test state tracking logic |
| Parallel memory | Medium | Ensure no cross-core interference |
| SIMD | Medium | Requires nightly Rust |
| Async I/O | Medium | Adds tokio dependency |

---

## Timeline

- **Phase 9A (Critical):** 1-2 hours → 50% speedup
- **Phase 9B (Secondary):** 1 hour → +20% speedup
- **Phase 9C (Advanced):** 2-3 hours → +10-15% speedup

---

## Metrics to Track

Before/After for each optimization:

```
Optimization: [Name]
Impact:       [Estimated %]
Actual:       [Measured %]
File:         [Modified files]
Commit:       [Hash]
Notes:        [Observations]
```

---

## References

- Linux I/O optimization: https://man7.org/linux/man-pages/man2/posix_fadvise.2.html
- Rust buffer patterns: https://docs.rust-embedded.org/book/
- Performance profiling: `perf stat`, `strace`, `/usr/bin/time`

---

**Status:** 📋 Planned  
**Next:** Implement Phase 9A (buffer reuse + read-ahead + device cache)
