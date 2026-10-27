//! Combat. Armies fight where their columns touch, and only the cells in contact
//! trade blows, so a column caught on the road fights with its head while a line
//! deployed across the valley fights with everything. Battles end when one side's
//! morale breaks and it routs.

use crate::army::{
    ArmyState, ENTRENCH_MAX, MARCH_COMPANIES_PER_CELL, MEN_PER_COMPANY, MORALE_MAX, STAMINA_MAX, Stance, room,
};
use crate::fort::Extra;
use crate::intel::{AMBUSH_SHOCK, Seen};
use crate::map::{MAP_H, MAP_W, Terrain};
use crate::{Cell, Sim, Weather, push_log, push_private};

/// An army whose morale falls below this breaks and runs.
pub const ROUT_MORALE: u32 = MORALE_MAX / 5;
/// Morale an army recovers to on its own, and starts with.
pub const BASE_MORALE: u32 = MORALE_MAX * 8 / 10;
const RALLY_MORALE: u32 = MORALE_MAX * 35 / 100;
const ROUT_MINUTES: u32 = 120;
/// Armies (and garrisons) smaller than this scatter.
const MIN_MEN: u32 = 10;
/// Casualties one company in contact inflicts per minute, in thousandths of a man,
/// before modifiers. Tuned so an even fight breaks someone in roughly 2–3 game hours.
const HITS_PER_COMPANY: u32 = 60;
/// Losing 1% of the army costs this many percent of morale.
const MORALE_PER_LOSS: u32 = 3;
/// Extra morale lost per minute for each extra direction an army is attacked from.
const FLANK_MORALE: u32 = 150;
const MORALE_RECOVERY: u32 = 60;
/// Most companies a garrison can bring to bear at once.
const GARRISON_FRONT: u32 = 16;
pub(crate) const AGGRO_RANGE: i32 = 4;
const EVADE_RANGE: i32 = 4;
const PURSUIT_RANGE: i32 = 6;
pub(crate) const RETARGET_EVERY: u32 = 10;
/// A fight between the same two armies within this many minutes isn't announced again.
const ANNOUNCE_AGAIN_AFTER: u32 = 120;
pub const ENTRENCH_PER_MINUTE: u16 = 3;
/// Minutes an army must stand still before it shakes out from column into line.
pub const DEPLOY_MINUTES: u16 = 20;
/// A halted army deploys when it knows of an enemy this close.
const DEPLOY_ALERT: i32 = 10;
/// Ranks a line may stack behind itself when the valley is too narrow.
const MAX_RANKS: i32 = 3;

/// Two cells trading blows this minute, for drawing the fighting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clash {
    pub a: Cell,
    pub b: Cell,
}

const AROUND: [(i32, i32); 9] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

fn distance(a: Cell, b: Cell) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

