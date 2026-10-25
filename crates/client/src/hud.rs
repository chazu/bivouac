use bivouac_sim::clock::{hhmm, moon_phase_name};
use bivouac_sim::path::FOREST_CAP;
use bivouac_sim::{Army, ArmyState, Cell, Fort, MAP_W, Side, Sim, Terrain};
use macroquad::prelude::*;

use crate::app::{App, Layout, Selection};
use crate::grid::{DENSITY, Grid, braille_char};
use crate::light::Light;
use crate::palette::{self, AMBER, DIM, HEADING, LINE, PANEL_BG, TEXT, rgb};

pub fn draw(g: &Grid, l: &Layout, app: &App, sim: &Sim, light: &Light) {
    status_bar(g, l, app, sim, light);
    minimap(g, l, app, sim);
    panel(g, l, app, sim);
}

/// Writes lines down a column, clipping to its width and height.
struct Pen<'a> {
    g: &'a Grid,
    col: i32,
    row: i32,
    width: i32,
    bottom: i32,
}

impl Pen<'_> {
    fn line(&mut self, s: &str, color: Color) {
        self.segs(&[(s, color)]);
    }

    fn segs(&mut self, parts: &[(&str, Color)]) {
        if self.row >= self.bottom {
            return;
        }
        let mut c = self.col;
        for &(s, color) in parts {
            c += self.g.text_clip(c, self.row, s, self.col + self.width - c, color);
        }
        self.row += 1;
    }

    /// Word-wraps `s`, indenting continuation lines by `indent` columns.
    fn wrap(&mut self, s: &str, indent: i32, color: Color) {
        let mut line = String::new();
        let mut first = true;
        for word in s.split(' ') {
            let limit = (self.width - if first { 0 } else { indent }) as usize;
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > limit {
                self.indented(&line, if first { 0 } else { indent }, color);
                line.clear();
                first = false;
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            self.indented(&line, if first { 0 } else { indent }, color);
        }
    }

    fn indented(&mut self, s: &str, indent: i32, color: Color) {
        let col = self.col;
        self.col += indent;
        self.width -= indent;
        self.line(s, color);
        self.col = col;
        self.width += indent;
    }

    fn gap(&mut self) {
        self.row += 1;
    }
}

fn bar(frac: f32, n: usize) -> String {
    let k = ((frac.clamp(0.0, 1.0) * n as f32).round() as usize).min(n);
    format!("{}{}", "▰".repeat(k), "▱".repeat(n - k))
}

fn duration(minutes: u32) -> String {
    match minutes {
        m if m < 60 => format!("{m}m"),
        m if m < 48 * 60 => format!("{}h {:02}m", m / 60, m % 60),
        m => format!("{:.1} days", m as f32 / 1440.0),
    }
}

fn side_name(s: Option<Side>, player: Side) -> &'static str {
    match s {
        Some(s) if s == player => "yours",
        Some(_) => "enemy",
        None => "neutral",
    }
}

fn status_bar(g: &Grid, l: &Layout, app: &App, sim: &Sim, light: &Light) {
    g.fill(0, 0, l.cols + 1, 1, rgb(0.07, 0.065, 0.075));
    let clock = &sim.clock;
    let phase = clock.moon_phase();
    let moon = if clock.is_blood_moon() {
        format!("● blood moon ({})", moon_phase_name(phase).trim_end_matches(" moon"))
    } else {
        let glyph = match phase {
            0 | 27 => '○',
            1..=13 => '◑',
            14 => '●',
            _ => '◐',
        };
        format!("{glyph} {}", moon_phase_name(phase))
    };
    let wind = match sim.wind {
        0 => "calm".to_string(),
        w => format!(
            "wind {} {}",
            if w < 0 { '←' } else { '→' },
            ["light", "fresh", "strong"][(w.unsigned_abs() - 1) as usize]
        ),
    };
    let when = if light.day > 0.5 { "day" } else { "night" };
    let right = if app.paused {
        "PAUSED  [Space] resume ".to_string()
    } else {
        format!("▶ ×{}   [Space] pause ", app.speed())
    };
    let right_col = l.cols - right.chars().count() as i32;
    g.text(right_col, 0, &right, if app.paused { AMBER } else { TEXT });

    let mut c = 1;
    c += g.text(c, 0, "BIVOUAC", HEADING);
    let rest = format!(
        "  ·  Day {}, {} {}  ·  {}  ·  {}  ·  {}",
        clock.day(),
        hhmm(clock.minute),
        when,
        moon,
        sim.weather.label(),
        wind
    );
    let color = if clock.is_blood_moon() && light.red > 0.1 { rgb(1.0, 0.55, 0.5) } else { TEXT };
    g.text_clip(c, 0, &rest, right_col - c - 2, color);
}

