use std::collections::VecDeque;

use crate::{Cell, Side};

pub const MEN_PER_COMPANY: u32 = 50;
/// Braille has 8 dots, so a cell shows at most 8 companies; bigger columns spill backward.
pub const COMPANIES_PER_CELL: u32 = 8;
pub const STAMINA_MAX: u16 = 1000;
/// Food is tracked in thousandths of a ration (one ration feeds one man for a day)
/// so hourly eating doesn't round away.
pub const MILLI: u64 = 1000;
pub const CARRY_DAYS: u64 = 5;
const TRAIL_LEN: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmyState {
    Holding,
    Marching,
    Resting,
}

#[derive(Clone, Debug)]
pub struct Army {
    pub id: u32,
    pub side: Side,
    pub name: String,
    pub men: u32,
    pub stamina: u16,
    /// Milli-rations carried.
    pub food: u64,
    pub pos: Cell,
    pub path: VecDeque<Cell>,
    /// Progress toward the next cell, in tenths of a game minute.
    pub progress: u32,
    pub state: ArmyState,
    /// Cells recently left, newest first. A long column stretches back along them.
    pub trail: VecDeque<Cell>,
    pub starving: bool,
}

impl Army {
    pub fn new(id: u32, side: Side, name: &str, men: u32, pos: Cell) -> Army {
        Army {
            id,
            side,
            name: name.to_string(),
            men,
            stamina: 800,
            food: men as u64 * 3 * MILLI,
            pos,
            path: VecDeque::new(),
            progress: 0,
            state: ArmyState::Holding,
            trail: VecDeque::new(),
            starving: false,
        }
    }

    pub fn companies(&self) -> u32 {
        self.men.div_ceil(MEN_PER_COMPANY).max(1)
    }

    pub fn food_capacity(&self) -> u64 {
        self.men as u64 * CARRY_DAYS * MILLI
    }

    pub fn food_days(&self) -> f32 {
        if self.men == 0 { 0.0 } else { self.food as f32 / (self.men as u64 * MILLI) as f32 }
    }

    pub fn exhausted(&self) -> bool {
        self.stamina < 200
    }

    /// Cells the column occupies, head first, with the companies shown in each.
    pub fn column(&self) -> Vec<(Cell, u32)> {
        let mut left = self.companies();
        let mut cells: Vec<(Cell, u32)> = Vec::new();
        let mut spots = std::iter::once(self.pos).chain(self.trail.iter().copied());
        let mut last = self.pos;
        while left > 0 {
            let here = spots.next().unwrap_or(last);
            let n = left.min(COMPANIES_PER_CELL);
            match cells.iter_mut().find(|e| e.0 == here) {
                // Out of trail: pile the rest into the tail cell.
                Some((_, k)) => *k = (*k + n).min(COMPANIES_PER_CELL),
                None => cells.push((here, n)),
            }
            last = here;
            left -= n;
        }
        cells
    }

    pub fn occupies(&self, c: Cell) -> bool {
        self.column().iter().any(|&(cell, _)| cell == c)
    }

    pub(crate) fn step_to(&mut self, c: Cell) {
        self.trail.push_front(self.pos);
        self.trail.truncate(TRAIL_LEN);
        self.pos = c;
    }
}
