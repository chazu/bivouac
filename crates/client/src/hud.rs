use bivouac_sim::clock::{FULL_MOON, hhmm, moon_phase_name};
use bivouac_sim::path::{FOREST_CAP, MOUNTAIN_CAP};
use bivouac_sim::{Army, ArmyState, Cell, Convoy, Fort, Leg, MAP_W, Seen, Side, Sim, Terrain};
use macroquad::prelude::*;

use crate::app::{App, Layout, Selection};
use crate::grid::{DENSITY, Grid, braille_char};
use crate::light::Light;
use crate::palette::{self, AMBER, DIM, HEADING, LINE, PANEL_BG, TEXT, rgb};

pub fn draw(g: &Grid, l: &Layout, app: &App, sim: &Sim, light: &Light) {
    status_bar(g, l, app, sim, light);
    minimap(g, l, app, sim);
    panel(g, l, app, sim);
    if app.intro {
        intro(g, l, app);
    } else if app.help {
        help(g, l);
    } else if let Some(winner) = sim.winner {
        let (title, line) = if winner == app.player {
            ("VICTORY", format!("{} has fallen to you.", main_fort_of(sim, winner.other())))
        } else {
            ("DEFEAT", format!("{} has fallen.", main_fort_of(sim, app.player)))
        };
        boxed(
            g,
            l,
            palette::side(winner),
            &[(title, palette::side(winner)), ("", TEXT), (&line, TEXT), ("[N] new game", DIM)],
        );
    }
}

fn main_fort_of(sim: &Sim, side: Side) -> &'static str {
    let original = if side == Side::West { 0 } else { sim.forts.len() - 1 };
    sim.forts[original].name
}

/// A centered box of text over the map.
fn boxed(g: &Grid, l: &Layout, edge: Color, lines: &[(&str, Color)]) {
    let width = lines.iter().map(|(s, _)| s.chars().count() as i32).max().unwrap_or(0) + 8;
    let height = lines.len() as i32 + 2;
    let col = (l.cols - width) / 2;
    let row = (l.map_top + bivouac_sim::MAP_H / 2 - height / 2).max(1);
    g.fill(col, row, width, height, Color::new(0.03, 0.03, 0.04, 0.94));
    let (x, y) = (col as f32 * g.cw, row as f32 * g.ch);
    draw_rectangle_lines(x, y, width as f32 * g.cw, height as f32 * g.ch, 2.0, edge);
    for (i, &(text, color)) in lines.iter().enumerate() {
        g.text(col + 4, row + 1 + i as i32, text, color);
    }
}

