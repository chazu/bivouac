use bivouac_sim::rng::hash2;
use bivouac_sim::{ArmyState, Cell, FortKind, MAP_H, MAP_W, Seen, Sim, Terrain};
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
const ROOF: Color = rgb(0.55, 0.47, 0.50);
const FOG_OF_WAR: Color = Color { r: 0.01, g: 0.01, b: 0.03, a: 0.58 };

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
    draw_tracks(g, &v, app, sim);
    draw_forts(g, &v, l, sim, light, t);
    if !app.reveal {
        for x in x0.max(0)..=x_end {
            for y in 0..MAP_H {
                if !app.watching(sim, (x, y)) {
                    draw_rectangle(v.px(x), v.py(y as f32), g.cw, g.ch, FOG_OF_WAR);
                }
            }
        }
        draw_rumors(g, &v, app, sim, t);
    }
    draw_armies(g, &v, l, app, sim, light, t);
    draw_clashes(g, &v, app, sim, t);

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
        // Sweeping tiled roofs: one tier, two for a citadel.
        let roof = light.lit_bright(ROOF);
        let tiers: &[(i32, i32)] = if f.kind == FortKind::Main { &[(3, 2), (2, 3)] } else { &[(3, 2)] };
        for &(half, up) in tiers {
            let y = fy - up;
            draw_rectangle(
                v.px(fx - half),
                v.py(y as f32),
                (2 * half + 1) as f32 * g.cw,
                g.ch,
                light.lit(rgb(0.05, 0.05, 0.06), 1.0),
            );
            for dx in -half..=half {
                let c = if dx == -half {
                    '◢'
                } else if dx == half {
                    '◣'
                } else {
                    '▀'
                };
                g.put_px(v.px(fx + dx), v.py(y as f32), c, roof);
            }
            draw_line(
                v.px(fx - half),
                v.py(y as f32 + 0.5),
                v.px(fx + half + 1),
                v.py(y as f32 + 0.5),
                1.5,
                scale(color, 0.8),
            );
        }
        let roof_top = fy - tiers.last().map_or(1, |t| t.1);
        let rows = [['▛', '▀', '▀', '▀', '▜'], ['▌', fill, fill, fill, '▐'], ['▙', '▄', '▄', '▄', '▟']];
        for (dy, row) in rows.iter().enumerate() {
            for (dx, &c) in row.iter().enumerate() {
                let (x, y) = (fx - 2 + dx as i32, fy - 1 + dy as i32);
                g.put_px(v.px(x), v.py(y as f32), c, if dy == 1 && dx % 4 != 0 { scale(color, 0.7) } else { color });
            }
        }
        if f.owner.is_some() {
            // A tall banner hanging from a crossbar, stirring in the wind.
            let pole = v.px(fx) + g.cw * 0.5;
            let foot = v.py(roof_top as f32);
            let top = foot - g.ch * 1.3;
            draw_line(pole, foot, pole, top, 1.5, scale(color, 0.8));
            draw_line(pole, top + 2.0, pole + g.cw * 0.9, top + 2.0, 1.5, scale(color, 0.8));
            let sway = (t * 2.2 + fx as f32).sin() * g.cw * 0.18;
            let (l0, r0, y0, y1) = (pole + 1.0, pole + g.cw * 0.85, top + 3.0, top + g.ch * 1.05);
            draw_triangle(vec2(l0, y0), vec2(r0, y0), vec2(r0 + sway, y1), color);
            draw_triangle(vec2(l0, y0), vec2(r0 + sway, y1), vec2(l0 + sway, y1 - g.ch * 0.15), color);
        }
        if let Some((side, held)) = f.capture_progress() {
            let w = 5.0 * g.cw * held as f32 / bivouac_sim::fort::CAPTURE_MINUTES as f32;
            draw_rectangle(v.px(fx - 2), v.py(roof_top as f32) - 4.0, w, 3.0, palette::side(side));
        }
        g.label(v.px(fx) + g.cw * 0.5, v.py((fy + 2) as f32) + 2.0, f.name, mix(color, palette::TEXT, 0.5));
    }
}