fn minimap(g: &Grid, l: &Layout, app: &App, sim: &Sim) {
    let top = l.minimap_top;
    g.fill(0, top, l.cols + 1, 2, rgb(0.05, 0.05, 0.06));
    let (x0, w) = l.minimap_span();
    let col_of = |x: i32| x0 + x * w / MAP_W;
    for c in 0..w {
        g.put(x0 + c, top, '·', rgb(0.25, 0.27, 0.22));
    }
    for f in &sim.forts {
        g.put(col_of(f.pos.0), top, '■', palette::side_or_stone(f.owner));
    }
    for a in &sim.armies {
        g.put(col_of(a.pos.0), top, '•', palette::side(a.side));
    }
    g.put(x0 - 1, top, '◄', DIM);
    g.put(x0 + w, top, '►', DIM);
    let vx = x0 as f32 + app.scroll * w as f32 / MAP_W as f32;
    let vw = l.cols as f32 * w as f32 / MAP_W as f32;
    draw_rectangle_lines(
        vx * g.cw,
        top as f32 * g.ch + 1.0,
        vw * g.cw,
        g.ch - 2.0,
        1.5,
        Color::new(1.0, 1.0, 1.0, 0.5),
    );

    let count = |owner: Option<Side>| sim.forts.iter().filter(|f| f.owner == owner).count();
    let summary = format!(
        "forts: yours {} · theirs {} · neutral {}",
        count(Some(app.player)),
        count(Some(app.player.other())),
        count(None)
    );
    g.text(x0 + w + 3, top, &summary, DIM);
    g.text(x0, top + 1, "[←→ / wheel] scroll   [click] jump   [S] sky   [Tab] next army   [C] center", DIM);
}

fn panel(g: &Grid, l: &Layout, app: &App, sim: &Sim) {
    let top = l.panel_top;
    let rows = l.rows - top;
    if rows <= 1 {
        return;
    }
    g.fill(0, top, l.cols + 1, rows + 1, PANEL_BG);
    draw_line(0.0, top as f32 * g.ch, screen_width(), top as f32 * g.ch, 1.0, LINE);
    let w1 = l.cols * 34 / 100;
    let w2 = l.cols * 36 / 100;
    for x in [w1, w1 + w2] {
        let px = x as f32 * g.cw - g.cw * 0.5;
        draw_line(px, top as f32 * g.ch + 4.0, px, screen_height() - 4.0, 1.0, LINE);
    }
    let bottom = l.rows;
    let mut hover = Pen { g, col: 1, row: top + 1, width: w1 - 2, bottom };
    let mut sel = Pen { g, col: w1 + 1, row: top + 1, width: w2 - 2, bottom };
    let mut log = Pen { g, col: w1 + w2 + 1, row: top + 1, width: l.cols - w1 - w2 - 2, bottom };

    hover_column(&mut hover, app, sim);
    selected_column(&mut sel, app, sim);

    log.line("LOG", HEADING);
    for e in &sim.log {
        let color = match e.side {
            Some(s) if s == app.player => TEXT,
            Some(_) => rgb(1.0, 0.6, 0.55),
            None => AMBER,
        };
        log.wrap(&format!("{} {}", hhmm(e.minute), e.text), 6, color);
    }

    if let Some(msg) = app.flash() {
        g.fill(w1 + 1, bottom - 1, w2 - 2, 1, PANEL_BG);
        g.text_clip(w1 + 1, bottom - 1, msg, w2 - 2, AMBER);
    }
}

