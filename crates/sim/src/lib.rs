//! Bivouac's game rules. Pure and deterministic: no rendering, no wall-clock time,
//! no platform randomness, so lockstep netplay, replays and headless AI games all work.
//! Everything that changes the game goes through [`Sim::apply`] and [`Sim::tick`].

pub mod army;
pub mod clock;
pub mod combat;
pub mod convoy;
pub mod fort;
pub mod intel;
pub mod map;
pub mod path;
pub mod rng;
mod scenario;

use std::collections::VecDeque;

pub use army::{Army, ArmyState, Pace, Stance};
pub use clock::{Clock, Weather};
pub use combat::Clash;
pub use convoy::{Convoy, Leg};
pub use fort::{Extra, Fort, FortKind};
pub use intel::{Intel, Report, Seen, Track};
pub use map::{MAP_H, MAP_W, Map, Road, Terrain};

use army::{ENTRENCH_MAX, MEN_PER_COMPANY, MILLI, STAMINA_MAX};
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

    pub fn faction(self) -> &'static str {
        match self {
            Side::West => "the Azure Remnant",
            Side::East => "the Vermilion Host",
        }
    }
}

/// Everything a player (or the AI, or a remote peer) can tell the game to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Order {
    March {
        army: u32,
        to: Cell,
    },
    /// Carry on from the end of the current march to `to`.
    Waypoint {
        army: u32,
        to: Cell,
    },
    Rest {
        army: u32,
    },
    Hold {
        army: u32,
    },
    /// Split off `men` (half the army if `None`) into a new one in the same cell.
    Divide {
        army: u32,
        men: Option<u32>,
    },
    /// Merge every friendly army standing in the same cell into this one.
    Join {
        army: u32,
    },
    /// Close with an enemy army and fight it, chasing it if it moves.
    Attack {
        army: u32,
        target: u32,
    },
    SetStance {
        army: u32,
        stance: Stance,
    },
    /// March slower to spare stamina, or force the pace.
    SetPace {
        army: u32,
        pace: Pace,
    },
    /// Put the army into the garrison of the friendly fort it stands in.
    Garrison {
        army: u32,
    },
    /// Form a new army from up to `men` of a friendly fort's garrison.
    Muster {
        fort: u32,
        men: u32,
    },
    /// Run supply wagons from fort `from` to fort `to` until told otherwise.
    /// A fort sends out one route; ordering another redirects it.
    Supply {
        from: u32,
        to: u32,
    },
    /// Call off the route out of fort `from`.
    CancelSupply {
        from: u32,
    },
    /// Run down an enemy supply train and take it.
    Raid {
        army: u32,
        convoy: u32,
    },
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub minute: u32,
    pub side: Option<Side>,
    pub text: String,
    /// Only `side` knows about this (its own movements, its scouts' reports).
    pub private: bool,
}

impl LogEntry {
    pub fn visible_to(&self, viewer: Side) -> bool {
        !self.private || self.side == Some(viewer)
    }
}

const LOG_LEN: usize = 64;
const MAX_GARRISON: u32 = 2000;

pub struct Sim {
    pub map: Map,
    pub forts: Vec<Fort>,
    pub armies: Vec<Army>,
    pub clock: Clock,
    pub weather: Weather,
    /// Wind along the front, -3..=3; negative blows toward the west.
    pub wind: i8,
    pub log: VecDeque<LogEntry>,
    /// Where fighting happened this minute, for drawing.
    pub clashes: Vec<Clash>,
    /// Ids of armies that fought this minute.
    pub engaged: Vec<u32>,
    /// Pairs of army ids (low, high) currently locked in battle.
    battles: Vec<(u32, u32)>,
    /// (fort id, army id) pairs where a garrison is fighting, so each is announced once.
    garrison_fights: Vec<(u32, u32)>,
    /// Armies raised so far by each side, for naming new ones.
    raised: [u32; 2],
    /// Battles recently announced in the log, with the minute.
    announced: Vec<((u32, u32), u32)>,
    /// Set when a side takes the other's main fort.
    pub winner: Option<Side>,
    /// What each side knows of the other, indexed by `Side as usize`.
    intel: [Intel; 2],
    /// Footprints, newest last. They fade with time; rain makes them last.
    pub tracks: Vec<Track>,
    /// Supply trains, one per route.
    pub convoys: Vec<Convoy>,
    rng: Rng,
    next_army_id: u32,
    next_convoy_id: u32,
}

/// Logs news everyone hears (battles, captures, weather).
fn push_log(log: &mut VecDeque<LogEntry>, minute: u32, side: Option<Side>, text: String) {
    log.push_front(LogEntry { minute, side, text, private: false });
    log.truncate(LOG_LEN);
}

/// Logs something only `side` knows.
fn push_private(log: &mut VecDeque<LogEntry>, minute: u32, side: Option<Side>, text: String) {
    log.push_front(LogEntry { minute, side, text, private: true });
    log.truncate(LOG_LEN);
}

