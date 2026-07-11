# Phase 4: Graphical Enhancements — Disk Test Progress Visualization

**Date:** 2026-07-11  
**Status:** 📋 Planned (not started)  
**Dependency:** Phase 3 ✅ Complete

---

## Current State (Post-Phase 3)

✅ **Disk benchmarks fully functional:**
- Quick test (t): 64 samples of 8 MB linear reads + 200 random seeks (~30 sec on NVMe, 2-3 min on HDD)
- Full test (T): 512 samples of 16 MB linear reads + 200 random seeks (~2-30 min depending on device)
- Results displayed as text: min/avg/max speeds, seek latency scatter plot (dots on a line)
- Status bar shows progress and error messages
- Results persist across multiple disk tests via `HashMap<String, DiskBenchResult>`

---

## Problem Statement

**Current User Experience During Extended Tests:**
- Press 'T' → binary reads from disk for 15-30 minutes
- Screen shows static status ("Random seek on sda...") with no progress
- User cannot tell if it's hung, progressing slowly, or done
- No visual feedback on completion percentage or ETA

**Goal:** Show real-time visual progress during extended tests to improve UX for long benchmarks.

---

## Proposed Solution: Real Charts with Progress

### 1. Live Linear Read Speed Graph
**Feature:** Chart widget showing speed (MB/s) vs position (%) as test runs
- Y-axis: Speed 0–max
- X-axis: Position 0–100%
- Each sample adds a point; chart updates every ~50 samples
- Color: Cyan for valid, Red for anomalies
- Title: "Linear Read Speed — Sample N/512 (X% complete, ETA 5m 30s)"

**Implementation:**
- Update `DiskBenchResult` with `current_sample: usize` 
- Send `BenchMsg::Progress(sample_idx, total_samples)` every N samples
- Main loop merges into `app.current_progress`
- `render_test()` uses `Chart::new().data_set(Dataset::default().data(&linear_data))`
- ratatui::widgets::Chart already imported

**Cost:** 40 lines in disk.rs (progress sends), 20 lines in main.rs (progress merge), 30 lines in render (chart widget)

### 2. Progress Status Line
**Feature:** Contextual status with progress bar and ETA
- Current: `Status: Random seek on sda...`
- New: `█████░░░░░░ 47% complete (12s elapsed, ~14s remaining) [Esc to cancel]`

**Implementation:**
- Calculate elapsed time from test start
- Estimate total time: `elapsed_time / (sample_idx / total_samples)`
- Render progress bar: filled blocks = `(sample_idx / total) * bar_width`
- BenchMsg::Progress() carries (current, total, elapsed_secs)

**Cost:** 15 lines in disk.rs (time tracking), 10 lines in render (progress bar formatting)

### 3. Seek Latency Live Scatter Plot
**Feature:** Animated scatter plot showing seek times vs attempt number as test runs
- Y-axis: Latency 0–max_observed
- X-axis: Seek number 0–K (200)
- Each seek result adds a dot
- Updates every ~20 seeks

**Implementation:**
- Store seek results in separate `Vec<f64>` for live display
- Send progress after every ~20 seeks
- Use Chart scatter dataset

**Cost:** 20 lines in disk.rs (progress sends), 20 lines in render

---

## Implementation Order (Priority)

### Priority 1: Progress Status Line
**Why:** Highest impact for lowest effort. Gives users immediate feedback that test is running.

```
// In disk.rs, after each sample:
if sample_idx % 50 == 0 {
    let elapsed = start_time.elapsed().as_secs_f64();
    let est_total = elapsed / (sample_idx as f64 / total as f64);
    let remaining = est_total - elapsed;
    tx.send(BenchMsg::Progress(sample_idx, total_samples, elapsed));
}

// In render_test():
if let Some((idx, total, elapsed)) = app.current_progress {
    let pct = (idx as f64 / total as f64) * 100.0;
    let est_remaining = elapsed / (idx as f64 / total as f64) - elapsed;
    let bar_width = 40;
    let filled = (pct / 100.0 * bar_width as f64) as usize;
    let bar = "█".repeat(filled) + "░".repeat(bar_width - filled);
    lines.push(Line::from(format!(
        "{} {:.0}% (~{:.0}s remaining)",
        bar, pct, est_remaining
    )));
}
```

