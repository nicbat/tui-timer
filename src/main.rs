mod effects;
#[cfg(feature = "metrics")]
mod metrics;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use effects::{CONFETTI_DURATION, THEMES};
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    stopwatch: bool,
    hundredths: bool,
    seconds: u64,
    prep: u64,
    size: u16,
    theme: usize,
    bell: bool,
    confetti: bool,
    pomodoro: bool,
    work_seconds: u64,
    rest_seconds: u64,
    cycles: u16,
    font: usize,
    long_rest_seconds: u64,
    long_rest_every: u16,
    final_rest: bool,
    color_mode: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            stopwatch: false,
            hundredths: true,
            seconds: 120,
            prep: 5,
            size: 0,
            theme: 0,
            bell: false,
            confetti: false,
            pomodoro: false,
            work_seconds: 1500,
            rest_seconds: 300,
            cycles: 4,
            font: 0,
            long_rest_seconds: 900,
            long_rest_every: 0,
            final_rest: true,
            color_mode: 0,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    settings: Settings,
    presets: BTreeMap<String, Settings>,
    drafts: BTreeMap<String, Settings>,
    scratch: Option<Settings>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            presets: BTreeMap::from([("hang".into(), Settings::default())]),
            drafts: BTreeMap::new(),
            scratch: None,
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
    for settings in config.drafts.values_mut() {
        validate(settings)?;
    }
    if let Some(scratch) = &mut config.scratch {
        validate(scratch)?;
    }
    config
        .drafts
        .retain(|name, _| config.presets.contains_key(name));
    Ok(config)
}
fn validate(s: &mut Settings) -> Result<(), String> {
    if s.seconds == 0
        || s.seconds > 359999
        || s.prep > 3600
        || s.size > 8
        || s.theme >= THEMES.len()
        || s.work_seconds == 0
        || s.work_seconds > 359999
        || s.rest_seconds == 0
        || s.rest_seconds > 359999
        || !(1..=99).contains(&s.cycles)
        || s.long_rest_seconds == 0
        || s.long_rest_seconds > 359999
        || s.long_rest_every > 99
        || s.color_mode > 2
        || s.font > 2
        || (s.stopwatch && s.pomodoro)
    {
        return Err("Config values out of range: durations 1s–99h59m59s, get ready 0–3600s, cycles 1–99, size 0–8, theme 0–10, font 0–2, long-rest interval 0–99, color mode 0–2; choose one mode".into());
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
            "pomodoro",
            "work",
            "rest",
            "cycles",
            "font",
            "long-rest",
            "long-rest-every",
            "final-rest",
            "no-final-rest",
            "color",
            "timer",
            "stopwatch",
            "hundredths",
            "no-hundredths",
            "countdown",
            "size",
            "theme",
            "bell",
            "confetti",
            "no-confetti",
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
            "--timer" | "--countdown" | "--size" | "--theme" | "--work" | "--rest" | "--cycles"
            | "--font" | "--long-rest" | "--long-rest-every" | "--color" => {
                i += 1;
                if i >= argv.len() {
                    return Err(format!("{a} needs a value"));
                }
            }
            "--pomodoro" | "--stopwatch" | "--bell" | "--no-bell" | "--no-countdown"
            | "--hundredths" | "--no-hundredths" | "--confetti" | "--no-confetti"
            | "--final-rest" | "--no-final-rest" => {}
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
                config.settings.pomodoro = false;
            }
            "--hundredths" => config.settings.hundredths = true,
            "--no-hundredths" => config.settings.hundredths = false,
            "--stopwatch" => {
                config.settings.stopwatch = true;
                config.settings.pomodoro = false;
            }
            "--pomodoro" => {
                config.settings.pomodoro = true;
                config.settings.stopwatch = false;
            }
            "--work" => {
                i += 1;
                config.settings.work_seconds = duration(&argv[i])?;
            }
            "--rest" => {
                i += 1;
                config.settings.rest_seconds = duration(&argv[i])?;
            }
            "--cycles" => {
                i += 1;
                config.settings.cycles = argv[i].parse().map_err(|_| "Cycles must be 1–99")?;
            }
            "--long-rest" => {
                i += 1;
                config.settings.long_rest_seconds = duration(&argv[i])?;
            }
            "--long-rest-every" => {
                i += 1;
                config.settings.long_rest_every = argv[i]
                    .parse()
                    .map_err(|_| "Long-rest interval must be 0–99")?;
            }
            "--final-rest" => config.settings.final_rest = true,
            "--no-final-rest" => config.settings.final_rest = false,
            "--color" => {
                i += 1;
                config.settings.color_mode = ["auto", "always", "never"]
                    .iter()
                    .position(|v| *v == argv[i])
                    .ok_or("Color must be auto, always, or never")?;
            }
            "--font" => {
                i += 1;
                config.settings.font = ["block", "slim", "dots"]
                    .iter()
                    .position(|font| *font == argv[i])
                    .ok_or("Font must be block, slim, or dots")?;
            }
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
                config.settings.theme =
                    THEMES
                        .iter()
                        .position(|t| t.name == argv[i])
                        .ok_or_else(|| {
                            format!(
                                "Theme must be one of: {}",
                                THEMES.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
                            )
                        })?;
            }
            "--confetti" => config.settings.confetti = true,
            "--no-confetti" => config.settings.confetti = false,
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

