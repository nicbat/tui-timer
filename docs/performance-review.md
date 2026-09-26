# Performance review

Reviewed 2026-09-26 against the release binary built at 02:31 local time, before the confetti/theme work. This document records the baseline; results are machine-specific.

## Measured baseline

Warm setup first-frame latency was **1.31 ms median**, 0.95–1.80 ms across 20 launches. The measurement starts immediately before spawning the binary and stops when the setup heading arrives on a PTY. It includes loading a small persisted config, but excludes real terminal painting, cold filesystem caches, shell startup, and slow/network home directories. Earlier integration-test results around 16 ms have a 15 ms polling floor and should not be treated as the application's actual startup latency.

CPU samples ran for two seconds after startup and read user+system CPU from `/proc/<pid>/stat`. Percentages represent one CPU core, with approximately 0.5 percentage point resolution. Terminal output was continuously drained. Every launch used a temporary `TUI_TIMER_CONFIG`; no personal settings were changed.

- **100×30:** setup 1.0%, running timer 1.0%, paused timer 1.0%, stopwatch 0.5%; resident memory approximately 3.3 MiB.
- **240×70:** setup 2.0%, running timer 3.5%, paused timer 3.5%, stopwatch 3.0%; resident memory approximately 4.6 MiB.
- **400×120:** setup 4.5%, running timer 6.5%, paused timer 7.0%, stopwatch 7.0%; resident memory approximately 7.6 MiB.
- Static paused screens still emitted approximately 530–570 bytes/second. A running stopwatch emitted approximately 1.9 KB/s, 17 KB/s, and 44 KB/s at the three sizes.

These are quick diagnostic measurements, not statistical comparisons: do not infer a meaningful advantage from differences of one percentage point in a single short sample.

## Prioritized findings

### P1: Avoid continuous redraw of static screens

The event loop unconditionally calls `terminal.draw` before polling for 33 ms. Setup, paused, and completed screens therefore redraw around 30 times per second. Ratatui suppresses unchanged cells, but frame construction, string allocation, buffer diffing, and small terminal control writes still happen. The measured fullscreen paused CPU is the strongest reason to fix this.

Use event-driven rendering in static states: draw after input or resize, then wait for another event. Keep periodic rendering while a timer/stopwatch or completion animation is active. Handle resize events explicitly when removing unconditional drawing. Completion animation must have a finite duration and return to a static completed screen after expiry.

Validation: measure static CPU/output again at 400×120; verify resize, popup typing, pause/resume, restart, and confetti expiry repaint promptly. With a blocked event poll, input can remain immediate even when the static redraw interval is eliminated.

### P2: Large-digit allocation is a secondary optimization

`big_digits` builds nested strings for every glyph cell, collects intermediate vectors, joins rows, and clones each row for vertical scaling on every frame. The terminal buffer is also rebuilt/diffed across its full area. This cost is inferred from the code; allocation counts were not measured independently.

First remove unnecessary frames. Only consider cached digit geometry keyed by text/font/scale, or direct buffer-cell drawing, if profiling then shows a material residual cost. A cache needs a bound because stopwatch text continually changes. The existing scale cap of eight and small memory footprint do not justify a large rendering rewrite yet.

### P3: Improve the startup test's measurement

The existing integration helper reads for a fixed 15 ms before returning from each expectation. Its reported first-frame median primarily measures that polling interval. Preserve the behavioral tests, but use a readiness wait that returns immediately when the target text is decoded for performance claims. Report warm PTY first-frame latency separately from visible terminal startup.

## Timing correctness

The clock uses monotonic `Instant` differences, captures elapsed time when pausing, and carries overshoot across preparation/work/rest transitions. Delayed frames therefore do not accumulate countdown drift. The transition loop is bounded by at most 99 configured cycles. Existing unit coverage includes delayed Pomodoro frames and precise pause/preparation accounting. No clock-model correctness fix emerged from this review.

The 33 ms polling interval adds up to roughly one frame of normal completion notification latency, plus rendering/OS scheduling time; it does not alter the measured duration. Startup currently starts an autostart clock before terminal initialization, so preparation includes that small initialization interval.

## Defer unless requested

Adaptive timer redraw based on the next digit or progress-bar change could reduce active fullscreen CPU further. Maintain smooth hundredths/animations and deadline accuracy if pursuing this; a fixed one-second sleep would make UI input unresponsive unless the event poll still wakes immediately. This is lower priority than stopping static redraws.

## Implemented and verified

The static-redraw fix is implemented. Setup, paused, and completed screens block on
terminal events; keyboard input and resize wake them immediately. Timers and
stopwatches retain a 33 ms frame budget. Confetti animates for five seconds only,
then clears and returns to static rendering. It starts only on the final `Done`
transition, never on an intermediate Pomodoro break. Space dismisses it immediately.

The updated PTY suite measured **0.00% CPU and zero terminal output** over three
seconds for both setup and paused states at **400×120**, after settling a resize.
This means no measurable CPU ticks in that short sample, not a guarantee of
literally zero resource use. Baseline samples at that size were roughly 4.5–7%
of one core. Confetti expiry and dismissal also produced zero subsequent idle
output. Resize, pause/resume, preset dialogs, and terminal restoration passed.

The integration helper now returns immediately on matching decoded terminal text,
removing the old 15 ms polling floor. One complete-suite run measured **8.3 ms**
median over ten warm launches. This differs from the earlier 1.31 ms standalone
sample; these short runs occurred under different concurrent system loads and are
not a controlled startup regression comparison. Both exclude real terminal paint.
No startup-performance claim should be inferred from the old 16 ms figures.

Direct glyph-buffer rendering/caching remains deferred until profiling shows a
material residual need. No dependency was added for the bounded 100-particle effect.
