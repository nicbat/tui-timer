use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Gauge, Paragraph},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    stopwatch: bool,
    seconds: u64,
    prep: u64,
    size: u16,
    theme: usize,
    bell: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            stopwatch: false,
            seconds: 120,
            prep: 5,
            size: 0,
            theme: 0,
            bell: false,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    settings: Settings,
    presets: BTreeMap<String, Settings>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            presets: BTreeMap::from([("hang".into(), Settings::default())]),
        }
    }
}
fn config_path() -> Result<PathBuf, String> {
    if let Some(p) = env::var_os("TUI_TIMER_CONFIG") {
        return Ok(p.into());
    }
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("tui-timer/config.toml"))
        .ok_or("Cannot locate your config directory".into())
}
fn load(path: &PathBuf) -> Result<Config, String> {
    let mut config: Config = match fs::read_to_string(path) {
        Ok(s) => {
            toml::from_str(&s).map_err(|e| format!("Invalid config {}: {e}", path.display()))?
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Config::default(),
        Err(e) => return Err(e.to_string()),
    };
    validate(&mut config.settings)?;
    for (name, settings) in &mut config.presets {
        if !valid_name(name) {
            return Err(format!("Invalid or reserved preset name: {name}"));
        }
        validate(settings)?;
    }
    Ok(config)
}
fn validate(s: &mut Settings) -> Result<(), String> {
    if s.seconds == 0 || s.seconds > 359999 || s.prep > 3600 || s.size > 8 || s.theme > 3 {
        return Err("Config values out of range: timer 1s–99h59m59s, preparation 0–3600s, size 0–8, theme 0–3".into());
    }
    Ok(())
}
fn save(path: &PathBuf, config: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(
        &temp,
        toml::to_string_pretty(config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}
fn duration(s: &str) -> Result<u64, String> {
    let invalid =
        || format!("Invalid duration '{s}'. Use 120, 2m, 1m30s, or 2:00 (maximum 99h59m59s).");
    let value = if s.contains(':') {
        let parts: Vec<_> = s.split(':').collect();
        if !(2..=3).contains(&parts.len()) {
            return Err(invalid());
        }
        let mut total = 0u64;
        for (i, part) in parts.iter().enumerate() {
            let n = part.parse::<u64>().map_err(|_| invalid())?;
            if i > 0 && n >= 60 {
                return Err(invalid());
            }
            total = total
                .checked_mul(60)
                .and_then(|v| v.checked_add(n))
                .ok_or_else(invalid)?;
        }
        total
    } else if s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse::<u64>().map_err(|_| invalid())?
    } else {
        let mut total = 0u64;
        let mut digits = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                digits.push(c);
            } else {
                let factor = match c {
                    'h' => 3600,
                    'm' => 60,
                    's' => 1,
                    _ => return Err(invalid()),
                };
                let n = digits.parse::<u64>().map_err(|_| invalid())?;
                total = n
                    .checked_mul(factor)
                    .and_then(|v| v.checked_add(total))
                    .ok_or_else(invalid)?;
                digits.clear();
            }
        }
        if !digits.is_empty() {
            return Err(invalid());
        }
        total
    };
    if value > 359999 || s.is_empty() {
        Err(invalid())
    } else {
        Ok(value)
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && name.as_bytes()[0].is_ascii_lowercase()
        && ![
            "timer",
            "stopwatch",
            "countdown",
            "size",
            "theme",
            "bell",
            "no-bell",
            "help",
            "version",
            "list",
            "preset",
            "config",
            "no-countdown",
        ]
        .contains(&name)
}
fn args(config: &mut Config, argv: &[String]) -> Result<bool, String> {
    // Resolve the preset first so explicit options override it regardless of order.
    let mut preset = None;
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        match a.as_str() {
            "--preset" => {
                i += 1;
                preset = Some(argv.get(i).ok_or("--preset needs a name")?.clone());
            }
            "--timer" | "--countdown" | "--size" | "--theme" => {
                i += 1;
                if i >= argv.len() {
                    return Err(format!("{a} needs a value"));
                }
            }
            "--stopwatch" | "--bell" | "--no-bell" | "--no-countdown" => {}
            _ if a.starts_with("--") && config.presets.contains_key(&a[2..]) => {
                preset = Some(a[2..].into())
            }
            _ => return Err(format!("Unknown option '{a}'. Use --help or --list.")),
        }
        i += 1;
    }
    if let Some(name) = preset {
        config.settings = config
            .presets
            .get(&name)
            .ok_or(format!("No preset named '{name}'"))?
            .clone();
    }
    i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--timer" => {
                i += 1;
                config.settings.seconds = duration(&argv[i])?;
                config.settings.stopwatch = false;
            }
            "--stopwatch" => config.settings.stopwatch = true,
            "--countdown" => {
                i += 1;
                config.settings.prep = duration(&argv[i])?;
            }
            "--no-countdown" => config.settings.prep = 0,
            "--size" => {
                i += 1;
                config.settings.size = if argv[i] == "auto" {
                    0
                } else {
                    argv[i].parse().map_err(|_| "Size must be auto or 1–8")?
                };
            }
            "--theme" => {
                i += 1;
                config.settings.theme = ["mint", "amber", "ice", "mono"]
                    .iter()
                    .position(|t| *t == argv[i])
                    .ok_or("Theme must be mint, amber, ice, or mono")?;
            }
            "--bell" => config.settings.bell = true,
            "--no-bell" => config.settings.bell = false,
            "--preset" => i += 1,
            _ => {}
        }
        i += 1;
    }
    validate(&mut config.settings)?;
    Ok(!argv.is_empty())
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Setup,
    Prep,
    Running,
    Done,
}
struct Clock {
    phase: Phase,
    elapsed: Duration,
    anchor: Instant,
    paused: bool,
}
impl Clock {
    fn new() -> Self {
        Self {
            phase: Phase::Setup,
            elapsed: Duration::ZERO,
            anchor: Instant::now(),
            paused: false,
        }
    }
    fn start(&mut self, s: &Settings, now: Instant) {
        self.phase = if s.prep > 0 {
            Phase::Prep
        } else {
            Phase::Running
        };
        self.elapsed = Duration::ZERO;
        self.anchor = now;
        self.paused = false;
    }
    fn elapsed_at(&self, now: Instant) -> Duration {
        self.elapsed
            + if self.paused {
                Duration::ZERO
            } else {
                now.saturating_duration_since(self.anchor)
            }
    }
    fn toggle(&mut self, now: Instant) {
        self.elapsed = self.elapsed_at(now);
        self.anchor = now;
        self.paused = !self.paused;
    }
    fn tick(&mut self, s: &Settings, now: Instant) -> bool {
        if self.paused {
            return false;
        }
        if self.phase == Phase::Prep && self.elapsed_at(now) >= Duration::from_secs(s.prep) {
            self.elapsed = self.elapsed_at(now) - Duration::from_secs(s.prep);
            self.anchor = now;
            self.phase = Phase::Running;
        }
        if self.phase == Phase::Running
            && !s.stopwatch
            && self.elapsed_at(now) >= Duration::from_secs(s.seconds)
        {
            self.phase = Phase::Done;
            self.elapsed = Duration::from_secs(s.seconds);
            self.paused = true;
            return true;
        }
        false
    }
}
struct App {
    config: Config,
    path: PathBuf,
    clock: Clock,
    selected: usize,
    input: Option<(bool, String)>,
    message: String,
    presets: bool,
    preset_index: usize,
    preset_name: Option<String>,
}
impl App {
    fn persist(&mut self) -> bool {
        match save(&self.path, &self.config) {
            Ok(()) => true,
            Err(e) => {
                self.message = format!("Couldn't save settings: {e}");
                false
            }
        }
    }
    fn key(&mut self, key: KeyCode) -> bool {
        if let Some((naming, mut value)) = self.input.take() {
            match key {
                KeyCode::Esc => return false,
                KeyCode::Enter => {
                    if naming {
                        if !valid_name(&value) {
                            self.message = "Use a–z, 0–9, hyphens; start with a letter. Reserved names unavailable.".into();
                        } else {
                            self.config
                                .presets
                                .insert(value.clone(), self.config.settings.clone());
                            if self.persist() {
                                self.preset_name = Some(value.clone());
                                self.message = format!("Saved. Launch with tui-timer --{value}");
                            }
                            return false;
                        }
                    } else {
                        match duration(&value) {
                            Ok(n)
                                if (self.selected == 1 && n > 0)
                                    || (self.selected == 2 && n <= 3600) =>
                            {
                                if self.selected == 1 {
                                    self.config.settings.seconds = n;
                                } else {
                                    self.config.settings.prep = n;
                                }
                                self.message.clear();
                                return false;
                            }
                            _ => {
                                self.message =
                                    "Enter a valid duration (timer > 0; preparation ≤ 1h).".into()
                            }
                        }
                    }
                }
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(c) if value.len() < 32 => value.push(c),
                _ => {}
            }
            self.input = Some((naming, value));
            return false;
        }
        if self.presets {
            let len = self.config.presets.len();
            match key {
                KeyCode::Esc | KeyCode::Char('l') => self.presets = false,
                KeyCode::Down => self.preset_index = (self.preset_index + 1) % len.max(1),
                KeyCode::Up => {
                    self.preset_index = (self.preset_index + len.max(1) - 1) % len.max(1)
                }
                KeyCode::Enter => {
                    if let Some((name, s)) = self.config.presets.iter().nth(self.preset_index) {
                        self.config.settings = s.clone();
                        self.preset_name = Some(name.clone());
                        self.message = format!("Editing --{name} · p saves changes · Enter starts");
                    }
                    self.presets = false;
                }
                _ => {}
            }
            return false;
        }
        if key == KeyCode::Char('q') {
            self.persist();
            return true;
        }
        if self.clock.phase == Phase::Setup {
            match key {
                KeyCode::Down | KeyCode::Tab => self.selected = (self.selected + 1) % 6,
                KeyCode::Up | KeyCode::BackTab => self.selected = (self.selected + 5) % 6,
                KeyCode::Left | KeyCode::Right => {
                    let plus = key == KeyCode::Right;
                    let s = &mut self.config.settings;
                    match self.selected {
                        0 => s.stopwatch = !s.stopwatch,
                        1 => {
                            s.seconds = if plus {
                                (s.seconds + 15).min(359999)
                            } else {
                                s.seconds.saturating_sub(15).max(1)
                            }
                        }
                        2 => {
                            s.prep = if plus {
                                (s.prep + 1).min(3600)
                            } else {
                                s.prep.saturating_sub(1)
                            }
                        }
                        3 => s.size = (s.size + if plus { 1 } else { 8 }) % 9,
                        4 => s.theme = (s.theme + if plus { 1 } else { 3 }) % 4,
                        5 => s.bell = !s.bell,
                        _ => {}
                    }
                }
                KeyCode::Char('e') if self.selected == 1 || self.selected == 2 => {
                    self.input = Some((false, String::new()))
                }
                KeyCode::Char('p') => {
                    self.message.clear();
                    self.input = Some((true, self.preset_name.clone().unwrap_or_default()));
                }
                KeyCode::Char('l') => {
                    self.presets = true;
                    self.preset_index = 0;
                }
                KeyCode::Enter => {
                    self.message.clear();
                    if self.persist() {
                        self.clock.start(&self.config.settings, Instant::now());
                    }
                }
                _ => {}
            }
        } else {
            match key {
                KeyCode::Char(' ') if self.clock.phase != Phase::Done => {
                    self.clock.toggle(Instant::now())
                }
                KeyCode::Char('r') | KeyCode::Enter
                    if self.clock.phase == Phase::Done || key == KeyCode::Char('r') =>
                {
                    self.clock.start(&self.config.settings, Instant::now())
                }
                KeyCode::Esc => {
                    self.clock = Clock::new();
                    self.message.clear();
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.config.settings.size = (self.config.settings.size + 1).min(8)
                }
                KeyCode::Char('-') => {
                    self.config.settings.size = self.config.settings.size.saturating_sub(1)
                }
                _ => {}
            }
        }
        false
    }
}
const DIGITS: [[&str; 5]; 11] = [
    ["111", "101", "101", "101", "111"],
    ["010", "110", "010", "010", "111"],
    ["111", "001", "111", "100", "111"],
    ["111", "001", "111", "001", "111"],
    ["101", "101", "111", "001", "001"],
    ["111", "100", "111", "001", "111"],
    ["111", "100", "111", "101", "111"],
    ["111", "001", "010", "010", "010"],
    ["111", "101", "111", "101", "111"],
    ["111", "101", "111", "001", "111"],
    ["0", "1", "0", "1", "0"],
];
fn display_time(seconds: u64) -> String {
    if seconds >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}
