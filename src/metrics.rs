//! Optional render timing, compiled out of normal builds.
use serde::Serialize;
use std::{path::Path, time::Duration};

#[derive(Default, Serialize)]
pub struct Sample {
    frames: u64,
    total_us: u64,
    max_us: u64,
    over_16ms: u64,
    over_33ms: u64,
}
impl Sample {
    fn record(&mut self, duration: Duration) {
        let us = duration.as_micros().min(u64::MAX as u128) as u64;
        self.frames += 1;
        self.total_us = self.total_us.saturating_add(us);
        self.max_us = self.max_us.max(us);
        self.over_16ms += u64::from(us > 16_667);
        self.over_33ms += u64::from(us > 33_333);
    }
}
#[derive(Default, Serialize)]
pub struct Metrics {
    pub all_frames: Sample,
    pub confetti_frames: Sample,
}
impl Metrics {
    pub fn record(&mut self, duration: Duration, confetti: bool) {
        self.all_frames.record(duration);
        if confetti {
            self.confetti_frames.record(duration);
        }
    }
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let data = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data)
            .map_err(|e| format!("Cannot write render metrics to {}: {e}", path.display()))
    }
}
