use ratatui::prelude::*;
use std::time::Duration;

pub struct Theme {
    pub name: &'static str,
    pub label: &'static str,
    pub accent: Color,
}

// Keep existing indexes stable for older configuration files.
pub const THEMES: &[Theme] = &[
    Theme {
        name: "mint",
        label: "Mint",
        accent: Color::Rgb(116, 224, 181),
    },
    Theme {
        name: "amber",
        label: "Amber",
        accent: Color::Rgb(245, 192, 106),
    },
    Theme {
        name: "ice",
        label: "Ice",
        accent: Color::Rgb(125, 194, 245),
    },
    Theme {
        name: "mono",
        label: "Mono",
        accent: Color::White,
    },
    Theme {
        name: "lavender",
        label: "Lavender",
        accent: Color::Rgb(191, 167, 255),
    },
    Theme {
        name: "rose",
        label: "Rose",
        accent: Color::Rgb(255, 163, 194),
    },
    Theme {
        name: "coral",
        label: "Coral",
        accent: Color::Rgb(255, 157, 132),
    },
    Theme {
        name: "ocean",
        label: "Ocean",
        accent: Color::Rgb(100, 219, 228),
    },
    Theme {
        name: "lime",
        label: "Lime",
        accent: Color::Rgb(193, 230, 116),
    },
    Theme {
        name: "sand",
        label: "Sand",
        accent: Color::Rgb(224, 207, 170),
    },
    Theme {
        name: "terminal",
        label: "Terminal",
        accent: Color::Reset,
    },
];
pub const CONFETTI_DURATION: Duration = Duration::from_secs(5);

// Deterministic particles keep animation independent of frame rate, with no RNG
// dependency, allocations, or state that grows with terminal size.
pub fn confetti(f: &mut Frame, area: Rect, elapsed: Duration, accent: Color) {
    if elapsed >= CONFETTI_DURATION || area.is_empty() {
        return;
    }
    let t = elapsed.as_secs_f32();
    let palette = [
        accent,
        Color::Rgb(245, 192, 106),
        Color::Rgb(255, 163, 194),
        Color::Rgb(125, 194, 245),
    ];
    for i in 0..100u32 {
        let seed = i.wrapping_mul(2654435761).wrapping_add(1013904223);
        let delay = (seed % 100) as f32 / 100.0;
        let age = t - delay;
        if age < 0.0 {
            continue;
        }
        let speed = 0.22 + ((seed >> 8) % 100) as f32 / 350.0;
        let y = age * speed * area.height as f32;
        if y >= area.height as f32 {
            continue;
        }
        let origin = ((seed >> 16) % 1000) as f32 / 1000.0 * area.width as f32;
        let drift = ((i % 7) as f32 - 3.0) * age;
        let x = (origin + drift).rem_euclid(area.width as f32) as u16;
        // A particle occupies exactly one terminal cell. Writing that cell avoids
        // building a text layout and Paragraph for every particle on every frame.
        // cell_mut also clips safely if a caller supplies a partially offscreen area.
        if let Some(cell) = f.buffer_mut().cell_mut((area.x + x, area.y + y as u16)) {
            cell.set_symbol(["*", "+", "."][i as usize % 3])
                .set_fg(palette[i as usize % palette.len()]);
        }
    }
}
