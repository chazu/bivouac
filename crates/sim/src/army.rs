use std::collections::VecDeque;

use crate::map::{Map, Terrain};
use crate::{Cell, Side};

pub const MEN_PER_COMPANY: u32 = 50;
/// Braille has 8 dots, so a cell shows at most 8 companies; bigger columns spill backward.
pub const COMPANIES_PER_CELL: u32 = 8;
/// On the march an army strings out along its road, a few companies to a cell.
pub const MARCH_COMPANIES_PER_CELL: u32 = 3;
/// Among the trees or the rocks a cell holds only a few companies, so a big army
/// strings out through them.
pub const CRAMPED_COMPANIES_PER_CELL: u32 = 3;
pub const STAMINA_MAX: u16 = 1000;
/// Food is tracked in thousandths of a ration (one ration feeds one man for a day)
/// so hourly eating doesn't round away.
pub const MILLI: u64 = 1000;
/// Days of food a man carries on his back. Beyond that an army lives on forts,
/// wagons and what it can forage.
pub const CARRY_DAYS: u64 = 3;
const TRAIL_LEN: usize = 24;
/// Morale is kept in thousandths of a percent so small losses still register.
pub const MORALE_MAX: u32 = 100_000;
pub const ENTRENCH_MAX: u16 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmyState {
    Holding,
    Marching,
    Resting,
    /// Morale broke: fleeing toward a friendly fort, deaf to orders until it rallies.
    Routing,
}

/// How an army behaves around enemies when it hasn't been told otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    /// Attacks any enemy that comes near.
    Aggressive,
    /// Fights back when attacked and holds its ground.
    Defensive,
    /// Avoids contact and falls back when threatened.
    Evasive,
}

impl Stance {
    pub fn label(self) -> &'static str {
        match self {
            Stance::Aggressive => "aggressive",
            Stance::Defensive => "defensive",
            Stance::Evasive => "evasive",
        }
    }

    pub fn next(self) -> Stance {
        match self {
            Stance::Aggressive => Stance::Defensive,
            Stance::Defensive => Stance::Evasive,
            Stance::Evasive => Stance::Aggressive,
        }
    }
}

/// How hard an army is pushed on the march.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Two-thirds speed for half the stamina per cell.
    Slow,
    Normal,
    /// Half again as fast, at double the stamina per cell.
    Fast,
}

impl Pace {
    pub fn label(self) -> &'static str {
        match self {
            Pace::Slow => "easy pace",
            Pace::Normal => "steady pace",
            Pace::Fast => "forced march",
        }
    }

    pub fn next(self) -> Pace {
        match self {
            Pace::Slow => Pace::Normal,
            Pace::Normal => Pace::Fast,
            Pace::Fast => Pace::Slow,
        }
    }

    /// Progress toward the next cell each minute, in tenths of a minute of its cost.
    pub fn speed(self) -> u32 {
        match self {
            Pace::Slow => 7,
            Pace::Normal => 10,
            Pace::Fast => 15,
        }
    }

    /// Scales the stamina a step costs.
    pub fn strain(self, cost: u16) -> u16 {
        match self {
            Pace::Slow => cost / 2,
            Pace::Normal => cost,
            Pace::Fast => cost * 2,
        }
    }
}

/// How many companies fit in a cell.
pub fn room(map: &Map, (x, y): Cell) -> u32 {
    match map.terrain(x, y) {
        Terrain::Forest | Terrain::Mountain if !map.is_road(x, y) => CRAMPED_COMPANIES_PER_CELL,
        _ => COMPANIES_PER_CELL,
    }
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
    /// Men who have slipped away for want of food since the last report.
    pub(crate) deserted: u32,
    pub morale: u32,
    pub stance: Stance,
    pub pace: Pace,
    /// How dug in the army is; builds while it stays put, lost when it moves.
    /// Inside fort walls it doesn't matter: the walls count as fully dug in.
    pub entrench: u16,
    /// Enemy army this one is attacking or chasing.
    pub target: Option<u32>,
    /// Enemy supply train this one is running down.
    pub quarry: Option<u32>,
    /// Minutes of rout left before the army rallies.
    pub rout_left: u32,
    /// Casualties not yet taken, in thousandths of a man.
    pub(crate) bleed: u32,
    /// Cells of the deployed line, center first; empty while in column.
    pub line: Vec<Cell>,
    /// Minutes the army has stood still; it deploys once this is long enough.
    pub(crate) settled: u16,
    /// Ordered to halt in line of battle rather than stand in column.
    pub form_line: bool,
}

