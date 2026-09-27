//! Same-process comparison against the original Paragraph-based renderer.
//! Compiled by bench_confetti.py against the application's exact Ratatui build.
use ratatui::{backend::TestBackend, prelude::*, widgets::Paragraph};
use std::{hint::black_box, time::{Duration, Instant}};

#[path = "../src/effects.rs"]
#[allow(dead_code)]
mod effects;

// Retained benchmark reference, not application code. Keep this implementation
// unchanged so future runs remain comparable to the initial renderer.
fn original(f: &mut Frame, area: Rect, elapsed: Duration, accent: Color) {
    if elapsed >= effects::CONFETTI_DURATION || area.is_empty() { return; }
    let t = elapsed.as_secs_f32();
    let palette = [accent, Color::Rgb(245, 192, 106), Color::Rgb(255, 163, 194), Color::Rgb(125, 194, 245)];
    for i in 0..100u32 {
        let seed = i.wrapping_mul(2654435761).wrapping_add(1013904223);
        let delay = (seed % 100) as f32 / 100.0;
        let age = t - delay;
        if age < 0.0 { continue; }
        let speed = 0.22 + ((seed >> 8) % 100) as f32 / 350.0;
        let y = age * speed * area.height as f32;
        if y >= area.height as f32 { continue; }
        let origin = ((seed >> 16) % 1000) as f32 / 1000.0 * area.width as f32;
        let drift = ((i % 7) as f32 - 3.0) * age;
        let x = (origin + drift).rem_euclid(area.width as f32) as u16;
        f.render_widget(
            Paragraph::new(["*", "+", "."][i as usize % 3]).style(palette[i as usize % palette.len()]),
            Rect::new(area.x + x, area.y + y as u16, 1, 1),
        );
    }
}

type Renderer = fn(&mut Frame, Rect, Duration, Color);

fn measure(render: Renderer, width: u16, height: u16, frames: usize, full: bool) -> f64 {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut run = |i| {
        // Traverse the complete animation repeatedly, including its densest part.
        let elapsed = Duration::from_millis(((i % 150) * 33) as u64);
        if full {
            terminal.draw(|f| render(f, f.area(), elapsed, Color::Cyan)).unwrap();
        } else {
            let mut f = terminal.get_frame();
            let area = f.area();
            render(&mut f, area, elapsed, Color::Cyan);
            black_box(f.buffer_mut());
        }
    };
    for i in 0..300 { run(i); }
    let start = Instant::now();
    for i in 0..frames { run(i); }
    start.elapsed().as_secs_f64() * 1_000_000.0 / frames as f64
}

fn verify() {
    for (width, height) in [(1,1), (36,14), (100,30), (300,100)] {
        let mut old = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut new = Terminal::new(TestBackend::new(width, height)).unwrap();
        for ms in (0..=5000).step_by(33).chain([5000, 6000]) {
            let elapsed = Duration::from_millis(ms);
            old.draw(|f| original(f, f.area(), elapsed, Color::Cyan)).unwrap();
            new.draw(|f| effects::confetti(f, f.area(), elapsed, Color::Cyan)).unwrap();
            assert_eq!(old.backend().buffer(), new.backend().buffer(), "{width}x{height} at {ms}ms");
        }
    }
    println!("Pixel-equivalence checks passed (4 sizes, 154 timestamps each).");
}

fn main() {
    let frames = std::env::args().nth(1).unwrap_or_else(|| "3000".into()).parse::<usize>().unwrap();
    assert!(frames > 0);
    verify();
    println!("kind,size,original_us_per_frame,current_us_per_frame,speedup");
    for full in [false, true] {
        for (width, height) in [(100, 30), (300, 100)] {
            let mut old = Vec::new();
            let mut new = Vec::new();
            // Alternate ordering to reduce cache/frequency bias; report median.
            for round in 0..5 {
                for current in if round % 2 == 0 { [false,true] } else { [true,false] } {
                    let us = measure(if current { effects::confetti } else { original }, width, height, frames, full);
                    if current { new.push(us); } else { old.push(us); }
                }
            }
            old.sort_by(f64::total_cmp); new.sort_by(f64::total_cmp);
            println!("{},{width}x{height},{:.3},{:.3},{:.2}x", if full { "full_testbackend_frame" } else { "effect_only" },old[2],new[2],old[2]/new[2]);
        }
    }
}
