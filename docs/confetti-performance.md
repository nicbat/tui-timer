# Confetti performance and repeatable measurements

The optimized effect writes each particle directly into a Ratatui buffer cell.
The original constructed and laid out a `Paragraph` for every particle on every
frame. The effect keeps the same 100 bounded, deterministic particles and finishes
in five seconds. It uses no RNG dependency or growing particle collection.

## Reproduce

```sh
python3 scripts/bench_confetti.py --frames 3000
```

The script builds the normal release app and compiles a renderer probe against its
exact Ratatui artifact. It first compares the original and optimized buffers at
154 timestamps across four terminal sizes. Then it measures both renderers in
five alternating-order rounds and reports the median microseconds per frame.
The original renderer remains in the probe as a fixed reference for future work.

Effect-only measurement isolates particle work; full TestBackend frame measurement
also includes terminal-buffer construction/diffing. Neither includes a real terminal
emulator painting pixels. Linux PTY probes additionally measure child-process CPU
and emitted terminal bytes. They use temporary configuration files and never touch
personal settings. Reports are written to `target/qa/confetti-benchmark.txt`.

## Recorded results

September 26, 2026, Rust 1.98.1, local Linux host, 1,500 frames per round:

- **100×30 effect-only:** 9.18 → 1.07 µs/frame, **8.61× faster**.
- **300×100 effect-only:** 9.92 → 0.99 µs/frame, **10.06× faster**.
- **100×30 full TestBackend frame:** 33.11 → 24.84 µs/frame, **1.33× faster**.
- **300×100 full TestBackend frame:** 281.23 → 270.18 µs/frame, **1.04× faster**.

Large terminal-buffer work dominates full-frame cost, so the particle speedup must
not be described as a tenfold improvement to the whole app.

Confetti now uses a 50 ms frame budget (20 FPS); clocks retain 33 ms. Particles are
positioned by elapsed time, so the five-second duration and trajectory are preserved.
In short local four-second PTY samples at 300×100, confetti CPU changed from **5.00%
to 3.75% of one core**, and emitted output changed from **93,108 to 64,966 bytes**.
These are diagnostic samples with CPU tick quantization and different concurrent
system load, not a statistically controlled estimate for every machine. At 100×30,
both samples recorded 0.75% CPU. After expiry, both sizes recorded zero CPU ticks
and zero emitted bytes during the one-second idle sample.

## Instrumented build

```sh
cargo build --release --features metrics --target-dir target/metrics
TUI_TIMER_METRICS=/tmp/render-metrics.toml target/metrics/release/tui-timer --timer 1s --no-countdown --confetti
python3 scripts/test_metrics.py
```

Exit the app to write the report. `all_frames` and `confetti_frames` each record
frame count, total render microseconds, maximum microseconds, and frames exceeding
16/33 ms. Mean time is `total_us / frames`. The measured interval includes building
the UI, Ratatui's diff, and terminal writes. Polling and blocked input are excluded.
No timing instrumentation is compiled into a normal build.

A 100×30 metrics smoke test recorded 131 total frames and 100 confetti frames;
confetti averaged 289 µs/frame with a 957 µs maximum and no frame over 16 ms.
This real-app measure includes work excluded by the isolated renderer probe and
is intentionally reported separately.

The renderer equivalence checks, animation expiry/dismissal checks, static idle
checks, and metrics output assertions pass. Future optimizations should retain
those checks and record reports under equivalent load before making speed claims.
