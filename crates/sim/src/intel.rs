//! Detection and fog of war. Each side keeps reports on enemy armies, refreshed every
//! few minutes. Big armies give themselves away at long range (dust by day, campfires
//! by night) but only as rumor; pinning one down takes line of sight and a dice roll.
//! Light, weather and forest cover all shift the odds, and armies leave tracks.

use crate::army::ArmyState;
use crate::clock::Weather;
use crate::map::{MAP_H, MAP_W, Terrain};
use crate::rng::hash2;
use crate::{Cell, Extra, Side, Sim, push_private};

pub const DETECT_EVERY: u32 = 5;
/// How long a lost contact stays on the map as a memory.
const MEMORY_MINUTES: u32 = 24 * 60;
/// A contact lost at least this long is announced again when found.
const REANNOUNCE_AFTER: u32 = 3 * 60;
pub const TRACK_MINUTES: u32 = 6 * 60;
pub const MUD_TRACK_MINUTES: u32 = 18 * 60;
/// Morale an ambushed army loses on the spot, out of [`crate::army::MORALE_MAX`].
pub const AMBUSH_SHOCK: u32 = 15_000;
/// Moonlight in percent by lunar phase (0 = new, 4 = full). A table rather than
/// `cos()` so every platform computes exactly the same thing.
const MOONLIGHT: [i32; crate::clock::LUNAR_CYCLE_DAYS as usize] = [0, 15, 50, 85, 100, 85, 50, 15];

/// How well a side knows an enemy army, worst to best.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Seen {
    /// Not currently detected; where it was last known to be.
    Remembered,
    /// Dust or campfires on the horizon: rough position, rough size.
    Rumored,
    /// Seen: exact position, size within about 15%.
    Sighted,
    /// Close enough to count heads and read banners.
    Identified,
}