fn terrain_words(sim: &Sim, (x, y): Cell) -> String {
    let m = &sim.map;
    if m.is_bridge(x, y) {
        return "road · bridge".into();
    }
    if m.is_road(x, y) {
        let name = m.roads.iter().find(|r| r.rows_in_column(x).contains(&y)).map_or("road", |r| r.name);
        return format!("road · {name}");
    }
    match m.terrain(x, y) {
        Terrain::Field => "open field · slow, tiring".into(),
        Terrain::Forest => format!("forest · at most {FOREST_CAP} men"),
        Terrain::Mountain => "mountain · impassable".into(),
        Terrain::River => "river · impassable".into(),
        Terrain::Fort => "fortification".into(),
    }
}

fn army_lines(pen: &mut Pen, a: &Army, player: Side) {
    let glyph = braille_char(DENSITY[(a.companies().min(8) - 1) as usize]).to_string();
    let who = if a.side == player { "yours" } else { "enemy" };
    pen.segs(&[
        (&glyph, palette::side(a.side)),
        (&format!(" {} ({who})", a.name), TEXT),
        (&format!("  {} men", a.men), DIM),
    ]);
    let state = match a.state {
        ArmyState::Marching => "marching",
        ArmyState::Resting => "resting",
        ArmyState::Holding => "holding",
    };
    pen.segs(&[
        ("  stamina ", DIM),
        (&bar(a.stamina as f32 / 1000.0, 8), stamina_color(a)),
        (&format!(" {:>3}%  {state}", a.stamina / 10), TEXT),
    ]);
    pen.segs(&[
        ("  food    ", DIM),
        (&bar(a.food_days() / 5.0, 8), if a.food_days() < 1.0 { AMBER } else { TEXT }),
        (&format!(" {:.1} days", a.food_days()), TEXT),
    ]);
}

fn stamina_color(a: &Army) -> Color {
    if a.exhausted() { AMBER } else { rgb(0.55, 0.85, 0.55) }
}

fn hover_column(pen: &mut Pen, app: &App, sim: &Sim) {
    let Some(c) = app.hover else {
        pen.line("HOVER", HEADING);
        pen.line("Point at the map for details.", DIM);
        return;
    };
    pen.segs(&[("HOVER  ", HEADING), (&terrain_words(sim, c), TEXT)]);
    if let Some(f) = sim.fort_at(c) {
        pen.segs(&[
            (f.name, palette::side_or_stone(f.owner)),
            (&format!(" · {} · {}", f.kind.label(), side_name(f.owner, app.player)), DIM),
        ]);
    }
    let here: Vec<&Army> = sim.armies.iter().filter(|a| a.occupies(c)).collect();
    for a in here.iter().take(3) {
        pen.gap();
        army_lines(pen, a, app.player);
    }
    if here.len() > 3 {
        pen.line(&format!("  …and {} more", here.len() - 3), DIM);
    }
}

fn selected_column(pen: &mut Pen, app: &App, sim: &Sim) {
    match app.selected {
        Some(Selection::Army(id)) => {
            if let Some(a) = sim.army(id) {
                army_detail(pen, a, app, sim);
            }
        }
        Some(Selection::Fort(id)) => {
            if let Some(f) = sim.forts.iter().find(|f| f.id == id) {
                fort_detail(pen, f, app);
            }
        }
        None => {
            pen.line("SELECTED", HEADING);
            pen.line("Click an army or fort to select it.", DIM);
            pen.line("Right-click the map to march there.", DIM);
            pen.gap();
            pen.line("[R]est  [H]old  [D]ivide  [J]oin", DIM);
            pen.line("[1-4] speed  [Space] pause", DIM);
        }
    }
}

