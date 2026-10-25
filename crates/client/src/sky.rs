use bivouac_sim::Sim;
use bivouac_sim::rng::hash2;
use macroquad::prelude::*;

use crate::app::Layout;
use crate::grid::{Grid, quadrant_char};
use crate::light::Light;
use crate::palette::{alpha, fbm, mix, rgb, scale};

const BLOOD_MOON_RADIUS: f32 = 5.2;
const MOON_RADIUS: f32 = 3.0;
/// The sun is old and swollen: big, dim enough to look at, spotted.
const SUN_RADIUS: f32 = 3.4;

/// Sky band: gradient, stars, sun or moon, clouds.
pub fn draw(g: &Grid, l: &Layout, light: &Light, sim: &Sim, t: f32) {
    let top = l.sky_top as f32 * g.ch;
    let height = l.sky_rows as f32 * g.ch;
    const BANDS: i32 = 48;
    for i in 0..BANDS {
        let v = i as f32 / (BANDS - 1) as f32;
        draw_rectangle(
            0.0,
            top + height * i as f32 / BANDS as f32,
            screen_width(),
            height / BANDS as f32 + 1.0,
            light.sky(v),
        );
    }

    if l.sky_rows < 6 {
        draw_strip(g, l, light, sim, t);
        return;
    }

    // Stars, fading with daylight and cloud.
    let star_alpha = (1.0 - light.day) * (1.0 - light.cloud);
    if star_alpha > 0.02 {
        for r in 0..l.sky_rows - 1 {
            for c in 0..l.cols {
                let h = hash2(c, r, 99);
                if !h.is_multiple_of(37) {
                    continue;
                }
                let glyph = if h.is_multiple_of(7) {
                    '*'
                } else if h.is_multiple_of(5) {
                    '+'
                } else {
                    '.'
                };
                let twinkle = 0.55 + 0.45 * (t * (0.7 + (h % 5) as f32 * 0.4) + (h % 100) as f32).sin();
                g.put(c, l.sky_top + r, glyph, alpha(rgb(0.85, 0.86, 1.0), star_alpha * twinkle));
            }
        }
    }

    let horizon = (l.sky_top + l.sky_rows) as f32;
    let span = l.sky_rows as f32;

    if light.sun.alt > -0.1 {
        let cx = light.sun.x * l.cols as f32;
        let cy = horizon - light.sun.alt * (span - SUN_RADIUS - 1.0);
        let low = 1.0 - light.sun.alt.clamp(0.0, 0.5) * 2.0;
        let color = mix(rgb(1.0, 0.62, 0.30), rgb(0.95, 0.30, 0.12), low);
        glow(g, cx, cy, SUN_RADIUS, color, 0.07 * (1.0 - light.cloud * 0.5));
        let churn = t * 0.03;
        disc(g, l, cx, cy, SUN_RADIUS, color, Color::new(0.0, 0.0, 0.0, 0.0), |u, v| {
            let spots = fbm(u * 3.0 + churn, v * 3.0 + 40.0, 9);
            let limb = (1.0 - (u * u + v * v)).max(0.0).sqrt();
            let dark = if spots > 0.68 { 0.45 } else { 1.0 };
            Some(((0.55 + 0.45 * limb) * dark).min(1.0))
        });
    }

    if light.moon.alt > -0.1 {
        let (radius, bright, dark) = if light.blood {
            let pulse = 0.92 + 0.08 * (t * 1.1).sin();
            (BLOOD_MOON_RADIUS, scale(rgb(0.95, 0.14, 0.09), pulse), rgb(0.22, 0.03, 0.03))
        } else {
            (MOON_RADIUS, rgb(0.95, 0.93, 0.84), alpha(rgb(0.32, 0.34, 0.42), 0.35))
        };
        let cx = light.moon.x * l.cols as f32;
        let cy = horizon - light.moon.alt * (span - radius - 0.6);
        let strength =
            if light.blood { 0.06 } else { 0.03 } * light.illum * (1.0 - light.cloud * 0.6) * (1.0 - light.day * 0.8);
        glow(g, cx, cy, radius, bright, strength);
        let waxing = light.phase < 14.0;
        let k = (std::f32::consts::TAU * light.phase / 28.0).cos();
        disc(g, l, cx, cy, radius, scale(bright, 1.0 - light.day * 0.4), dark, |u, v| {
            let edge = (1.0 - v * v).max(0.0).sqrt();
            let along = if waxing { u } else { -u };
            (along >= k * edge).then(|| {
                let craters = fbm(u * 2.4 + 5.0, v * 2.4 + 9.0, 3);
                let limb = (1.0 - (u * u + v * v)).max(0.0).sqrt();
                ((0.45 + 0.7 * craters) * (0.7 + 0.3 * limb)).min(1.0)
            })
        });
    }

    clouds(g, l, light, sim, t, 0, l.sky_rows);
}

