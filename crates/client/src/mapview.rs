use bivouac_sim::rng::hash2;
use bivouac_sim::{ArmyState, FortKind, MAP_H, MAP_W, Sim, Terrain};
use macroquad::prelude::*;

use crate::app::{App, Layout};
use crate::grid::{DENSITY, Grid, braille_char};
use crate::light::Light;
use crate::palette::{self, alpha, mix, rgb, scale};

const FIELD: Color = rgb(0.46, 0.52, 0.30);
const MUD: Color = rgb(0.50, 0.40, 0.26);
const FOREST: Color = rgb(0.18, 0.56, 0.25);
const ROCK: Color = rgb(0.56, 0.56, 0.62);
const SNOW: Color = rgb(0.88, 0.88, 0.94);
const WATER: Color = rgb(0.30, 0.50, 0.88);
const ROAD: Color = rgb(0.74, 0.62, 0.42);
const PLANKS: Color = rgb(0.46, 0.33, 0.19);

/// Converts map coordinates to pixels for the current scroll position.
struct View {
    x0: i32,
    frac: f32,
    top: f32,
    cw: f32,
    ch: f32,
}

impl View {
    fn px(&self, x: i32) -> f32 {
        (x - self.x0) as f32 * self.cw - self.frac
    }
    fn py(&self, y: f32) -> f32 {
        self.top + y * self.ch
    }
    fn visible(&self, x: i32, cols: i32) -> bool {
        x >= self.x0 - 1 && x <= self.x0 + cols + 1
    }
}

pub fn draw(g: &Grid, l: &Layout, app: &App, sim: &Sim, light: &Light, t: f32) {
    let x0 = app.scroll.floor() as i32;
    let v = View { x0, frac: (app.scroll - x0 as f32) * g.cw, top: l.map_top as f32 * g.ch, cw: g.cw, ch: g.ch };
    let depth = |y: i32| y as f32 / (MAP_H - 1) as f32;

    for y in 0..MAP_H {
        let bg = light.lit(mix(rgb(0.06, 0.075, 0.05), rgb(0.10, 0.11, 0.07), depth(y)), depth(y));
        draw_rectangle(0.0, v.py(y as f32), screen_width(), g.ch, bg);
    }

    let x_end = (x0 + l.cols + 1).min(MAP_W - 1);
    for x in x0.max(0)..=x_end {
        for y in 0..MAP_H {
            let Some((glyph, color)) = terrain_glyph(sim, x, y, light, t) else { continue };
            g.put_px(v.px(x), v.py(y as f32), glyph, light.lit(color, depth(y)));
        }
    }

    draw_roads(&v, sim, light, x0, x_end);
    draw_forts(g, &v, l, sim, light, t);
    draw_armies(g, &v, l, app, sim, light, t);

    if let Some((x, y)) = app.hover {
        draw_rectangle_lines(v.px(x), v.py(y as f32), g.cw, g.ch, 1.5, Color::new(1.0, 1.0, 1.0, 0.55));
    }
}

fn terrain_glyph(sim: &Sim, x: i32, y: i32, light: &Light, t: f32) -> Option<(char, Color)> {
    let h = hash2(x, y, 1);
    let vary = 0.8 + 0.3 * (h % 100) as f32 / 100.0;
    let road = sim.map.is_road(x, y);
    Some(match sim.map.terrain(x, y) {
        Terrain::Fort => return None,
        Terrain::River => {
            let wave = ((t * 1.6 + x as f32 * 0.31) as i32 + y) % 3 == 0;
            (if wave { '~' } else { '≈' }, scale(WATER, vary))
        }
        _ if road => return None,
        Terrain::Field => {
            let ground = mix(FIELD, MUD, light.rain * 0.7);
            let glyph = if (x + y) % 2 == 0 {
                '·'
            } else if h.is_multiple_of(13) {
                ','
            } else if h.is_multiple_of(29) && y > 8 {
                '"'
            } else {
                return None;
            };
            (glyph, scale(ground, vary))
        }
        Terrain::Forest => ('♣', scale(FOREST, vary)),
        Terrain::Mountain => {
            // The far ridge is hazier and more varied, so it reads as distance, not a border.
            let top = y == 0 || sim.map.terrain(x, y - 1) != Terrain::Mountain;
            let snowy = top && h.is_multiple_of(3);
            let glyph = if y == 0 && h % 4 == 1 { '^' } else { '▲' };
            let haze = if y == 0 { 0.7 } else { 1.0 };
            (glyph, scale(if snowy { SNOW } else { ROCK }, vary * haze))
        }
    })
}

