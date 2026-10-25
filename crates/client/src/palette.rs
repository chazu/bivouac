use bivouac_sim::Side;
use bivouac_sim::rng::hash2;
use macroquad::prelude::Color;

pub const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

/// The Azure Remnant.
pub const WEST: Color = rgb(0.36, 0.72, 0.96);
/// The Vermilion Host.
pub const EAST: Color = rgb(0.97, 0.34, 0.22);
pub const STONE: Color = rgb(0.62, 0.60, 0.55);
pub const TEXT: Color = rgb(0.86, 0.84, 0.78);
pub const DIM: Color = rgb(0.52, 0.51, 0.48);
pub const HEADING: Color = rgb(0.95, 0.80, 0.50);
pub const AMBER: Color = rgb(1.00, 0.72, 0.30);
pub const PANEL_BG: Color = rgb(0.045, 0.045, 0.055);
pub const LINE: Color = rgb(0.22, 0.21, 0.24);

pub fn side(s: Side) -> Color {
    match s {
        Side::West => WEST,
        Side::East => EAST,
    }
}

pub fn scale(c: Color, k: f32) -> Color {
    Color { r: c.r * k, g: c.g * k, b: c.b * k, a: c.a }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in 0..1.
pub fn vnoise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |dx: i32, dy: i32| hash2(xi as i32 + dx, yi as i32 + dy, seed) as f32 / u32::MAX as f32;
    let top = h(0, 0) + (h(1, 0) - h(0, 0)) * sx;
    let bottom = h(0, 1) + (h(1, 1) - h(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// Three octaves of value noise, in 0..1.
pub fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    0.5 * vnoise(x, y, seed) + 0.3 * vnoise(x * 2.03, y * 2.03, seed + 1) + 0.2 * vnoise(x * 4.1, y * 4.1, seed + 2)
}

pub fn side_or_stone(s: Option<Side>) -> Color {
    s.map_or(STONE, side)
}