impl Seen {
    pub fn label(self) -> &'static str {
        match self {
            Seen::Remembered => "last seen",
            Seen::Rumored => "rumored",
            Seen::Sighted => "sighted",
            Seen::Identified => "identified",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub army: u32,
    pub seen: Seen,
    /// Exact when sighted or identified, approximate when rumored, last known when remembered.
    pub cell: Cell,
    /// Estimated strength.
    pub men: u32,
    /// When this was last true.
    pub minute: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Track {
    pub cell: Cell,
    pub side: Side,
    pub minute: u32,
    /// Laid in rain: lasts much longer.
    pub mud: bool,
}

impl Track {
    pub fn lifetime(&self) -> u32 {
        if self.mud { MUD_TRACK_MINUTES } else { TRACK_MINUTES }
    }
}

/// One side's picture of the battlefield.
#[derive(Clone, Debug, Default)]
pub struct Intel {
    pub reports: Vec<Report>,
    /// Cells some friendly observer can currently see.
    vision: Vec<bool>,
}

impl Intel {
    pub fn report(&self, army: u32) -> Option<&Report> {
        self.reports.iter().find(|r| r.army == army)
    }

    pub fn level(&self, army: u32) -> Option<Seen> {
        self.report(army).map(|r| r.seen)
    }

    pub fn sees(&self, (x, y): Cell) -> bool {
        self.vision.get((y * MAP_W + x) as usize).copied().unwrap_or(false)
    }

    fn upsert(&mut self, r: Report) {
        match self.reports.iter_mut().find(|old| old.army == r.army) {
            Some(old) => *old = r,
            None => self.reports.push(r),
        }
    }
}

fn distance(a: Cell, b: Cell) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

impl Sim {
    pub fn intel(&self, side: Side) -> &Intel {
        &self.intel[side as usize]
    }

    /// How well `side` currently knows enemy army `army`.
    pub fn knows(&self, side: Side, army: u32) -> Option<Seen> {
        self.intel(side).level(army)
    }

    /// How far observers can see right now, before per-observer bonuses.
    pub fn sight(&self) -> i32 {
        let base = if self.clock.is_night() {
            let moon = if self.clock.moon_up() { MOONLIGHT[self.clock.moon_phase() as usize] } else { 0 };
            let moon = if matches!(self.weather, Weather::Cloudy | Weather::Rain) { moon / 3 } else { moon };
            4 + 6 * moon / 100
        } else {
            12
        };
        match self.weather {
            Weather::Fog => base.min(3),
            Weather::Rain => base * 7 / 10,
            _ => base,
        }
    }

    pub(crate) fn lay_track(&mut self, side: Side, cell: Cell) {
        let mud = self.weather == Weather::Rain;
        self.tracks.push(Track { cell, side, minute: self.clock.minute, mud });
    }

    pub(crate) fn fade_tracks(&mut self) {
        let now = self.clock.minute;
        self.tracks.retain(|t| now - t.minute < t.lifetime());
    }

    /// Whoever is fighting knows exactly who they're fighting.
    pub(crate) fn reveal_contacts(&mut self) {
        let now = self.clock.minute;
        let pairs: Vec<(u32, u32)> = self.battles.clone();
        for (p, q) in pairs {
            for (who, by) in [(p, q), (q, p)] {
                let (Some(a), Some(b)) = (self.army(who), self.army(by)) else { continue };
                let r = Report { army: a.id, seen: Seen::Identified, cell: a.pos, men: a.men, minute: now };
                let side = b.side as usize;
                self.intel[side].upsert(r);
            }
        }
    }

    /// Rolls for detection: every observer against every enemy army.
    pub(crate) fn detect(&mut self) {
        let now = self.clock.minute;
        let night = self.clock.is_night();
        let base = self.sight();
        for side in [Side::West, Side::East] {
            let mut observers: Vec<(Cell, i32)> = Vec::new();
            for a in self.armies.iter().filter(|a| a.side == side) {
                let r = if a.routing() { base / 2 } else { base };
                observers.extend(a.column().into_iter().map(|(c, _)| (c, r)));
            }
            for f in self.forts.iter().filter(|f| f.owner == Some(side)) {
                let tower = if f.extras.contains(&Extra::Watchtower) { 6 } else { 0 };
                observers.push((f.pos, base + 2 + tower));
            }

            let mut vision = vec![false; (MAP_W * MAP_H) as usize];
            for &((ox, oy), r) in &observers {
                for y in (oy - r).max(0)..=(oy + r).min(MAP_H - 1) {
                    for x in (ox - r).max(0)..=(ox + r).min(MAP_W - 1) {
                        vision[(y * MAP_W + x) as usize] = true;
                    }
                }
            }

            let mut reports = Vec::new();
            let mut newly_seen = Vec::new();
            for i in 0..self.armies.len() {
                let e = &self.armies[i];
                if e.side == side {
                    continue;
                }
                let prev = self.intel[side as usize].report(e.id).copied();
                let cells = e.column();
                let near = |o: Cell| cells.iter().map(|&(c, _)| distance(o, c)).min().unwrap_or(i32::MAX);
                let closest = observers.iter().map(|&(o, _)| near(o)).min().unwrap_or(i32::MAX);
                // Best margin inside some observer's range, and that observer's range.
                let best = observers.iter().map(|&(o, r)| (r - near(o), r)).max_by_key(|&(m, _)| m);
                let companies = e.companies() as i32;
                let in_forest = self.map.terrain(e.pos.0, e.pos.1) == Terrain::Forest;
                let camped = matches!(e.state, ArmyState::Holding | ArmyState::Resting);

                let mut seen = None;
                if closest <= 1 {
                    seen = Some(Seen::Identified);
                } else if let Some((margin, range)) = best.filter(|&(m, _)| m >= 0) {
                    let mut chance = (30 + companies * 4).min(90);
                    if e.state == ArmyState::Marching {
                        chance += 20;
                    }
                    if night && camped {
                        chance += 15; // campfires
                    }
                    if in_forest {
                        chance = chance * 35 / 100;
                    }
                    if companies <= 3 {
                        chance = chance * 6 / 10;
                    }
                    if self.weather == Weather::Rain && e.state == ArmyState::Marching {
                        chance += 25; // a fresh trail through the mud
                    }
                    if prev.as_ref().is_some_and(|p| p.seen >= Seen::Sighted) {
                        chance += 40; // keeping track is easier than finding
                    }
                    chance = chance * (50 + 50 * margin / range.max(1)) / 100;
                    if self.rng.range(0, 100) < chance.clamp(3, 97) {
                        seen = Some(if closest <= 3 { Seen::Identified } else { Seen::Sighted });
                    }
                }
                if seen.is_none() {
                    // Dust by day and campfires by night carry much farther than sight.
                    let mut range = 8 + companies * 3 / 2;
                    if e.state == ArmyState::Marching {
                        range = range * 13 / 10;
                    }
                    if night && !camped {
                        range /= 2;
                    }
                    match self.weather {
                        Weather::Fog => range = range * 4 / 10,
                        Weather::Rain if !night => range = range * 6 / 10,
                        _ => {}
                    }
                    if in_forest {
                        range = range * 3 / 10;
                    }
                    if closest <= range {
                        seen = Some(Seen::Rumored);
                    }
                }

                let h = hash2(e.id as i32, (now / 60) as i32, 5);
                let report = match seen {
                    Some(Seen::Identified) => {
                        Report { army: e.id, seen: Seen::Identified, cell: e.pos, men: e.men, minute: now }
                    }
                    Some(Seen::Sighted) => {
                        let est = e.men * (85 + h % 31) / 100;
                        Report {
                            army: e.id,
                            seen: Seen::Sighted,
                            cell: e.pos,
                            men: (est.div_ceil(50) * 50).max(50),
                            minute: now,
                        }
                    }
                    Some(_) => {
                        let jitter = ((h % 7) as i32 - 3, ((h / 7) % 5) as i32 - 2);
                        let cell = ((e.pos.0 + jitter.0).clamp(0, MAP_W - 1), (e.pos.1 + jitter.1).clamp(0, MAP_H - 1));
                        let est = e.men * (70 + h % 61) / 100;
                        Report {
                            army: e.id,
                            seen: Seen::Rumored,
                            cell,
                            men: (est.div_ceil(250) * 250).max(250),
                            minute: now,
                        }
                    }
                    None => match prev {
                        Some(p) if now - p.minute < MEMORY_MINUTES => Report { seen: Seen::Remembered, ..p },
                        _ => continue,
                    },
                };
                // Announce new contacts, and old ones found again after being lost a while.
                let fresh = match prev {
                    None => true,
                    Some(p) => p.seen == Seen::Remembered && now - p.minute >= REANNOUNCE_AFTER,
                };
                if fresh {
                    let place = self.place_name(report.cell);
                    newly_seen.push(match report.seen {
                        Seen::Rumored => format!("Dust on the horizon {place}: a host of perhaps {} men.", report.men),
                        _ => format!("Scouts report {}, about {} men, {place}.", e.name, report.men),
                    });
                }
                reports.push(report);
            }
            self.intel[side as usize] = crate::intel::Intel { reports, vision };
            for text in newly_seen {
                push_private(&mut self.log, now, Some(side), text);
            }
        }
    }
}