/// Sparks where cells clash, and dust hanging over the fighting.
fn draw_clashes(g: &Grid, v: &View, app: &App, sim: &Sim, t: f32) {
    let frame = (t * 24.0) as i32;
    for (k, c) in sim.clashes.iter().enumerate() {
        if !app.watching(sim, c.a) && !app.watching(sim, c.b) {
            continue;
        }
        let (ax, ay) = (v.px(c.a.0) + g.cw * 0.5, v.py(c.a.1 as f32 + 0.5));
        let (bx, by) = (v.px(c.b.0) + g.cw * 0.5, v.py(c.b.1 as f32 + 0.5));
        let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
        for p in 0..3 {
            let h = hash2(frame, k as i32 * 7 + p, 13);
            if h.is_multiple_of(3) {
                continue;
            }
            let ang = (h % 628) as f32 / 100.0;
            let len = g.cw * (0.25 + (h % 50) as f32 / 100.0);
            let (dx, dy) = (ang.cos() * len, ang.sin() * len);
            let color =
                if h.is_multiple_of(2) { Color::new(1.0, 0.95, 0.7, 0.9) } else { Color::new(1.0, 0.55, 0.2, 0.85) };
            draw_line(mx, my, mx + dx, my + dy, 1.2, color);
        }
        // Dust rises and drifts: two puffs per clash on a slow loop.
        for p in 0..2 {
            let life = (t * 0.35 + (k as f32 * 0.37) + p as f32 * 0.5).fract();
            let r = g.cw * (0.4 + 1.2 * life);
            let rise = g.ch * 1.4 * life;
            draw_circle(
                mx + sim.wind as f32 * life * g.cw * 0.6,
                my - rise,
                r,
                Color::new(0.55, 0.5, 0.42, 0.16 * (1.0 - life)),
            );
        }
    }
}

fn draw_armies(g: &Grid, v: &View, l: &Layout, app: &App, sim: &Sim, light: &Light, t: f32) {
    let selected = app.selected_army(sim).map(|a| a.id);
    for a in &sim.armies {
        if !v.visible(a.pos.0, l.cols + 8) || !app.in_view(sim, a) {
            continue;
        }
        let mut color = light.lit_bright(palette::side(a.side));
        if a.state == ArmyState::Marching {
            color = scale(color, 0.85 + 0.15 * (t * 7.0 + a.pos.0 as f32).sin());
        }
        if a.exhausted() {
            color = scale(color, 0.55);
        }
        // Routing troops fade and jitter: a panicked crowd, not a formation.
        let (shake, routing) = if a.routing() {
            color = alpha(mix(color, palette::DIM, 0.5), 0.6 + 0.4 * (t * 9.0 + a.id as f32).sin().abs());
            ((t * 23.0 + a.id as f32).sin() * g.cw * 0.12, true)
        } else {
            (0.0, false)
        };
        // On a road the column rides the road line itself, curving with it.
        let column: Vec<(Cell, u32, f32)> = a
            .column()
            .into_iter()
            .map(|(c, n)| (c, n, sim.map.road_line_y(c.0, c.1).map_or(c.1 as f32, |ly| ly - 0.5)))
            .collect();
        for &((x, _), n, fy) in &column {
            let (px, py) = (v.px(x) + shake, v.py(fy));
            // A dark backing keeps the dots legible over forts, roads and forest.
            if !routing {
                draw_rectangle(px, py, g.cw, g.ch, Color::new(0.02, 0.02, 0.03, 0.75));
                draw_rectangle(px, py, g.cw, g.ch, alpha(palette::side(a.side), 0.14));
            }
            g.put_px(px, py, braille_char(DENSITY[(n - 1) as usize]), color);
            if a.entrench > 150 {
                // Earthworks thrown up in front of a dug-in army.
                let w = g.cw * (0.3 + 0.7 * a.entrench as f32 / 1000.0);
                let front = if a.side == bivouac_sim::Side::West { px + g.cw - 2.0 } else { px + 1.0 };
                draw_line(front, py + (g.ch - w) / 2.0, front, py + (g.ch + w) / 2.0, 2.0, light.lit_bright(MUD));
            }
        }
        if a.state == ArmyState::Resting && light.day < 0.5 {
            let flicker = 0.6 + 0.4 * (t * 13.0 + a.id as f32).sin() * (t * 7.3).cos();
            let (x, y) = a.pos;
            draw_circle(v.px(x) + g.cw * 0.5, v.py(y as f32) - 1.0, 2.2, Color::new(1.0, 0.6, 0.2, 0.8 * flicker));
        }
        if selected == Some(a.id) {
            let pulse = 0.5 + 0.4 * (t * 4.0).sin();
            for &((x, _), _, fy) in &column {
                draw_rectangle_lines(v.px(x), v.py(fy), g.cw, g.ch, 1.5, Color::new(1.0, 1.0, 1.0, pulse));
            }
            let mark = alpha(palette::side(a.side), 0.6);
            let on_ground = |(x, y): Cell| v.py(sim.map.road_line_y(x, y).unwrap_or(y as f32 + 0.5));
            for &c in &a.path {
                draw_circle(v.px(c.0) + g.cw * 0.5, on_ground(c), 1.8, mark);
            }
            if let Some(&c) = a.path.back() {
                draw_circle_lines(v.px(c.0) + g.cw * 0.5, on_ground(c), g.cw * 0.45, 1.5, mark);
            }
            if let Some(target) = a.target.and_then(|id| sim.army(id)).filter(|t| app.in_view(sim, t)) {
                let (from, to) = (a.pos, target.pos);
                let (x0, y0) = (v.px(from.0) + g.cw * 0.5, v.py(from.1 as f32 + 0.5));
                let (x1, y1) = (v.px(to.0) + g.cw * 0.5, v.py(to.1 as f32 + 0.5));
                let dashes = 12;
                for k in (0..dashes).step_by(2) {
                    let (s, e) = (k as f32 / dashes as f32, (k + 1) as f32 / dashes as f32);
                    let red = Color::new(1.0, 0.45, 0.35, 0.8);
                    draw_line(x0 + (x1 - x0) * s, y0 + (y1 - y0) * s, x0 + (x1 - x0) * e, y0 + (y1 - y0) * e, 1.5, red);
                }
            }
        }
    }
}