fn intro(g: &Grid, l: &Layout, app: &App) {
    let you = app.player.faction();
    let them = app.player.other().faction();
    let us = format!("You command {you}, holding Lantern Gate in the west.");
    let foe = format!("{} marches from the Cinnabar Citadel in the east.", capitalize(them));
    boxed(
        g,
        l,
        palette::side(app.player),
        &[
            ("BIVOUAC", HEADING),
            ("", TEXT),
            ("The sun is dying. The roads still matter.", DIM),
            ("", TEXT),
            (&us, TEXT),
            (&foe, TEXT),
            ("Take the Citadel, or hold your gate until they break.", TEXT),
            ("", TEXT),
            ("Armies standing still deploy into line and dig in.", DIM),
            ("Forts are entered only by their gates, where the roads run in; walls count as earthworks.", DIM),
            ("Your scouts only see so far: dust, campfires and tracks give enemies away.", DIM),
            ("Forests hide small parties. Night, fog and rain shorten everyone's sight.", DIM),
            ("", TEXT),
            ("[F1] controls     [Space] begin", HEADING),
        ],
    );
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn help(g: &Grid, l: &Layout) {
    boxed(
        g,
        l,
        HEADING,
        &[
            ("CONTROLS", HEADING),
            ("", TEXT),
            ("left-click       select an army or fort (click again to cycle)", TEXT),
            ("right-click      march there, or attack an enemy you can see", TEXT),
            ("  +shift         add a waypoint, to steer along a road", TEXT),
            ("R / H            rest / halt in line of battle", TEXT),
            ("Q                cycle stance: aggressive, defensive, evasive", TEXT),
            ("P                cycle pace: easy (slow, spares stamina), steady, forced", TEXT),
            ("D / shift+D      detach a 150-man raiding party / divide in half", TEXT),
            ("J                join armies in the same cell", TEXT),
            ("G                put the army into the garrison of your fort", TEXT),
            ("M                (fort selected) muster 400 men from its garrison", TEXT),
            ("right-click fort (fort selected) run supply wagons there; X recalls", TEXT),
            ("right-click wagons (army selected) raid an enemy supply train", TEXT),
            ("Tab / C          next army / center on it", TEXT),
            ("← → / wheel      scroll; click the minimap to jump", TEXT),
            ("S                collapse or expand the sky", TEXT),
            ("Space / 1-4      pause / speed ×1 ×2 ×4 ×8", TEXT),
            ("F1 or ?          this help", TEXT),
        ],
    );
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

pub(crate) fn duration(minutes: u32) -> String {
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
            0 => '○',
            p if p < FULL_MOON => '◑',
            FULL_MOON => '●',
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
        "  ·  Day {}, {} {}  ·  {}  ·  {}  ·  {}  ·  sight {}",
        clock.day() + 1,
        hhmm(clock.minute),
        when,
        moon,
        sim.weather.label(),
        wind,
        sim.sight()
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
    for c in sim.convoys.iter().filter(|c| c.on_the_road() && app.sees_convoy(sim, c)) {
        g.put(col_of(c.pos.0), top, '▪', palette::alpha(palette::side(c.side), 0.7));
    }
    for a in sim.armies.iter().filter(|a| a.side == app.player || app.reveal) {
        g.put(col_of(a.pos.0), top, '•', palette::side(a.side));
    }
    if !app.reveal {
        let enemy = palette::side(app.player.other());
        for r in &sim.intel(app.player).reports {
            let (glyph, a) = match r.seen {
                Seen::Remembered => ('·', 0.5),
                Seen::Rumored => ('?', 0.7),
                _ => ('•', 1.0),
            };
            g.put(col_of(r.cell.0), top, glyph, palette::alpha(enemy, a));
        }
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
    for e in sim.log.iter().filter(|e| e.visible_to(app.player) || app.reveal) {
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
        Terrain::Forest => format!("forest · hides; over {FOREST_CAP} men crowd and crawl"),
        Terrain::Hill => "hills · slow; sees farther, defends better".into(),
        Terrain::Mountain => format!("mountain · only parties of {MOUNTAIN_CAP} or fewer; lookout"),
        Terrain::River => "river · impassable".into(),
        Terrain::Fort => "fortification".into(),
    }
}

/// What the player can tell about an army's strength.
fn strength(a: &Army, app: &App, sim: &Sim) -> String {
    match app.seen(sim, a) {
        Some(Seen::Identified) => format!("{} men", a.men),
        _ => format!("about {} men", sim.intel(app.player).report(a.id).map_or(a.men, |r| r.men)),
    }
}

fn army_lines(pen: &mut Pen, a: &Army, app: &App, sim: &Sim) {
    let glyph = braille_char(DENSITY[(a.companies().min(8) - 1) as usize]).to_string();
    let who = if a.side == app.player { "yours" } else { "enemy" };
    pen.segs(&[
        (&glyph, palette::side(a.side)),
        (&format!(" {} ({who})", a.name), TEXT),
        (&format!("  {}", strength(a, app, sim)), DIM),
    ]);
    if app.seen(sim, a) != Some(Seen::Identified) {
        pen.line("  sighted at a distance; get closer to learn more", DIM);
        return;
    }
    pen.segs(&[
        ("  stamina ", DIM),
        (&bar(a.stamina as f32 / 1000.0, 6), stamina_color(a)),
        (&format!(" {:>3}%  ", a.stamina / 10), TEXT),
        ("morale ", DIM),
        (&bar(a.morale_pct() as f32 / 100.0, 6), morale_color(a)),
        (&format!(" {:>3}%", a.morale_pct()), TEXT),
    ]);
    pen.segs(&[("  ", DIM), (&status_words(a), state_color(a)), (&format!("  ·  food {:.1} d", a.food_days()), DIM)]);
}

fn status_words(a: &Army) -> String {
    let state = match a.state {
        ArmyState::Marching => "marching",
        ArmyState::Resting => "resting",
        ArmyState::Holding => "holding",
        ArmyState::Routing => return format!("ROUTING · rallies in {}", duration(a.rout_left)),
    };
    let formation = if a.deployed() { "in line" } else { "in column" };
    format!("{state} {formation} · {} · {}", a.stance.label(), a.pace.label())
}

fn state_color(a: &Army) -> Color {
    if a.routing() { AMBER } else { TEXT }
}

fn morale_color(a: &Army) -> Color {
    match a.morale_pct() {
        0..=30 => AMBER,
        _ => rgb(0.75, 0.7, 0.95),
    }
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
    if let Some(r) = sim.map.region_at(c.0).filter(|_| sim.fort_at(c).is_none()) {
        pen.segs(&[(r.name, TEXT), (&format!(" · {}", r.country.label()), DIM)]);
    }
    if let Some(f) = sim.fort_at(c) {
        pen.segs(&[
            (f.name, palette::side_or_stone(f.owner)),
            (&format!(" · {} · {}", f.kind.label(), side_name(f.owner, app.player)), DIM),
        ]);
    }
    if !app.watching(sim, c) {
        pen.line("beyond your sight", DIM);
    }
    let here: Vec<&Army> = sim.armies.iter().filter(|a| a.occupies(&sim.map, c) && app.in_view(sim, a)).collect();
    for a in here.iter().take(3) {
        pen.gap();
        army_lines(pen, a, app, sim);
    }
    if !app.reveal {
        for r in sim.intel(app.player).reports.iter().filter(|r| r.cell == c && r.seen < Seen::Sighted) {
            pen.gap();
            let ago = duration(sim.clock.minute - r.minute);
            let what = match r.seen {
                Seen::Rumored => format!("? Dust or fires: a host of perhaps {} men", r.men),
                _ => format!("? Enemy last seen here {ago} ago, about {} men", r.men),
            };
            pen.line(&what, palette::side(app.player.other()));
        }
    }
    if here.len() > 3 {
        pen.line(&format!("  …and {} more", here.len() - 3), DIM);
    }
    for c in app.convoys_at(sim, c).take(2) {
        pen.gap();
        let who = if c.side == app.player { "yours" } else { "enemy" };
        let load = if c.load() > 0 { "laden" } else { "empty" };
        pen.segs(&[("▬ ", palette::side(c.side)), (&format!("{} ({who})", c.name), TEXT), (&format!("  {load}"), DIM)]);
    }
}

fn fort_name(sim: &Sim, id: u32) -> &'static str {
    sim.forts.iter().find(|f| f.id == id).map_or("?", |f| f.name)
}

/// What a train is up to, in a few words.
fn convoy_doing(c: &Convoy, sim: &Sim) -> String {
    let eta = || duration(sim.convoy_eta(c));
    match c.leg {
        Leg::Lost { until } => format!("lost; a new train in {}", duration(until - sim.clock.minute)),
        _ if c.recalled => format!("recalled → {}  ·  ETA {}", fort_name(sim, c.heading), eta()),
        Leg::Loading(_) => format!("loading at {}", fort_name(sim, c.from)),
        Leg::Outbound => format!("→ {}  ·  ETA {}", fort_name(sim, c.to), eta()),
        Leg::Unloading(_) => format!("unloading at {}", fort_name(sim, c.to)),
        Leg::Returning => format!("returning → {}  ·  ETA {}", fort_name(sim, c.from), eta()),
    }
}

fn convoy_detail(pen: &mut Pen, c: &Convoy, app: &App, sim: &Sim) {
    pen.segs(&[("SELECTED  ", HEADING), (&c.name, palette::side(c.side))]);
    let names = bivouac_sim::fort::GOODS;
    let cargo: Vec<String> =
        names.iter().zip(c.cargo).filter(|(_, n)| *n > 0).map(|(g, n)| format!("{g} {n}")).collect();
    if c.side != app.player {
        pen.line("Enemy supply wagons.", DIM);
        pen.segs(&[("cargo    ", DIM), (if c.load() > 0 { "laden" } else { "empty, going home" }, TEXT)]);
        pen.line("Select an army and right-click the wagons to raid them.", DIM);
        pen.line("Raiders keep what food they can carry and burn the rest.", DIM);
        return;
    }
    pen.segs(&[("route    ", DIM), (&format!("{} → {}", fort_name(sim, c.from), fort_name(sim, c.to)), TEXT)]);
    let load = if cargo.is_empty() { "empty".to_string() } else { format!("{}  ({})", c.load(), cargo.join(", ")) };
    pen.segs(&[("cargo    ", DIM), (&load, TEXT)]);
    pen.segs(&[("status   ", DIM), (&convoy_doing(c, sim), TEXT)]);
    pen.segs(&[("carried  ", DIM), (&format!("{} rations delivered", c.delivered), TEXT)]);
    let exposure = if sim.sees_convoy(c.side.other(), c) && c.on_the_road() {
        ("in sight of the enemy", AMBER)
    } else {
        ("unseen by the enemy", rgb(0.55, 0.85, 0.55))
    };
    pen.segs(&[("exposure ", DIM), exposure]);
    pen.line("Wagons can't fight: an army beside them keeps them safe.", DIM);
    pen.line("Select the source fort and press [X] to recall.", DIM);
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
                fort_detail(pen, f, app, sim);
            }
        }
        Some(Selection::Convoy(id)) => {
            if let Some(c) = sim.convoy(id) {
                convoy_detail(pen, c, app, sim);
            }
        }
        None => {
            pen.line("SELECTED", HEADING);
            pen.line("Click an army or fort to select it.", DIM);
            pen.line("Right-click to march (shift: waypoint), or on an enemy to attack.", DIM);
            pen.gap();
            pen.line("[R]est  [H]old  [D]ivide  [J]oin", DIM);
            pen.line("[1-4] speed  [Space] pause", DIM);
        }
    }
}

