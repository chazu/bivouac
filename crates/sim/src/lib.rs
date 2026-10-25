//! Bivouac's game rules. Pure and deterministic: no rendering, no wall-clock time,
//! no platform randomness, so lockstep netplay, replays and headless AI games all work.
//! Everything that changes the game goes through [`Sim::apply`] and [`Sim::tick`].

pub mod army;
pub mod clock;
pub mod fort;
pub mod map;
pub mod path;
pub mod rng;
mod scenario;

use std::collections::VecDeque;

pub use army::{Army, ArmyState};
pub use clock::{Clock, Weather};
pub use fort::{Extra, Fort, FortKind};
pub use map::{MAP_H, MAP_W, Map, Road, Terrain};

use army::{MEN_PER_COMPANY, MILLI, STAMINA_MAX};
use fort::CAPTURE_MINUTES;
use rng::Rng;

pub type Cell = (i32, i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    West,
    East,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::West => Side::East,
            Side::East => Side::West,
        }
    }

    fn idx(self) -> usize {
        self as usize
    }
}

/// Everything a player (or the AI, or a remote peer) can tell the game to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Order {
    March {
        army: u32,
        to: Cell,
    },
    Rest {
        army: u32,
    },
    Hold {
        army: u32,
    },
    /// Split off half the army into a new one in the same cell.
    Divide {
        army: u32,
    },
    /// Merge every friendly army standing in the same cell into this one.
    Join {
        army: u32,
    },
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub minute: u32,
    pub side: Option<Side>,
    pub text: String,
}

const LOG_LEN: usize = 64;

pub struct Sim {
    pub map: Map,
    pub forts: Vec<Fort>,
    pub armies: Vec<Army>,
    pub clock: Clock,
    pub weather: Weather,
    /// Wind along the front, -3..=3; negative blows toward the west.
    pub wind: i8,
    pub log: VecDeque<LogEntry>,
    rng: Rng,
    next_army_id: u32,
}

fn push_log(log: &mut VecDeque<LogEntry>, minute: u32, side: Option<Side>, text: String) {
    log.push_front(LogEntry { minute, side, text });
    log.truncate(LOG_LEN);
}

impl Sim {
    pub fn new(seed: u64) -> Sim {
        scenario::ashford_valley(seed)
    }

    pub fn army(&self, id: u32) -> Option<&Army> {
        self.armies.iter().find(|a| a.id == id)
    }

    pub fn fort_at(&self, c: Cell) -> Option<&Fort> {
        self.forts.iter().find(|f| f.contains(c))
    }

    /// Debug/scenario hook; normal weather comes from [`Sim::tick`].
    pub fn set_weather(&mut self, w: Weather) {
        self.weather = w;
    }

    fn log(&mut self, side: Option<Side>, text: String) {
        push_log(&mut self.log, self.clock.minute, side, text);
    }

    pub fn plan_path(&self, army: &Army, to: Cell) -> Option<Vec<Cell>> {
        let enemies: Vec<Cell> = self.armies.iter().filter(|a| a.side != army.side).map(|a| a.pos).collect();
        path::find_path(&self.map, army.pos, to, army.men, self.weather, |c| enemies.contains(&c))
    }

    /// Game minutes until `army` finishes its march, at current conditions.
    pub fn eta_minutes(&self, army: &Army) -> u32 {
        let total: u32 =
            army.path.iter().map(|&c| path::enter_cost(&self.map, c, army.men, self.weather).unwrap_or(0)).sum();
        total.saturating_sub(army.progress / 10)
    }