fn army_detail(pen: &mut Pen, a: &Army, app: &App, sim: &Sim) {
    pen.segs(&[("SELECTED  ", HEADING), (&a.name, palette::side(a.side))]);
    pen.segs(&[("men      ", DIM), (&format!("{}  ({} companies)", a.men, a.companies()), TEXT)]);
    pen.segs(&[
        ("stamina  ", DIM),
        (&bar(a.stamina as f32 / 1000.0, 10), stamina_color(a)),
        (&format!(" {}%{}", a.stamina / 10, if a.exhausted() { "  exhausted" } else { "" }), TEXT),
    ]);
    let per_day = if a.state == ArmyState::Marching { "-1.25/day marching" } else { "-1/day" };
    pen.segs(&[
        ("food     ", DIM),
        (&bar(a.food_days() / 5.0, 10), if a.food_days() < 1.0 { AMBER } else { TEXT }),
        (&format!(" {:.1} d  ({per_day})", a.food_days()), TEXT),
    ]);
    let order = match (a.state, a.path.back()) {
        (ArmyState::Marching, Some(&dest)) => {
            let place = sim.fort_at(dest).map_or_else(|| format!("({}, {})", dest.0, dest.1), |f| f.name.to_string());
            format!("march → {place}  ·  ETA {}", duration(sim.eta_minutes(a)))
        }
        (ArmyState::Resting, _) => "resting".into(),
        _ => "holding".into(),
    };
    pen.segs(&[("order    ", DIM), (&order, TEXT)]);
    if a.starving {
        pen.line("STARVING: stamina is draining", AMBER);
    }
    pen.gap();
    if a.side == app.player {
        pen.line("[RMB] march  [R]est  [H]old  [D]ivide  [J]oin", DIM);
    } else {
        pen.line("Enemy army: you can't command it.", DIM);
    }
}

fn fort_detail(pen: &mut Pen, f: &Fort, app: &App) {
    pen.segs(&[("SELECTED  ", HEADING), (f.name, palette::side_or_stone(f.owner))]);
    pen.segs(&[
        (&format!("{} · {}", f.kind.label(), side_name(f.owner, app.player)), DIM),
        (&format!("  · garrison {}", f.garrison), DIM),
    ]);
    let names = bivouac_sim::fort::GOODS;
    let stores: Vec<String> = names.iter().zip(f.stores).map(|(n, s)| format!("{n} {s}")).collect();
    pen.segs(&[("stores   ", DIM), (&stores.join("  "), TEXT)]);
    let total = f.total_stores();
    let lasts = total.checked_div(f.garrison).map_or(String::new(), |d| format!("  ·  ~{d} days for garrison"));
    pen.segs(&[
        ("capacity ", DIM),
        (&bar(total as f32 / f.capacity() as f32, 10), TEXT),
        (&format!(" {total}/{}{lasts}", f.capacity()), TEXT),
    ]);
    let prod = f.daily_production();
    let made: Vec<String> = names.iter().zip(prod).filter(|(_, p)| *p > 0).map(|(n, p)| format!("+{p} {n}")).collect();
    let made = if made.is_empty() { "nothing".to_string() } else { made.join("  ") };
    pen.segs(&[("per day  ", DIM), (&format!("{made}  -{} eaten", f.garrison), TEXT)]);
    let extras: Vec<String> = f.extras.iter().map(|e| e.label()).collect();
    if !extras.is_empty() {
        pen.segs(&[("extras   ", DIM), (&extras.join(", "), TEXT)]);
    }
    if let Some((s, n)) = f.capture_progress() {
        pen.line(&format!("Being taken by {}: {n}/60 min", side_name(Some(s), app.player)), AMBER);
    }
}