/// Roads are lines between quantized heights at each column edge, so they curve
/// smoothly but still land on the same cells the sim walks.
fn draw_roads(v: &View, sim: &Sim, light: &Light, x0: i32, x_end: i32) {
    let color = light.lit_bright(ROAD);
    for (i, road) in sim.map.roads.iter().enumerate() {
        let (a, b) = road.x_range();
        let thick = v.ch * if i == 0 { 0.16 } else { 0.11 };
        for x in a.max(x0 - 1)..b.min(x_end + 1) {
            for y in road.rows_in_column(x) {
                if sim.map.is_bridge(x, y) {
                    let mid = v.py(y as f32 + 0.5);
                    draw_rectangle(v.px(x), mid - v.ch * 0.22, v.cw, v.ch * 0.44, light.lit_bright(PLANKS));
                }
            }
            let (ya, yb) = (road.edge_y(x), road.edge_y(x + 1));
            draw_line(v.px(x), v.py(ya), v.px(x + 1), v.py(yb), thick, color);
        }
    }
}

fn draw_forts(g: &Grid, v: &View, l: &Layout, sim: &Sim, light: &Light, t: f32) {
    for f in &sim.forts {
        let (fx, fy) = f.pos;
        if !v.visible(fx, l.cols + 3) {
            continue;
        }
        let base = palette::side_or_stone(f.owner);
        let color = light.lit_bright(base);
        let fill = if f.kind == FortKind::Main { '▓' } else { '▒' };
        let rows = [['▛', '▀', '▀', '▀', '▜'], ['▌', fill, fill, fill, '▐'], ['▙', '▄', '▄', '▄', '▟']];
        for (dy, row) in rows.iter().enumerate() {
            for (dx, &c) in row.iter().enumerate() {
                let (x, y) = (fx - 2 + dx as i32, fy - 1 + dy as i32);
                g.put_px(v.px(x), v.py(y as f32), c, if dy == 1 && dx % 4 != 0 { scale(color, 0.7) } else { color });
            }
        }
        if f.owner.is_some() {
            let pole = v.px(fx) + g.cw * 0.5;
            let foot = v.py((fy - 1) as f32);
            let top = foot - g.ch * 0.95;
            draw_line(pole, foot, pole, top, 1.5, scale(color, 0.8));
            let wave = (t * 3.0 + fx as f32).sin() * g.ch * 0.08;
            draw_triangle(
                vec2(pole, top),
                vec2(pole + g.cw * 1.3, top + g.ch * 0.18 + wave),
                vec2(pole, top + g.ch * 0.4),
                color,
            );
        }
        if let Some((side, held)) = f.capture_progress() {
            let w = 5.0 * g.cw * held as f32 / bivouac_sim::fort::CAPTURE_MINUTES as f32;
            draw_rectangle(v.px(fx - 2), v.py((fy - 1) as f32) - 4.0, w, 3.0, palette::side(side));
        }
        g.label(v.px(fx) + g.cw * 0.5, v.py((fy + 2) as f32) + 2.0, f.name, mix(color, palette::TEXT, 0.5));
    }
}

fn draw_armies(g: &Grid, v: &View, l: &Layout, app: &App, sim: &Sim, light: &Light, t: f32) {
    let selected = app.selected_army(sim).map(|a| a.id);
    for a in &sim.armies {
        if !v.visible(a.pos.0, l.cols + 8) {
            continue;
        }
        let mut color = light.lit_bright(palette::side(a.side));
        if a.state == ArmyState::Marching {
            color = scale(color, 0.85 + 0.15 * (t * 7.0 + a.pos.0 as f32).sin());
        }
        if a.exhausted() {
            color = scale(color, 0.55);
        }
        let column = a.column();
        for &((x, y), n) in &column {
            // A dark backing keeps the dots legible over forts, roads and forest.
            draw_rectangle(v.px(x), v.py(y as f32), g.cw, g.ch, Color::new(0.02, 0.02, 0.03, 0.75));
            draw_rectangle(v.px(x), v.py(y as f32), g.cw, g.ch, alpha(palette::side(a.side), 0.14));
            g.put_px(v.px(x), v.py(y as f32), braille_char(DENSITY[(n - 1) as usize]), color);
        }
        if a.state == ArmyState::Resting && light.day < 0.5 {
            let flicker = 0.6 + 0.4 * (t * 13.0 + a.id as f32).sin() * (t * 7.3).cos();
            let (x, y) = a.pos;
            draw_circle(v.px(x) + g.cw * 0.5, v.py(y as f32) - 1.0, 2.2, Color::new(1.0, 0.6, 0.2, 0.8 * flicker));
        }
        if selected == Some(a.id) {
            let pulse = 0.5 + 0.4 * (t * 4.0).sin();
            for &((x, y), _) in &column {
                draw_rectangle_lines(v.px(x), v.py(y as f32), g.cw, g.ch, 1.5, Color::new(1.0, 1.0, 1.0, pulse));
            }
            let mark = alpha(palette::side(a.side), 0.6);
            for &(x, y) in &a.path {
                draw_circle(v.px(x) + g.cw * 0.5, v.py(y as f32 + 0.5), 1.8, mark);
            }
            if let Some(&(x, y)) = a.path.back() {
                draw_circle_lines(v.px(x) + g.cw * 0.5, v.py(y as f32 + 0.5), g.cw * 0.45, 1.5, mark);
            }
        }
    }
}