impl Settings {
    fn rest_for(&self, cycle: u16) -> u64 {
        if cycle == self.cycles && !self.final_rest {
            0
        } else if self.long_rest_every > 0 && cycle.is_multiple_of(self.long_rest_every) {
            self.long_rest_seconds
        } else {
            self.rest_seconds
        }
    }
    fn mode_name(&self) -> &'static str {
        if self.pomodoro {
            "Pomodoro"
        } else if self.stopwatch {
            "Stopwatch"
        } else {
            "Timer"
        }
    }
    fn duration(&self) -> u64 {
        if self.pomodoro {
            self.work_seconds
        } else {
            self.seconds
        }
    }
    fn summary(&self) -> String {
        if self.pomodoro {
            format!(
                "{} work / {} rest × {}",
                display_time(self.work_seconds),
                display_time(self.rest_seconds),
                self.cycles
            )
        } else if self.stopwatch {
            "count up".into()
        } else {
            display_time(self.seconds)
        }
    }
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Setup,
    Prep,
    Running,
    Rest,
    Done,
}
#[derive(Clone)]
struct Clock {
    phase: Phase,
    elapsed: Duration,
    anchor: Instant,
    paused: bool,
    cycle: u16,
}
impl Clock {
    fn new() -> Self {
        Self {
            phase: Phase::Setup,
            elapsed: Duration::ZERO,
            anchor: Instant::now(),
            paused: false,
            cycle: 1,
        }
    }
    fn start(&mut self, s: &Settings, now: Instant) {
        *self = Self::new();
        self.phase = if s.prep > 0 {
            Phase::Prep
        } else {
            Phase::Running
        };
        self.anchor = now;
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
    fn limit(&self, s: &Settings) -> u64 {
        match self.phase {
            Phase::Prep => s.prep,
            Phase::Rest => s.rest_for(self.cycle),
            _ => s.duration(),
        }
    }
    fn skip(&mut self, s: &Settings, now: Instant) -> bool {
        if !s.pomodoro || !matches!(self.phase, Phase::Running | Phase::Rest) {
            return false;
        }
        let was_paused = self.paused;
        self.elapsed = Duration::from_secs(self.limit(s));
        self.anchor = now;
        self.paused = false;
        let changed = self.tick(s, now);
        if self.phase != Phase::Done {
            self.paused = was_paused;
        }
        changed
    }
    fn tick(&mut self, s: &Settings, now: Instant) -> bool {
        if self.paused || matches!(self.phase, Phase::Setup | Phase::Done) {
            return false;
        }
        let mut transitioned = false;
        loop {
            if self.phase == Phase::Running && s.stopwatch {
                break;
            }
            let limit = Duration::from_secs(self.limit(s));
            let elapsed = self.elapsed_at(now);
            if elapsed < limit {
                break;
            }
            self.elapsed = elapsed - limit;
            self.anchor = now;
            match self.phase {
                Phase::Prep => self.phase = Phase::Running,
                Phase::Running if s.pomodoro && s.rest_for(self.cycle) > 0 => {
                    self.phase = Phase::Rest;
                    transitioned = true;
                }
                Phase::Rest if self.cycle < s.cycles => {
                    self.cycle += 1;
                    self.phase = Phase::Running;
                    transitioned = true;
                }
                _ => {
                    self.phase = Phase::Done;
                    self.elapsed = limit;
                    self.paused = true;
                    return true;
                }
            }
        }
        transitioned
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Field {
    Mode,
    Hundredths,
    Duration,
    Rest,
    Cycles,
    LongRest,
    LongEvery,
    FinalRest,
    ColorMode,
    Prep,
    Font,
    Size,
    Theme,
    Bell,
    Confetti,
}
#[derive(Clone)]
enum Edit {
    Duration(Field),
    NewPreset,
    Rename(String),
}
struct Input {
    kind: Edit,
    value: String,
    fresh: bool,
}
struct App {
    config: Config,
    path: PathBuf,
    clock: Clock,
    selected: usize,
    input: Option<Input>,
    message: String,
    presets: bool,
    preset_index: usize,
    preset_name: Option<String>,
    delete: Option<String>,
    rendered_scale: u16,
    max_scale: u16,
    celebration: Option<Instant>,
    suspended: Option<(Clock, Settings, Option<String>)>,
    no_color: bool,
}
impl App {
    fn new(config: Config, path: PathBuf) -> Self {
        Self {
            config,
            path,
            clock: Clock::new(),
            selected: 0,
            input: None,
            message: String::new(),
            presets: false,
            preset_index: 0,
            preset_name: None,
            delete: None,
            rendered_scale: 1,
            max_scale: 8,
            celebration: None,
            suspended: None,
            no_color: env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()),
        }
    }
    fn stash_draft(&mut self) {
        if self.clock.phase != Phase::Setup {
            return;
        }
        if let Some(name) = &self.preset_name {
            if self.config.presets.get(name) != Some(&self.config.settings) {
                self.config
                    .drafts
                    .insert(name.clone(), self.config.settings.clone());
            } else {
                self.config.drafts.remove(name);
            }
        } else {
            self.config.scratch = Some(self.config.settings.clone());
        }
    }
    fn open_settings(&mut self, now: Instant) {
        if self.clock.phase != Phase::Done {
            if !self.clock.paused {
                self.clock.toggle(now);
            }
            self.suspended = Some((
                self.clock.clone(),
                self.config.settings.clone(),
                self.preset_name.clone(),
            ));
        }
        self.clock = Clock::new();
        if let Some(name) = &self.preset_name
            && let Some(draft) = self.config.drafts.get(name)
        {
            self.config.settings = draft.clone();
        }
        self.selected = 0;
        self.celebration = None;
        self.message = if self.suspended.is_some() {
            "Session paused. Esc returns; Enter starts new.".into()
        } else {
            String::new()
        };
    }
    fn return_to_session(&mut self) {
        if let Some((clock, settings, name)) = self.suspended.take() {
            self.stash_draft();
            self.clock = clock;
            self.config.settings = settings;
            self.preset_name = name;
            self.message.clear();
        }
    }
    fn advance(&mut self, now: Instant) -> bool {
        let before = self.clock.phase;
        let transitioned = self.clock.tick(&self.config.settings, now);
        if before != Phase::Done && self.clock.phase == Phase::Done && self.config.settings.confetti
        {
            self.celebration = Some(now);
        } else if self.clock.phase != Phase::Done
            || self
                .celebration
                .is_some_and(|start| now.saturating_duration_since(start) >= CONFETTI_DURATION)
        {
            self.celebration = None;
        }
        transitioned
    }
    fn animating(&self, now: Instant) -> bool {
        (!self.clock.paused
            && matches!(self.clock.phase, Phase::Prep | Phase::Running | Phase::Rest))
            || (self.clock.phase == Phase::Done
                && self
                    .celebration
                    .is_some_and(|start| now.saturating_duration_since(start) < CONFETTI_DURATION))
    }
    fn fields(&self) -> Vec<Field> {
        let mut fields = vec![Field::Mode];
        if self.config.settings.stopwatch {
            fields.push(Field::Hundredths);
        }
        if !self.config.settings.stopwatch {
            fields.push(Field::Duration);
        }
        if self.config.settings.pomodoro {
            fields.extend([
                Field::Rest,
                Field::Cycles,
                Field::LongEvery,
                Field::LongRest,
                Field::FinalRest,
            ]);
        }
        fields.extend([
            Field::Prep,
            Field::Font,
            Field::Size,
            Field::Theme,
            Field::ColorMode,
            Field::Bell,
            Field::Confetti,
        ]);
        fields
    }
    fn persist(&mut self) -> bool {
        self.stash_draft();
        match save(&self.path, &self.config) {
            Ok(()) => {
                if self.message.starts_with("Couldn't save") {
                    self.message.clear();
                }
                true
            }
            Err(e) => {
                self.message = format!("Couldn't save settings: {e}");
                false
            }
        }
    }
    fn selected_preset(&self) -> Option<String> {
        self.config.presets.keys().nth(self.preset_index).cloned()
    }
    fn save_preset(&mut self) {
        if let Some(name) = self.preset_name.clone() {
            self.config
                .presets
                .insert(name.clone(), self.config.settings.clone());
            if self.persist() {
                self.message = format!("Saved --{name}");
            }
        } else {
            self.input = Some(Input {
                kind: Edit::NewPreset,
                value: String::new(),
                fresh: true,
            });
        }
    }
    fn zoom(&mut self, plus: bool) {
        let current = if self.config.settings.size == 0 {
            self.rendered_scale
        } else {
            self.config.settings.size.min(self.max_scale)
        }
        .max(1);
        self.config.settings.size = if plus {
            (current + 1).min(self.max_scale.max(1))
        } else {
            current.saturating_sub(1).max(1)
        };
    }
    fn edit_key(&mut self, key: KeyCode, mut input: Input) {
        match key {
            KeyCode::Esc => {
                self.message.clear();
                return;
            }
            KeyCode::Enter => match &input.kind {
                Edit::Duration(field) => {
                    let parsed = if matches!(field, Field::Cycles | Field::LongEvery) {
                        input
                            .value
                            .parse::<u64>()
                            .map_err(|_| "Cycles must be 1–99".into())
                    } else {
                        duration(&input.value)
                    };
                    match parsed {
                        Ok(n)
                            if match field {
                                Field::Prep => n <= 3600,
                                Field::Cycles => (1..=99).contains(&n),
                                Field::LongEvery => n <= 99,
                                _ => n > 0,
                            } =>
                        {
                            let s = &mut self.config.settings;
                            match field {
                                Field::Prep => s.prep = n,
                                Field::Rest => s.rest_seconds = n,
                                Field::LongRest => s.long_rest_seconds = n,
                                Field::LongEvery => s.long_rest_every = n as u16,
                                Field::Cycles => s.cycles = n as u16,
                                _ if s.pomodoro => s.work_seconds = n,
                                _ => s.seconds = n,
                            }
                            self.message.clear();
                            return;
                        }
                        _ => {
                            self.message = match field {
                                Field::Prep => {
                                    "Get-ready countdown: 0 (off) to 3600 seconds.".into()
                                }
                                Field::Cycles => "Choose 1–99 work periods.".into(),
                                Field::LongEvery => "Choose 0–99; zero disables long rests.".into(),
                                _ => "Use a duration such as 2m or 1:30, greater than zero.".into(),
                            }
                        }
                    }
                }
                Edit::NewPreset | Edit::Rename(_) => {
                    let name = &input.value;
                    let same = matches!(&input.kind, Edit::Rename(old) if old == name);
                    if !valid_name(name) {
                        self.message =
                            "Use a–z, 0–9, hyphens; start with a letter. CLI names are reserved."
                                .into();
                    } else if self.config.presets.contains_key(name) && !same {
                        self.message = "That name already exists. Choose another name.".into();
                    } else {
                        self.stash_draft();
                        let settings = match &input.kind {
                            Edit::Rename(old) => {
                                if let Some(draft) = self.config.drafts.remove(old) {
                                    self.config.drafts.insert(name.clone(), draft);
                                }
                                self.config.presets.remove(old).unwrap()
                            }
                            _ => self.config.settings.clone(),
                        };
                        self.config.presets.insert(name.clone(), settings);
                        if let Edit::Rename(old) = &input.kind
                            && let Some((_, _, Some(saved_name))) = &mut self.suspended
                            && saved_name == old
                        {
                            *saved_name = name.clone();
                        }
                        if matches!(&input.kind, Edit::NewPreset)
                            || matches!(&input.kind, Edit::Rename(old) if self.preset_name.as_ref() == Some(old))
                        {
                            self.preset_name = Some(name.clone());
                        }
                        self.preset_index = self
                            .config
                            .presets
                            .keys()
                            .position(|n| n == name)
                            .unwrap_or(0);
                        if self.persist() {
                            self.message = format!("Saved --{name}");
                        }
                        return;
                    }
                }
            },
            KeyCode::Backspace => {
                if input.fresh {
                    input.value.clear();
                } else {
                    input.value.pop();
                }
                input.fresh = false;
            }
            KeyCode::Char(c) if input.fresh || input.value.len() < 32 => {
                if input.fresh {
                    input.value.clear();
                }
                input.value.push(c);
                input.fresh = false;
            }
            _ => {}
        }
        self.input = Some(input);
    }
    fn key(&mut self, key: KeyCode) -> bool {
        if let Some(input) = self.input.take() {
            self.edit_key(key, input);
            return false;
        }
        if let Some(name) = self.delete.take() {
            match key {
                KeyCode::Char('y') => {
                    self.config.presets.remove(&name);
                    self.config.drafts.remove(&name);
                    if let Some((_, _, saved_name)) = &mut self.suspended
                        && saved_name.as_ref() == Some(&name)
                    {
                        *saved_name = None;
                    }
                    if self.preset_name.as_ref() == Some(&name) {
                        self.preset_name = None;
                    }
                    self.preset_index = self
                        .preset_index
                        .min(self.config.presets.len().saturating_sub(1));
                    if self.persist() {
                        self.message = format!("Deleted --{name}");
                    }
                }
                KeyCode::Esc | KeyCode::Char('n') => {}
                _ => self.delete = Some(name),
            }
            return false;
        }
        if self.presets {
            let len = self.config.presets.len().max(1);
            match key {
                KeyCode::Esc | KeyCode::Char('p') | KeyCode::Char('q') => self.presets = false,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.preset_index = (self.preset_index + 1) % len
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.preset_index = (self.preset_index + len - 1) % len
                }
                KeyCode::Char('g') => self.preset_index = 0,
                KeyCode::Char('G') => self.preset_index = len - 1,
                KeyCode::Enter | KeyCode::Char('e') | KeyCode::Char('l') => {
                    if let Some(name) = self.selected_preset() {
                        self.stash_draft();
                        self.config.settings = self
                            .config
                            .drafts
                            .get(&name)
                            .unwrap_or(&self.config.presets[&name])
                            .clone();
                        self.preset_name = Some(name.clone());
                        self.selected = 0;
                        self.presets = false;
                        self.message = format!("Editing --{name} · s saves changes");
                    }
                }
                KeyCode::Char('n') => {
                    self.input = Some(Input {
                        kind: Edit::NewPreset,
                        value: String::new(),
                        fresh: true,
                    });
                }
                KeyCode::Char('r') => {
                    if let Some(name) = self.selected_preset() {
                        self.input = Some(Input {
                            kind: Edit::Rename(name.clone()),
                            value: name,
                            fresh: true,
                        });
                    }
                }
                KeyCode::Char('u') => {
                    self.stash_draft();
                    if let Some(scratch) = &self.config.scratch {
                        self.config.settings = scratch.clone();
                        self.preset_name = None;
                        self.selected = 0;
                        self.presets = false;
                    }
                }
                KeyCode::Char('d') => self.delete = self.selected_preset(),
                _ => {}
            }
            return false;
        }
        if key == KeyCode::Char('q') {
            return self.persist();
        }
        if self.clock.phase == Phase::Setup {
            let fields = self.fields();
            let field = fields[self.selected.min(fields.len() - 1)];
            match key {
                KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => {
                    self.selected = (self.selected + 1) % fields.len()
                }
                KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => {
                    self.selected = (self.selected + fields.len() - 1) % fields.len()
                }
                KeyCode::Char('g') => self.selected = 0,
                KeyCode::Char('G') => self.selected = fields.len() - 1,
                KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                    let plus = matches!(key, KeyCode::Right | KeyCode::Char('l'));
                    let s = &mut self.config.settings;
                    let adjust = |v: u64, step: u64, min: u64, max: u64| {
                        if plus {
                            ((v / step + 1) * step).min(max)
                        } else {
                            (v.saturating_sub(1) / step * step).max(min)
                        }
                    };
                    match field {
                        Field::Mode => {
                            let mode = if s.pomodoro {
                                2
                            } else if s.stopwatch {
                                1
                            } else {
                                0
                            };
                            let mode = (mode + if plus { 1 } else { 2 }) % 3;
                            s.stopwatch = mode == 1;
                            s.pomodoro = mode == 2;
                        }
                        Field::Duration if s.pomodoro => {
                            s.work_seconds = adjust(s.work_seconds, 60, 1, 359999)
                        }
                        Field::Duration => s.seconds = adjust(s.seconds, 15, 1, 359999),
                        Field::Rest => s.rest_seconds = adjust(s.rest_seconds, 60, 1, 359999),
                        Field::LongRest => {
                            s.long_rest_seconds = adjust(s.long_rest_seconds, 60, 1, 359999)
                        }
                        Field::LongEvery => {
                            s.long_rest_every = adjust(s.long_rest_every as u64, 1, 0, 99) as u16
                        }
                        Field::FinalRest => s.final_rest = !s.final_rest,
                        Field::ColorMode => {
                            s.color_mode = (s.color_mode + if plus { 1 } else { 2 }) % 3
                        }
                        Field::Cycles => s.cycles = adjust(s.cycles as u64, 1, 1, 99) as u16,
                        Field::Prep => s.prep = adjust(s.prep, 1, 0, 3600),
                        Field::Hundredths => s.hundredths = !s.hundredths,
                        Field::Font => s.font = (s.font + if plus { 1 } else { 2 }) % 3,
                        Field::Size => self.zoom(plus),
                        Field::Theme => {
                            s.theme =
                                (s.theme + if plus { 1 } else { THEMES.len() - 1 }) % THEMES.len()
                        }
                        Field::Bell => s.bell = !s.bell,
                        Field::Confetti => s.confetti = !s.confetti,
                    }
                }
                KeyCode::Char('e') | KeyCode::Char('i')
                    if matches!(
                        field,
                        Field::Duration
                            | Field::Prep
                            | Field::Rest
                            | Field::Cycles
                            | Field::LongRest
                            | Field::LongEvery
                    ) =>
                {
                    let s = &self.config.settings;
                    let n = match field {
                        Field::Prep => s.prep,
                        Field::Rest => s.rest_seconds,
                        Field::LongRest => s.long_rest_seconds,
                        Field::LongEvery => s.long_rest_every as u64,
                        Field::Cycles => s.cycles as u64,
                        _ => s.duration(),
                    };
                    self.input = Some(Input {
                        kind: Edit::Duration(field),
                        value: if matches!(field, Field::Cycles | Field::LongEvery) {
                            n.to_string()
                        } else {
                            display_time(n)
                        },
                        fresh: true,
                    });
                    self.message.clear();
                }
                KeyCode::Char('a') => self.config.settings.size = 0,
                KeyCode::Char('p') => {
                    self.presets = true;
                    self.message.clear();
                    self.preset_index = self
                        .preset_name
                        .as_ref()
                        .and_then(|name| self.config.presets.keys().position(|n| n == name))
                        .unwrap_or(0);
                }
                KeyCode::Char('s') => self.save_preset(),
                KeyCode::Char('n') => {
                    self.input = Some(Input {
                        kind: Edit::NewPreset,
                        value: String::new(),
                        fresh: true,
                    })
                }
                KeyCode::Esc => self.return_to_session(),
                KeyCode::Enter => {
                    self.message.clear();
                    if self.persist() {
                        self.suspended = None;
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
                KeyCode::Char('c') if self.clock.phase == Phase::Done => {
                    self.celebration = Some(Instant::now())
                }
                KeyCode::Char(' ') if self.clock.phase == Phase::Done => self.celebration = None,
                KeyCode::Char('r') => self.clock.start(&self.config.settings, Instant::now()),
                KeyCode::Enter if self.clock.phase == Phase::Done => {
                    self.clock.start(&self.config.settings, Instant::now())
                }
                KeyCode::Esc => self.open_settings(Instant::now()),
                KeyCode::Char('n') if self.config.settings.pomodoro => {
                    let now = Instant::now();
                    if self.clock.skip(&self.config.settings, now)
                        && self.clock.phase == Phase::Done
                        && self.config.settings.confetti
                    {
                        self.celebration = Some(now);
                    }
                }
                KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char('l') => self.zoom(true),
                KeyCode::Char('-') | KeyCode::Char('h') => self.zoom(false),
                KeyCode::Char('a') => self.config.settings.size = 0,
                _ => {}
            }
        }
        false
    }
}

const DIGITS: [[&str; 5]; 12] = [
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
    ["0", "0", "0", "0", "1"],
];
fn display_stopwatch(elapsed: Duration, hundredths: bool) -> String {
    let whole = display_time(elapsed.as_secs());
    if hundredths {
        format!("{whole}.{:02}", elapsed.subsec_millis() / 10)
    } else {
        whole
    }
}
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
const SLIM: [[&str; 7]; 12] = [
    [
        "01110", "10001", "10001", "10001", "10001", "10001", "01110",
    ],
    [
        "00100", "01100", "00100", "00100", "00100", "00100", "01110",
    ],
    [
        "01110", "10001", "00001", "00010", "00100", "01000", "11111",
    ],
    [
        "11110", "00001", "00001", "01110", "00001", "00001", "11110",
    ],
    [
        "00010", "00110", "01010", "10010", "11111", "00010", "00010",
    ],
    [
        "11111", "10000", "10000", "11110", "00001", "00001", "11110",
    ],
    [
        "01110", "10000", "10000", "11110", "10001", "10001", "01110",
    ],
    [
        "11111", "00001", "00010", "00100", "01000", "01000", "01000",
    ],
    [
        "01110", "10001", "10001", "01110", "10001", "10001", "01110",
    ],
    [
        "01110", "10001", "10001", "01111", "00001", "00001", "01110",
    ],
    ["0", "0", "1", "0", "1", "0", "0"],
    ["0", "0", "0", "0", "0", "0", "1"],
];
fn digit_fit(area: Rect, text: &str, font: usize) -> u16 {
    let units: u16 = text
        .chars()
        .map(|c| {
            if c == ':' || c == '.' {
                1
            } else if font == 1 {
                5
            } else {
                3
            }
        })
        .sum::<u16>()
        + text.len() as u16
        - 1;
    (area.width / (units * 2))
        .min(area.height / if font == 1 { 7 } else { 5 })
        .min(8)
}
fn big_digits(
    f: &mut Frame,
    area: Rect,
    text: &str,
    requested: u16,
    font: usize,
    color: Color,
) -> (u16, u16) {
    let fit = digit_fit(area, text, font);
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
        return (1, 1);
    }
    let scale = if requested == 0 {
        fit
    } else {
        requested.min(fit)
    };
    let height = if font == 1 { 7 } else { 5 };
    let mut lines = Vec::new();
    for row in 0..height {
        let line = text
            .chars()
            .map(|c| {
                let index =
                    c.to_digit(10)
                        .map(|n| n as usize)
                        .unwrap_or(if c == '.' { 11 } else { 10 });
                let pattern = if font == 1 {
                    SLIM[index][row]
                } else {
                    DIGITS[index][row]
                };
                pattern
                    .chars()
                    .map(|c| {
                        if c == '1' {
                            if font == 2 {
                                "● ".repeat(scale as usize)
                            } else {
                                "█".repeat((scale * 2) as usize)
                            }
                        } else {
                            " ".repeat((scale * 2) as usize)
                        }
                    })
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
            y: area.y + (area.height - height as u16 * scale) / 2,
            height: height as u16 * scale,
            ..area
        },
    );
    (scale, fit)
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
fn session_remaining(clock: &Clock, s: &Settings, now: Instant) -> u64 {
    if clock.phase == Phase::Done {
        return 0;
    }
    let current = Duration::from_secs(clock.limit(s))
        .saturating_sub(clock.elapsed_at(now))
        .as_secs_f64()
        .ceil() as u64;
    let future = match clock.phase {
        Phase::Prep => (1..=s.cycles)
            .map(|cycle| s.work_seconds + s.rest_for(cycle))
            .sum(),
        Phase::Running => {
            s.rest_for(clock.cycle)
                + ((clock.cycle + 1)..=s.cycles)
                    .map(|cycle| s.work_seconds + s.rest_for(cycle))
                    .sum::<u64>()
        }
        Phase::Rest => ((clock.cycle + 1)..=s.cycles)
            .map(|cycle| s.work_seconds + s.rest_for(cycle))
            .sum(),
        _ => 0,
    };
    current + future
}
fn running_hint(width: u16, phase: Phase, paused: bool) -> String {
    if phase == Phase::Done {
        return "Enter restart  c confetti\nSpace dismiss\nEsc settings  q quit".into();
    }
    let action = if paused { "resume" } else { "pause" };
    if width < 90 {
        format!("Space {action}  r restart\nEsc settings  q quit\nh/l or -/+ zoom  a auto-fit")
    } else {
        format!("Space {action}   r restart   Esc settings   h/l or -/+ zoom   a auto-fit   q quit")
    }
}
fn ui(f: &mut Frame, app: &mut App) {
    ui_content(f, app);
    if app.config.settings.color_mode == 2
        || (app.config.settings.color_mode == 0 && app.no_color)
        || THEMES[app.config.settings.theme].name == "terminal"
    {
        for cell in &mut f.buffer_mut().content {
            cell.set_fg(Color::Reset).set_bg(Color::Reset);
        }
    }
}
fn ui_content(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let s = &app.config.settings;
    let accent = THEMES[s.theme].accent;
    let muted = Color::Rgb(135, 146, 160);
    let bg = Color::Rgb(15, 19, 25);
    let amber = Color::Rgb(245, 192, 106);
    f.render_widget(
        Block::default().style(Style::default().bg(bg).fg(Color::Rgb(222, 229, 237))),
        area,
    );
    if app.clock.phase == Phase::Done
        && let Some(started) = app.celebration
    {
        effects::confetti(f, area, started.elapsed(), accent);
    }
    let (min_w, min_h) = if app.clock.phase == Phase::Setup {
        (60, 24)
    } else {
        (36, 14)
    };
    if area.width < min_w || area.height < min_h {
        f.render_widget(
            Paragraph::new(format!(
                "Enlarge terminal to {min_w} × {min_h}\nq quit · Esc settings"
            ))
            .centered(),
            centered(area, area.width, 2),
        );
        return;
    }
    if app.clock.phase == Phase::Setup {
        let panel = centered(
            area,
            if s.stopwatch && s.hundredths { 88 } else { 72 },
            area.height.saturating_sub(2),
        );
        let fields = app.fields();
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length((fields.len() as u16).min(area.height.saturating_sub(15))),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(panel);
        let title = app
            .preset_name
            .as_ref()
            .map(|name| {
                let dirty = app.config.presets.get(name) != Some(s);
                format!("--{name}{}", if dirty { " · unsaved edits" } else { "" })
            })
            .unwrap_or_else(|| "T I M E R".into());
        f.render_widget(
            Paragraph::new(title)
                .centered()
                .style(Style::default().fg(accent).bold()),
            rows[0],
        );
        let lines = fields
            .iter()
            .enumerate()
            .skip(
                app.selected
                    .saturating_sub(rows[1].height.saturating_sub(1) as usize),
            )
            .take(rows[1].height as usize)
            .map(|(i, field)| {
                let (label, value) = match field {
                    Field::Mode => ("Mode", s.mode_name().into()),
                    Field::Duration => (
                        if s.pomodoro {
                            "Work duration"
                        } else {
                            "Duration"
                        },
                        display_time(s.duration()),
                    ),
                    Field::Rest => ("Rest duration", display_time(s.rest_seconds)),
                    Field::Cycles => ("Cycles", format!("{} work periods", s.cycles)),
                    Field::LongRest => ("Long rest", display_time(s.long_rest_seconds)),
                    Field::LongEvery => (
                        "Long rest every",
                        if s.long_rest_every == 0 {
                            "Off".into()
                        } else {
                            format!("{} cycles", s.long_rest_every)
                        },
                    ),
                    Field::FinalRest => (
                        "Final rest",
                        if s.final_rest {
                            "Include".into()
                        } else {
                            "Skip".into()
                        },
                    ),
                    Field::ColorMode => (
                        "Color output",
                        ["Auto · honors NO_COLOR", "Always", "Never"][s.color_mode].into(),
                    ),
                    Field::Prep => (
                        "Get ready",
                        if s.prep == 0 {
                            "Off".into()
                        } else {
                            format!("{}s before starting", s.prep)
                        },
                    ),
                    Field::Hundredths => (
                        "Hundredths",
                        if s.hundredths {
                            "On · 00:00.00".into()
                        } else {
                            "Off · 00:00".into()
                        },
                    ),
                    Field::Font => ("Digit font", ["Block", "Slim", "Dots"][s.font].into()),
                    Field::Size => (
                        "Digit size",
                        if s.size == 0 {
                            "Auto-fit".into()
                        } else {
                            format!("{} · a restores auto-fit", s.size)
                        },
                    ),
                    Field::Theme => ("Color", THEMES[s.theme].label.into()),
                    Field::Confetti => (
                        "Finish effect",
                        if s.confetti {
                            "Confetti".into()
                        } else {
                            "Off".into()
                        },
                    ),
                    Field::Bell => (
                        "Terminal bell",
                        if s.bell {
                            "On · at transitions".into()
                        } else {
                            "Off".into()
                        },
                    ),
                };
                Line::from(format!(
                    "  {}  {label:<17} {value}",
                    if i == app.selected { "›" } else { " " }
                ))
                .style(if i == app.selected {
                    Style::default().fg(accent).bold()
                } else {
                    Style::default().fg(muted)
                })
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), rows[1]);
        let preview = if s.stopwatch {
            display_stopwatch(Duration::ZERO, s.hundredths)
        } else {
            display_time(s.duration())
        };
        let preview_area = if s.pomodoro {
            let columns = Layout::horizontal([
                Constraint::Min(0),
                Constraint::Length(if panel.width < 64 { 12 } else { 18 }),
            ])
            .split(rows[2]);
            let rest_area = centered(columns[1], 18, 5);
            f.render_widget(
                Paragraph::new(format!("\n{}", display_time(s.rest_seconds)))
                    .centered()
                    .block(Block::default().borders(Borders::ALL).title(" Rest "))
                    .style(Color::Rgb(125, 194, 245)),
                rest_area,
            );
            columns[0]
        } else {
            rows[2]
        };
        let (scale, fit) = big_digits(f, preview_area, &preview, s.size, s.font, accent);
        app.rendered_scale = scale;
        app.max_scale = fit;
        let setup_hint = format!(
            "j/k select ({}/{})  h/l change  e/i type\nEnter {}  p presets  s save  n new\na auto-fit  {}q quit",
            app.selected + 1,
            fields.len(),
            if app.suspended.is_some() {
                "new session"
            } else {
                "start"
            },
            if app.suspended.is_some() {
                "Esc return paused  "
            } else {
                ""
            }
        );
        f.render_widget(Paragraph::new(setup_hint).centered().style(muted), rows[3]);
        f.render_widget(
            Paragraph::new(app.message.as_str())
                .centered()
                .style(accent),
            rows[4],
        );
    } else {
        let now = Instant::now();
        let elapsed = app.clock.elapsed_at(now);
        let paused = app.clock.paused && app.clock.phase != Phase::Done;
        let value = if app.clock.phase == Phase::Done {
            0
        } else if s.stopwatch && app.clock.phase == Phase::Running {
            elapsed.as_secs()
        } else {
            Duration::from_secs(app.clock.limit(s))
                .saturating_sub(elapsed)
                .as_secs_f64()
                .ceil() as u64
        };
        let text = if app.clock.phase == Phase::Prep {
            value.to_string()
        } else if s.stopwatch {
            display_stopwatch(elapsed, s.hundredths)
        } else {
            display_time(value)
        };
        let rows = Layout::vertical([
            Constraint::Min(5),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(3),
        ])
        .margin(1)
        .split(area);
        let color = if paused {
            Color::Rgb(135, 146, 160)
        } else if app.clock.phase == Phase::Rest {
            Color::Rgb(125, 194, 245)
        } else {
            accent
        };
        let (scale, fit) = big_digits(f, rows[0], &text, s.size, s.font, color);
        app.rendered_scale = scale;
        app.max_scale = fit;
        let status = if paused {
            "  ||   Space to resume  ".to_string()
        } else if app.clock.phase == Phase::Prep {
            "Get ready".into()
        } else if app.clock.phase == Phase::Done {
            "Complete".into()
        } else {
            String::new()
        };
        let badge = centered(rows[1], status.chars().count() as u16, 1);
        f.render_widget(
            Paragraph::new(status).centered().style(if paused {
                Style::default().bg(amber).fg(bg).bold()
            } else {
                Style::default().fg(accent)
            }),
            badge,
        );
        if s.pomodoro {
            let stage = match app.clock.phase {
                Phase::Rest
                    if s.long_rest_every > 0
                        && app.clock.cycle.is_multiple_of(s.long_rest_every) =>
                {
                    "Long rest"
                }
                Phase::Rest => "Rest",
                Phase::Prep => "Ready",
                Phase::Done => "Finished",
                _ => "Work",
            };
            let info = if area.width < 68 {
                format!(
                    "{stage} · {}/{}\n{} total left",
                    app.clock.cycle,
                    s.cycles,
                    display_time(session_remaining(&app.clock, s, now))
                )
            } else {
                format!(
                    "{stage} · cycle {} of {} · {} total left",
                    app.clock.cycle,
                    s.cycles,
                    display_time(session_remaining(&app.clock, s, now))
                )
            };
            f.render_widget(Paragraph::new(info).centered().style(muted), rows[2]);
        }
        if !s.stopwatch && app.clock.phase != Phase::Prep {
            let ratio = if app.clock.phase == Phase::Done {
                1.0
            } else {
                (elapsed.as_secs_f64() / app.clock.limit(s) as f64).clamp(0.0, 1.0)
            };
            f.render_widget(
                Gauge::default()
                    .ratio(ratio)
                    .gauge_style(Style::default().fg(color).bg(Color::Rgb(34, 43, 53)))
                    .label(""),
                centered(rows[3], area.width.saturating_sub(8).min(90), 1),
            );
        }
        let mut hint = running_hint(area.width, app.clock.phase, paused);
        if s.pomodoro && matches!(app.clock.phase, Phase::Running | Phase::Rest) {
            hint = hint.replace("r restart", "r restart  n skip");
        }
        f.render_widget(Paragraph::new(hint).centered().style(muted), rows[4]);
    }
    if app.presets {
        let popup = centered(area, 68, 18);
        f.render_widget(Clear, popup);
        f.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .title(" Presets ")
                .style(Style::default().bg(bg).fg(accent)),
            popup,
        );
        let inner = popup.inner(Margin::new(2, 1));
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);
        f.render_widget(
            Paragraph::new("Select a preset to edit its settings.").style(muted),
            rows[0],
        );
        let capacity = rows[1].height as usize;
        let offset = app.preset_index.saturating_sub(capacity.saturating_sub(1));
        let lines = app
            .config
            .presets
            .iter()
            .enumerate()
            .skip(offset)
            .take(capacity)
            .map(|(i, (name, s))| {
                Line::from(format!(
                    "{} --{name}{}   {}",
                    if i == app.preset_index { "›" } else { " " },
                    if app.config.drafts.contains_key(name) {
                        " *draft"
                    } else {
                        ""
                    },
                    s.summary()
                ))
                .style(if i == app.preset_index {
                    Style::default().fg(accent).bold()
                } else {
                    Style::default().fg(muted)
                })
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(if lines.is_empty() {
                vec![Line::from(
                    "No presets yet. Press n to save the current setup.",
                )]
            } else {
                lines
            }),
            rows[1],
        );
        f.render_widget(Paragraph::new("j/k or ↑↓ select   Enter / e edit   r rename\nn new from current setup   d delete\nu unnamed setup   Esc back").style(muted),rows[2]);
        f.render_widget(Paragraph::new(app.message.as_str()).style(accent), rows[3]);
    }
    if let Some(input) = &app.input {
        let popup = centered(area, 58, 8);
        f.render_widget(Clear, popup);
        let title = match &input.kind {
            Edit::NewPreset => " New preset ",
            Edit::Rename(_) => " Rename preset ",
            Edit::Duration(Field::Cycles) => " Number of cycles ",
            Edit::Duration(Field::LongEvery) => " Long rest every · 0 disables ",
            Edit::Duration(Field::Prep) => " Get-ready countdown · 0 turns it off ",
            _ => " Duration · e.g. 25m or 1:30 ",
        };
        f.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().bg(bg).fg(accent)),
            popup,
        );
        let inner = popup.inner(Margin::new(2, 1));
        let text = Line::from(input.value.clone()).style(if input.fresh {
            Style::default().bg(accent).fg(bg)
        } else {
            Style::default().fg(accent)
        });
        f.render_widget(
            Paragraph::new(vec![
                text,
                Line::from(""),
                Line::from("Enter save · Esc cancel · typing replaces selection"),
                Line::from(app.message.clone()),
            ]),
            inner,
        );
    }
    if let Some(name) = &app.delete {
        let popup = centered(area, 58, 6);
        f.render_widget(Clear, popup);
        f.render_widget(
            Paragraph::new(format!("\nDelete --{name}?\n\ny delete · n / Esc cancel"))
                .centered()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Delete preset "),
                )
                .style(Style::default().bg(bg).fg(amber)),
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
        print!("{}", include_str!("../help.txt"));
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
                s.mode_name(),
                s.summary(),
                s.prep
            );
        }
        return Ok(());
    }
    let autostart = args(&mut config, &argv)?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Run in an interactive terminal (or use --help).".into());
    }
    let mut app = App::new(config, path);
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
    crossterm::style::force_color_output(true);
    #[cfg(feature = "metrics")]
    let mut render_metrics = metrics::Metrics::default();
    let mut terminal = ratatui::init();
    let result = (|| -> io::Result<()> {
        loop {
            let frame_time = Instant::now();
            let transitioned = app.advance(frame_time);
            let animate = app.animating(frame_time);
            if transitioned && app.config.settings.bell {
                print!("\x07");
                io::stdout().flush()?;
            }
            #[cfg(feature = "metrics")]
            let render_started = Instant::now();
            terminal.draw(|f| ui(f, &mut app))?;
            #[cfg(feature = "metrics")]
            render_metrics.record(render_started.elapsed(), app.celebration.is_some());
            // Static screens block for input/resize; active timing and confetti
            // keep a bounded animation cadence. A resize wakes event::read too.
            let frame_budget = if app.clock.phase == Phase::Done {
                50
            } else {
                33
            };
            if animate && !event::poll(Duration::from_millis(frame_budget))? {
                continue;
            }
            if let Event::Key(key) = event::read()?
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
    #[cfg(feature = "metrics")]
    if let Some(path) = env::var_os("TUI_TIMER_METRICS") {
        render_metrics.write(std::path::Path::new(&path))?;
    }
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
            let mut app = App::new(Config::default(), PathBuf::new());
            for phase in [Phase::Setup, Phase::Prep, Phase::Running, Phase::Done] {
                app.clock.phase = phase;
                terminal.draw(|f| ui(f, &mut app)).unwrap();
            }
        }
    }
    #[test]
    fn pomodoro_cycles_include_each_rest_and_account_for_delayed_frames() {
        let settings = Settings {
            pomodoro: true,
            work_seconds: 10,
            rest_seconds: 3,
            cycles: 2,
            prep: 2,
            ..Settings::default()
        };
        let start = Instant::now();
        let mut clock = Clock::new();
        clock.start(&settings, start);
        assert_eq!(session_remaining(&clock, &settings, start), 28);
        assert!(!clock.tick(&settings, start + Duration::from_secs(2)));
        assert_eq!(clock.phase, Phase::Running);
        assert!(clock.tick(&settings, start + Duration::from_secs(12)));
        assert_eq!(clock.phase, Phase::Rest);
        assert_eq!(
            session_remaining(&clock, &settings, start + Duration::from_secs(12)),
            16
        );
        clock.toggle(start + Duration::from_secs(13));
        assert!(!clock.tick(&settings, start + Duration::from_secs(50)));
        assert_eq!(
            session_remaining(&clock, &settings, start + Duration::from_secs(50)),
            15
        );
        clock.toggle(start + Duration::from_secs(50));
        assert!(clock.tick(&settings, start + Duration::from_secs(53)));
        assert_eq!((clock.phase, clock.cycle), (Phase::Running, 2));
        assert_eq!(
            clock.elapsed_at(start + Duration::from_secs(53)),
            Duration::from_secs(1)
        );
        assert!(clock.tick(&settings, start + Duration::from_secs(65)));
        assert_eq!(clock.phase, Phase::Done);
        assert_eq!(
            session_remaining(&clock, &settings, start + Duration::from_secs(65)),
            0
        );
        clock.start(&settings, start);
        assert!(clock.tick(&settings, start + Duration::from_secs(100)));
        assert_eq!((clock.phase, clock.cycle), (Phase::Done, 2));
    }
    #[test]
    fn zoom_clamps_without_returning_to_auto() {
        let mut app = App::new(Config::default(), PathBuf::new());
        app.rendered_scale = 3;
        app.max_scale = 3;
        app.zoom(false);
        assert_eq!(app.config.settings.size, 2);
        for _ in 0..20 {
            app.zoom(false);
        }
        assert_eq!(app.config.settings.size, 1);
        app.zoom(true);
        assert_eq!(app.config.settings.size, 2);
        for _ in 0..20 {
            app.zoom(true);
        }
        assert_eq!(app.config.settings.size, 3);
        app.config.settings.size = 8;
        app.max_scale = 2;
        app.zoom(false);
        assert_eq!(app.config.settings.size, 1);
        app.key(KeyCode::Char('a'));
        assert_eq!(app.config.settings.size, 0);
    }
    #[test]
    fn old_settings_get_new_defaults_and_cli_validates_pomodoro() {
        let legacy: Config =
            toml::from_str("[settings]\nseconds = 60\n[presets.hang]\nseconds = 90").unwrap();
        assert_eq!(legacy.settings.work_seconds, 1500);
        assert_eq!(legacy.presets["hang"].seconds, 90);
        let mut config = Config::default();
        let argv = [
            "--pomodoro",
            "--work",
            "15m",
            "--rest",
            "3m",
            "--cycles",
            "2",
            "--font",
            "slim",
        ]
        .map(String::from);
        args(&mut config, &argv).unwrap();
        assert!(config.settings.pomodoro);
        assert_eq!(config.settings.work_seconds, 900);
        assert_eq!(config.settings.rest_seconds, 180);
        assert_eq!(config.settings.cycles, 2);
        assert_eq!(config.settings.font, 1);
        for argv in [
            vec!["--cycles", "0"],
            vec!["--rest", "0"],
            vec!["--font", "unknown"],
        ] {
            assert!(
                args(
                    &mut Config::default(),
                    &argv.into_iter().map(String::from).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
    }
    #[test]
    fn pause_is_symbol_only_and_every_font_renders() {
        for font in 0..3 {
            let mut app = App::new(Config::default(), PathBuf::new());
            app.config.settings.font = font;
            app.config.settings.prep = 0;
            app.clock.start(&app.config.settings, Instant::now());
            app.clock.toggle(Instant::now());
            let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
            terminal.draw(|f| ui(f, &mut app)).unwrap();
            let content = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(content.contains("||"));
            assert!(content.contains("Space to resume"));
            assert!(!content.contains("PAUSED"));
            assert!(!content.contains("TIME REMAINING"));
            assert!(content.contains(if font == 2 { "●" } else { "█" }));
        }
    }
    #[test]
    fn duration_arrows_snap_to_grid_in_both_directions() {
        let mut app = App::new(Config::default(), PathBuf::new());
        app.selected = 1;
        for (initial, right, expected) in [
            (1, true, 15),
            (16, true, 30),
            (16, false, 15),
            (61, false, 60),
            (60, true, 75),
            (15, false, 1),
            (1, false, 1),
            (359999, false, 359985),
        ] {
            app.config.settings.seconds = initial;
            app.key(if right { KeyCode::Right } else { KeyCode::Left });
            assert_eq!(app.config.settings.seconds, expected);
        }
        app.config.settings.pomodoro = true;
        for field in [1, 2] {
            app.selected = field;
            app.config.settings.work_seconds = 1;
            app.config.settings.rest_seconds = 1;
            app.key(KeyCode::Char('l'));
            assert_eq!(
                if field == 1 {
                    app.config.settings.work_seconds
                } else {
                    app.config.settings.rest_seconds
                },
                60
            );
            app.key(KeyCode::Char('l'));
            assert_eq!(
                if field == 1 {
                    app.config.settings.work_seconds
                } else {
                    app.config.settings.rest_seconds
                },
                120
            );
            app.key(KeyCode::Char('h'));
            assert_eq!(
                if field == 1 {
                    app.config.settings.work_seconds
                } else {
                    app.config.settings.rest_seconds
                },
                60
            );
        }
    }
    #[test]
    fn stopwatch_hundredths_format_and_toggle() {
        for (ms, expected) in [
            (0, "00:00.00"),
            (9, "00:00.00"),
            (10, "00:00.01"),
            (999, "00:00.99"),
            (1000, "00:01.00"),
            (61239, "01:01.23"),
            (3600000, "01:00:00.00"),
        ] {
            assert_eq!(display_stopwatch(Duration::from_millis(ms), true), expected);
        }
        assert_eq!(
            display_stopwatch(Duration::from_millis(61239), false),
            "01:01"
        );
        let mut app = App::new(Config::default(), PathBuf::new());
        args(
            &mut app.config,
            &["--stopwatch".into(), "--no-hundredths".into()],
        )
        .unwrap();
        assert!(!app.config.settings.hundredths);
        app.selected = 1;
        app.key(KeyCode::Right);
        assert!(app.config.settings.hundredths);
        assert_eq!(DIGITS[11], ["0", "0", "0", "0", "1"]);
    }
    #[test]
    fn rest_preview_and_hundredths_fit_at_normal_terminal_sizes() {
        for (width, height) in [(60, 24), (80, 24), (100, 30)] {
            let mut app = App::new(Config::default(), PathBuf::new());
            app.config.settings.pomodoro = true;
            let mut terminal =
                Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| ui(f, &mut app)).unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("┌ Rest "));
            assert!(text.contains("05:00"));
            app.config.settings.pomodoro = false;
            app.config.settings.stopwatch = true;
            for font in 0..3 {
                app.config.settings.font = font;
                terminal.draw(|f| ui(f, &mut app)).unwrap();
            }
        }
    }
    #[test]
    fn confetti_only_on_final_completion_then_returns_to_static() {
        let mut app = App::new(Config::default(), PathBuf::new());
        app.config.settings.pomodoro = true;
        app.config.settings.confetti = true;
        app.config.settings.prep = 0;
        app.config.settings.work_seconds = 1;
        app.config.settings.rest_seconds = 1;
        app.config.settings.cycles = 2;
        let start = Instant::now();
        app.clock.start(&app.config.settings, start);
        for seconds in 1..4 {
            app.advance(start + Duration::from_secs(seconds));
            assert!(app.celebration.is_none());
        }
        app.advance(start + Duration::from_secs(4));
        assert_eq!(app.clock.phase, Phase::Done);
        assert!(app.animating(start + Duration::from_secs(4)));
        app.advance(start + Duration::from_secs(9));
        assert!(!app.animating(start + Duration::from_secs(9)));
        assert!(app.celebration.is_none());
        app.key(KeyCode::Char('c'));
        assert!(app.celebration.is_some());
        app.key(KeyCode::Char(' '));
        assert!(app.celebration.is_none());
        app.key(KeyCode::Char('c'));
        app.key(KeyCode::Esc);
        app.advance(start + Duration::from_secs(10));
        assert!(app.celebration.is_none());
        assert!(!app.animating(start + Duration::from_secs(10)));
    }
    #[test]
    fn all_themes_parse_and_confetti_clips_and_expires() {
        for theme in THEMES {
            let mut config = Config::default();
            args(
                &mut config,
                &["--theme".into(), theme.name.into(), "--confetti".into()],
            )
            .unwrap();
            assert!(config.settings.confetti);
            assert_eq!(THEMES[config.settings.theme].name, theme.name);
        }
        for (w, h) in [(0, 0), (1, 1), (36, 14), (100, 30)] {
            let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|f| {
                    effects::confetti(f, f.area(), Duration::from_millis(1500), THEMES[0].accent)
                })
                .unwrap();
            terminal
                .draw(|f| effects::confetti(f, f.area(), CONFETTI_DURATION, THEMES[0].accent))
                .unwrap();
            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .all(|cell| cell.symbol() == " ")
            );
        }
    }
    #[test]
    fn footer_fits_smallest_terminal_and_describes_actual_action() {
        for phase in [Phase::Running, Phase::Prep, Phase::Rest, Phase::Done] {
            for paused in [true, false] {
                let hint = running_hint(36, phase, paused);
                assert!(hint.lines().all(|line| line.chars().count() <= 34));
                assert!(hint.contains("q quit"));
                assert!(hint.contains("Esc settings"));
                if phase != Phase::Done && paused {
                    assert!(hint.contains("Space resume"));
                }
            }
        }
    }
    #[test]
    fn quitting_keeps_save_errors_visible() {
        let blocker =
            std::env::temp_dir().join(format!("tui-timer-save-blocker-{}", std::process::id()));
        std::fs::write(&blocker, "file, not directory").unwrap();
        let mut app = App::new(Config::default(), blocker.join("config.toml"));
        assert!(!app.key(KeyCode::Char('q')));
        assert!(app.message.starts_with("Couldn't save"));
        std::fs::remove_file(blocker).unwrap();
    }
    #[test]
    fn long_rests_final_policy_and_skip_keep_totals_consistent() {
        let settings = Settings {
            pomodoro: true,
            work_seconds: 10,
            rest_seconds: 2,
            long_rest_seconds: 5,
            long_rest_every: 2,
            cycles: 4,
            final_rest: false,
            prep: 0,
            ..Settings::default()
        };
        let start = Instant::now();
        let mut clock = Clock::new();
        clock.start(&settings, start);
        assert_eq!(session_remaining(&clock, &settings, start), 49);
        clock.tick(&settings, start + Duration::from_secs(22));
        assert_eq!((clock.phase, clock.cycle), (Phase::Rest, 2));
        assert_eq!(clock.limit(&settings), 5);
        clock.toggle(start + Duration::from_secs(23));
        clock.skip(&settings, start + Duration::from_secs(100));
        assert_eq!(
            (clock.phase, clock.cycle, clock.paused),
            (Phase::Running, 3, true)
        );
        assert_eq!(
            session_remaining(&clock, &settings, start + Duration::from_secs(100)),
            22
        );
        clock.skip(&settings, start + Duration::from_secs(101));
        assert_eq!((clock.phase, clock.cycle), (Phase::Rest, 3));
        clock.skip(&settings, start + Duration::from_secs(102));
        clock.skip(&settings, start + Duration::from_secs(103));
        assert_eq!(clock.phase, Phase::Done);
        assert_eq!(session_remaining(&clock, &settings, start), 0);
    }
    #[test]
    fn settings_roundtrip_preserves_clock_and_separate_draft() {
        let mut app = App::new(Config::default(), PathBuf::new());
        app.preset_name = Some("hang".into());
        let start = Instant::now();
        app.config.settings.prep = 0;
        app.clock.start(&app.config.settings, start);
        app.open_settings(start + Duration::from_millis(1250));
        app.config.settings.seconds = 180;
        app.return_to_session();
        assert!(app.clock.paused);
        assert_eq!(
            app.clock.elapsed_at(start + Duration::from_secs(90)),
            Duration::from_millis(1250)
        );
        assert_eq!(app.config.settings.seconds, 120);
        assert_eq!(app.config.drafts["hang"].seconds, 180);
        app.stash_draft();
        assert_eq!(app.config.drafts["hang"].seconds, 180);
        app.open_settings(start + Duration::from_secs(91));
        assert_eq!(app.config.settings.seconds, 180);
    }
    #[test]
    fn preset_switches_restore_drafts_and_keep_saved_values_unchanged() {
        let mut config = Config::default();
        config.presets.insert("second".into(), Settings::default());
        let mut app = App::new(config, PathBuf::new());
        app.preset_name = Some("hang".into());
        app.config.settings.seconds = 333;
        app.presets = true;
        app.preset_index = 1;
        app.key(KeyCode::Enter);
        assert_eq!(app.config.settings.seconds, 120);
        app.presets = true;
        app.preset_index = 0;
        app.key(KeyCode::Enter);
        assert_eq!(app.config.settings.seconds, 333);
        assert_eq!(app.config.presets["hang"].seconds, 120);
        let restored: Config = toml::from_str(&toml::to_string(&app.config).unwrap()).unwrap();
        assert_eq!(restored.drafts["hang"].seconds, 333);
    }
    #[test]
    fn color_policy_precedence_is_explicit() {
        let mut app = App::new(Config::default(), PathBuf::new());
        app.no_color = true;
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        for mode in [0, 2] {
            app.config.settings.color_mode = mode;
            terminal.draw(|f| ui(f, &mut app)).unwrap();
            assert!(
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
            );
        }
        app.config.settings.color_mode = 1;
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|c| c.fg == THEMES[0].accent)
        );
        let mut config = Config::default();
        args(
            &mut config,
            &[
                "--color".into(),
                "never".into(),
                "--long-rest-every".into(),
                "2".into(),
                "--long-rest".into(),
                "10m".into(),
                "--no-final-rest".into(),
            ],
        )
        .unwrap();
        assert_eq!(config.settings.long_rest_every, 2);
        assert_eq!(config.settings.long_rest_seconds, 600);
        assert!(!config.settings.final_rest);
    }
}
