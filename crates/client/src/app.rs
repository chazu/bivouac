use bivouac_sim::path::FOREST_CAP;
use bivouac_sim::{Army, Cell, Convoy, MAP_H, MAP_W, Order, Seen, Side, Sim};
use macroquad::prelude::*;

use crate::grid::Grid;

pub const SKY_ROWS: i32 = 12;
pub const SKY_ROWS_COLLAPSED: i32 = 2;
pub const MINIMAP_ROWS: i32 = 2;
const PANEL_ROWS: i32 = 10;

/// Rows the window height is divided into: status bar, sky, map, minimap and panel.
/// Collapsing the sky makes every cell bigger rather than leaving the panel empty.
pub fn rows_to_fit(sky_collapsed: bool) -> f32 {
    let sky = if sky_collapsed { SKY_ROWS_COLLAPSED } else { SKY_ROWS };
    (1 + sky + MAP_H + MINIMAP_ROWS + PANEL_ROWS) as f32
}
pub const SPEEDS: [u32; 4] = [1, 2, 4, 8];
const FLASH_SECONDS: f64 = 3.0;
/// How close (in rows) to a road line the mouse must be to pick the road.
const ROAD_SNAP: f32 = 0.2;

/// Where each band of the screen starts, in rows.
pub struct Layout {
    pub cols: i32,
    pub rows: i32,
    pub sky_top: i32,
    pub sky_rows: i32,
    pub map_top: i32,
    pub minimap_top: i32,
    pub panel_top: i32,
}

impl Layout {
    pub fn new(g: &Grid, sky_collapsed: bool) -> Layout {
        let sky_top = 1;
        let sky_rows = if sky_collapsed { SKY_ROWS_COLLAPSED } else { SKY_ROWS };
        let map_top = sky_top + sky_rows;
        let minimap_top = map_top + MAP_H;
        Layout {
            cols: g.cols(),
            rows: g.rows(),
            sky_top,
            sky_rows,
            map_top,
            minimap_top,
            panel_top: minimap_top + MINIMAP_ROWS,
        }
    }

    /// First column and width of the minimap strip.
    pub fn minimap_span(&self) -> (i32, i32) {
        (2, (self.cols - 50).max(10))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Army(u32),
    Fort(u32),
    Convoy(u32),
}

pub struct App {
    pub player: Side,
    /// Leftmost visible map column; fractional for smooth scrolling.
    pub scroll: f32,
    pub selected: Option<Selection>,
    pub hover: Option<Cell>,
    pub sky_collapsed: bool,
    pub paused: bool,
    pub speed_idx: usize,
    /// Debug: ignore fog of war.
    pub reveal: bool,
    /// The opening briefing, shown until dismissed.
    pub intro: bool,
    pub help: bool,
    /// Set when the player asks for a new game; main swaps in a fresh sim.
    pub new_game: bool,
    flash: Option<(String, f64)>,
}

/// Muster this many men at a time from a fort's garrison.
pub const MUSTER_MEN: u32 = 400;

impl App {
    pub fn new(player: Side) -> App {
        App {
            player,
            scroll: 0.0,
            selected: None,
            hover: None,
            sky_collapsed: false,
            paused: false,
            speed_idx: 0,
            reveal: false,
            intro: true,
            help: false,
            new_game: false,
            flash: None,
        }
    }

    /// How well the player knows an army: their own always fully, enemies only
    /// as far as their scouts report.
    pub fn seen(&self, sim: &Sim, a: &Army) -> Option<Seen> {
        if a.side == self.player || self.reveal { Some(Seen::Identified) } else { sim.knows(self.player, a.id) }
    }

    /// Whether the player can currently see an army where it actually stands.
    pub fn in_view(&self, sim: &Sim, a: &Army) -> bool {
        self.seen(sim, a).is_some_and(|s| s >= Seen::Sighted)
    }

    /// Whether the player can see a supply train.
    pub fn sees_convoy(&self, sim: &Sim, c: &Convoy) -> bool {
        !c.lost() && (self.reveal || sim.sees_convoy(self.player, c))
    }