/// Collapsed sky: a thin weather strip.
fn draw_strip(g: &Grid, l: &Layout, light: &Light, sim: &Sim, t: f32) {
    clouds(g, l, light, sim, t, 0, l.sky_rows);
    if light.rain > 0.3 {
        let shift = (t * 8.0) as i32;
        for r in 0..l.sky_rows {
            for c in 0..l.cols {
                if hash2(c + shift, r - shift, 5).is_multiple_of(4) {
                    g.put(c, l.sky_top + r, '╱', alpha(rgb(0.6, 0.7, 0.9), 0.5 * light.rain));
                }
            }
        }
    }
    for (body, glyph, color) in [
        (light.sun, '●', rgb(1.0, 0.85, 0.4)),
        (
            light.moon,
            if light.blood { '●' } else { '◐' },
            if light.blood { rgb(0.95, 0.15, 0.1) } else { rgb(0.9, 0.9, 0.85) },
        ),
    ] {
        if body.up() {
            let row = l.sky_top + if body.alt > 0.5 { 0 } else { l.sky_rows - 1 };
            g.put((body.x * l.cols as f32) as i32, row, glyph, color);
        }
    }
}

fn clouds(g: &Grid, l: &Layout, light: &Light, sim: &Sim, t: f32, from: i32, to: i32) {
    if light.cloud < 0.05 {
        return;
    }
    let drift = sim.clock.minute as f32 * 0.003 * sim.wind as f32 + t * 0.12;
    let threshold = 0.9 - light.cloud * 0.75;
    let lit = mix(rgb(0.20, 0.20, 0.26), rgb(0.92, 0.92, 0.96), light.day);
    let color = mix(lit, rgb(0.40, 0.08, 0.08), light.red * 0.8);
    for r in from..to {
        for c in 0..l.cols {
            let n = fbm(c as f32 * 0.045 - drift, r as f32 * 0.32 + 11.0, 21);
            if n <= threshold {
                continue;
            }
            let d = ((n - threshold) / (1.0 - threshold)).min(1.0);
            let row = l.sky_top + r;
            // A soft body that dims what's behind it (the moon included), with dithered texture on top.
            g.fill(c, row, 1, 1, alpha(scale(color, 0.55), (d * 1.4).min(0.75)));
            let glyph = if d < 0.35 {
                '░'
            } else if d < 0.7 {
                '▒'
            } else {
                '▓'
            };
            g.put(c, row, glyph, alpha(color, 0.12 + 0.25 * d));
        }
    }
}

/// Soft halo around a body; `cx`, `cy` and `radius` are in cells.
fn glow(g: &Grid, cx: f32, cy: f32, radius: f32, color: Color, strength: f32) {
    if strength <= 0.0 {
        return;
    }
    let (px, py, r) = (cx * g.cw, cy * g.ch, radius * g.ch);
    for i in (1..=14).rev() {
        draw_circle(px, py, r * (1.0 + i as f32 * 0.22), alpha(color, strength));
    }
}