fn pair(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

/// Damage modifiers that apply to every blow this minute.
fn conditions(sim: &Sim, d: u32) -> u32 {
    let d = if sim.clock.is_night() { d * 80 / 100 } else { d };
    if sim.weather == Weather::Rain { d * 90 / 100 } else { d }
}

impl Sim {
    fn index_of(&self, id: u32) -> Option<usize> {
        self.armies.iter().position(|a| a.id == id)
    }

    /// Whether any part of army `i` is next to any part of army `j`, with no wall between.
    fn touching(&self, i: usize, j: usize) -> bool {
        let theirs = self.armies[j].column(&self.map);
        self.armies[i]
            .column(&self.map)
            .iter()
            .any(|&(c, _)| theirs.iter().any(|&(d, _)| distance(c, d) <= 1 && self.blows_pass(c, d)))
    }

    /// A free cell next to army `j`'s head, as close to army `i` as possible. If
    /// `j` is behind gates shut to `i`, the ground before the gates.
    fn approach(&self, i: usize, j: usize) -> Option<Vec<Cell>> {
        let (me, them) = (&self.armies[i], &self.armies[j]);
        if self.fort_at(them.pos).is_some_and(|f| f.bars(me.side) && !f.contains(me.pos)) {
            return self.plan_march(me, them.pos);
        }
        let blocked = self.enemy_cells(me.side);
        let mut goals: Vec<Cell> = AROUND
            .iter()
            .map(|&(dx, dy)| (them.pos.0 + dx, them.pos.1 + dy))
            .filter(|&c| {
                !blocked.contains(&c)
                    && crate::path::enter_cost(&self.map, c, me.men, self.weather).is_some()
                    && self.blows_pass(c, them.pos)
            })
            .collect();
        goals.sort_by_key(|&c| distance(c, me.pos));
        goals.into_iter().find_map(|g| self.plan_path(me, g))
    }

    /// A route away from danger: the nearest friendly fort, else back toward home.
    pub(crate) fn retreat_path(&self, i: usize) -> Option<Vec<Cell>> {
        let a = &self.armies[i];
        let fort = self
            .forts
            .iter()
            .filter(|f| f.owner == Some(a.side) && f.walled() && !f.contains(a.pos))
            .min_by_key(|f| (f.pos.0 - a.pos.0).abs());
        if let Some(path) = fort.and_then(|f| self.plan_path(a, f.pos)) {
            return Some(path);
        }
        let home = if a.side == crate::Side::West { -1 } else { 1 };
        let x = (a.pos.0 + 12 * home).clamp(0, MAP_W - 1);
        (0..MAP_H).map(|dy| (x, (a.pos.1 + dy) % MAP_H)).find_map(|c| self.plan_path(a, c))
    }

    pub fn place_name(&self, c: Cell) -> String {
        match self.forts.iter().min_by_key(|f| distance(f.pos, c)) {
            Some(f) if distance(f.pos, c) <= 12 => format!("near {}", f.name),
            _ => match self.map.region_at(c.0) {
                Some(r) => format!("in {}", r.name),
                None => "in open country".into(),
            },
        }
    }

    /// Cells for army `i`'s line: across the valley's rows, center first, with extra
    /// ranks behind (toward home) if the ground runs out.
    fn line_cells(&self, i: usize) -> Vec<Cell> {
        let a = &self.armies[i];
        let companies = a.companies();
        let blocked = self.enemy_cells(a.side);
        // A line stays on its own side of any wall.
        let fort = self.walls_at(a.pos).map(|f| f.id);
        let ok = |c: Cell| {
            !blocked.contains(&c)
                && crate::path::enter_cost(&self.map, c, a.men, self.weather).is_some()
                && self.walls_at(c).map(|f| f.id) == fort
        };
        // Enough cells to hold everyone; woods hold fewer than open ground.
        let full = |cells: &[Cell]| cells.iter().map(|&c| room(&self.map, c)).sum::<u32>() >= companies;
        let fill = |candidates: Vec<Cell>| {
            let mut cells = Vec::new();
            for c in candidates {
                if full(&cells) {
                    break;
                }
                cells.push(c);
            }
            cells
        };
        if let Some(f) = self.walls_at(a.pos) {
            // Behind walls, men spread out to man them, nearest first.
            let mut cells: Vec<Cell> = (-1..=1)
                .flat_map(|dy| (-2..=2).map(move |dx| (f.pos.0 + dx, f.pos.1 + dy)))
                .filter(|&c| ok(c))
                .collect();
            cells.sort_by_key(|&c| (c != a.pos, distance(c, a.pos), (c.1 - a.pos.1).abs(), c.0, c.1));
            return fill(cells);
        }
        let rear = if a.side == crate::Side::West { -1 } else { 1 };
        let mut cells = vec![a.pos];
        for rank in 0..MAX_RANKS {
            let x = a.pos.0 + rank * rear;
            if rank > 0 && ok((x, a.pos.1)) {
                cells.push((x, a.pos.1));
            }
            let (mut up, mut down) = (a.pos.1, a.pos.1);
            while !full(&cells) {
                let mut grew = false;
                if ok((x, up - 1)) {
                    up -= 1;
                    cells.push((x, up));
                    grew = true;
                }
                if !full(&cells) && ok((x, down + 1)) {
                    down += 1;
                    cells.push((x, down));
                    grew = true;
                }
                if !grew {
                    break;
                }
            }
            if full(&cells) {
                break;
            }
        }
        fill(cells)
    }

    /// Armies that have stood still long enough deploy into line, if there's reason
    /// to: a known enemy nearby, a fight, walls to man, or orders. Otherwise they
    /// stay in column on the road, ready to move.
    pub(crate) fn deploy(&mut self) {
        for i in 0..self.armies.len() {
            let a = &mut self.armies[i];
            if !matches!(a.state, ArmyState::Holding | ArmyState::Resting) {
                continue;
            }
            a.settled = a.settled.saturating_add(1);
            let a = &self.armies[i];
            if a.settled < DEPLOY_MINUTES || a.deployed() || a.companies() <= MARCH_COMPANIES_PER_CELL {
                continue;
            }
            let threatened = self.armies.iter().any(|b| {
                b.side != a.side
                    && distance(a.pos, b.pos) <= DEPLOY_ALERT
                    && self.knows(a.side, b.id).is_some_and(|s| s >= Seen::Sighted)
            });
            let in_fort = self.walls_at(a.pos).is_some();
            if a.form_line || threatened || in_fort || self.engaged.contains(&a.id) {
                let line = self.line_cells(i);
                let a = &mut self.armies[i];
                a.line = line;
                // Whoever marches off from the line forms a fresh column at its head.
                a.trail.clear();
            }
        }
    }

    /// Stance behavior: aggressive armies pick fights, evasive ones slip away,
    /// and armies with a target close in on it.
    pub(crate) fn behave(&mut self) {
        let minute = self.clock.minute;
        for i in 0..self.armies.len() {
            if self.armies[i].routing() {
                continue;
            }
            if let Some(t) = self.armies[i].target {
                let side = self.armies[i].side;
                let keep = self.index_of(t).is_some_and(|j| {
                    (!self.armies[j].routing() || distance(self.armies[i].pos, self.armies[j].pos) <= PURSUIT_RANGE)
                        && self.knows(side, t).is_some_and(|s| s >= Seen::Sighted)
                });
                if !keep {
                    self.armies[i].target = None;
                }
            }

            let a = &self.armies[i];
            match a.stance {
                Stance::Aggressive if a.target.is_none() => {
                    let prey = self
                        .armies
                        .iter()
                        .filter(|b| b.side != a.side && !b.routing() && distance(a.pos, b.pos) <= AGGRO_RANGE)
                        .filter(|b| self.knows(a.side, b.id).is_some_and(|s| s >= Seen::Sighted))
                        .min_by_key(|b| (distance(a.pos, b.pos), b.id))
                        .map(|b| b.id);
                    self.armies[i].target = prey;
                }
                Stance::Evasive if a.state != ArmyState::Marching => {
                    let threatened = self.armies.iter().any(|b| {
                        b.side != a.side
                            && !b.routing()
                            && (b.stance == Stance::Aggressive || b.target == Some(a.id))
                            && distance(a.pos, b.pos) <= EVADE_RANGE
                            && self.knows(a.side, b.id).is_some_and(|s| s >= Seen::Rumored)
                    });
                    if let Some(path) = threatened.then(|| self.retreat_path(i)).flatten() {
                        let a = &mut self.armies[i];
                        a.path = path.into();
                        a.state = ArmyState::Marching;
                    }
                }
                _ => {}
            }

            let Some(j) = self.armies[i].target.and_then(|t| self.index_of(t)) else { continue };
            if self.touching(i, j) {
                let a = &mut self.armies[i];
                if a.state == ArmyState::Marching {
                    a.path.clear();
                    a.state = ArmyState::Holding;
                }
            } else if (self.armies[i].path.is_empty() || minute.is_multiple_of(RETARGET_EVERY))
                && let Some(path) = self.approach(i, j)
            {
                let a = &mut self.armies[i];
                a.path = path.into();
                a.state = if a.path.is_empty() { ArmyState::Holding } else { ArmyState::Marching };
            }
        }
    }

    /// One minute of fighting: find contacts, trade casualties, break morale, rout.
    pub(crate) fn fight(&mut self) {
        let minute = self.clock.minute;
        let n = self.armies.len();
        let columns: Vec<Vec<(Cell, u32)>> = self.armies.iter().map(|a| a.column(&self.map)).collect();
        let idx = |(x, y): Cell| (y * MAP_W + x) as usize;
        let mut grid: Vec<Vec<usize>> = vec![Vec::new(); (MAP_W * MAP_H) as usize];
        for (i, col) in columns.iter().enumerate() {
            for &(c, _) in col {
                grid[idx(c)].push(i);
            }
        }

        // Every (striker, its cell, struck, their cell) where enemies touch.
        let mut contacts: Vec<(usize, Cell, u32, usize, Cell)> = Vec::new();
        for (i, col) in columns.iter().enumerate() {
            for &(c, companies) in col {
                for (dx, dy) in AROUND {
                    let nb = (c.0 + dx, c.1 + dy);
                    if !self.map.in_bounds(nb.0, nb.1) {
                        continue;
                    }
                    if !self.blows_pass(c, nb) {
                        continue; // no fighting through a wall, only at its gates
                    }
                    for &j in &grid[idx(nb)] {
                        if self.armies[j].side != self.armies[i].side {
                            contacts.push((i, c, companies, j, nb));
                        }
                    }
                }
            }
        }

        // Which touching pairs actually fight: someone has to want it, and once
        // started a fight goes on until they separate or someone breaks.
        let wants = |a: &crate::Army, b: &crate::Army| {
            !a.routing() && (a.stance == Stance::Aggressive || a.target == Some(b.id))
        };
        let mut battles: Vec<(u32, u32)> = Vec::new();
        for &(i, _, _, j, _) in &contacts {
            let p = pair(self.armies[i].id, self.armies[j].id);
            if battles.contains(&p) {
                continue;
            }
            let (a, b) = (&self.armies[i], &self.armies[j]);
            // Only the willing chase a routed enemy; otherwise a rout ends the fight.
            let ongoing = self.battles.contains(&p) && !a.routing() && !b.routing();
            if ongoing || wants(a, b) || wants(b, a) {
                battles.push(p);
                let announced = self.announced.iter().any(|&(q, at)| q == p && minute - at < ANNOUNCE_AGAIN_AFTER);
                if !self.battles.contains(&p) && !announced {
                    self.announced.retain(|&(q, at)| q != p && minute - at < ANNOUNCE_AGAIN_AFTER);
                    self.announced.push((p, minute));
                    let (atk, def) = if wants(a, b) { (i, j) } else { (j, i) };
                    let text = format!(
                        "{} attacks {} {}.",
                        self.armies[atk].name,
                        self.armies[def].name,
                        self.place_name(self.armies[def].pos)
                    );
                    push_log(&mut self.log, minute, Some(self.armies[atk].side), text);
                    // Falling on an enemy that never saw you coming shakes them badly.
                    let (atk_id, def_side) = (self.armies[atk].id, self.armies[def].side);
                    if self.knows(def_side, atk_id).is_none_or(|s| s < Seen::Sighted) {
                        let d = &mut self.armies[def];
                        d.morale = d.morale.saturating_sub(AMBUSH_SHOCK);
                        let text = format!("Ambush! {} is caught unawares.", d.name);
                        push_log(&mut self.log, minute, Some(self.armies[atk].side), text);
                    }
                    // Both sides stop to fight, except the evasive, who keep slipping away.
                    for k in [i, j] {
                        let a = &mut self.armies[k];
                        if a.stance != Stance::Evasive && a.state == ArmyState::Marching {
                            a.path.clear();
                            a.state = ArmyState::Holding;
                        }
                    }
                }
            }
        }
        contacts.retain(|&(i, _, _, j, _)| battles.contains(&pair(self.armies[i].id, self.armies[j].id)));
        self.battles = battles;

        // Each cell's strength is split across the enemy cells it's touching.
        let mut fronts: Vec<(usize, Cell, u32)> = Vec::new();
        for &(i, c, _, _, _) in &contacts {
            match fronts.iter_mut().find(|f| f.0 == i && f.1 == c) {
                Some(f) => f.2 += 1,
                None => fronts.push((i, c, 1)),
            }
        }

        let mut hurt = vec![0u32; n];
        let mut directions: Vec<Vec<(i32, i32)>> = vec![Vec::new(); n];
        let mut engaged = vec![false; n];
        self.clashes.clear();
        for &(i, c, companies, j, nb) in &contacts {
            engaged[i] = true;
            engaged[j] = true;
            if !self.clashes.iter().any(|k| (k.a == nb && k.b == c) || (k.a == c && k.b == nb)) {
                self.clashes.push(Clash { a: c, b: nb });
            }
            let (a, b) = (&self.armies[i], &self.armies[j]);
            if a.routing() {
                continue;
            }
            let dir = ((c.0 - nb.0).signum(), (c.1 - nb.1).signum());
            if !directions[j].contains(&dir) {
                directions[j].push(dir);
            }
            let share = fronts.iter().find(|f| f.0 == i && f.1 == c).map_or(1, |f| f.2);
            let mut d = companies * HITS_PER_COMPANY / share;
            d = d * (60 + 40 * a.stamina as u32 / STAMINA_MAX as u32) / 100;
            if self.map.terrain(c.0, c.1) == Terrain::River {
                d = d * 60 / 100; // striking from a bridge: a cramped front
            }
            d = match self.map.terrain(nb.0, nb.1) {
                Terrain::Forest | Terrain::Mountain => d * 60 / 100, // defenders in the trees or the rocks
                Terrain::Hill => d * 80 / 100,                       // striking uphill
                _ => d,
            };
            // Fort walls are as good as the deepest earthworks.
            let cover = if self.map.terrain(nb.0, nb.1) == Terrain::Fort { ENTRENCH_MAX } else { b.entrench };
            d = d * (100 - 40 * cover as u32 / ENTRENCH_MAX as u32) / 100;
            if b.routing() {
                d *= 2; // cutting down men who've turned their backs
            }
            d = conditions(self, d) * self.rng.range(50, 151) as u32 / 100;
            hurt[j] += d;
        }

        // Garrisons fight anyone hostile who comes within reach of their walls.
        let mut garrison_hurt = vec![0u32; self.forts.len()];
        for (fi, f) in self.forts.iter().enumerate() {
            if f.garrison == 0 {
                continue;
            }
            let hostile: Vec<(usize, Cell, u32)> = columns
                .iter()
                .enumerate()
                .filter(|&(i, _)| Some(self.armies[i].side) != f.owner)
                .flat_map(|(i, col)| col.iter().filter(|(c, _)| f.within_reach(*c)).map(move |&(c, k)| (i, c, k)))
                .collect();
            let front = f.garrison.div_ceil(MEN_PER_COMPANY).min(GARRISON_FRONT);
            // The garrison shoots at anyone near its walls, but it can only be
            // struck back by men inside or storming a gate.
            let gates = self.assault_cells(f);
            for &(i, c, companies) in &hostile {
                engaged[i] = true;
                let wall = (c.0.clamp(f.pos.0 - 2, f.pos.0 + 2), c.1.clamp(f.pos.1 - 1, f.pos.1 + 1));
                self.clashes.push(Clash { a: c, b: wall });
                let a = &self.armies[i];
                let mut d = front * HITS_PER_COMPANY / hostile.len() as u32;
                if a.routing() {
                    d *= 2;
                }
                d = conditions(self, d) * self.rng.range(50, 151) as u32 / 100;
                hurt[i] += d;
                if !a.routing() && (f.contains(c) || gates.contains(&c)) {
                    // Garrisons are fully dug in.
                    let mut e = companies * HITS_PER_COMPANY * (60 + 40 * a.stamina as u32 / STAMINA_MAX as u32) / 100;
                    e = conditions(self, e * 60 / 100) * self.rng.range(50, 151) as u32 / 100;
                    garrison_hurt[fi] += e;
                }
                if !self.garrison_fights.contains(&(f.id, a.id)) {
                    self.garrison_fights.push((f.id, a.id));
                    let text = format!("The garrison of {} engages {}.", f.name, a.name);
                    push_private(&mut self.log, minute, f.owner, text);
                }
            }
        }
        let fighting: Vec<u32> = (0..n).filter(|&i| engaged[i]).map(|i| self.armies[i].id).collect();
        let manned: Vec<u32> = self.forts.iter().filter(|f| f.garrison > 0).map(|f| f.id).collect();
        self.garrison_fights.retain(|(fid, aid)| fighting.contains(aid) && manned.contains(fid));

        // Casualties and morale.
        for (j, a) in self.armies.iter_mut().enumerate() {
            if !engaged[j] {
                let gain = if a.state == ArmyState::Resting { MORALE_RECOVERY * 2 } else { MORALE_RECOVERY };
                // Hungry men don't take heart.
                if a.morale < BASE_MORALE && !a.starving {
                    a.morale = (a.morale + gain).min(BASE_MORALE);
                }
                continue;
            }
            let men = a.men.max(1);
            a.bleed += hurt[j];
            let lost = (a.bleed / 1000).min(a.men);
            a.bleed %= 1000;
            a.men -= lost;
            let mut shaken = hurt[j] * MORALE_PER_LOSS * (MORALE_MAX / 1000) / men;
            shaken += directions[j].len().saturating_sub(1) as u32 * FLANK_MORALE;
            if a.exhausted() {
                shaken += 30;
            }
            a.morale = a.morale.saturating_sub(shaken);
            a.stamina = a.stamina.saturating_sub(1);
        }
        self.engaged = fighting;

        for (fi, f) in self.forts.iter_mut().enumerate() {
            if f.garrison == 0 {
                continue;
            }
            if garrison_hurt[fi] == 0 {
                f.garrison_morale = (f.garrison_morale + MORALE_RECOVERY).min(BASE_MORALE);
                continue;
            }
            let men = f.garrison.max(1);
            f.garrison_bleed += garrison_hurt[fi];
            f.garrison -= (f.garrison_bleed / 1000).min(f.garrison);
            f.garrison_bleed %= 1000;
            let mut shaken = garrison_hurt[fi] * MORALE_PER_LOSS * (MORALE_MAX / 1000) / men;
            if f.extras.contains(&Extra::Palisade) {
                shaken /= 2; // men behind a palisade don't break easily
            }
            f.garrison_morale = f.garrison_morale.saturating_sub(shaken);
            if f.garrison_morale < ROUT_MORALE || f.garrison < MIN_MEN {
                f.garrison = 0;
                f.garrison_morale = BASE_MORALE;
                // Open gates are plain to everyone.
                let text = format!("The garrison of {} breaks and scatters! Its gates stand open.", f.name);
                push_log(&mut self.log, minute, f.owner, text);
            }
        }

        self.routs_and_rallies();
    }

    fn routs_and_rallies(&mut self) {
        let minute = self.clock.minute;
        for i in 0..self.armies.len() {
            let a = &self.armies[i];
            if a.routing() {
                let a = &mut self.armies[i];
                a.rout_left = a.rout_left.saturating_sub(1);
                if a.rout_left == 0 {
                    a.state = ArmyState::Holding;
                    a.path.clear();
                    a.morale = a.morale.max(RALLY_MORALE);
                    let text = format!("{} rallies with {} men.", a.name, a.men);
                    push_private(&mut self.log, minute, Some(a.side), text);
                } else if a.path.is_empty()
                    && minute.is_multiple_of(30)
                    && let Some(path) = self.retreat_path(i)
                {
                    self.armies[i].path = path.into();
                }
            } else if a.morale < ROUT_MORALE {
                let path = self.retreat_path(i);
                let a = &mut self.armies[i];
                a.state = ArmyState::Routing;
                a.line.clear();
                a.rout_left = ROUT_MINUTES;
                a.target = None;
                a.path = path.unwrap_or_default().into();
                a.progress = 0;
                let text = format!("{} breaks and flees! {} men remain.", a.name, a.men);
                push_log(&mut self.log, minute, Some(a.side), text);
            }
        }
        let mut gone = Vec::new();
        for a in self.armies.iter().filter(|a| a.men < MIN_MEN) {
            gone.push(a.id);
            push_private(&mut self.log, minute, Some(a.side), format!("{} is scattered to the winds.", a.name));
        }
        self.armies.retain(|a| !gone.contains(&a.id));
    }
}