impl Sim {
    pub fn new(seed: u64) -> Sim {
        let mut sim = scenario::ashford_valley(seed);
        sim.detect();
        sim
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

    fn note(&mut self, side: Option<Side>, text: String) {
        push_private(&mut self.log, self.clock.minute, side, text);
    }

    /// Every cell an enemy of `side` occupies. Armies can't walk through each other.
    pub fn enemy_cells(&self, side: Side) -> Vec<Cell> {
        self.armies
            .iter()
            .filter(|a| a.side != side)
            .flat_map(|a| a.column(&self.map).into_iter().map(|(c, _)| c))
            .collect()
    }

    /// Routes around enemies this army's side can see; unseen ones are bumped into.
    pub fn plan_path(&self, army: &Army, to: Cell) -> Option<Vec<Cell>> {
        self.plan_path_from(army, army.pos, to)
    }

    fn plan_path_from(&self, army: &Army, from: Cell, to: Cell) -> Option<Vec<Cell>> {
        let blocked = self.blocked_cells(army.side, from);
        path::find_path(&self.map, from, to, army.men, self.weather, |c| blocked.contains(&c))
    }

    /// Where a march order to `to` would take `army`: there, or if the army can't
    /// get there (too big for the woods, say), as close as the ground allows. A
    /// march on a fort whose gates are shut goes to the ground before its gates.
    pub fn plan_march(&self, army: &Army, to: Cell) -> Option<Vec<Cell>> {
        self.plan_march_from(army, army.pos, to)
    }

    fn plan_march_from(&self, army: &Army, from: Cell, to: Cell) -> Option<Vec<Cell>> {
        let blocked = self.blocked_cells(army.side, from);
        if let Some(f) = self.fort_at(to).filter(|f| f.bars(army.side) && !f.contains(from)) {
            let mut gates = self.assault_cells(f);
            gates.sort_by_key(|&c| ((c.0 - from.0).abs().max((c.1 - from.1).abs()), c));
            let storm = gates
                .iter()
                .find_map(|&g| path::find_path(&self.map, from, g, army.men, self.weather, |c| blocked.contains(&c)));
            if storm.is_some() {
                return storm;
            }
        }
        let cost = |c| path::enter_cost(&self.map, c, army.men, self.weather);
        path::find_route_near(&self.map, from, to, cost, |c| blocked.contains(&c))
    }

    /// Cells an army of `side` setting out from `from` must route around: enemies
    /// it can see, and forts whose gates are shut to it (unless it's already inside).
    fn blocked_cells(&self, side: Side, from: Cell) -> Vec<Cell> {
        let mut cells = self.sighted_enemy_cells(side);
        cells
            .extend(self.barred_cells(side).into_iter().filter(|&c| self.fort_at(c).is_none_or(|f| !f.contains(from))));
        cells
    }

    /// Every cell of every fort whose gates are shut to `side`.
    fn barred_cells(&self, side: Side) -> Vec<Cell> {
        self.forts
            .iter()
            .filter(|f| f.bars(side))
            .flat_map(|f| (-2..=2).flat_map(move |dx| (-1..=1).map(move |dy| (f.pos.0 + dx, f.pos.1 + dy))))
            .collect()
    }

    /// The ground outside fort `f` right before one of its gates: the only place a
    /// fort can be stormed from.
    pub fn assault_cells(&self, f: &Fort) -> Vec<Cell> {
        let mut cells = Vec::new();
        for (gx, gy) in (-2..=2).flat_map(|dx| (-1..=1).map(move |dy| (f.pos.0 + dx, f.pos.1 + dy))) {
            if !self.map.is_gate(gx, gy) {
                continue;
            }
            for (dx, dy) in map::NEIGHBORS {
                let c = (gx + dx, gy + dy);
                if self.map.in_bounds(c.0, c.1) && !f.contains(c) && !cells.contains(&c) {
                    cells.push(c);
                }
            }
        }
        cells
    }

    /// Whether blows can pass between neighboring cells `a` and `b`. Walls stop
    /// them except at a gate.
    pub(crate) fn blows_pass(&self, a: Cell, b: Cell) -> bool {
        let (fa, fb) = (self.fort_at(a).map(|f| f.id), self.fort_at(b).map(|f| f.id));
        fa == fb || ((fa.is_none() || self.map.is_gate(a.0, a.1)) && (fb.is_none() || self.map.is_gate(b.0, b.1)))
    }

    fn sighted_enemy_cells(&self, side: Side) -> Vec<Cell> {
        let intel = self.intel(side);
        self.armies
            .iter()
            .filter(|a| a.side != side && intel.level(a.id).is_some_and(|s| s >= Seen::Sighted))
            .flat_map(|a| a.column(&self.map).into_iter().map(|(c, _)| c))
            .collect()
    }

    /// Game minutes for `army` to walk `route` at its pace, at current conditions.
    pub fn route_minutes(&self, army: &Army, route: &[Cell]) -> u32 {
        let cost: u32 =
            route.iter().map(|&c| path::enter_cost(&self.map, c, army.men, self.weather).unwrap_or(0)).sum();
        cost * 10 / army.pace.speed()
    }

    pub fn in_battle(&self, army: u32) -> bool {
        self.engaged.contains(&army)
    }

    /// Game minutes until `army` finishes its march, at current conditions.
    pub fn eta_minutes(&self, army: &Army) -> u32 {
        let route: Vec<Cell> = army.path.iter().copied().collect();
        self.route_minutes(army, &route).saturating_sub(army.progress / army.pace.speed())
    }

    pub fn apply(&mut self, side: Side, order: Order) -> Result<(), &'static str> {
        match order {
            Order::Muster { fort, men } => return self.muster(side, fort, men),
            Order::Supply { from, to } => return self.supply(side, from, to),
            Order::CancelSupply { from } => return self.cancel_supply(side, from),
            _ => {}
        }
        let id = match order {
            Order::Muster { .. } | Order::Supply { .. } | Order::CancelSupply { .. } => unreachable!("handled above"),
            Order::March { army, .. }
            | Order::Waypoint { army, .. }
            | Order::Rest { army }
            | Order::Hold { army }
            | Order::Divide { army, .. }
            | Order::Join { army }
            | Order::Raid { army, .. }
            | Order::Attack { army, .. }
            | Order::SetStance { army, .. }
            | Order::SetPace { army, .. }
            | Order::Garrison { army } => army,
        };
        let i = self.armies.iter().position(|a| a.id == id).ok_or("no such army")?;
        if self.armies[i].side != side {
            return Err("not your army");
        }
        if self.armies[i].routing() && !matches!(order, Order::SetStance { .. } | Order::SetPace { .. }) {
            return Err("routing: deaf to orders until it rallies");
        }
        match order {
            Order::March { to, .. } => {
                let mut path = self.plan_march(&self.armies[i], to).ok_or("no route there")?;
                // Doubling back: the column turns about and its tail leads.
                let cells: Vec<Cell> = self.armies[i].column(&self.map).iter().map(|&(c, _)| c).collect();
                if !self.armies[i].deployed() && cells.len() > 1 && path.first() == cells.get(1) {
                    let mut turned = self.armies[i].clone();
                    turned.pos = cells[cells.len() - 1];
                    turned.trail = cells[..cells.len() - 1].iter().rev().copied().collect();
                    if let Some(p) = self.plan_march(&turned, to) {
                        self.armies[i].pos = turned.pos;
                        self.armies[i].trail = turned.trail;
                        path = p;
                    }
                }
                let a = &mut self.armies[i];
                a.path = path.into();
                a.progress = 0;
                a.target = None;
                a.quarry = None;
                a.form_line = false;
                a.state = if a.path.is_empty() { ArmyState::Holding } else { ArmyState::Marching };
            }
            Order::Waypoint { army, to } => {
                let a = &self.armies[i];
                let Some(&end) = a.path.back().filter(|_| a.state == ArmyState::Marching) else {
                    return self.apply(side, Order::March { army, to });
                };
                let more = self.plan_march_from(a, end, to).ok_or("no route there")?;
                self.armies[i].path.extend(more);
            }
            Order::Rest { .. } | Order::Hold { .. } => {
                let a = &mut self.armies[i];
                a.path.clear();
                a.progress = 0;
                a.target = None;
                a.quarry = None;
                a.state = if matches!(order, Order::Rest { .. }) { ArmyState::Resting } else { ArmyState::Holding };
                // Holding means standing to in line of battle.
                a.form_line = matches!(order, Order::Hold { .. });
            }
            Order::Attack { target, .. } => {
                let enemy = self.army(target).ok_or("no such army")?;
                if enemy.side == side {
                    return Err("that army is on your side");
                }
                if self.knows(side, target).is_none_or(|s| s < Seen::Sighted) {
                    return Err("you can't see that army");
                }
                self.armies[i].target = Some(target);
                self.armies[i].quarry = None;
                // The chase itself is steered each minute by `behave`.
                self.armies[i].path.clear();
            }
            Order::Raid { convoy, .. } => {
                let c = self.convoy(convoy).ok_or("no such convoy")?;
                if c.side == side {
                    return Err("those are your own wagons");
                }
                if !self.sees_convoy(side, c) {
                    return Err("you can't see those wagons");
                }
                let a = &mut self.armies[i];
                a.quarry = Some(convoy);
                a.target = None;
                // Steered each minute by `hunt_convoys`.
                a.path.clear();
            }
            Order::Muster { .. } | Order::Supply { .. } | Order::CancelSupply { .. } => unreachable!("handled above"),
            Order::SetStance { stance, .. } => {
                self.armies[i].stance = stance;
            }
            Order::SetPace { pace, .. } => {
                self.armies[i].pace = pace;
            }
            Order::Garrison { .. } => {
                let a = &self.armies[i];
                let f = self
                    .forts
                    .iter_mut()
                    .find(|f| f.owner == Some(side) && f.contains(a.pos))
                    .ok_or("must stand in a fort you hold")?;
                let total = f.garrison + a.men;
                f.garrison_morale = ((f.garrison_morale as u64 * f.garrison as u64 + a.morale as u64 * a.men as u64)
                    / total as u64) as u32;
                f.garrison = total;
                let text = format!("{} joins the garrison of {} ({} men).", a.name, f.name, total);
                self.armies.remove(i);
                self.note(Some(side), text);
            }
            Order::Divide { men, .. } => {
                let a = &self.armies[i];
                if a.men < 2 * MEN_PER_COMPANY {
                    return Err("too few men to divide");
                }
                let split = men.unwrap_or(a.men / 2).clamp(MEN_PER_COMPANY, a.men - MEN_PER_COMPANY);
                let food = a.food * split as u64 / a.men as u64;
                let pos = a.pos;
                let name = self.new_army_name(side);
                let a = &self.armies[i];
                let mut d = Army::new(self.next_army_id, side, &name, split, pos);
                d.stamina = a.stamina;
                d.morale = a.morale;
                d.stance = a.stance;
                d.pace = a.pace;
                d.food = food;
                self.next_army_id += 1;
                let a = &mut self.armies[i];
                a.men -= split;
                a.food -= food;
                let text = format!("{} splits off {} ({} men).", a.name, d.name, d.men);
                self.armies.push(d);
                self.note(Some(side), text);
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
                self.note(Some(side), text);
            }
        }
        Ok(())
    }