fn army_detail(pen: &mut Pen, a: &Army, app: &App, sim: &Sim) {
    pen.segs(&[("SELECTED  ", HEADING), (&a.name, palette::side(a.side))]);
    if app.seen(sim, a) != Some(Seen::Identified) {
        pen.segs(&[("men      ", DIM), (&strength(a, app, sim), TEXT)]);
        pen.line("Sighted at a distance. Get closer to learn more.", DIM);
        return;
    }
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
    pen.segs(&[
        ("morale   ", DIM),
        (&bar(a.morale_pct() as f32 / 100.0, 10), morale_color(a)),
        (&format!(" {}%", a.morale_pct()), TEXT),
        (
            &if sim.fort_at(a.pos).is_some() {
                "   behind walls".to_string()
            } else if a.entrench > 0 {
                format!("   dug in {}%", a.entrench / 10)
            } else {
                String::new()
            },
            DIM,
        ),
    ]);
    pen.segs(&[("status   ", DIM), (&status_words(a), state_color(a))]);
    let target = a.target.and_then(|t| sim.army(t));
    let order = match (a.state, a.path.back()) {
        (ArmyState::Routing, _) => "fleeing".into(),
        _ if target.is_some() => {
            let t = target.unwrap();
            let fighting = if sim.in_battle(a.id) { "fighting" } else { "closing on" };
            format!("attack → {fighting} {}", t.name)
        }
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
    if a.side == app.player {
        let exposure = match sim.knows(a.side.other(), a.id) {
            Some(Seen::Identified) => ("identified by the enemy", AMBER),
            Some(Seen::Sighted) => ("sighted by the enemy", AMBER),
            Some(Seen::Rumored) => ("rumored: the enemy sees dust or fires", TEXT),
            _ => ("unseen by the enemy", rgb(0.55, 0.85, 0.55)),
        };
        pen.segs(&[("exposure ", DIM), exposure]);
        pen.line("[RMB] march / attack  [R]est [H]old [Q] stance [P]ace", DIM);
        pen.line("[D] detach 150 (shift: half) [J]oin [G]arrison", DIM);
    } else {
        pen.line("Enemy army: you can't command it.", DIM);
    }
}

fn fort_detail(pen: &mut Pen, f: &Fort, app: &App, sim: &Sim) {
    let known = f.owner == Some(app.player) || app.watching(sim, f.pos);
    if !known {
        pen.segs(&[("SELECTED  ", HEADING), (f.name, palette::side_or_stone(f.owner))]);
        pen.segs(&[(&format!("{} · {}", f.kind.label(), side_name(f.owner, app.player)), DIM)]);
        pen.line("Beyond your sight: garrison and stores unknown.", DIM);
        return;
    }
    pen.segs(&[("SELECTED  ", HEADING), (f.name, palette::side_or_stone(f.owner))]);
    pen.segs(&[
        (&format!("{} · {}", f.kind.label(), side_name(f.owner, app.player)), DIM),
        (&format!("  · garrison {}", f.garrison), DIM),
    ]);
    if f.garrison > 0 {
        let pct = f.garrison_morale / 1000;
        pen.segs(&[
            ("morale   ", DIM),
            (&bar(pct as f32 / 100.0, 10), rgb(0.75, 0.7, 0.95)),
            (&format!(" {pct}%"), TEXT),
        ]);
    }
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
    if f.owner == Some(app.player) {
        if let Some(c) = sim.route_from(f.id) {
            pen.segs(&[("supply   ", DIM), (&format!("{}: {}", c.name, convoy_doing(c, sim)), TEXT)]);
        }
        let incoming: Vec<&str> = sim
            .convoys
            .iter()
            .filter(|c| c.to == f.id && c.side == app.player)
            .map(|c| fort_name(sim, c.from))
            .collect();
        if !incoming.is_empty() {
            pen.segs(&[("fed by   ", DIM), (&incoming.join(", "), TEXT)]);
        }
        pen.line(&format!("[M] muster {} men from the garrison", crate::app::MUSTER_MEN.min(f.garrison)), DIM);
        let recall = if sim.route_from(f.id).is_some() { "  [X] recall wagons" } else { "" };
        pen.line(&format!("[RMB] another fort: send wagons there{recall}"), DIM);
    }
}