    pub fn apply(&mut self, side: Side, order: Order) -> Result<(), &'static str> {
        let id = match order {
            Order::March { army, .. }
            | Order::Rest { army }
            | Order::Hold { army }
            | Order::Divide { army }
            | Order::Join { army } => army,
        };
        let i = self.armies.iter().position(|a| a.id == id).ok_or("no such army")?;
        if self.armies[i].side != side {
            return Err("not your army");
        }
        match order {
            Order::March { to, .. } => {
                let path = self.plan_path(&self.armies[i], to).ok_or("no route there")?;
                let a = &mut self.armies[i];
                a.path = path.into();
                a.progress = 0;
                a.state = if a.path.is_empty() { ArmyState::Holding } else { ArmyState::Marching };
            }
            Order::Rest { .. } | Order::Hold { .. } => {
                let a = &mut self.armies[i];
                a.path.clear();
                a.progress = 0;
                a.state = if matches!(order, Order::Rest { .. }) { ArmyState::Resting } else { ArmyState::Holding };
            }
            Order::Divide { .. } => {
                let a = &self.armies[i];
                if a.men < 2 * MEN_PER_COMPANY {
                    return Err("too few men to divide");
                }
                let split = a.men / 2;
                let food = a.food * split as u64 / a.men as u64;
                let mut d =
                    Army::new(self.next_army_id, side, &format!("Detachment {}", self.next_army_id), split, a.pos);
                d.stamina = a.stamina;
                d.food = food;
                self.next_army_id += 1;
                let a = &mut self.armies[i];
                a.men -= split;
                a.food -= food;
                let text = format!("{} splits off {} ({} men).", a.name, d.name, d.men);
                self.armies.push(d);
                self.log(Some(side), text);
            }
            Order::Join { .. } => {
                let pos = self.armies[i].pos;
                let others: Vec<usize> = (0..self.armies.len())
                    .filter(|&j| j != i && self.armies[j].side == side && self.armies[j].pos == pos)
                    .collect();
                if others.is_empty() {
                    return Err("no friendly army here to join");
                }
                let (mut men, mut food, mut stamina_sum) = (0u32, 0u64, 0u64);
                for &j in others.iter().chain(std::iter::once(&i)) {
                    let a = &self.armies[j];
                    men += a.men;
                    food += a.food;
                    stamina_sum += a.stamina as u64 * a.men as u64;
                }
                let a = &mut self.armies[i];
                a.men = men;
                a.food = food;
                a.stamina = (stamina_sum / men as u64) as u16;
                a.path.clear();
                a.state = ArmyState::Holding;
                let text = format!("{} absorbs {} unit(s), now {} men.", a.name, others.len(), men);
                let keep = self.armies[i].id;
                let gone: Vec<u32> = others.iter().map(|&j| self.armies[j].id).collect();
                self.armies.retain(|a| a.id == keep || !gone.contains(&a.id));
                self.log(Some(side), text);
            }
        }
        Ok(())
    }

    /// Advances the game by one minute.
    pub fn tick(&mut self) {
        self.clock.minute += 1;
        let m = self.clock.minute_of_day();
        self.move_armies();
        self.recover_stamina();
        self.update_captures();
        if m.is_multiple_of(60) {
            self.eat_and_resupply();
        }
        if m.is_multiple_of(180) {
            self.update_weather();
        }
        if m == 0 {
            self.produce();
        }
        if m == self.clock.moon_rise() && self.clock.is_blood_moon() {
            self.log(None, "The blood moon rises. Men mutter of omens.".into());
        }
    }

    fn move_armies(&mut self) {
        let weather = self.weather;
        let minute = self.clock.minute;
        let mut heads: Vec<(Side, Cell)> = self.armies.iter().map(|a| (a.side, a.pos)).collect();
        for (i, a) in self.armies.iter_mut().enumerate() {
            if a.state != ArmyState::Marching {
                continue;
            }
            let Some(&next) = a.path.front() else {
                a.state = ArmyState::Holding;
                continue;
            };
            // Stand-in until combat is designed: enemy armies can't share a cell,
            // so an army can block a road but not yet fight for it.
            if heads.iter().any(|&(s, c)| s != a.side && c == next) {
                continue;
            }
            let Some(cost) = path::enter_cost(&self.map, next, a.men, weather) else {
                a.path.clear();
                a.state = ArmyState::Holding;
                push_log(&mut self.log, minute, Some(a.side), format!("{} finds the way impassable.", a.name));
                continue;
            };
            a.progress += if a.exhausted() { 5 } else { 10 };
            if a.progress < cost * 10 {
                continue;
            }
            a.progress -= cost * 10;
            a.path.pop_front();
            a.step_to(next);
            heads[i].1 = next;
            a.stamina = a.stamina.saturating_sub(path::stamina_cost(&self.map, next, weather));
            if a.path.is_empty() {
                a.state = ArmyState::Holding;
                a.progress = 0;
                let text = match self.forts.iter().find(|f| f.contains(a.pos)) {
                    Some(f) => format!("{} arrives at {}.", a.name, f.name),
                    None => format!("{} halts.", a.name),
                };
                push_log(&mut self.log, minute, Some(a.side), text);
            }
        }
    }

    fn recover_stamina(&mut self) {
        for a in &mut self.armies {
            let in_own_fort = self.forts.iter().any(|f| f.owner == Some(a.side) && f.contains(a.pos));
            let gain = match a.state {
                ArmyState::Resting if in_own_fort => 4,
                ArmyState::Resting => 2,
                ArmyState::Holding => 1,
                ArmyState::Marching => 0,
            };
            if !a.starving {
                a.stamina = (a.stamina + gain).min(STAMINA_MAX);
            }
        }
    }

    fn eat_and_resupply(&mut self) {
        let minute = self.clock.minute;
        for a in &mut self.armies {
            if let Some(f) = self.forts.iter_mut().find(|f| f.owner == Some(a.side) && f.contains(a.pos)) {
                let want = (a.food_capacity().saturating_sub(a.food) / MILLI) as u32;
                a.food += f.take(want) as u64 * MILLI;
            }
            let mut need = a.men as u64 * MILLI / 24;
            if a.state == ArmyState::Marching {
                need = need * 5 / 4;
            }
            if a.food >= need {
                a.food -= need;
                a.starving = false;
            } else {
                a.food = 0;
                if !a.starving {
                    push_log(&mut self.log, minute, Some(a.side), format!("{} has run out of food!", a.name));
                }
                a.starving = true;
                a.stamina = a.stamina.saturating_sub(30);
            }
        }
    }

    fn update_captures(&mut self) {
        let minute = self.clock.minute;
        for f in &mut self.forts {
            let mut present = [false; 2];
            for a in self.armies.iter().filter(|a| f.contains(a.pos)) {
                present[a.side.idx()] = true;
            }
            let claimant = match present {
                [true, false] => Some(Side::West),
                [false, true] => Some(Side::East),
                _ => None,
            };
            // Garrisons don't resist yet: that waits on the combat design.
            match claimant {
                Some(s) if f.owner != Some(s) => {
                    let held = match f.capture {
                        Some((cs, n)) if cs == s => n + 1,
                        _ => 1,
                    };
                    if held >= CAPTURE_MINUTES {
                        f.owner = Some(s);
                        f.capture = None;
                        push_log(&mut self.log, minute, Some(s), format!("{} has been taken.", f.name));
                    } else {
                        f.capture = Some((s, held));
                    }
                }
                _ => f.capture = None,
            }
        }
    }

    fn produce(&mut self) {
        for f in &mut self.forts {
            f.add(f.daily_production());
            f.take(f.garrison);
        }
    }

    fn update_weather(&mut self) {
        use Weather::*;
        let hour = self.clock.minute_of_day() / 60;
        let misty_hours = !(9..21).contains(&hour);
        let roll = self.rng.range(0, 100);
        let next = match self.weather {
            Clear if roll < 60 => Clear,
            Clear if roll < 85 || !misty_hours => Cloudy,
            Clear => Fog,
            Cloudy if roll < 40 => Cloudy,
            Cloudy if roll < 65 => Clear,
            Cloudy => Rain,
            Rain if roll < 50 => Rain,
            Rain => Cloudy,
            Fog if roll < 40 && hour < 11 => Fog,
            Fog => Clear,
        };
        self.wind = (self.wind + self.rng.range(-1, 2) as i8).clamp(-3, 3);
        if next != self.weather {
            self.weather = next;
            let text = match next {
                Clear => "The sky clears.",
                Cloudy => "Clouds roll in.",
                Rain => "Rain begins to fall. The roads turn to mud.",
                Fog => "Fog settles over the lowlands.",
            };
            self.log(None, text.into());
        }
    }

    /// FNV-1a over the game state, for desync detection between lockstep peers.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: u64| {
            for b in v.to_le_bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
            }
        };
        mix(self.clock.minute as u64);
        mix(self.weather as u64);
        mix(self.wind as u64);
        for a in &self.armies {
            for v in
                [a.id as u64, a.men as u64, a.stamina as u64, a.food, a.pos.0 as u64, a.pos.1 as u64, a.progress as u64]
            {
                mix(v);
            }
        }
        for f in &self.forts {
            mix(f.owner.map_or(9, |s| s as u64));
            for s in f.stores {
                mix(s as u64);
            }
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 7;

    #[test]
    fn same_seed_same_game() {
        let run = || {
            let mut sim = Sim::new(SEED);
            sim.apply(Side::West, Order::March { army: 2, to: sim.forts[1].pos }).unwrap();
            for _ in 0..5000 {
                sim.tick();
            }
            sim.checksum()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn roads_are_connected() {
        let sim = Sim::new(SEED);
        for road in &sim.map.roads {
            let (x0, x1) = road.x_range();
            for x in x0..x1 - 1 {
                let (a, b) = (road.rows_in_column(x), road.rows_in_column(x + 1));
                let touch = a.clone().any(|ya| b.clone().any(|yb| (ya - yb).abs() <= 1));
                assert!(touch, "{} breaks between columns {x} and {}", road.name, x + 1);
            }
        }
    }

    #[test]
    fn rivers_are_crossed_only_at_bridges() {
        let sim = Sim::new(SEED);
        let scouts = sim.army(3).unwrap();
        let far_bank = (100, 13);
        let route = sim.plan_path(scouts, far_bank).expect("a bridge should exist");
        for &(x, y) in &route {
            if sim.map.terrain(x, y) == Terrain::River {
                assert!(sim.map.is_bridge(x, y), "forded the river at ({x},{y})");
            }
        }
    }

    #[test]
    fn big_armies_cannot_enter_forest() {
        let sim = Sim::new(SEED);
        let forest = (0..MAP_W)
            .flat_map(|x| (0..MAP_H).map(move |y| (x, y)))
            .find(|&(x, y)| sim.map.terrain(x, y) == Terrain::Forest && !sim.map.is_road(x, y))
            .unwrap();
        assert!(path::enter_cost(&sim.map, forest, path::FOREST_CAP, Weather::Clear).is_some());
        assert!(path::enter_cost(&sim.map, forest, path::FOREST_CAP + 1, Weather::Clear).is_none());
    }

    #[test]
    fn holding_a_fort_for_an_hour_takes_it() {
        let mut sim = Sim::new(SEED);
        let mill = sim.forts[1].pos;
        sim.armies[1].pos = mill;
        for _ in 0..CAPTURE_MINUTES {
            sim.tick();
        }
        assert_eq!(sim.forts[1].owner, Some(Side::West));
    }

    #[test]
    fn divide_then_join_round_trips() {
        let mut sim = Sim::new(SEED);
        let before = sim.army(1).unwrap().men;
        sim.apply(Side::West, Order::Divide { army: 1 }).unwrap();
        assert_eq!(sim.armies.len(), 6);
        sim.apply(Side::West, Order::Join { army: 1 }).unwrap();
        assert_eq!(sim.armies.len(), 5);
        assert_eq!(sim.army(1).unwrap().men, before);
    }
}