/// Footprints: your own faintly everywhere, the enemy's only where you can see.
fn draw_tracks(g: &Grid, v: &View, app: &App, sim: &Sim) {
    let now = sim.clock.minute;
    for t in &sim.tracks {
        let mine = t.side == app.player;
        if !v.visible(t.cell.0, 400) || (!mine && !app.watching(sim, t.cell)) {
            continue;
        }
        let fresh = 1.0 - (now - t.minute) as f32 / t.lifetime() as f32;
        let tint = mix(palette::side(t.side), MUD, if t.mud { 0.6 } else { 0.3 });
        let color = alpha(tint, fresh * if mine { 0.3 } else { 0.7 });
        let (x, y) = (v.px(t.cell.0), v.py(t.cell.1 as f32));
        let r = (g.cw * 0.09).max(1.0);
        for (dx, dy) in [(0.3, 0.7), (0.55, 0.45), (0.75, 0.75)] {
            draw_circle(x + g.cw * dx, y + g.ch * dy, r, color);
        }
    }
}

/// Enemies known only by rumor (a smudge of dust or firelight) or by memory
/// (a faded ghost where they were last seen).
fn draw_rumors(g: &Grid, v: &View, app: &App, sim: &Sim, t: f32) {
    let enemy = palette::side(app.player.other());
    let now = sim.clock.minute;
    for r in &sim.intel(app.player).reports {
        let (x, y) = r.cell;
        if !v.visible(x, 400) {
            continue;
        }
        match r.seen {
            Seen::Rumored => {
                let pulse = 0.35 + 0.15 * (t * 1.7 + x as f32).sin();
                for dx in -1..=1 {
                    g.put_px(v.px(x + dx), v.py(y as f32), '░', alpha(enemy, pulse * if dx == 0 { 1.0 } else { 0.6 }));
                }
                g.put_px(v.px(x), v.py(y as f32), '?', alpha(enemy, 0.9));
            }
            Seen::Remembered => {
                let age = (now - r.minute) as f32 / (24.0 * 60.0);
                let companies = (r.men / 50).clamp(1, 8);
                let ghost = alpha(enemy, (0.45 * (1.0 - age)).max(0.12));
                g.put_px(v.px(x), v.py(y as f32), braille_char(DENSITY[(companies - 1) as usize]), ghost);
                draw_rectangle_lines(v.px(x), v.py(y as f32), g.cw, g.ch, 1.0, alpha(enemy, 0.25));
            }
            _ => {}
        }
    }
}