### Priority 2: Linear Read Chart
**Why:** Visual representation of speed across device; helps detect performance cliffs.

```
// In render_test(), split layout: info left (30%), chart right (70%)
let chart = Chart::default()
    .block(Block::default().title("Linear Read Speed").borders(Borders::ALL))
    .x_axis(Axis::default().bounds([0.0, 100.0]).title("Position %"))
    .y_axis(Axis::default().bounds([0.0, max_speed]))
    .datasets(vec![
        Dataset::default()
            .data(linear_data)  // from result.linear_speed_mbs
            .marker(Marker::Dot)
            .style(Style::default().fg(Color::Cyan))
    ]);
f.render_widget(chart, chart_area);
```

### Priority 3: Seek Latency Chart
**Why:** Visual representation of random access consistency; lower priority than progress feedback.

---

## Testing Strategy

### Unit Tests
- [ ] Progress calculation: verify elapsed, estimated total, remaining
- [ ] Progress bar formatting: verify filled/empty block counts

### Integration Tests
- [ ] Run quick test (t), verify progress updates every 8 samples
- [ ] Run full test (T) for 60 seconds, verify chart rendering updates
- [ ] Press Esc mid-test, verify cancel stops chart updates
- [ ] Test on both NVMe (fast, completes quickly) and HDD (slow, visible progress)

### Manual Testing
- [ ] Visual inspection of progress bar on 2-minute test
- [ ] Verify chart axes match data ranges
- [ ] Verify no performance regression (chart rendering cost)

---

## Metrics & Success Criteria

| Criterion | Target |
|-----------|--------|
| Progress update frequency | Every 50 samples (every ~5-10 sec on HDD) |
| Chart render latency | <10ms per frame (doesn't block UI) |
| Progress bar length | 40 chars (fits in 80-column terminal) |
| ETA accuracy | ±20% of actual time |
| No regressions | Results unchanged; only UI enhanced |

---

## Files to Modify

1. `src/bench/mod.rs` → Add `Progress(usize, usize, f64)` variant to `BenchMsg`
2. `src/bench/disk.rs` → Send progress updates every N samples
3. `src/main.rs` → Merge progress messages into `app.current_progress`
4. `src/app.rs` → Add `current_progress: Option<(usize, usize, f64)>` field
5. `src/ui/disks.rs` → Render progress bar and live charts

---

## Deferred to Phase 5+

- [ ] Live memory bandwidth chart
- [ ] CPU benchmark progress visualization
- [ ] Persistent progress history (save results + ETA per device model)
- [ ] Report export with embedded charts

---

## Risk Assessment

**Low Risk:**
- Progress messages don't affect benchmark timing (sent asynchronously)
- Chart rendering is optional UI detail
- Fallback: if chart fails, status message still visible

**Medium Risk:**
- Chart library (ratatui::Chart) has complex axis logic; needs careful bounds testing
- ETA calculation can be inaccurate on first few samples

**Mitigation:**
- Test chart rendering on multiple terminal sizes
- Use conservative ETA (ignore first 10% of samples)
- Log all BenchMsg traffic in debug mode for troubleshooting

---

## Next Steps

1. Implement Priority 1 (progress bar + status line)
2. Test on 5-minute full test; verify readability and ETA accuracy
3. Implement Priority 2 (linear read chart)
4. Test chart rendering on 80-column and 120-column terminals
5. Implement Priority 3 (seek latency chart) if time permits
6. Commit as `Phase 4: Add graphical progress display for extended benchmarks`

---

**Estimated Effort:** 4-6 hours (including testing on real hardware)

**Complexity:** Medium (ratatui Chart widget is well-documented; ETA math is straightforward)

**Impact:** High (transforms long benchmarks from black-box wait to interactive progress visualization)
