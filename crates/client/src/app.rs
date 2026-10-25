use bivouac_sim::{Army, Cell, MAP_H, MAP_W, Order, Side, Sim};
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
    flash: Option<(String, f64)>,
}

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
            flash: None,
        }
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
    pub fn cell_at(&self, g: &Grid, l: &Layout, (mx, my): (f32, f32)) -> Option<Cell> {
        let row = (my / g.ch).floor() as i32 - l.map_top;
        if !(0..MAP_H).contains(&row) {
            return None;
        }
        let x0 = self.scroll.floor();
        let frac = (self.scroll - x0) * g.cw;
        let col = ((mx + frac) / g.cw).floor() as i32 + x0 as i32;
        (0..MAP_W).contains(&col).then_some((col, row))
    }

    pub fn handle_input(&mut self, sim: &mut Sim, g: &Grid, l: &Layout) {
        let mouse = mouse_position();
        self.hover = self.cell_at(g, l, mouse);

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
        if is_mouse_button_pressed(MouseButton::Right)
            && let (Some(to), Some(Selection::Army(army))) = (self.hover, self.selected)
        {
            self.order(sim, Order::March { army, to });
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
                Some(Order::Divide { army })
            } else if is_key_pressed(KeyCode::J) {
                Some(Order::Join { army })
            } else {
                None
            };
            if let Some(o) = order {
                self.order(sim, o);
            }
        }
        if let Some(Selection::Army(id)) = self.selected
            && sim.army(id).is_none()
        {
            self.selected = None;
        }
    }

    fn order(&mut self, sim: &mut Sim, o: Order) {
        if let Err(e) = sim.apply(self.player, o) {
            self.say(e);
        }
    }

    /// Clicking a cell cycles through the armies in it (yours first), then the fort.
    fn select_at(&mut self, sim: &Sim, c: Cell) {
        let mut here: Vec<&Army> = sim.armies.iter().filter(|a| a.occupies(c)).collect();
        here.sort_by_key(|a| (a.side != self.player, a.id));
        let mut choices: Vec<Selection> = here.iter().map(|a| Selection::Army(a.id)).collect();
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
