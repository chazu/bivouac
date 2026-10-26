//! Supply convoys: wagon trains that shuttle stores along the roads from one fort to
//! another for as long as the route stands. They can't fight. Any enemy army that
//! reaches one takes it, unless a friendly army is close enough to stand in the way.

use std::collections::VecDeque;

use crate::army::MILLI;
use crate::combat::RETARGET_EVERY;
use crate::{Cell, Side, Sim, path, push_log};

/// Rations a train carries at most.
pub const CONVOY_LOAD: u32 = 1000;
pub const WAGONS: u32 = 4;
/// How long it takes to raise a new train after one is lost.
pub const REBUILD_MINUTES: u32 = 12 * 60;
/// Days of food the source fort keeps back for its own garrison.
const RESERVE_DAYS: u32 = 3;
/// Not worth setting out with less than this.
const MIN_LOAD: u32 = 100;
const HANDLING_MINUTES: u32 = 30;
/// How often a train with nothing to carry checks again.
const WAIT_MINUTES: u32 = 60;
/// Aggressive armies go after enemy wagons they can see this close.
pub(crate) const RAID_RANGE: i32 = 6;
const TRAIL_LEN: usize = 3;

const NAMES: [&str; 12] =
    ["Millet", "Rice", "Lotus", "Salt", "Jade", "Plum", "Bamboo", "Ox", "Crane", "Copper", "Thistle", "Willow"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leg {
    /// At the source, minutes left; it waits here while there's nothing to send.
    Loading(u32),
    Outbound,
    /// At the destination, minutes left.
    Unloading(u32),
    Returning,
    /// Taken or burned; a new train sets out from the source at `until`.
    Lost { until: u32 },
}

#[derive(Clone, Debug)]
pub struct Convoy {
    pub id: u32,
    pub side: Side,
    pub name: String,
    /// Fort ids at either end of the route.
    pub from: u32,
    pub to: u32,
    /// Rations of each good aboard.
    pub cargo: [u32; 3],
    pub pos: Cell,
    pub path: VecDeque<Cell>,
    /// Progress toward the next cell, in tenths of a game minute.
    pub progress: u32,
    pub leg: Leg,
    /// Cells just left, newest first, where the rear wagons are.
    pub trail: VecDeque<Cell>,
    /// The route is called off: the train makes for the nearest end still held,
    /// unloads there and disbands.
    pub recalled: bool,
    /// Fort the current path leads to.
    pub heading: u32,
    /// Rations delivered so far.
    pub delivered: u32,
}

impl Convoy {
    pub fn load(&self) -> u32 {
        self.cargo.iter().sum()
    }

    pub fn lost(&self) -> bool {
        matches!(self.leg, Leg::Lost { .. })
    }

    pub fn on_the_road(&self) -> bool {
        matches!(self.leg, Leg::Outbound | Leg::Returning)
    }

    /// Cells the wagons stand in, lead wagon first.
    pub fn cells(&self) -> Vec<Cell> {
        if self.lost() {
            return Vec::new();
        }
        let mut cells = vec![self.pos];
        for &c in &self.trail {
            if !cells.contains(&c) {
                cells.push(c);
            }
        }
        cells.truncate(2);
        cells
    }
}

fn distance(a: Cell, b: Cell) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

impl Sim {
    pub fn convoy(&self, id: u32) -> Option<&Convoy> {
        self.convoys.iter().find(|c| c.id == id)
    }

    /// The route out of fort `from`, if one runs.
    pub fn route_from(&self, from: u32) -> Option<&Convoy> {
        self.convoys.iter().find(|c| c.from == from)
    }

    /// Whether `side` can see the wagons: its own always, the enemy's while they're
    /// in sight. Trains are big, slow and noisy, so no dice.
    pub fn sees_convoy(&self, side: Side, c: &Convoy) -> bool {
        c.side == side || (!c.lost() && c.cells().iter().any(|&cell| self.intel(side).sees(cell)))
    }

    /// Game minutes until a train reaches the end of its road, at current conditions.
    pub fn convoy_eta(&self, c: &Convoy) -> u32 {
        let total: u32 = c.path.iter().map(|&p| path::wagon_cost(&self.map, p, self.weather).unwrap_or(0)).sum();
        total.saturating_sub(c.progress / 10)
    }

    fn wagon_route(&self, from: Cell, to: Cell) -> Option<Vec<Cell>> {
        let weather = self.weather;
        path::find_route(&self.map, from, to, |c| path::wagon_cost(&self.map, c, weather), |_| false)
    }

    fn fort_pos(&self, id: u32) -> Cell {
        self.forts.iter().find(|f| f.id == id).map_or((0, 0), |f| f.pos)
    }

    fn holds(&self, side: Side, fort: u32) -> bool {
        self.forts.iter().any(|f| f.id == fort && f.owner == Some(side))
    }

    /// Sets up (or redirects) the supply route out of fort `from`.
    pub(crate) fn supply(&mut self, side: Side, from: u32, to: u32) -> Result<(), &'static str> {
        if !self.holds(side, from) || !self.holds(side, to) {
            return Err("supply runs only between forts you hold");
        }
        if from == to {
            return Err("a route needs two forts");
        }
        let (a, b) = (self.fort_pos(from), self.fort_pos(to));
        if self.wagon_route(a, b).is_none() {
            return Err("no road joins those forts");
        }
        let names = |s: &Sim| {
            let name = |id| s.forts.iter().find(|f| f.id == id).map_or("?", |f| f.name);
            (name(from), name(to))
        };
        if let Some(i) = self.convoys.iter().position(|c| c.from == from) {
            if self.convoys[i].recalled {
                return Err("that train is being recalled");
            }
            let redirect = self.wagon_route(self.convoys[i].pos, b);
            let c = &mut self.convoys[i];
            c.to = to;
            if let (Leg::Outbound, Some(route)) = (c.leg, redirect) {
                c.heading = to;
                c.path = route.into();
            }
            let (f, t) = names(self);
            let text = format!("The {} now runs from {f} to {t}.", self.convoys[i].name);
            self.note(Some(side), text);
            return Ok(());
        }
        let id = self.next_convoy_id;
        self.next_convoy_id += 1;
        let name = format!("{} Train", NAMES[(id as usize - 1) % NAMES.len()]);
        let (f, t) = names(self);
        let text = format!("The {name} will carry stores from {f} to {t}.");
        self.convoys.push(Convoy {
            id,
            side,
            name,
            from,
            to,
            cargo: [0; 3],
            pos: a,
            path: VecDeque::new(),
            progress: 0,
            leg: Leg::Loading(HANDLING_MINUTES),
            trail: VecDeque::new(),
            recalled: false,
            heading: from,
            delivered: 0,
        });
        self.note(Some(side), text);
        Ok(())
    }

    pub(crate) fn cancel_supply(&mut self, side: Side, from: u32) -> Result<(), &'static str> {
        let i = self.convoys.iter().position(|c| c.from == from && c.side == side).ok_or("no route runs from there")?;
        let text = format!("The {} is recalled.", self.convoys[i].name);
        self.recall(i);
        self.note(Some(side), text);
        Ok(())
    }

    /// Calls a train home. Returns false if it was disbanded on the spot.
    fn recall(&mut self, i: usize) -> bool {
        let c = &self.convoys[i];
        let home = [c.from, c.to].into_iter().find(|&f| self.holds(c.side, f));
        let idle = matches!(c.leg, Leg::Loading(_) | Leg::Lost { .. });
        let route = home.and_then(|h| self.wagon_route(c.pos, self.fort_pos(h)));
        match (home, route) {
            (Some(h), Some(route)) if !idle => {
                let c = &mut self.convoys[i];
                c.recalled = true;
                c.heading = h;
                c.path = route.into();
                c.leg = Leg::Returning;
                true
            }
            _ => {
                // Still at the source (or nothing left to save): unpack and go home.
                let c = self.convoys.remove(i);
                if let Some(f) = self.forts.iter_mut().find(|f| f.id == c.from && f.owner == Some(c.side)) {
                    f.add(c.cargo);
                }
                false
            }
        }
    }

    /// One minute of the supply trains: loading, the road, unloading, and losing
    /// routes whose forts have fallen.
    pub(crate) fn run_convoys(&mut self) {
        let now = self.clock.minute;
        let mut i = 0;
        while i < self.convoys.len() {
            let c = &self.convoys[i];
            if !c.recalled && !(self.holds(c.side, c.from) && self.holds(c.side, c.to)) {
                let text = format!("With its fort lost, the {} turns for home.", c.name);
                let side = c.side;
                self.note(Some(side), text);
                if !self.recall(i) {
                    continue;
                }
            }
            match self.convoys[i].leg {
                Leg::Lost { until } if now >= until => {
                    let c = &mut self.convoys[i];
                    c.leg = Leg::Loading(HANDLING_MINUTES);
                    let text = format!("A new {} is raised.", c.name);
                    let side = c.side;
                    self.note(Some(side), text);
                }
                Leg::Lost { .. } => {}
                Leg::Loading(n) if n > 0 => self.convoys[i].leg = Leg::Loading(n - 1),
                Leg::Loading(_) => self.depart(i),
                Leg::Unloading(n) if n > 0 => self.convoys[i].leg = Leg::Unloading(n - 1),
                Leg::Unloading(_) => self.unload(i),
                Leg::Outbound | Leg::Returning => {
                    if self.drive(i) {
                        continue; // disbanded on arrival
                    }
                }
            }
            i += 1;
        }
    }

    /// Loads what the source can spare and sets out, or waits for stores to build up.
    fn depart(&mut self, i: usize) {
        let (from, to) = (self.convoys[i].from, self.convoys[i].to);
        let room = self.forts.iter().find(|f| f.id == to).map_or(0, |f| f.room());
        let spare = self.forts.iter().find(|f| f.id == from).map_or(0, |f| {
            f.total_stores().saturating_sub(f.garrison * RESERVE_DAYS)
        });
        let want = CONVOY_LOAD.min(spare).min(room);
        let route = self.wagon_route(self.convoys[i].pos, self.fort_pos(to));
        let (Some(route), true) = (route, want >= MIN_LOAD) else {
            self.convoys[i].leg = Leg::Loading(WAIT_MINUTES);
            return;
        };
        let f = self.forts.iter_mut().find(|f| f.id == from).expect("source is held");
        let cargo = f.take_goods(want);
        let c = &mut self.convoys[i];
        c.cargo = cargo;
        c.path = route.into();
        c.progress = 0;
        c.heading = to;
        c.leg = Leg::Outbound;
    }

    fn unload(&mut self, i: usize) {
        let c = &self.convoys[i];
        let (to, cargo) = (c.to, c.cargo);
        let left = match self.forts.iter_mut().find(|f| f.id == to) {
            Some(f) => f.add(cargo),
            None => cargo,
        };
        let route = self.wagon_route(c.pos, self.fort_pos(c.from));
        let c = &mut self.convoys[i];
        c.delivered += c.load() - left.iter().sum::<u32>();
        // Whatever didn't fit rides home again.
        c.cargo = left;
        c.path = route.unwrap_or_default().into();
        c.progress = 0;
        c.heading = c.from;
        c.leg = Leg::Returning;
    }

    /// Moves a train one minute along its road. Returns true if it disbanded.
    fn drive(&mut self, i: usize) -> bool {
        let side = self.convoys[i].side;
        let Some(&next) = self.convoys[i].path.front() else { return self.arrive(i) };
        // Wagons can't push through an army; they wait, which is often fatal.
        if self.armies.iter().any(|a| a.side != side && a.occupies(next)) {
            return false;
        }
        let Some(cost) = path::wagon_cost(&self.map, next, self.weather) else {
            self.convoys[i].path.clear();
            return false;
        };
        let c = &mut self.convoys[i];
        c.progress += 10;
        if c.progress < cost * 10 {
            return false;
        }
        c.progress -= cost * 10;
        c.path.pop_front();
        c.trail.push_front(c.pos);
        c.trail.truncate(TRAIL_LEN);
        c.pos = next;
        if c.path.is_empty() { self.arrive(i) } else { false }
    }

    fn arrive(&mut self, i: usize) -> bool {
        let c = &mut self.convoys[i];
        c.progress = 0;
        c.trail.clear();
        if c.recalled {
            let c = self.convoys.remove(i);
            let fort = self.forts.iter_mut().find(|f| f.id == c.heading);
            let name = fort.as_ref().map_or("home", |f| f.name);
            let text = format!("The {} reaches {name} and disbands.", c.name);
            if let Some(f) = fort {
                f.add(c.cargo);
            }
            self.note(Some(c.side), text);
            return true;
        }
        c.leg = if c.leg == Leg::Outbound { Leg::Unloading(HANDLING_MINUTES) } else { Leg::Loading(HANDLING_MINUTES) };
        false
    }

    /// Enemy armies that reach a train take it, unless an escort or a garrison
    /// stands with it.
    pub(crate) fn raid_convoys(&mut self) {
        let minute = self.clock.minute;
        for i in 0..self.convoys.len() {
            let c = &self.convoys[i];
            if c.lost() {
                continue;
            }
            let cells = c.cells();
            let near = |a: &crate::Army| {
                !a.routing() && a.column().iter().any(|&(x, _)| cells.iter().any(|&w| distance(x, w) <= 1))
            };
            let guarded = self.armies.iter().any(|a| a.side == c.side && near(a))
                || self.forts.iter().any(|f| f.owner == Some(c.side) && f.garrison > 0 && f.contains(c.pos));
            if guarded {
                continue;
            }
            let Some(r) = self.armies.iter().position(|a| a.side != c.side && near(a)) else { continue };
            let place = self.place_name(c.pos);
            let total = c.load();
            let raider = &mut self.armies[r];
            let room = (raider.food_capacity().saturating_sub(raider.food) / MILLI) as u32;
            let seized = total.min(room);
            raider.food += seized as u64 * MILLI;
            let raider_name = raider.name.clone();
            let c = &mut self.convoys[i];
            let text = if total == 0 {
                format!("{raider_name} falls on the empty {} {place} and burns the wagons.", c.name)
            } else {
                format!(
                    "{raider_name} falls on the {} {place}: {seized} rations seized, {} burned.",
                    c.name,
                    total - seized
                )
            };
            c.cargo = [0; 3];
            c.path.clear();
            c.trail.clear();
            c.progress = 0;
            c.pos = self.forts.iter().find(|f| f.id == c.from).map_or(c.pos, |f| f.pos);
            c.leg = Leg::Lost { until: minute + REBUILD_MINUTES };
            let id = c.id;
            push_log(&mut self.log, minute, Some(self.armies[r].side), text);
            for a in &mut self.armies {
                if a.quarry == Some(id) {
                    a.quarry = None;
                }
            }
        }
        // A recalled train that was taken has nothing to come back to.
        self.convoys.retain(|c| !(c.recalled && c.lost()));
    }

    /// Armies sent after a train (or aggressive ones that spot one) run it down.
    pub(crate) fn hunt_convoys(&mut self) {
        let minute = self.clock.minute;
        for i in 0..self.armies.len() {
            let a = &self.armies[i];
            if a.routing() || a.target.is_some() {
                continue;
            }
            if a.quarry.is_none() && a.stance == crate::Stance::Aggressive {
                let prey = self
                    .convoys
                    .iter()
                    .filter(|c| c.side != a.side && self.sees_convoy(a.side, c))
                    .filter(|c| distance(c.pos, a.pos) <= RAID_RANGE)
                    .min_by_key(|c| (distance(c.pos, a.pos), c.id))
                    .map(|c| c.id);
                self.armies[i].quarry = prey;
            }
            let a = &self.armies[i];
            let Some(q) = a.quarry else { continue };
            let seen = self.convoy(q).filter(|c| !c.lost() && self.sees_convoy(a.side, c)).map(|c| c.pos);
            match seen {
                Some(at) if a.path.is_empty() || minute.is_multiple_of(RETARGET_EVERY) => {
                    if let Some(route) = self.plan_path(a, at) {
                        let a = &mut self.armies[i];
                        a.path = route.into();
                        if !a.path.is_empty() {
                            a.state = crate::ArmyState::Marching;
                        }
                    }
                }
                Some(_) => {}
                // Out of sight: carry on to where it was last seen, then give up.
                None if a.path.is_empty() => self.armies[i].quarry = None,
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Order, Side, Sim};

    use super::*;

    /// A sim where the West also holds Willow Ford Mill and a train runs to it.
    fn with_route() -> Sim {
        let mut sim = Sim::new(7);
        sim.forts[1].owner = Some(Side::West);
        sim.forts[1].garrison = 0;
        sim.forts[1].stores = [0; 3];
        sim.apply(Side::West, Order::Supply { from: 0, to: 1 }).unwrap();
        sim
    }

    #[test]
    fn trains_carry_stores_down_the_road() {
        let mut sim = with_route();
        let before = sim.forts[0].total_stores();
        for _ in 0..24 * 60 {
            sim.tick();
            assert!(sim.convoys[0].cells().iter().all(|&(x, y)| {
                sim.map.is_road(x, y) || sim.map.terrain(x, y) == crate::Terrain::Fort
            }));
        }
        assert!(sim.convoys[0].delivered >= CONVOY_LOAD, "delivered {}", sim.convoys[0].delivered);
        assert!(sim.forts[1].total_stores() > 0);
        assert!(sim.forts[0].total_stores() < before + 2500);
    }

    #[test]
    fn raiders_take_an_unguarded_train() {
        let mut sim = with_route();
        while sim.convoys[0].leg != Leg::Outbound {
            sim.tick();
        }
        for _ in 0..200 {
            sim.tick();
        }
        // Drop the Ash Wolves just ahead of the wagons.
        let ahead = sim.convoys[0].path[2];
        let wolves = sim.armies.iter_mut().find(|a| a.id == 5).unwrap();
        wolves.pos = ahead;
        wolves.trail.clear();
        wolves.food = 0;
        for _ in 0..90 {
            sim.tick();
            if sim.convoys[0].lost() {
                break;
            }
        }
        assert!(sim.convoys[0].lost());
        assert!(sim.army(5).unwrap().food > 0, "raiders eat well");
        assert!(sim.log.iter().any(|e| e.text.contains("falls on the")));
    }

    #[test]
    fn an_escort_keeps_the_wagons_safe() {
        let mut sim = with_route();
        while sim.convoys[0].leg != Leg::Outbound {
            sim.tick();
        }
        let at = sim.convoys[0].pos;
        let banner = sim.armies.iter_mut().find(|a| a.id == 1).unwrap();
        banner.pos = at;
        banner.trail.clear();
        let wolves = sim.armies.iter_mut().find(|a| a.id == 5).unwrap();
        wolves.pos = (at.0 + 1, at.1);
        sim.raid_convoys();
        assert!(!sim.convoys[0].lost());
    }

    #[test]
    fn losing_a_fort_ends_its_route() {
        let mut sim = with_route();
        for _ in 0..60 {
            sim.tick();
        }
        sim.forts[1].owner = Some(Side::East);
        for _ in 0..24 * 60 {
            sim.tick();
        }
        assert!(sim.convoys.is_empty());
    }

    #[test]
    fn every_fort_is_on_the_road_network() {
        for seed in 0..20 {
            let sim = Sim::new(seed);
            for f in &sim.forts {
                assert!(sim.wagon_route(sim.forts[0].pos, f.pos).is_some(), "seed {seed}: no road to {}", f.name);
            }
        }
    }
}
