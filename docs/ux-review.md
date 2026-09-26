# UX review and research notes

Reviewed September 26, 2026. Scope: source and README inspection, existing PTY test coverage, and primary-source research. This is a heuristic review, not a user study or a claim of accessibility compliance. Findings describe the version at review start; implementation dispositions reflect the changes completed during this review.

The product direction is sound: immediately usable presets, large readable digits, visible contextual controls, and a compact setup screen suit the user's two-minute hanging routine. Keep those priorities ahead of productivity dashboards or account-based features.

## Obvious improvements implemented

1. **Paused digits were too faint.** The original foreground `(74,86,101)` against `(15,19,25)` measured approximately **2.49:1** using relative luminance. Retain the requested `||` badge while brightening the digits so the frozen time remains readable from across a room. **Disposition: implemented brighter paused digits.** W3C's contrast guidance provides useful design benchmarks of 4.5:1 for ordinary text and 3:1 for large text; terminal glyph dimensions and rendering vary, so this is guidance rather than a TUI conformance assertion. [W3C contrast guidance](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html)

2. **The paused footer contradicted the pause badge.** The badge said Space resumes, but the footer continued to say Space pauses. Derive both from the current state. **Disposition: implemented state-dependent footer.** Consistent, predictable keyboard behavior reduces the amount users need to remember. [W3C keyboard interface guidance](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/)

3. **Running controls clipped at the advertised minimum width.** The 36-column layout used footer lines wider than the available area. Critical controls should remain visible when resizing; use a compact three-line footer at narrow widths. **Disposition: implemented narrow footer.** Discoverability is especially relevant to the explicit requirement that keyboard bindings should not require memorization. [Command Line Interface Guidelines](https://clig.dev/)

4. **Sound labeling overpromised.** The app sends a terminal bell; whether it is audible depends on terminal configuration. Label it as a terminal bell rather than generic sound. **Disposition: implemented clearer label.** This recommendation follows the implementation's actual behavior, not a claim about every terminal.

5. **Quit discarded the opportunity to see save errors.** The quit handler called persistence and exited even when saving failed. Keep the app open with the existing error message when normal quit cannot save. Ctrl+C remains the explicit immediate exit. **Disposition: implemented.** This protects edits without introducing routine confirmation prompts.

6. **Reserved preset names produced misleading validation.** A name such as `timer` satisfies the displayed character rules but conflicts with CLI options. Explain the reserved-name restriction. **Disposition: implemented clearer validation.** Errors should explain how users can recover. [Command Line Interface Guidelines](https://clig.dev/)

## Confetti and theme guidance

The requested celebration fits the app if it remains optional and bounded. Use a brief, finite burst on completion, keep the zero-time display and controls readable, and allow ordinary restart, settings, and quit actions immediately. Never loop indefinitely or flash the whole screen. Save the preference with presets so `--hang` can celebrate while a focus preset stays quiet. **Disposition: implemented opt-in, five-second completion confetti and additional themes.**

W3C recommends that unnecessary motion can be disabled, and describes pause/stop controls for automatically moving content under its stated conditions. These web guidelines inform the design; they do not establish terminal accessibility compliance. [Animation from interactions](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html), [Pause, stop, hide](https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html)

Keep Work/Rest labels and the pause symbol when adding colors: phase must remain understandable without distinguishing hues. Preserve a monochrome option, and check every theme's normal text, selected row, paused digits, and modal text against its background. The preview is the right place to show a palette before starting. [W3C use of color](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html)

## Deferred improvements and decisions

These are notes for a future conversation, not features authorized for automatic expansion during this review.

- **Preserve unsaved preset edits when loading another preset.** Currently loading replaces the working setup, as documented. A small draft cache or explicit discard indication could make editing safer. Choose the interaction deliberately; a confirmation on every load would slow the primary flow.
- **Return from settings without resetting a running session.** Esc currently opens setup by resetting the clock. A future edit/resume distinction could help accidental Esc presses, but raises questions about applying changed durations and Pomodoro cycles mid-session. For now, document the reset behavior clearly.
- **Pomodoro long rests and final-rest policy.** Current cycles consistently include work plus rest, including the last rest. Optional longer breaks every N cycles or omitting the final rest are plausible additions, but increase setup complexity. Keep the existing policy visible and predictable until requested.
- **Skip the current Pomodoro phase.** Potentially useful after an interruption, but requires a decision about cycle counts, elapsed totals, and accidental activation. It should not reuse Vim navigation keys while running.
- **Terminal-native color preference and `NO_COLOR`.** Crossterm already honors `NO_COLOR` when emitting terminal styles. A separate terminal-default palette and deliberate override controls remain possible additions. The informal NO_COLOR convention permits explicit configuration and CLI arguments to override the environment. Define precedence and test both light and dark terminals before adding it. [NO_COLOR specification and FAQ](https://no-color.org/)
- **Plain-text accessibility mode.** Block, Slim, and Dots are visual digit renderings; screen-reader usefulness cannot be assumed. A plain-text time display with low-frequency announcements would need actual assistive-technology testing. Do not advertise screen-reader support based solely on keyboard access.
- **Desktop notifications, session restoration, history, and statistics.** These could help timers running out of sight, but add platform integration, storage, and product complexity. Keep them on the backlog rather than introducing background services or databases into the fast-launch app.

## Comparable apps and research implications

[caarlos0/timer](https://github.com/caarlos0/timer) presents a small command-line timer with duration input, progress, remaining time, and naming. Its restrained scope supports keeping the immediate launch path short. This is a comparison of documented features, not a measured speed comparison.

[lostf1sh/pomo](https://github.com/lostf1sh/pomo) documents large timer digits, automatic work/short-break/long-break cycles, visible help, multiple themes and theme previews, notifications, persisted sessions, and statistics. Theme preview and predictable controls fit this project now; its history, task tracking, and database-backed features illustrate a broader product direction to defer.

The official [Pomodoro Technique site](https://www.pomodorotechnique.com/) frames the method as a wider practice with training and tools. The application can provide configurable work/rest intervals without trying to reproduce an entire productivity system. Research here does not establish an optimal work duration or a health/productivity benefit for any particular interval.

## Suggested future verification

For subsequent visual changes, exercise 60×24 setup and 36×14 running layouts, all modes, paused and completed states, long durations, and terminal resizing. Check that every essential action remains visible, timer values do not disappear behind effects, and disabling animation yields a static completion screen. For preset changes, include failed writes and cancellation paths. Keep performance checks focused on first-frame latency, idle CPU, and responsive input; visual polish should not require continuous redraw when nothing is changing.