fn big_digits(f: &mut Frame, area: Rect, text: &str, requested: u16, color: Color) {
    let patterns: Vec<_> = text
        .chars()
        .map(|c| &DIGITS[c.to_digit(10).map(|n| n as usize).unwrap_or(10)])
        .collect();
    let units = patterns.iter().map(|p| p[0].len() as u16).sum::<u16>() + patterns.len() as u16 - 1;
    let fit = (area.width / (units * 2)).min(area.height / 5).min(8);
    if fit == 0 {
        f.render_widget(
            Paragraph::new(text)
                .centered()
                .style(Style::default().fg(color).bold()),
            Rect {
                y: area.y + area.height / 2,
                height: area.height.min(1),
                ..area
            },
        );
        return;
    }
    let scale = if requested == 0 {
        fit
    } else {
        requested.min(fit)
    };
    let mut lines = Vec::new();
    for row in 0..5 {
        let line = patterns
            .iter()
            .map(|p| {
                p[row]
                    .chars()
                    .map(|c| if c == '1' { "█" } else { " " }.repeat((scale * 2) as usize))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(&" ".repeat((scale * 2) as usize));
        for _ in 0..scale {
            lines.push(Line::from(line.clone()));
        }
    }
    f.render_widget(
        Paragraph::new(lines).centered().style(color),
        Rect {
            y: area.y + (area.height - 5 * scale) / 2,
            height: 5 * scale,
            ..area
        },
    );
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
fn ui(f: &mut Frame, app: &App) {
    let area = f.area();
    let s = &app.config.settings;
    let accent = [
        Color::Rgb(116, 224, 181),
        Color::Rgb(245, 192, 106),
        Color::Rgb(125, 194, 245),
        Color::White,
    ][s.theme];
    let muted = Color::Rgb(135, 146, 160);
    let bg = Color::Rgb(15, 19, 25);
    f.render_widget(
        Block::default().style(Style::default().bg(bg).fg(Color::Rgb(222, 229, 237))),
        area,
    );
    let (min_width, min_height) = if app.clock.phase == Phase::Setup {
        (60, 18)
    } else {
        (36, 12)
    };
    if area.width < min_width || area.height < min_height {
        f.render_widget(
            Paragraph::new(format!(
                "Enlarge terminal to {min_width} × {min_height}\nq quit · Esc settings"
            ))
            .centered(),
            centered(area, area.width, 2),
        );
        return;
    }
    if app.clock.phase == Phase::Setup {
        let panel = centered(area, 60, 22);
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(8),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(panel);
        f.render_widget(
            Paragraph::new("T I M E R")
                .centered()
                .style(Style::default().fg(accent).bold()),
            rows[0],
        );
        f.render_widget(
            Paragraph::new(
                app.preset_name
                    .as_ref()
                    .map(|name| format!("Preset: --{name} · p to save edits"))
                    .unwrap_or_else(|| "Make a little room for one thing.".into()),
            )
            .centered()
            .style(muted),
            rows[1],
        );
        let values = [
            if s.stopwatch {
                "Stopwatch".into()
            } else {
                "Timer".into()
            },
            display_time(s.seconds),
            if s.prep == 0 {
                "Off".into()
            } else {
                format!("{} seconds", s.prep)
            },
            if s.size == 0 {
                "Auto · fill terminal".into()
            } else {
                format!("{} (clamped to fit)", s.size)
            },
            ["Mint", "Amber", "Ice", "Mono"][s.theme].into(),
            if s.bell {
                "On · terminal bell".into()
            } else {
                "Off".into()
            },
        ];
        let labels = [
            "Mode",
            "Duration",
            "Get ready",
            "Digit size",
            "Color",
            "Finish sound",
        ];
        let lines: Vec<Line> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                Line::from(format!(
                    " {}  {:<15} {}",
                    if app.selected == i { "›" } else { " " },
                    label,
                    values[i]
                ))
                .style(if app.selected == i {
                    Style::default().fg(accent).bold()
                } else {
                    Style::default().fg(muted)
                })
            })
            .collect();
        f.render_widget(Paragraph::new(lines), rows[2]);
        f.render_widget(
            Paragraph::new("[ Enter to start ]")
                .centered()
                .style(accent),
            rows[3],
        );
        f.render_widget(
            Paragraph::new(
                "↑↓ select   ←→ change   e type duration\np save preset   l load preset   q quit",
            )
            .centered()
            .style(muted),
            rows[4],
        );
    } else {
        let now = Instant::now();
        let elapsed = app.clock.elapsed_at(now);
        let (label, value) = match app.clock.phase {
            Phase::Prep => (
                "GET READY",
                Duration::from_secs(s.prep)
                    .saturating_sub(elapsed)
                    .as_secs_f64()
                    .ceil() as u64,
            ),
            Phase::Done => ("COMPLETE", 0),
            _ if s.stopwatch => ("STOPWATCH", elapsed.as_secs()),
            _ => (
                "TIME REMAINING",
                Duration::from_secs(s.seconds)
                    .saturating_sub(elapsed)
                    .as_secs_f64()
                    .ceil() as u64,
            ),
        };
        let rows = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(area);
        f.render_widget(
            Paragraph::new(if app.clock.paused && app.clock.phase != Phase::Done {
                "P A U S E D"
            } else {
                label
            })
            .centered()
            .style(accent),
            rows[0],
        );
        let text = if app.clock.phase == Phase::Prep {
            value.to_string()
        } else {
            display_time(value)
        };
        big_digits(f, rows[1], &text, s.size, accent);
        if !s.stopwatch && app.clock.phase != Phase::Prep {
            let bar = centered(rows[2], area.width.saturating_sub(8).min(90), 1);
            f.render_widget(
                Gauge::default()
                    .ratio((elapsed.as_secs_f64() / s.seconds as f64).clamp(0.0, 1.0))
                    .gauge_style(Style::default().fg(accent).bg(Color::Rgb(34, 43, 53)))
                    .label(""),
                bar,
            );
        }
        f.render_widget(
            Paragraph::new(if app.clock.phase == Phase::Done {
                "Enter restart   Esc settings   q quit"
            } else {
                if area.width < 68 {
                    "Space pause   r restart   q quit\nEsc settings   +/− size"
                } else {
                    "Space pause/resume   r restart   Esc settings   +/− size   q quit"
                }
            })
            .centered()
            .style(muted),
            rows[3],
        );
    }
    if !app.message.is_empty() {
        f.render_widget(
            Paragraph::new(app.message.as_str())
                .centered()
                .style(accent),
            Rect::new(area.x, area.bottom() - 1, area.width, 1),
        );
    }
    if let Some((naming, value)) = &app.input {
        let popup = centered(area, 58, 7);
        f.render_widget(Clear, popup);
        f.render_widget(
            Paragraph::new(format!(
                "\n {value}▏\n\n Enter save · Esc cancel{}",
                if *naming {
                    " · existing name replaces"
                } else {
                    " · e.g. 2m or 1:30"
                }
            ))
            .block(Block::default().borders(Borders::ALL).title(if *naming {
                " Save named preset "
            } else {
                " Type duration "
            }))
            .style(Style::default().bg(bg).fg(accent)),
            popup,
        );
    }
    if app.presets {
        let popup = centered(
            area,
            58,
            (app.config.presets.len() as u16 + 4).min(area.height),
        );
        f.render_widget(Clear, popup);
        let capacity = popup.height.saturating_sub(3) as usize;
        let offset = app.preset_index.saturating_sub(capacity.saturating_sub(1));
        let lines = app
            .config
            .presets
            .keys()
            .enumerate()
            .skip(offset)
            .take(capacity)
            .map(|(i, n)| {
                Line::from(format!(
                    " {} --{n}",
                    if i == app.preset_index { "›" } else { " " }
                ))
                .style(if i == app.preset_index { accent } else { muted })
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Presets · ↑↓ select · Enter load · Esc close "),
                )
                .style(Style::default().bg(bg)),
            popup,
        );
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("tui-timer: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let argv: Vec<String> = env::args().skip(1).collect();
    if argv.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "tui-timer — a little room for one thing\n\nUSAGE\n  tui-timer                     Open remembered setup\n  tui-timer --timer 2m           Start a timer\n  tui-timer --hang               Start a saved preset\n  tui-timer --stopwatch          Start a stopwatch\n\nOPTIONS\n  --countdown 5s                 Preparation before timing\n  --no-countdown                 Start immediately\n  --size auto|1–8                Digit size (always clamped to fit)\n  --theme mint|amber|ice|mono     Accent color\n  --bell / --no-bell             Terminal bell at completion\n  --preset NAME                 Start a saved preset\n  --list                        List presets\n  --config                      Print config path\n  --version                     Print version\n\nIn setup: arrows change settings, e types a duration, p saves a named\npreset (available as --NAME), l loads one, Enter starts.\nWhile running: Space pauses, r restarts, Esc opens setup, q quits.\nSettings live in your OS config directory; TUI_TIMER_CONFIG overrides\nthe file path. Use your terminal's fullscreen shortcut for fullscreen."
        );
        return Ok(());
    }
    if argv == ["--version"] {
        println!("tui-timer {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let path = config_path()?;
    if argv == ["--config"] {
        println!("{}", path.display());
        return Ok(());
    }
    let mut config = load(&path)?;
    if argv == ["--list"] {
        for (name, s) in &config.presets {
            println!(
                "--{name:<18} {} · {} · {}s preparation",
                if s.stopwatch { "stopwatch" } else { "timer" },
                display_time(s.seconds),
                s.prep
            );
        }
        return Ok(());
    }
    let autostart = args(&mut config, &argv)?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Run in an interactive terminal (or use --help).".into());
    }
    let mut app = App {
        config,
        path,
        clock: Clock::new(),
        selected: 0,
        input: None,
        message: String::new(),
        presets: false,
        preset_index: 0,
        preset_name: None,
    };
    app.preset_name = argv.iter().rev().find_map(|arg| {
        arg.strip_prefix("--")
            .filter(|name| app.config.presets.contains_key(*name))
            .map(String::from)
    });
    if let Some(index) = argv.iter().position(|arg| arg == "--preset") {
        app.preset_name = argv.get(index + 1).cloned();
    }
    if autostart {
        save(&app.path, &app.config)?;
        app.clock.start(&app.config.settings, Instant::now());
    }
    let mut terminal = ratatui::init();
    let result = (|| -> io::Result<()> {
        loop {
            if app.clock.tick(&app.config.settings, Instant::now()) && app.config.settings.bell {
                print!("\x07");
                io::stdout().flush()?;
            }
            terminal.draw(|f| ui(f, &app))?;
            if event::poll(Duration::from_millis(33))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.persist();
                    break;
                }
                if app.key(key.code) {
                    break;
                }
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result.map_err(|e| e.to_string())?;
    if app.message.starts_with("Couldn't save") {
        return Err(app.message);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations() {
        for (s, n) in [
            ("2m", 120),
            ("1m30s", 90),
            ("2:00", 120),
            ("1:02:03", 3723),
            ("0", 0),
        ] {
            assert_eq!(duration(s).unwrap(), n);
        }
        for s in ["", "-1", "1:99", "2m3", "999999999999999999999h", "100h"] {
            assert!(duration(s).is_err(), "{s}");
        }
    }
    #[test]
    fn preparation_and_pause_are_precise() {
        let s = Settings::default();
        let now = Instant::now();
        let mut c = Clock::new();
        c.start(&s, now);
        c.tick(&s, now + Duration::from_millis(5500));
        assert_eq!(c.phase, Phase::Running);
        assert_eq!(
            c.elapsed_at(now + Duration::from_millis(5500)),
            Duration::from_millis(500)
        );
        c.toggle(now + Duration::from_secs(6));
        assert_eq!(
            c.elapsed_at(now + Duration::from_secs(100)),
            Duration::from_secs(1)
        );
        c.toggle(now + Duration::from_secs(100));
        assert!(!c.tick(&s, now + Duration::from_secs(218)));
        assert!(c.tick(&s, now + Duration::from_secs(219)));
        assert!(!c.tick(&s, now + Duration::from_secs(220)));
    }
    #[test]
    fn paused_preparation_and_stopwatch() {
        let s = Settings {
            stopwatch: true,
            ..Settings::default()
        };
        let now = Instant::now();
        let mut c = Clock::new();
        c.start(&s, now);
        c.toggle(now + Duration::from_secs(2));
        c.tick(&s, now + Duration::from_secs(20));
        assert_eq!(c.phase, Phase::Prep);
        c.toggle(now + Duration::from_secs(20));
        c.tick(&s, now + Duration::from_secs(23));
        assert_eq!(c.phase, Phase::Running);
        assert!(!c.tick(&s, now + Duration::from_secs(1000)));
    }
    #[test]
    fn preset_overrides_and_errors() {
        let mut c = Config::default();
        let a = ["--timer", "30s", "--hang", "--no-countdown"].map(String::from);
        assert!(args(&mut c, &a).unwrap());
        assert_eq!(c.settings.seconds, 30);
        assert_eq!(c.settings.prep, 0);
        assert!(args(&mut c, &["--timer".into()]).is_err());
        assert!(args(&mut c, &["--timer".into(), "0".into()]).is_err());
        assert!(!valid_name("timer"));
        assert!(valid_name("hang-long"));
    }
    #[test]
    fn config_roundtrip() {
        let c = Config::default();
        let restored: Config = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert_eq!(restored.presets["hang"].seconds, 120);
    }
    #[test]
    fn layouts_fit() {
        for (w, h) in [(20, 8), (36, 12), (80, 24), (160, 48), (300, 100)] {
            let backend = ratatui::backend::TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = App {
                config: Config::default(),
                path: PathBuf::new(),
                clock: Clock::new(),
                selected: 0,
                input: None,
                message: String::new(),
                presets: false,
                preset_index: 0,
                preset_name: None,
            };
            for phase in [Phase::Setup, Phase::Prep, Phase::Running, Phase::Done] {
                app.clock.phase = phase;
                terminal.draw(|f| ui(f, &app)).unwrap();
            }
        }
    }
}