    /// Visible supply trains out on the road with a wagon in cell `cell`.
    pub fn convoys_at<'a>(&self, sim: &'a Sim, cell: Cell) -> impl Iterator<Item = &'a Convoy> {
        let seen: Vec<&'a Convoy> =
            sim.convoys.iter().filter(|c| c.on_the_road() && self.sees_convoy(sim, c)).collect();
        seen.into_iter().filter(move |c| c.cells().contains(&cell))
    }

    /// Whether a cell is inside the player's current field of view.
    pub fn watching(&self, sim: &Sim, c: Cell) -> bool {
        self.reveal || sim.intel(self.player).sees(c)
    }

    pub fn speed(&self) -> u32 {
        SPEEDS[self.speed_idx]
    }

    pub fn flash(&self) -> Option<&str> {
        self.flash.as_ref().filter(|(_, at)| get_time() - at < FLASH_SECONDS).map(|(s, _)| s.as_str())
    }

    fn say(&mut self, msg: &str) {
        self.flash = Some((msg.to_string(), get_time()));
    }

    pub fn selected_army<'a>(&self, sim: &'a Sim) -> Option<&'a Army> {
        match self.selected {
            Some(Selection::Army(id)) => sim.army(id),
            _ => None,
        }
    }

    pub fn center_on(&mut self, x: i32, l: &Layout) {
        self.scroll = x as f32 - l.cols as f32 / 2.0;
        self.clamp_scroll(l);
    }

    fn clamp_scroll(&mut self, l: &Layout) {
        self.scroll = self.scroll.clamp(0.0, (MAP_W - l.cols).max(0) as f32);
    }

    /// Map cell under a screen position.
    /// The map cell under the mouse. Near a road line it's the road cell, so
    /// clicking a road always means the road.
    pub fn cell_at(&self, sim: &Sim, g: &Grid, l: &Layout, (mx, my): (f32, f32)) -> Option<Cell> {
        let fy = my / g.ch - l.map_top as f32;
        let row = fy.floor() as i32;
        if !(0..MAP_H).contains(&row) {
            return None;
        }
        let fx = self.scroll + mx / g.cw;
        let col = fx.floor() as i32;
        if !(0..MAP_W).contains(&col) {
            return None;
        }
        Some(sim.map.road_near(fx, fy, ROAD_SNAP).unwrap_or((col, row)))
    }

    pub fn handle_input(&mut self, sim: &mut Sim, g: &Grid, l: &Layout) {
        let mouse = mouse_position();
        self.hover = self.cell_at(sim, g, l, mouse);

        let mut dx = 0.0;
        if is_key_down(KeyCode::Left) {
            dx -= 1.0;
        }
        if is_key_down(KeyCode::Right) {
            dx += 1.0;
        }
        let (wheel_x, wheel_y) = mouse_wheel();
        self.scroll += dx * 45.0 * get_frame_time() + (wheel_x - wheel_y) * 0.6;
        self.clamp_scroll(l);

        let mouse_row = (mouse.1 / g.ch).floor() as i32;
        if is_mouse_button_down(MouseButton::Left) && mouse_row == l.minimap_top {
            let (x0, w) = l.minimap_span();
            let frac = (mouse.0 / g.cw - x0 as f32) / w as f32;
            if (0.0..=1.0).contains(&frac) {
                self.center_on((frac * MAP_W as f32) as i32, l);
            }
        }

        if is_mouse_button_pressed(MouseButton::Left)
            && let Some(c) = self.hover
        {
            self.select_at(sim, c);
        }
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        if is_mouse_button_pressed(MouseButton::Right)
            && let (Some(to), Some(Selection::Army(army))) = (self.hover, self.selected)
        {
            // Right-clicking an enemy attacks it, enemy wagons are raided, anywhere else is a march.
            let side = sim.army(army).map(|a| a.side);
            let enemy = sim
                .armies
                .iter()
                .find(|a| Some(a.side) != side && a.occupies(&sim.map, to) && self.in_view(sim, a))
                .map(|a| a.id);
            let wagons = self.convoys_at(sim, to).find(|c| Some(c.side) != side).map(|c| c.id);
            // Shift adds a waypoint, to steer the march along a chosen road.
            let order = match (enemy, wagons) {
                (Some(target), _) => Order::Attack { army, target },
                (None, Some(convoy)) => Order::Raid { army, convoy },
                _ if shift => Order::Waypoint { army, to },
                _ => Order::March { army, to },
            };
            self.order(sim, order);
        }
        // From a selected fort, right-clicking another of yours sets up a supply route.
        if is_mouse_button_pressed(MouseButton::Right)
            && let (Some(c), Some(Selection::Fort(from))) = (self.hover, self.selected)
            && let Some(to) = sim.fort_at(c).map(|f| f.id)
            && to != from
        {
            self.order(sim, Order::Supply { from, to });
        }

        if self.intro {
            if is_key_pressed(KeyCode::Space)
                || is_key_pressed(KeyCode::Enter)
                || is_mouse_button_pressed(MouseButton::Left)
            {
                self.intro = false;
                self.paused = false;
            }
            return;
        }
        if is_key_pressed(KeyCode::F1) || is_key_pressed(KeyCode::Slash) {
            self.help = !self.help;
        }
        if sim.winner.is_some() && is_key_pressed(KeyCode::N) {
            self.new_game = true;
        }
        if is_key_pressed(KeyCode::Space) {
            self.paused = !self.paused;
        }
        for (i, key) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4].into_iter().enumerate() {
            if is_key_pressed(key) {
                self.speed_idx = i;
            }
        }
        if is_key_pressed(KeyCode::S) {
            self.sky_collapsed = !self.sky_collapsed;
        }
        if is_key_pressed(KeyCode::Escape) {
            self.selected = None;
            self.help = false;
        }
        if let Some(Selection::Fort(from)) = self.selected
            && is_key_pressed(KeyCode::X)
        {
            self.order(sim, Order::CancelSupply { from });
        }
        if let Some(Selection::Fort(fort)) = self.selected
            && is_key_pressed(KeyCode::M)
        {
            let before = sim.armies.len();
            self.order(sim, Order::Muster { fort, men: MUSTER_MEN });
            if sim.armies.len() > before {
                self.selected = sim.armies.last().map(|a| Selection::Army(a.id));
            }
        }
        if is_key_pressed(KeyCode::Tab) {
            self.cycle_armies(sim, l);
        }
        if is_key_pressed(KeyCode::C)
            && let Some(x) = self.selected_army(sim).map(|a| a.pos.0)
        {
            self.center_on(x, l);
        }
        if let Some(Selection::Army(army)) = self.selected {
            let order = if is_key_pressed(KeyCode::R) {
                Some(Order::Rest { army })
            } else if is_key_pressed(KeyCode::H) {
                Some(Order::Hold { army })
            } else if is_key_pressed(KeyCode::D) {
                // A raiding party small enough for the forests, or with shift, half.
                Some(Order::Divide { army, men: if shift { None } else { Some(FOREST_CAP) } })
            } else if is_key_pressed(KeyCode::J) {
                Some(Order::Join { army })
            } else if is_key_pressed(KeyCode::G) {
                Some(Order::Garrison { army })
            } else if is_key_pressed(KeyCode::Q) {
                sim.army(army).map(|a| Order::SetStance { army, stance: a.stance.next() })
            } else if is_key_pressed(KeyCode::P) {
                sim.army(army).map(|a| Order::SetPace { army, pace: a.pace.next() })
            } else {
                None
            };
            if let Some(o) = order {
                self.order(sim, o);
            }
        }
        // Drop a selection that's gone: destroyed, or an enemy that slipped out of view.
        let gone = match self.selected {
            Some(Selection::Army(id)) => sim.army(id).is_none_or(|a| !self.in_view(sim, a)),
            Some(Selection::Convoy(id)) => sim.convoy(id).is_none_or(|c| !self.sees_convoy(sim, c)),
            _ => false,
        };
        if gone {
            self.selected = None;
        }
    }

    fn order(&mut self, sim: &mut Sim, o: Order) {
        if let Err(e) = sim.apply(self.player, o) {
            self.say(e);
        }
    }

    /// Clicking a cell cycles through the armies in it (yours first), wagons, then the fort.
    fn select_at(&mut self, sim: &Sim, c: Cell) {
        let mut here: Vec<&Army> =
            sim.armies.iter().filter(|a| a.occupies(&sim.map, c) && self.in_view(sim, a)).collect();
        here.sort_by_key(|a| (a.side != self.player, a.id));
        let mut choices: Vec<Selection> = here.iter().map(|a| Selection::Army(a.id)).collect();
        choices.extend(self.convoys_at(sim, c).map(|w| Selection::Convoy(w.id)));
        if let Some(f) = sim.fort_at(c) {
            choices.push(Selection::Fort(f.id));
        }
        self.selected = match self.selected.and_then(|s| choices.iter().position(|&x| x == s)) {
            Some(i) => Some(choices[(i + 1) % choices.len()]),
            None => choices.first().copied(),
        };
    }

    fn cycle_armies(&mut self, sim: &Sim, l: &Layout) {
        let mine: Vec<&Army> = sim.armies.iter().filter(|a| a.side == self.player).collect();
        if mine.is_empty() {
            return;
        }
        let next = match self.selected_army(sim).and_then(|s| mine.iter().position(|a| a.id == s.id)) {
            Some(i) => mine[(i + 1) % mine.len()],
            None => mine[0],
        };
        self.selected = Some(Selection::Army(next.id));
        self.center_on(next.pos.0, l);
    }
}