    /// A name for a new army: "Third Banner" for the Remnant, "Second Cohort" for the Host.
    fn new_army_name(&mut self, side: Side) -> String {
        const ORDINALS: [&str; 12] = [
            "First", "Second", "Third", "Fourth", "Fifth", "Sixth", "Seventh", "Eighth", "Ninth", "Tenth", "Eleventh",
            "Twelfth",
        ];
        let n = &mut self.raised[side.idx()];
        *n += 1;
        let word = if side == Side::West { "Banner" } else { "Cohort" };
        match ORDINALS.get(*n as usize - 1) {
            Some(o) => format!("{o} {word}"),
            None => format!("{word} {n}"),
        }
    }

    fn muster(&mut self, side: Side, fort: u32, men: u32) -> Result<(), &'static str> {
        let f = self.forts.iter().find(|f| f.id == fort).ok_or("no such fort")?;
        if f.owner != Some(side) {
            return Err("not your fort");
        }
        let men = men.min(f.garrison);
        if men < MEN_PER_COMPANY {
            return Err("too few men in the garrison");
        }
        let id = self.next_army_id;
        self.next_army_id += 1;
        let name = self.new_army_name(side);
        let f = self.forts.iter_mut().find(|f| f.id == fort).expect("checked above");
        f.garrison -= men;
        let mut a = Army::new(id, side, &name, men, f.pos);
        a.morale = f.garrison_morale;
        a.food = f.take(men * 3) as u64 * MILLI;
        let text = format!("{name} musters at {} with {men} men.", f.name);
        self.armies.push(a);
        self.note(Some(side), text);
        Ok(())
    }

    /// Advances the game by one minute.
    pub fn tick(&mut self) {
        self.clock.minute += 1;
        let m = self.clock.minute_of_day();
        self.behave();
        self.hunt_convoys();
        self.move_armies();
        self.run_convoys();
        self.raid_convoys();
        self.fight();
        self.reveal_contacts();
        if self.clock.minute.is_multiple_of(intel::DETECT_EVERY) {
            self.detect();
        }
        self.recover_stamina();
        self.deploy();
        self.update_captures();
        if m.is_multiple_of(60) {
            self.eat_and_resupply();
            self.fade_tracks();
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
        let mut laid = Vec::new();
        let mut occupied: Vec<(Side, Cell)> =
            self.armies.iter().flat_map(|a| a.column(&self.map).into_iter().map(move |(c, _)| (a.side, c))).collect();
        let barred = [self.barred_cells(Side::West), self.barred_cells(Side::East)];
        for a in self.armies.iter_mut() {
            if !matches!(a.state, ArmyState::Marching | ArmyState::Routing) {
                continue;
            }
            let Some(&next) = a.path.front() else {
                if a.state == ArmyState::Marching {
                    a.state = ArmyState::Holding;
                }
                continue;
            };
            // Armies can't walk through each other; they have to fight their way past.
            if occupied.iter().any(|&(s, c)| s != a.side && c == next) {
                continue;
            }
            // A garrisoned fort's gates are shut to its enemies.
            let shut = barred[a.side.idx()].contains(&next) && !self.forts.iter().any(|f| f.contains(a.pos));
            let cost = path::enter_cost(&self.map, next, a.men, weather)
                .filter(|_| !self.map.crosses_wall(a.pos, next) && !shut);
            let Some(cost) = cost else {
                a.path.clear();
                a.state = ArmyState::Holding;
                push_private(&mut self.log, minute, Some(a.side), format!("{} finds the way impassable.", a.name));
                continue;
            };
            // Exhaustion halves speed, and so does pulling out of a fight.
            let mut pace = if a.routing() { Pace::Normal.speed() } else { a.pace.speed() };
            if a.exhausted() {
                pace /= 2;
            }
            if self.engaged.contains(&a.id) && a.state != ArmyState::Routing {
                pace /= 2;
            }
            a.progress += pace;
            if a.progress < cost * 10 {
                continue;
            }
            a.progress -= cost * 10;
            a.path.pop_front();
            a.step_to(next);
            occupied.push((a.side, next));
            if !self.map.is_road(next.0, next.1) {
                laid.push((a.side, next));
            }
            let strain = path::stamina_cost(&self.map, next, a.men, weather);
            // Routing men run flat out whatever their orders.
            let strain = if a.routing() { strain } else { a.pace.strain(strain) };
            a.stamina = a.stamina.saturating_sub(strain);
            if a.path.is_empty() && a.state == ArmyState::Marching {
                a.state = ArmyState::Holding;
                a.progress = 0;
                let text = match self.forts.iter().find(|f| f.contains(a.pos)) {
                    Some(f) => format!("{} arrives at {}.", a.name, f.name),
                    None => format!("{} halts.", a.name),
                };
                push_private(&mut self.log, minute, Some(a.side), text);
            }
        }
        for (side, cell) in laid {
            self.lay_track(side, cell);
        }
    }

    fn recover_stamina(&mut self) {
        for a in &mut self.armies {
            let in_own_fort = self.forts.iter().any(|f| f.owner == Some(a.side) && f.contains(a.pos));
            let gain = match a.state {
                ArmyState::Resting if in_own_fort => 4,
                ArmyState::Resting => 2,
                ArmyState::Holding => 1,
                ArmyState::Marching | ArmyState::Routing => 0,
            };
            if !a.starving {
                a.stamina = (a.stamina + gain).min(STAMINA_MAX);
            }
            if matches!(a.state, ArmyState::Holding | ArmyState::Resting) {
                a.entrench = (a.entrench + combat::ENTRENCH_PER_MINUTE).min(ENTRENCH_MAX);
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
                    push_private(&mut self.log, minute, Some(a.side), format!("{} has run out of food!", a.name));
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
            for a in self.armies.iter().filter(|a| a.column(&self.map).iter().any(|&(c, _)| f.contains(c))) {
                present[a.side.idx()] = true;
            }
            let claimant = match present {
                [true, false] => Some(Side::West),
                [false, true] => Some(Side::East),
                _ => None,
            };
            // A fort can only be taken once its garrison is gone.
            match claimant {
                Some(s) if f.owner != Some(s) && f.garrison == 0 => {
                    let held = match f.capture {
                        Some((cs, n)) if cs == s => n + 1,
                        _ => 1,
                    };
                    if held >= CAPTURE_MINUTES {
                        f.owner = Some(s);
                        f.capture = None;
                        push_log(&mut self.log, minute, Some(s), format!("{} has been taken.", f.name));
                        if f.kind == FortKind::Main && self.winner.is_none() {
                            self.winner = Some(s);
                            let text = format!("{} has fallen. The war is over.", f.name);
                            push_log(&mut self.log, minute, Some(s), text);
                        }
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
            // Held towns send recruits; neutral ones keep to themselves.
            if f.owner.is_some() {
                let recruits = match f.kind {
                    FortKind::Main => 120,
                    FortKind::Village => 40,
                    _ => 15,
                };
                f.garrison = (f.garrison + recruits).min(MAX_GARRISON);
            }
        }
    }

    fn update_weather(&mut self) {
        use Weather::*;
        let hour = self.clock.minute_of_day() / 60;
        let misty_hours = !(9..21).contains(&hour);
        let roll = self.rng.range(0, 100);
        let next = match self.weather {
            // Tuned so rain falls about 7% of the time, a spell every three days or so.
            Clear if roll < 72 => Clear,
            Clear if roll < 90 || !misty_hours => Cloudy,
            Clear => Fog,
            Cloudy if roll < 50 => Cloudy,
            Cloudy if roll < 88 => Clear,
            Cloudy => Rain,
            Rain if roll < 45 => Rain,
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
            for v in [a.id as u64, a.men as u64, a.stamina as u64, a.food, a.pos.0 as u64, a.pos.1 as u64] {
                mix(v);
            }
            for v in [
                a.progress as u64,
                a.morale as u64,
                a.entrench as u64,
                a.state as u64,
                a.pace as u64,
                a.line.len() as u64,
            ] {
                mix(v);
            }
        }
        for c in &self.convoys {
            for v in [c.id as u64, c.pos.0 as u64, c.pos.1 as u64, c.progress as u64, c.load() as u64, c.to as u64] {
                mix(v);
            }
        }
        for f in &self.forts {
            mix(f.owner.map_or(9, |s| s as u64));
            mix(f.garrison as u64);
            mix(f.garrison_morale as u64);
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
    fn walls_are_passed_only_at_gates() {
        for seed in 0..20 {
            let mut sim = Sim::new(seed);
            for f in &mut sim.forts {
                f.garrison = 0; // open every gate: this is about the walls
            }
            let host = sim.army(1).unwrap();
            let mut walked = 0;
            for f in &sim.forts {
                let (fx, fy) = f.pos;
                let gates = (fx - 2..=fx + 2).flat_map(|x| (fy - 1..=fy + 1).map(move |y| (x, y)));
                assert!(
                    gates.filter(|&(x, y)| sim.map.is_gate(x, y)).count() > 0,
                    "seed {seed}: {} has no gate",
                    f.name
                );
                for start in [(fx, fy - 2), (fx, fy + 2), (fx - 3, fy), (fx + 3, fy)] {
                    if !sim.map.in_bounds(start.0, start.1) {
                        continue;
                    }
                    let Some(route) = sim.plan_path_from(host, start, f.pos) else { continue };
                    walked += 1;
                    let mut at = start;
                    for &next in &route {
                        assert!(!sim.map.crosses_wall(at, next), "seed {seed}: climbed {}'s wall at {next:?}", f.name);
                        at = next;
                    }
                }
            }
            assert!(walked >= 2 * sim.forts.len(), "seed {seed}: only {walked} routes into forts");
        }
    }

    #[test]
    fn a_line_in_a_fort_stays_behind_its_walls() {
        let mut sim = Sim::new(SEED);
        sim.armies.retain(|a| a.id == 1);
        sim.armies[0].pos = (sim.forts[1].pos.0 + 1, sim.forts[1].pos.1 - 1);
        sim.apply(Side::West, Order::Hold { army: 1 }).unwrap();
        for _ in 0..combat::DEPLOY_MINUTES {
            sim.tick();
        }
        let line = sim.armies[0].column(&sim.map);
        assert_eq!(line.len(), 5, "1800 men still need 5 cells of line");
        assert!(line.iter().all(|&(c, _)| sim.forts[1].contains(c)), "line spilled outside the walls: {line:?}");
    }

    fn open_forest(sim: &Sim) -> Cell {
        (0..MAP_W)
            .flat_map(|x| (0..MAP_H).map(move |y| (x, y)))
            .find(|&(x, y)| {
                sim.map.terrain(x, y) == Terrain::Forest
                    && !sim.map.is_road(x, y)
                    && sim.fort_at((x, y)).is_none()
                    && (-1..=1).all(|dx| sim.map.terrain(x + dx, y) == Terrain::Forest && !sim.map.is_road(x + dx, y))
            })
            .unwrap()
    }

    #[test]
    fn big_armies_crawl_through_forest() {
        let sim = Sim::new(SEED);
        let forest = open_forest(&sim);
        let small = path::enter_cost(&sim.map, forest, path::FOREST_CAP, Weather::Clear).unwrap();
        let big = path::enter_cost(&sim.map, forest, 1800, Weather::Clear).unwrap();
        assert!(big >= small * 2, "1800 men should crawl: {big} vs {small} minutes a cell");
        assert!(
            path::stamina_cost(&sim.map, forest, 1800, Weather::Clear)
                > path::stamina_cost(&sim.map, forest, path::FOREST_CAP, Weather::Clear),
            "and tire faster"
        );
    }

    #[test]
    fn woods_hold_only_a_few_companies_a_cell() {
        let mut sim = Sim::new(SEED);
        sim.armies.retain(|a| a.id == 1);
        sim.armies[0].pos = open_forest(&sim);
        sim.apply(Side::West, Order::Hold { army: 1 }).unwrap();
        for _ in 0..combat::DEPLOY_MINUTES {
            sim.tick();
        }
        let line = sim.armies[0].column(&sim.map);
        for &(c, n) in &line {
            assert!(n <= army::room(&sim.map, c), "{n} companies crammed into {c:?}");
        }
        let cramped = line.iter().filter(|&&(c, _)| army::room(&sim.map, c) < army::COMPANIES_PER_CELL).count();
        assert!(cramped > 0 && line.len() > 5, "a line in the woods spreads wider: {line:?}");
    }

    #[test]
    fn an_easy_pace_spares_stamina_and_a_forced_march_spends_it() {
        let run = |pace| {
            let mut sim = Sim::new(SEED);
            sim.armies.retain(|a| a.id == 2);
            sim.apply(Side::West, Order::SetPace { army: 2, pace }).unwrap();
            sim.apply(Side::West, Order::March { army: 2, to: sim.forts[1].pos }).unwrap();
            let mut minutes = 0;
            while sim.armies[0].state == ArmyState::Marching && minutes < 4000 {
                sim.tick();
                minutes += 1;
            }
            (minutes, sim.armies[0].stamina)
        };
        let (slow, fresh) = run(Pace::Slow);
        let (steady, _) = run(Pace::Normal);
        let (fast, tired) = run(Pace::Fast);
        assert!(slow > steady && steady > fast, "{slow} / {steady} / {fast} minutes");
        assert!(fresh > tired, "easy pace should arrive fresher: {fresh} vs {tired}");
    }

    #[test]
    fn marching_at_a_mountain_stops_at_its_foot() {
        let sim = Sim::new(SEED);
        let host = sim.army(1).unwrap();
        let peak = (host.pos.0 + 20, 0);
        assert_eq!(sim.map.terrain(peak.0, peak.1), Terrain::Mountain);
        assert!(sim.plan_path(host, peak).is_none());
        let route = sim.plan_march(host, peak).expect("it should get as close as it can");
        let end = *route.last().unwrap();
        assert_ne!(sim.map.terrain(end.0, end.1), Terrain::Mountain);
        assert!(end.1 <= 4, "should end up at the foot of the ridge, not {end:?}");
    }

    #[test]
    fn every_valley_has_all_four_countries() {
        for seed in 0..20 {
            let sim = Sim::new(seed);
            let regions = &sim.map.regions;
            assert_eq!(regions.len(), sim.forts.len() - 1);
            for country in
                [map::Country::Woodland, map::Country::Narrows, map::Country::Highlands, map::Country::Farmland]
            {
                assert!(regions.iter().any(|r| r.country == country), "seed {seed}: no {country:?}");
            }
            for r in regions {
                assert_eq!(regions.iter().filter(|q| q.name == r.name).count(), 1, "seed {seed}: two {}", r.name);
                let cells = (r.x0..=r.x1).flat_map(|x| (3..10).map(move |y| (x, y)));
                let count = |t| cells.clone().filter(|&(x, y)| sim.map.terrain(x, y) == t).count();
                let (wood, hill, rock) = (count(Terrain::Forest), count(Terrain::Hill), count(Terrain::Mountain));
                match r.country {
                    map::Country::Woodland => assert!(wood > 80, "seed {seed}: {} is thin ({wood})", r.name),
                    map::Country::Narrows => {
                        // Somewhere the passable ground north of the river shrinks to a defile.
                        let open = |x| {
                            (0..10)
                                .filter(|&y| !matches!(sim.map.terrain(x, y), Terrain::Mountain | Terrain::River))
                                .count()
                        };
                        let narrowest = (r.x0..=r.x1).map(open).min().unwrap();
                        assert!(rock > 15 && narrowest <= 5, "seed {seed}: {} is wide open ({narrowest} rows)", r.name);
                    }
                    map::Country::Highlands => assert!(hill > 25, "seed {seed}: {} is flat ({hill})", r.name),
                    map::Country::Farmland => assert!(wood < 60, "seed {seed}: {} is overgrown ({wood})", r.name),
                }
            }
        }
    }

    #[test]
    fn holding_an_empty_fort_for_an_hour_takes_it() {
        let mut sim = Sim::new(SEED);
        sim.forts[1].garrison = 0;
        sim.armies[1].pos = sim.forts[1].pos;
        for _ in 0..CAPTURE_MINUTES {
            sim.tick();
        }
        assert_eq!(sim.forts[1].owner, Some(Side::West));
    }

    #[test]
    fn a_garrison_must_be_beaten_before_the_fort_falls() {
        let mut sim = Sim::new(SEED);
        sim.armies[0].pos = sim.forts[1].pos; // 1800 men against 190 militia
        for _ in 0..CAPTURE_MINUTES {
            sim.tick();
        }
        assert_eq!(sim.forts[1].owner, None, "fell while the garrison still stood");
        for _ in 0..12 * 60 {
            sim.tick();
        }
        assert_eq!(sim.forts[1].owner, Some(Side::West), "the militia should have broken by now");
    }

    #[test]
    fn a_garrison_shuts_its_gates() {
        let mut sim = Sim::new(SEED);
        let f = sim.forts[1].clone();
        let host = sim.army(1).unwrap();
        let route = sim.plan_march(host, f.pos).expect("should reach the gates");
        let end = *route.last().unwrap();
        assert!(!f.contains(end), "marched into a garrisoned fort");
        assert!(sim.assault_cells(&f).contains(&end), "stopped at {end:?}, not before a gate");
        assert!(route.iter().all(|&c| !f.contains(c)), "walked through a shut gate");
        sim.forts[1].garrison = 0;
        let host = sim.army(1).unwrap();
        let route = sim.plan_march(host, f.pos).unwrap();
        assert_eq!(route.last(), Some(&f.pos), "an empty fort's gates are open");
    }

    #[test]
    fn walls_are_stormed_only_at_their_gates() {
        let mut sim = Sim::new(SEED);
        sim.armies.retain(|a| a.id == 2);
        let f = sim.forts[1].clone();
        let gates = sim.assault_cells(&f);
        // Up against the wall but away from any gate: shot at, can't strike back.
        let wall = (-3..=3)
            .flat_map(|dx| [(f.pos.0 + dx, f.pos.1 - 2), (f.pos.0 + dx, f.pos.1 + 2)])
            .find(|&c| !gates.contains(&c) && path::enter_cost(&sim.map, c, 640, Weather::Clear).is_some())
            .expect("open ground along the wall");
        sim.armies[0].pos = wall;
        sim.armies[0].state = ArmyState::Resting;
        for _ in 0..120 {
            sim.tick();
        }
        assert_eq!(sim.forts[1].garrison, 190, "struck the garrison over its wall");
        assert!(sim.armies[0].men < 640, "the garrison should shoot at men under its walls");
    }

    /// Puts two armies face to face in open ground, away from forts.
    fn duel(west_men: u32, east_men: u32) -> Sim {
        let mut sim = Sim::new(SEED);
        let road = sim.map.road_cell(0, 120);
        let (x, y) = road;
        sim.armies.retain(|a| a.id == 2 || a.id == 5);
        sim.armies[0].men = west_men;
        sim.armies[0].pos = (x - 1, y);
        sim.armies[1].men = east_men;
        sim.armies[1].pos = (x, y);
        sim.armies[1].stance = Stance::Aggressive;
        sim
    }

    #[test]
    fn battles_end_in_a_rout() {
        let mut sim = duel(400, 400);
        let mut routed = None;
        for minute in 0..8 * 60 {
            sim.tick();
            if let Some(a) = sim.armies.iter().find(|a| a.routing()) {
                routed = Some((a.id, a.men, minute));
                break;
            }
        }
        let (_, men, minute) = routed.expect("someone should break within 8 hours");
        assert!(men > 200, "a rout should come well before annihilation, {men} left");
        assert!(minute > 30, "battles shouldn't be instant ({minute} min)");
    }

    #[test]
    fn the_bigger_army_usually_wins() {
        let mut west_wins = 0;
        for seed in 0..10 {
            let mut sim = duel(800, 300);
            sim.rng = Rng::new(seed);
            for _ in 0..8 * 60 {
                sim.tick();
                if sim.armies.iter().any(|a| a.routing()) {
                    break;
                }
            }
            if sim.armies.iter().any(|a| a.side == Side::East && a.routing()) {
                west_wins += 1;
            }
        }
        assert!(west_wins >= 8, "800 beat 300 only {west_wins}/10 times");
    }

    /// Battle pacing numbers for tuning: `cargo test -p bivouac-sim battle_report -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn battle_report() {
        for (w, e) in [(400, 400), (640, 400), (800, 300), (1800, 2000)] {
            for seed in 0..3 {
                let mut sim = duel(w, e);
                sim.rng = Rng::new(seed);
                let mut result = "no result in 12h".to_string();
                for minute in 0..12 * 60 {
                    sim.tick();
                    if let Some(a) = sim.armies.iter().find(|a| a.routing()) {
                        let men: Vec<u32> = sim.armies.iter().map(|a| a.men).collect();
                        result = format!("{} routs after {}h{:02}, men {:?}", a.name, minute / 60, minute % 60, men);
                        break;
                    }
                }
                println!("{w} vs {e} (seed {seed}): {result}");
            }
        }
        let mut sim = Sim::new(SEED);
        sim.armies[0].pos = sim.forts[1].pos;
        for minute in 0..24 * 60 {
            sim.tick();
            if sim.forts[1].garrison == 0 {
                println!(
                    "1st Army (1800) breaks Ashford's 190 militia in {}h{:02}, {} men left",
                    minute / 60,
                    minute % 60,
                    sim.armies[0].men
                );
                break;
            }
        }
    }

    /// Every West man thrown at an undefended Cinnabar Citadel:
    /// `cargo test -p bivouac-sim siege_report -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn siege_report() {
        for (men, garrison) in [(2440, 800), (1800, 800), (2440, 1200), (640, 190)] {
            for seed in 0..3 {
                let mut sim = Sim::new(seed);
                sim.armies.retain(|a| a.id == 1);
                sim.armies[0].men = men;
                sim.armies[0].pos = sim.map.road_cell(0, 215);
                sim.forts[5].garrison = garrison;
                sim.apply(Side::West, Order::March { army: 1, to: sim.forts[5].pos }).unwrap();
                let mut broke = None;
                let mut start = 0;
                for minute in 0..48 * 60 {
                    sim.tick();
                    if start == 0 && sim.in_battle(1) {
                        start = minute;
                    }
                    if sim.armies.is_empty() || sim.armies[0].routing() {
                        println!("{men} vs {garrison} (seed {seed}): the stormers break after {}h", minute / 60);
                        break;
                    }
                    if broke.is_none() && sim.forts[5].garrison == 0 {
                        broke = Some(minute);
                        let pos = sim.forts[5].pos;
                        sim.apply(Side::West, Order::March { army: 1, to: pos }).unwrap();
                    }
                    if sim.winner.is_some() {
                        let (b, minute) = (broke.unwrap() - start, minute - start);
                        println!(
                            "{men} vs {garrison} (seed {seed}): garrison breaks {}h{:02}, citadel falls {}h{:02}, {} men left",
                            b / 60,
                            b % 60,
                            minute / 60,
                            minute % 60,
                            sim.armies[0].men
                        );
                        break;
                    }
                }
                if sim.winner.is_none() && !sim.armies.is_empty() && !sim.armies[0].routing() {
                    println!(
                        "{men} vs {garrison} (seed {seed}): still standing after 48h, garrison {}",
                        sim.forts[5].garrison
                    );
                }
            }
        }
    }

    #[test]
    fn armies_ordered_to_hold_deploy_into_line() {
        let mut sim = Sim::new(SEED);
        sim.armies.retain(|a| a.id == 1);
        sim.armies[0].pos = sim.map.road_cell(0, 120);
        assert_eq!(sim.armies[0].column(&sim.map).len(), 1, "a fresh army is bunched in one cell");
        sim.apply(Side::West, Order::Hold { army: 1 }).unwrap();
        for _ in 0..combat::DEPLOY_MINUTES {
            sim.tick();
        }
        let line = sim.armies[0].column(&sim.map);
        // 36 companies make 5 cells of line in the open, more where it runs into woods.
        assert!(line.len() >= 5, "1800 men = 36 companies = at least 5 cells of line");
        assert_eq!(line.iter().map(|&(_, n)| n).sum::<u32>(), 36);
        sim.apply(Side::West, Order::March { army: 1, to: sim.forts[2].pos }).unwrap();
        for _ in 0..60 {
            sim.tick();
        }
        assert!(!sim.armies[0].deployed(), "marching re-forms the column");
    }

    #[test]
    fn marching_armies_string_out_along_the_road_and_stay_in_column() {
        let mut sim = Sim::new(SEED);
        sim.armies.retain(|a| a.id == 1);
        let start = sim.map.road_cell(0, 100);
        sim.armies[0].pos = start;
        sim.apply(Side::West, Order::March { army: 1, to: sim.map.road_cell(0, 130) }).unwrap();
        for _ in 0..24 * 60 {
            sim.tick();
        }
        let a = &sim.armies[0];
        assert_eq!(a.state, ArmyState::Holding);
        let column = a.column(&sim.map);
        assert_eq!(column.len(), 12, "36 companies, 3 to a cell");
        assert!(column.iter().all(|&((x, y), _)| sim.map.is_road(x, y)), "the whole column is on the road");
        assert!(!a.deployed(), "with no enemy about, a halted army stays in column");

        // Sent back the way it came, the column turns about rather than folding up.
        let back = sim.map.road_cell(0, 100);
        let tail = column.last().unwrap().0;
        sim.apply(Side::West, Order::March { army: 1, to: back }).unwrap();
        assert_eq!(sim.armies[0].pos, tail, "the tail becomes the head");
        assert_eq!(sim.armies[0].column(&sim.map).len(), 12);
    }

    #[test]
    fn waypoints_extend_the_march() {
        let mut sim = Sim::new(SEED);
        sim.armies.retain(|a| a.id == 1);
        sim.armies[0].pos = sim.map.road_cell(0, 100);
        let (mid, end) = (sim.map.road_cell(0, 110), sim.map.road_cell(0, 120));
        sim.apply(Side::West, Order::Waypoint { army: 1, to: mid }).unwrap();
        assert_eq!(sim.armies[0].path.back(), Some(&mid), "with no march underway a waypoint is a march");
        sim.apply(Side::West, Order::Waypoint { army: 1, to: end }).unwrap();
        let path = &sim.armies[0].path;
        assert!(path.contains(&mid) && path.back() == Some(&end));
    }

    #[test]
    fn big_armies_meeting_head_on_still_reach_a_decision() {
        let mut sim = duel(1800, 2000);
        let decided = (0..10 * 60).any(|_| {
            sim.tick();
            sim.armies.iter().any(|a| a.routing())
        });
        assert!(decided, "two big armies stalemated for 10 hours");
    }

    /// Fraction of detection rolls over `cycles` in which East spots army `id`.
    fn spotted_fraction(sim: &mut Sim, id: u32, cycles: u32) -> f32 {
        let mut seen = 0;
        for _ in 0..cycles {
            sim.detect();
            if sim.knows(Side::East, id).is_some_and(|s| s >= Seen::Sighted) {
                seen += 1;
            }
            // Forget, so every roll is a fresh search rather than keeping track.
            sim.intel[Side::East as usize].reports.clear();
        }
        seen as f32 / cycles as f32
    }

    /// Ash Wolves on open ground, Grey Sparrows `gap` cells west of them on `cell_kind`.
    fn lookout(forest: bool, gap: i32) -> Sim {
        let mut sim = Sim::new(SEED);
        sim.armies.retain(|a| a.id == 3 || a.id == 5);
        let spot = (0..MAP_W - gap)
            .flat_map(|x| (2..MAP_H - 1).map(move |y| (x, y)))
            .find(|&(x, y)| {
                let t = sim.map.terrain(x, y);
                let here = if forest { t == Terrain::Forest && !sim.map.is_road(x, y) } else { t == Terrain::Field };
                here && sim.map.terrain(x + gap, y) == Terrain::Field && sim.fort_at((x, y)).is_none()
            })
            .unwrap();
        sim.armies[0].pos = spot;
        sim.armies[1].pos = (spot.0 + gap, spot.1);
        sim
    }

    #[test]
    fn forest_hides_small_parties() {
        let in_trees = spotted_fraction(&mut lookout(true, 6), 3, 200);
        let in_open = spotted_fraction(&mut lookout(false, 6), 3, 200);
        assert!(in_trees < 0.25, "spotted in the forest {in_trees:.2} of the time");
        assert!(in_open > in_trees * 2.0, "open {in_open:.2} vs forest {in_trees:.2}");
    }

    #[test]
    fn big_armies_are_rumored_from_afar() {
        let mut sim = Sim::new(SEED);
        sim.clock.minute = 15 * 1440 + 12 * 60; // midday
        sim.weather = Weather::Clear;
        sim.armies[3].pos = sim.map.road_cell(0, 120);
        sim.armies[0].pos = sim.map.road_cell(0, 80); // 40 cells away
        sim.armies[3].state = ArmyState::Marching;
        sim.detect();
        assert!(sim.knows(Side::West, 4).is_some(), "a marching host of 2000 should raise dust 40 cells off");
        assert_eq!(sim.knows(Side::West, 4), Some(Seen::Rumored), "but that far it's only a rumor");
    }

    #[test]
    fn striking_unseen_is_an_ambush() {
        let mut sim = lookout(true, 1);
        sim.armies[0].stance = Stance::Aggressive;
        let wolves = sim.armies[1].clone();
        sim.intel[Side::West as usize].reports =
            vec![Report { army: 5, seen: Seen::Sighted, cell: wolves.pos, men: wolves.men, minute: 0 }];
        sim.intel[Side::East as usize].reports.clear();
        let before = sim.armies[1].morale;
        sim.tick();
        assert!(sim.log.iter().any(|e| e.text.starts_with("Ambush!")));
        assert!(sim.armies[1].morale + intel::AMBUSH_SHOCK / 2 <= before);
    }

    #[test]
    fn own_movements_stay_out_of_the_enemy_log() {
        let mut sim = Sim::new(SEED);
        sim.apply(Side::West, Order::Divide { army: 1, men: None }).unwrap();
        let entry = &sim.log[0];
        assert!(entry.visible_to(Side::West));
        assert!(!entry.visible_to(Side::East));
    }

    #[test]
    fn mustering_raises_an_army_from_the_garrison() {
        let mut sim = Sim::new(SEED);
        let before = sim.forts[0].garrison;
        sim.apply(Side::West, Order::Muster { fort: 0, men: 200 }).unwrap();
        assert_eq!(sim.forts[0].garrison, before - 200);
        let raised = sim.armies.last().unwrap();
        assert_eq!((raised.name.as_str(), raised.men, raised.side), ("Third Banner", 200, Side::West));
        assert!(sim.apply(Side::West, Order::Muster { fort: 5, men: 200 }).is_err(), "not our fort");
    }

    #[test]
    fn every_seed_makes_a_playable_valley() {
        for seed in 0..40 {
            let mut sim = Sim::new(seed);
            for f in &mut sim.forts {
                f.garrison = 0; // about the ground, not the gates
            }
            let host = sim.army(4).unwrap();
            for f in &sim.forts {
                assert!(sim.plan_path(host, f.pos).is_some(), "seed {seed}: the host can't reach {}", f.name);
            }
            for a in &sim.armies {
                assert!(
                    path::enter_cost(&sim.map, a.pos, a.men, Weather::Clear).is_some(),
                    "seed {seed}: {} starts stuck",
                    a.name
                );
            }
        }
    }

    #[test]
    fn routed_armies_ignore_orders() {
        let mut sim = duel(400, 400);
        sim.armies[0].morale = 0;
        sim.tick();
        assert!(sim.armies[0].routing());
        assert!(sim.apply(Side::West, Order::Hold { army: 2 }).is_err());
    }

    #[test]
    fn divide_then_join_round_trips() {
        let mut sim = Sim::new(SEED);
        let before = sim.army(1).unwrap().men;
        sim.apply(Side::West, Order::Divide { army: 1, men: None }).unwrap();
        assert_eq!(sim.armies.len(), 6);
        sim.apply(Side::West, Order::Join { army: 1 }).unwrap();
        assert_eq!(sim.armies.len(), 5);
        assert_eq!(sim.army(1).unwrap().men, before);
    }
}
