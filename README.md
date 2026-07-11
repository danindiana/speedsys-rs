# speedsys-rs

A Rust + [ratatui](https://ratatui.rs) homage to SYSTEM SPEED TEST 4.78
(the classic DOS benchmark by Vladimir Afanasiev).

## Build & run
    cargo build --release        # release mode matters: benchmarks are optimized
    ./target/release/speedsys-rs

Keys: `r` rerun benchmarks, `q`/`Esc` quit.
`--dump` renders one frame as plain text (CI / screenshots).

## What it does
- System inventory from /proc and /sys (CPU, CPUID family/model/stepping,
  caches, RAM, block devices, mainboard/BIOS DMI, OS) — no external commands.
- Integer CPU benchmark (LCG loop) placed on a vintage-flavoured comparison
  ladder, like the original's 386DX-40 → Athlon-600 chart.
- Cache/memory read-throughput sweep from 4 KB to 64 MB, drawn as the
  classic staircase graph: each drop marks falling out of L1 → L2 → L3 → RAM.

Requires rustc ≥ 1.74 (tested on Ubuntu's rustc 1.75; unicode-segmentation
is pinned to 1.12 in Cargo.lock for MSRV compatibility — newer toolchains
can `cargo update` freely).