impl Army {
    pub fn new(id: u32, side: Side, name: &str, men: u32, pos: Cell) -> Army {
        Army {
            id,
            side,
            name: name.to_string(),
            men,
            stamina: 800,
            food: men as u64 * CARRY_DAYS * MILLI,
            pos,
            path: VecDeque::new(),
            progress: 0,
            state: ArmyState::Holding,
            trail: VecDeque::new(),
            starving: false,
            deserted: 0,
            morale: MORALE_MAX * 8 / 10,
            stance: Stance::Defensive,
            pace: Pace::Normal,
            entrench: 0,
            target: None,
            quarry: None,
            rout_left: 0,
            bleed: 0,
            line: Vec::new(),
            settled: 0,
            form_line: false,
        }
    }

    pub fn companies(&self) -> u32 {
        self.men.div_ceil(MEN_PER_COMPANY).max(1)
    }

    pub fn food_capacity(&self) -> u64 {
        self.men as u64 * CARRY_DAYS * MILLI
    }

    /// Milli-rations eaten an hour: a ration a man a day, more on the march and
    /// more again at a forced march.
    pub fn ration(&self) -> u64 {
        let need = self.men as u64 * MILLI / 24;
        match (self.state, self.pace) {
            (ArmyState::Marching, Pace::Fast) => need * 3 / 2,
            (ArmyState::Marching, _) => need * 5 / 4,
            _ => need,
        }
    }

    pub fn food_days(&self) -> f32 {
        if self.men == 0 { 0.0 } else { self.food as f32 / (self.men as u64 * MILLI) as f32 }
    }

    pub fn exhausted(&self) -> bool {
        self.stamina < 200
    }

    pub fn deployed(&self) -> bool {
        !self.line.is_empty()
    }

    /// Cells the army occupies with the companies in each: its deployed line,
    /// or, on the march, a column stretching back along its trail. Woods and
    /// rocks hold fewer companies than open ground.
    pub fn column(&self, map: &Map) -> Vec<(Cell, u32)> {
        let mut left = self.companies();
        let mut cells: Vec<(Cell, u32)> = Vec::new();
        if self.deployed() {
            for &c in &self.line {
                if left == 0 {
                    break;
                }
                let n = left.min(room(map, c));
                cells.push((c, n));
                left -= n;
            }
            // Too few cells for everyone (losses rearranged nothing): crowd the center.
            for cell in cells.iter_mut() {
                let more = left.min(room(map, cell.0).saturating_sub(cell.1));
                cell.1 += more;
                left -= more;
            }
            return cells;
        }
        let mut spots: Vec<Cell> = Vec::new();
        for c in std::iter::once(self.pos).chain(self.trail.iter().copied()) {
            if !spots.contains(&c) {
                spots.push(c);
            }
        }
        // Thin along a long trail, bunched up when there's no room behind.
        let per = left.div_ceil(spots.len() as u32).clamp(MARCH_COMPANIES_PER_CELL, COMPANIES_PER_CELL);
        for c in spots {
            if left == 0 {
                break;
            }
            let n = left.min(per).min(room(map, c));
            cells.push((c, n));
            left -= n;
        }
        // Whoever didn't fit packs in at the tail.
        for cell in cells.iter_mut().rev() {
            let more = left.min(room(map, cell.0).saturating_sub(cell.1));
            cell.1 += more;
            left -= more;
        }
        cells
    }

    pub fn occupies(&self, map: &Map, c: Cell) -> bool {
        self.column(map).iter().any(|&(cell, _)| cell == c)
    }

    pub fn routing(&self) -> bool {
        self.state == ArmyState::Routing
    }

    pub fn morale_pct(&self) -> u32 {
        self.morale / 1000
    }

    pub(crate) fn step_to(&mut self, c: Cell) {
        self.entrench = 0;
        self.line.clear();
        self.settled = 0;
        self.trail.push_front(self.pos);
        self.trail.truncate(TRAIL_LEN);
        self.pos = c;
    }
}
