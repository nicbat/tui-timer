# tui-timer

A small, keyboard-driven timer and stopwatch. Large responsive digits, a preparation
countdown, visible controls, and named presets. Written in Rust with Ratatui.

## Run

```sh
cargo run --release
cargo run --release -- --hang
```

For instant launches after compiling, run `./target/release/tui-timer` directly.
To install on your PATH:

```sh
cargo install --path . --locked
tui-timer --hang
```

Cargo installs into `~/.cargo/bin`; add that directory to PATH if needed.
Use your terminal emulator's fullscreen shortcut; digits resize automatically.

## Command line

```sh
tui-timer                            # remembered setup screen
tui-timer --hang                      # 5s preparation, then a full 2 minutes
tui-timer --timer 2m --countdown 5s
tui-timer --stopwatch --no-countdown
tui-timer --hang --timer 3m --theme amber
tui-timer --preset hang --size 2
tui-timer --list
tui-timer --config
tui-timer --help
```

Durations accept seconds (`120`), units (`1m30s`, `2h`), or clock notation (`2:00`).
Any run options launch immediately. Explicit options override preset values;
otherwise omitted options inherit your remembered settings. The preparation time
is separate from the timer duration. Stopwatch mode also supports preparation.
The optional terminal bell depends on your terminal's bell settings; it's off by default.
On completion, the timer holds at zero until you restart or leave.

## Controls and editable presets

Setup shows the controls at all times:

- **Up/Down or Tab:** select a setting; **Left/Right:** change it.
- **e:** type the selected duration or preparation time; Enter applies it.
- **Enter:** start; **q:** quit.
- **p:** save current settings as a named preset, e.g. `hang-long`.
- **l:** load a preset using arrows and Enter.

To customize a preset, load it with **l**, change its settings, then press **p**.
The existing name is prefilled: **Enter** updates it, or edit the name to save a copy.
Loading or running a preset does not overwrite its stored settings automatically.
Changes are saved to the preset only with **p**. Saving an existing name replaces it.
Every named preset is available as `--NAME` on your next launch.

While running: **Space** pauses/resumes, **r** restarts including preparation,
**Esc** returns to settings, **+/-** adjusts size (zero means automatic), **q** quits.
After `--hang`, press Esc to edit that preset and p to save it.
**Ctrl+C** exits anywhere and restores the terminal.

## Configuration

Linux: `~/.config/tui-timer/config.toml` (honors `XDG_CONFIG_HOME`).
On other platforms, the OS's standard user config directory is used.
The file is created when starting, saving, or quitting the app. `--config` prints its path.
For isolated development or testing:

```sh
TUI_TIMER_CONFIG=/tmp/my-timer.toml ./target/release/tui-timer
```

The TOML file contains `[settings]` and `[presets.hang]` sections with:
`stopwatch`, `seconds`, `prep`, `size`, `theme`, and `bell`.
Size: 0 (auto) or 1–8. Themes: 0 mint, 1 amber, 2 ice, 3 monochrome.
Timer: 1–359999 seconds; preparation: 0–3600 seconds.
Preset names start with a lowercase letter and contain lowercase letters, digits,
or hyphens, up to 32 characters; built-in CLI option names are reserved.
Edit or remove preset sections directly while the app is closed if desired.
Invalid configuration is reported rather than silently replaced.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
```

Timing uses a monotonic clock rather than counting render frames. Pauses preserve
fractional seconds; delayed frames carry elapsed preparation time into the timer.
The application is a single executable with no runtime downloads or network calls.

A Linux PTY integration test exercises preset editing, countdown, pause/resume,
completion, restart, stopwatch, resizing, and terminal-mode restoration. It also
captures terminal screenshots in `target/qa/` and measures launch-to-first-frame:

```sh
uv run --with pyte --with pillow scripts/test_terminal.py
```