/// Draws a disc as block glyphs, with quadrant blocks for smooth edges.
/// `shade(u, v)` gets disc coordinates (-1..1) and returns brightness if lit.
#[allow(clippy::too_many_arguments)]
fn disc(
    g: &Grid,
    l: &Layout,
    cx: f32,
    cy: f32,
    r_rows: f32,
    bright: Color,
    dark: Color,
    shade: impl Fn(f32, f32) -> Option<f32>,
) {
    let r_cols = r_rows * g.ch / g.cw;
    let rows = (cy - r_rows).floor() as i32..=(cy + r_rows).ceil() as i32;
    for row in rows.filter(|r| (l.sky_top..l.sky_top + l.sky_rows).contains(r)) {
        for col in ((cx - r_cols).floor() as i32..=(cx + r_cols).ceil() as i32).filter(|c| (0..l.cols).contains(c)) {
            let (mut lit, mut unlit, mut sum) = (0u8, 0u8, 0.0);
            for (q, (ox, oy)) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)].into_iter().enumerate() {
                let u = (col as f32 + ox - cx) / r_cols;
                let v = (row as f32 + oy - cy) / r_rows;
                if u * u + v * v > 1.0 {
                    continue;
                }
                match shade(u, v) {
                    Some(b) => {
                        lit |= 1 << q;
                        sum += b;
                    }
                    None => unlit |= 1 << q,
                }
            }
            if unlit != 0 {
                g.put(col, row, quadrant_char(unlit), dark);
            }
            if lit == 15 {
                let b = sum / 4.0;
                g.fill(col, row, 1, 1, scale(bright, 0.55 * b));
                let glyph = if b > 0.85 {
                    '█'
                } else if b > 0.68 {
                    '▓'
                } else if b > 0.5 {
                    '▒'
                } else {
                    '░'
                };
                g.put(col, row, glyph, scale(bright, (b * 1.1).min(1.0)));
            } else if lit != 0 {
                g.put(col, row, quadrant_char(lit), scale(bright, sum / lit.count_ones() as f32));
            }
        }
    }
}

/// Effects over the land: blood-moon light, fog banks, rain.
pub fn draw_overlays(g: &Grid, l: &Layout, light: &Light, sim: &Sim, t: f32) {
    let map_top = l.map_top as f32 * g.ch;
    let map_h = bivouac_sim::MAP_H as f32 * g.ch;

    if light.red > 0.0 {
        draw_rectangle(0.0, map_top, screen_width(), map_h, Color::new(0.45, 0.0, 0.02, 0.06 * light.red));
        if l.sky_rows >= 6 {
            let horizon = (l.sky_top + l.sky_rows) as f32;
            let cy = horizon - light.moon.alt * (l.sky_rows as f32 - BLOOD_MOON_RADIUS - 0.6);
            let (px, py) = (light.moon.x * l.cols as f32 * g.cw, cy * g.ch);
            for i in 1..=10 {
                draw_circle(
                    px,
                    py,
                    g.ch * (BLOOD_MOON_RADIUS + i as f32 * 1.6),
                    Color::new(0.6, 0.02, 0.02, 0.012 * light.red),
                );
            }
        }
    }

    if light.fog > 0.02 {
        let x0 = 0;
        for y in 0..bivouac_sim::MAP_H {
            let lowland = 0.35 + 0.65 * (y as f32 / (bivouac_sim::MAP_H - 1) as f32);
            for c in x0..=l.cols {
                let n = fbm(c as f32 * 0.11 + t * 0.05, y as f32 * 0.45 + t * 0.02, 41);
                let d = ((n - 0.3).max(0.0) * 1.8 * light.fog * lowland).min(0.85);
                if d < 0.03 {
                    continue;
                }
                let fog = light.lit(rgb(0.78, 0.78, 0.82), 1.0);
                let fog = mix(fog, rgb(0.55, 0.57, 0.62), 0.5);
                g.fill(c, l.map_top + y, 1, 1, alpha(fog, d));
                if d > 0.45 && hash2(c, y, 3).is_multiple_of(3) {
                    g.put(c, l.map_top + y, '░', alpha(fog, d));
                }
            }
        }
    }

    if light.rain > 0.02 {
        let top = l.sky_top as f32 * g.ch;
        let h = map_top + map_h - top;
        let w = screen_width();
        let slant = sim.wind as f32 * 2.0;
        let drops = (500.0 * light.rain) as u32;
        for i in 0..drops {
            let hx = hash2(i as i32, 1, 77) as f32 / u32::MAX as f32;
            let hy = hash2(i as i32, 2, 77) as f32 / u32::MAX as f32;
            let speed = 0.8 + 0.4 * (hash2(i as i32, 3, 77) % 100) as f32 / 100.0;
            let y = (hy * h + t * h * speed * 0.9).rem_euclid(h);
            let x = (hx * w + y * slant * 0.02).rem_euclid(w);
            draw_line(
                x,
                top + y,
                x + slant,
                top + y + g.ch * 0.6,
                1.0,
                Color::new(0.62, 0.70, 0.88, 0.35 * light.rain),
            );
        }
    }
}
