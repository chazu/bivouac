//! The computer opponent. It plays through the same [`Order`]s a human sends, sees
//! only what its own side has detected, and is asked for orders once per tick.

use bivouac_sim::{Army, ArmyState, Cell, Extra, Fort, FortKind, Order, Pace, Seen, Side, Sim, Stance};

/// The plan, re-made every half hour from what the side can see:
/// - **Home guard.** The capital's garrison stays at full strength; only men
///   beyond it are mustered out.
/// - **Defend.** When an enemy force big enough to storm the capital is on our
///   half of the valley, every army that can get home before the siege is done
///   marches back at the double.
/// - **Counterstroke.** Armies too far away to save the capital strike at the
///   enemy's instead, or at the weakest fort the enemy has left behind.
/// - **Gather, then strike.** Otherwise the field armies take the next fort up
///   the valley together, waiting at the nearest friendly fort until there are
///   enough of them to storm it.
const MUSTER_AT: u32 = 600;
const MUSTER_MEN: u32 = 400;
/// Men the capital's garrison keeps however many are mustered.
const HOME_GUARD: u32 = 800;
/// Armies this close together count as one force.
const GATHER_RANGE: i32 = 6;
/// Known enemies this close to a fort will stand and fight for it.
const DEFENDERS_RANGE: i32 = 8;
/// Armies short of food detour this far to fill up at a friendly fort.
const TOP_UP_RANGE: i32 = 12;
/// Days of food an army keeps in hand beyond the march back to stores.
const FOOD_MARGIN: f32 = 0.5;
/// Days of food a storm is reckoned to take.
const STORM_DAYS: f32 = 0.5;
/// Columns an army can count on covering in a day on the road, allowing for rests.
const COLUMNS_PER_DAY: f32 = 64.0;
/// How long, in minutes, an enemy last seen still counts as there.
const MEMORY: u32 = 12 * 60;
/// Columns a relief force can be behind the enemy and still arrive while the
/// walls hold: storming a capital takes most of a day, and armies cover about
/// four columns an hour on the road.
const SIEGE_SLACK: i32 = 30;

pub fn think(sim: &Sim, side: Side) -> Vec<Order> {
    if !sim.clock.minute.is_multiple_of(30) {
        return Vec::new();
    }
    let mut orders = supply(sim, side);
    let Some(home) = sim.capital(side) else {
        return orders;
    };
    // Raise new cohorts from swelling garrisons, never thinning the capital's.
    for f in sim.forts.iter().filter(|f| f.owner == Some(side)) {
        let keep = if f.id == home.id { HOME_GUARD } else { 0 };
        if f.garrison >= MUSTER_AT.max(keep + MUSTER_MEN) {
            orders.push(Order::Muster { fort: f.id, men: MUSTER_MEN });
        }
    }

    let enemies = known_enemies(sim, side);
    let theirs = sim.capital(side.other());
    // An enemy force on our half of the valley that could storm the capital.
    let on_our_half = |c: Cell| theirs.is_none_or(|t| (c.0 - home.pos.0).abs() < (c.0 - t.pos.0).abs());
    let invaders: Vec<&(Cell, u32)> = enemies.iter().filter(|&&(c, _)| on_our_half(c)).collect();
    let home_strength = home.garrison * 3 / 2
        + sim
            .armies
            .iter()
            .filter(|a| a.side == side && near(a.pos, home.pos, GATHER_RANGE))
            .map(|a| a.men)
            .sum::<u32>();
    let threat = invaders.iter().map(|&&(_, m)| m).sum::<u32>() > home_strength;
    let threat_distance = invaders.iter().map(|&&(c, _)| (c.0 - home.pos.0).abs()).min().unwrap_or(0);

    let objective = sim
        .forts
        .iter()
        .filter(|f| f.owner != Some(side))
        .min_by_key(|f| (sim.map.road_distance(home.pos, f.pos), f.id));
    let field: Vec<&Army> = sim.armies.iter().filter(|a| a.side == side && !a.routing()).collect();

    for a in &field {
        if a.stance != Stance::Aggressive {
            orders.push(Order::SetStance { army: a.id, stance: Stance::Aggressive });
        }
        let defending = threat && (a.pos.0 - home.pos.0).abs() <= threat_distance + SIEGE_SLACK;
        let pace = if defending && a.stamina > 400 { Pace::Fast } else { Pace::Normal };
        if a.pace != pace {
            orders.push(Order::SetPace { army: a.id, pace });
        }
        if a.target.is_some() {
            continue; // already in a fight or chasing one
        }
        // Supply. An army eats where a friendly fort can feed it; anywhere else it
        // lives on what it carries. When that runs low it makes for the nearest
        // food it can reach: a fort of its own, or one it's strong enough to take.
        let group = |at: Cell| field.iter().filter(|b| near(b.pos, at, GATHER_RANGE)).map(|b| b.men).sum::<u32>();
        let eating = sim.forts.iter().any(|r| r.contains(a.pos) && feeds(side, r, a));
        let base = base_near(sim, side, a.pos, a);
        let prize = sim
            .forts
            .iter()
            .filter(|f| f.owner != Some(side) && would_feed(sim, side, f, a))
            .filter(|f| group(a.pos) >= needed(sim, side, f, &enemies))
            .min_by_key(|f| (sim.map.road_distance(a.pos, f.pos), f.id));
        // Food it would have to fight for is worth it only if it can get there and
        // win before it starves; if it can reach nothing, the nearest food will do.
        let cost = |f: &Fort| days(sim, a.pos, f.pos) + if f.owner == Some(side) { 0.0 } else { STORM_DAYS };
        let options = [base, prize].into_iter().flatten();
        let food = options
            .clone()
            .filter(|f| a.food_days() >= cost(f))
            .min_by_key(|f| sim.map.road_distance(a.pos, f.pos))
            .or_else(|| options.min_by_key(|f| sim.map.road_distance(a.pos, f.pos)));
        let low = |f: &&Fort| a.food_days() < cost(f) + FOOD_MARGIN;
        // Already on the way to food that's still there: keep going rather than dither.
        let heading = a.path.back().and_then(|&c| sim.fort_at(c)).filter(|_| a.state == ArmyState::Marching);
        let still_there = |f: &&Fort| {
            if f.owner == Some(side) { f.total_stores() >= a.men / 4 } else { would_feed(sim, side, f, a) }
        };
        if !eating && food.is_some_and(|f| low(&f)) && heading.is_some_and(|f| still_there(&f)) {
            continue;
        }
        if !eating && let Some(f) = food.filter(low) {
            orders.extend(if f.owner == Some(side) { march(sim, a, f.pos) } else { go_take(sim, side, a, f) });
            continue;
        }
        // Anyone passing a fort with stores to spare tops up.
        let larder = sim
            .forts
            .iter()
            .filter(|r| r.owner == Some(side) && r.total_stores() >= a.men / 4 && near(a.pos, r.pos, TOP_UP_RANGE))
            .min_by_key(|r| (sim.map.road_distance(a.pos, r.pos), r.id));
        if let Some(r) = larder.filter(|r| !r.contains(a.pos) && a.food_days() < 2.0) {
            orders.extend(march(sim, a, r.pos));
            continue;
        }
        // Tired men rest, unless home is in danger or the gates ahead stand open.
        let walk_in = objective.is_some_and(|f| !f.bars(side) && near(a.pos, f.pos, GATHER_RANGE));
        let worn = if defending || walk_in { 120 } else { 250 };
        // Starving men get nothing from rest, and a column on the march keeps
        // going until it's nearly spent.
        let urgent = defending || walk_in;
        let spent = if a.state == ArmyState::Marching { worn / 2 } else { worn };
        match a.state {
            _ if a.starving => {}
            ArmyState::Resting if a.stamina < 900 && !(urgent && a.stamina >= worn) => continue,
            _ if a.stamina < spent => {
                if a.state != ArmyState::Resting {
                    orders.push(Order::Rest { army: a.id });
                }
                continue;
            }
            _ => {}
        }

        if defending {
            // Home, and the aggressive stance falls on any besieger in the way.
            if !home.contains(a.pos) {
                orders.extend(march(sim, a, home.pos));
            }
            continue;
        }
        if threat {
            // Too far to save the capital: hit theirs, or whatever they left thinly held.
            let strike = sim
                .forts
                .iter()
                .filter(|f| f.owner == Some(side.other()))
                .filter(|f| group(a.pos) >= needed(sim, side, f, &enemies))
                .min_by_key(|f| (f.kind != FortKind::Main, sim.map.road_distance(a.pos, f.pos)));
            if let Some(f) = strike {
                orders.extend(go_take(sim, side, a, f));
            }
            continue;
        }
        let Some(f) = objective else { continue };
        let need = needed(sim, side, f, &enemies);
        let fighting = sim.in_battle(a.id) && group(a.pos) >= need / 2;
        // Only strike where the food holds out: there, the storm, and on to
        // wherever it can eat afterward, which may be the prize itself.
        let after = if would_feed(sim, side, f, a) {
            0.0
        } else {
            base_near(sim, side, f.pos, a).map_or(0.0, |b| days(sim, f.pos, b.pos))
        };
        let in_reach = a.food_days() >= days(sim, a.pos, f.pos) + STORM_DAYS + after;
        if fighting || (group(a.pos) >= need && in_reach) {
            orders.extend(go_take(sim, side, a, f));
            continue;
        }
        // Not enough here, or too far: gather at the fort nearest the objective that
        // can feed everyone there, and fold into one army.
        let Some(rally) = sim
            .forts
            .iter()
            .filter(|r| r.owner == Some(side) && r.total_stores() >= group(r.pos).max(a.men) / 2)
            .min_by_key(|r| (sim.map.road_distance(f.pos, r.pos), r.id))
        else {
            continue;
        };
        if a.pos != rally.pos {
            orders.extend(march(sim, a, rally.pos));
        } else {
            let biggest = field.iter().filter(|b| b.pos == a.pos).max_by_key(|b| (b.men, b.id)).map(|b| b.id);
            if field.iter().filter(|b| b.pos == a.pos).count() > 1 && biggest == Some(a.id) {
                orders.push(Order::Join { army: a.id });
            }
        }
    }
    orders
}

/// Whether friendly fort `r` can feed army `a`: it holds half a day's stores for
/// it. One the army is already eating from keeps it until it's down to a quarter
/// day, so it doesn't dither.
fn feeds(side: Side, r: &Fort, a: &Army) -> bool {
    let low = if r.contains(a.pos) { a.men / 4 } else { a.men / 2 };
    r.owner == Some(side) && r.total_stores() >= low
}

/// Whether taking fort `f` would feed army `a`, going by what's in it if the side
/// can see, or what a fort of its kind usually holds if not.
fn would_feed(sim: &Sim, side: Side, f: &Fort, a: &Army) -> bool {
    let stores = if sim.intel(side).sees(f.pos) {
        f.total_stores()
    } else {
        // Guessed low: an army that counts on stores that aren't there starves.
        match f.kind {
            FortKind::Main => 1000,
            FortKind::Keep | FortKind::Mill => 300,
            FortKind::Village | FortKind::Farm => 100,
        }
    };
    stores >= a.men / 4
}

/// The fort nearest `c` that can feed army `a`.
fn base_near<'s>(sim: &'s Sim, side: Side, c: Cell, a: &Army) -> Option<&'s Fort> {
    sim.forts.iter().filter(|r| feeds(side, r, a)).min_by_key(|r| (sim.map.road_distance(c, r.pos), r.id))
}

/// Days of marching from `from` to `to` by road.
fn days(sim: &Sim, from: Cell, to: Cell) -> f32 {
    sim.map.road_distance(from, to) as f32 / COLUMNS_PER_DAY
}

/// Marches on fort `f`, or attacks whoever stands in front of it.
fn go_take(sim: &Sim, side: Side, a: &Army, f: &Fort) -> Option<Order> {
    if f.contains(a.pos) {
        return (a.state != ArmyState::Holding).then_some(Order::Hold { army: a.id });
    }
    let defender =
        sim.intel(side).reports.iter().find(|r| {
            r.seen >= Seen::Sighted && f.within_reach(r.cell) && sim.army(r.army).is_some_and(|b| !b.routing())
        });
    match defender {
        Some(d) => Some(Order::Attack { army: a.id, target: d.army }),
        // Asked again each time, so an army waiting at the gates walks in once they open.
        None => march(sim, a, f.pos),
    }
}

/// A march order to `to`, unless the army is already on its way to wherever that
/// would take it (re-ordering restarts the step it's partway through).
fn march(sim: &Sim, a: &Army, to: Cell) -> Option<Order> {
    let end = *sim.plan_march(a, to)?.last()?;
    let going = a.state == ArmyState::Marching && a.path.back() == Some(&end);
    (!going).then_some(Order::March { army: a.id, to })
}

/// Men it should take to storm fort `f`: walls want three to every defender,
/// open ground two, plus half again any field army standing by it.
fn needed(sim: &Sim, side: Side, f: &Fort, enemies: &[(Cell, u32)]) -> u32 {
    let garrison = if sim.intel(side).sees(f.pos) {
        f.garrison
    } else {
        match f.kind {
            FortKind::Main => 1000, // what the walls hide is assumed to have grown
            FortKind::Keep | FortKind::Mill => 150,
            FortKind::Village => 60,
            FortKind::Farm => 0,
        }
    };
    let walls = if f.extras.contains(&Extra::Palisade) { 3 } else { 2 };
    let defenders: u32 = enemies.iter().filter(|&&(c, _)| near(c, f.pos, DEFENDERS_RANGE)).map(|&(_, m)| m).sum();
    (garrison * walls + defenders * 3 / 2).max(1)
}

/// Enemy armies this side has seen lately, with where and how strong.
fn known_enemies(sim: &Sim, side: Side) -> Vec<(Cell, u32)> {
    let now = sim.clock.minute;
    sim.intel(side)
        .reports
        .iter()
        .filter(|r| r.seen >= Seen::Rumored || now.saturating_sub(r.minute) < MEMORY)
        .filter(|r| sim.army(r.army).is_some_and(|b| !b.routing()))
        .map(|r| (r.cell, r.men))
        .collect()
}

fn near(a: Cell, b: Cell, range: i32) -> bool {
    (a.0 - b.0).abs().max((a.1 - b.1).abs()) <= range
}

/// Keeps stores flowing toward the front: a chain of wagon routes from the capital
/// through each held fort to the next one closer to the enemy, and each held farm
/// sending its harvest to the nearest of them.
fn supply(sim: &Sim, side: Side) -> Vec<Order> {
    let Some(capital) = sim.capital(side) else {
        return Vec::new();
    };
    let mut held: Vec<_> = sim.forts.iter().filter(|f| f.owner == Some(side) && f.walled()).collect();
    held.sort_by_key(|f| (sim.map.road_distance(capital.pos, f.pos), f.id));
    let mut routes: Vec<(u32, u32)> = held.windows(2).map(|pair| (pair[0].id, pair[1].id)).collect();
    for farm in sim.forts.iter().filter(|f| f.owner == Some(side) && !f.walled()) {
        if let Some(to) = held.iter().min_by_key(|f| (sim.map.road_distance(farm.pos, f.pos), f.id)) {
            routes.push((farm.id, to.id));
        }
    }
    let mut orders = Vec::new();
    for (from, to) in routes {
        match sim.route_from(from) {
            Some(r) if r.to == to || r.recalled => {}
            _ => orders.push(Order::Supply { from, to }),
        }
    }
    orders
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valley with only the given armies in it, `(id, men, column)`, each on the
    /// Imperial Road, and a look around so each side knows what it can see.
    fn valley(armies: &[(u32, u32, i32)]) -> Sim {
        let mut sim = Sim::new(0x0B1B_00AC);
        sim.armies.retain(|a| armies.iter().any(|&(id, _, _)| id == a.id));
        for &(id, men, x) in armies {
            let at = sim.map.road_cell(0, x);
            let a = sim.armies.iter_mut().find(|a| a.id == id).unwrap();
            a.men = men;
            a.pos = at;
        }
        for _ in 0..30 {
            sim.tick();
        }
        sim
    }

    /// Hands the East every holding from fort `i`'s column eastward but fort `i`
    /// itself, unguarded and empty, so a test decides what it can eat and the next
    /// fort it wants is `i`.
    fn east_beyond(sim: &mut Sim, i: usize) {
        let x = sim.forts[i].pos.0;
        for f in sim.forts.iter_mut().filter(|f| f.pos.0 > x - 3 && f.id != i as u32 && f.kind != FortKind::Main) {
            f.owner = Some(Side::East);
            f.garrison = 0;
            f.stores = [0; 3];
        }
    }

    fn marches_to(orders: &[Order], army: u32) -> Option<bivouac_sim::Cell> {
        orders.iter().find_map(|o| match *o {
            Order::March { army: a, to } if a == army => Some(to),
            _ => None,
        })
    }

    #[test]
    fn the_host_comes_home_to_meet_a_rush() {
        // Every West man on the Host's side of the valley, with the Host out in front.
        let sim = valley(&[(1, 2440, 205), (4, 2000, 200), (5, 400, 214)]);
        let r = sim.road_forts();
        assert!(sim.intel(Side::East).report(1).is_some(), "the rush should have been seen");
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 4), Some(sim.forts[r[5]].pos), "the Host should march home: {orders:?}");
        assert!(orders.iter().any(|o| matches!(o, Order::SetPace { army: 4, pace: Pace::Fast })), "at the double");
    }

    #[test]
    fn armies_too_far_to_help_strike_back() {
        let mut sim = valley(&[(1, 2440, 205), (5, 400, 214), (4, 2000, 60)]);
        let r = sim.road_forts();
        sim.forts[r[1]].owner = Some(Side::West);
        sim.forts[r[1]].garrison = 15;
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 4), Some(sim.forts[r[1]].pos), "the Host should fall on the mill: {orders:?}");
    }

    #[test]
    fn a_hungry_army_turns_back_for_stores() {
        let mut sim = valley(&[(5, 400, 160)]);
        let r = sim.road_forts();
        sim.forts[r[3]].owner = Some(Side::East);
        sim.forts[r[3]].garrison = 0;
        sim.armies[0].food = sim.armies[0].men as u64 * 500;
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 5), Some(sim.forts[r[3]].pos), "half a day's food: back to stores: {orders:?}");
    }

    #[test]
    fn a_host_gathers_where_it_can_eat() {
        let mut sim = valley(&[(4, 2000, 158)]);
        let r = sim.road_forts();
        east_beyond(&mut sim, r[1]);
        // The next fort down the road is nearer the mill, but bare; the one after
        // is stocked. The mill itself is bare too, so taking it won't feed the Host.
        sim.forts[r[3]].stores = [1500, 0, 0];
        sim.forts[r[1]].stores = [0; 3];
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 4), Some(sim.forts[r[3]].pos), "should wait where there's food: {orders:?}");
    }

    #[test]
    fn a_small_army_waits_for_help_before_storming() {
        let mut sim = valley(&[(5, 400, 112)]);
        let r = sim.road_forts();
        east_beyond(&mut sim, r[1]);
        sim.forts[r[2]].stores = [500, 0, 0];
        // The mill's garrison, guessed at 150 behind a palisade, wants more than 400.
        sim.forts[r[1]].extras.push(Extra::Palisade);
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 5), Some(sim.forts[r[2]].pos), "should fall back on the next fort: {orders:?}");
        let mut sim = valley(&[(5, 400, 112), (4, 2000, 114)]);
        let r = sim.road_forts();
        east_beyond(&mut sim, r[1]);
        sim.forts[r[2]].stores = [500, 0, 0];
        sim.forts[r[1]].extras.push(Extra::Palisade);
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 5), Some(sim.forts[r[1]].pos), "together they're enough: {orders:?}");
    }

    #[test]
    fn the_capital_keeps_its_guard() {
        let mut sim = valley(&[(4, 2000, 200)]);
        let r = sim.road_forts();
        let home = r[5] as u32;
        let musters =
            |sim: &Sim| think(sim, Side::East).iter().any(|o| matches!(o, Order::Muster { fort, .. } if *fort == home));
        sim.forts[r[5]].garrison = HOME_GUARD + MUSTER_MEN - 1;
        assert!(!musters(&sim));
        sim.forts[r[5]].garrison = HOME_GUARD + MUSTER_MEN;
        assert!(musters(&sim));
    }

    /// Every West man thrown down the road at Cinnabar Citadel, against the AI:
    /// `cargo test -p bivouac-ai rush_report -- --ignored --nocapture`.
    /// The AI against itself, to see whether wars still end and how hunger plays:
    /// `cargo test -p bivouac-ai duel_report -- --ignored --nocapture`. With
    /// `SEED=n` it plays that seed alone and tells where armies go hungry.
    #[test]
    #[ignore]
    fn duel_report() {
        let seeds = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).map_or(0..6, |s: u64| s..s + 1);
        for seed in seeds {
            let mut sim = Sim::new(seed);
            let mut deserted = [0u32; 2];
            for _ in 0..14 * 1440 {
                for side in [Side::West, Side::East] {
                    for o in think(&sim, side) {
                        let _ = sim.apply(side, o);
                    }
                }
                sim.tick();
                for e in sim.log.iter().take_while(|e| e.minute == sim.clock.minute) {
                    let t = &e.text;
                    if let Some(rest) = t.strip_prefix("Hunger thins ").or_else(|| t.split(" eats again, ").nth(1)) {
                        let n: u32 = rest.split_whitespace().find_map(|w| w.parse().ok()).unwrap_or(0);
                        deserted[e.side.map_or(0, |s| s as usize)] += n;
                    }
                    if std::env::var("SEED").is_ok()
                        && (t.contains("Hunger") || t.contains("food") || t.contains("eats again"))
                    {
                        let at =
                            sim.armies.iter().find(|a| t.contains(a.name.as_str())).map(|a| (a.pos, a.men, a.state));
                        println!("  day {} {} {t} {at:?}", e.minute / 1440, bivouac_sim::clock::hhmm(e.minute));
                    }
                    if !e.private && (t.contains("taken") || t.contains("war is over")) {
                        println!("  seed {seed} day {} {} {t}", e.minute / 1440, bivouac_sim::clock::hhmm(e.minute));
                    }
                }
                if sim.winner.is_some() {
                    break;
                }
            }
            let men = |side| sim.armies.iter().filter(|a| a.side == side).map(|a| a.men).sum::<u32>();
            let forts = |side| sim.forts.iter().filter(|f| f.owner == Some(side)).count();
            println!(
                "seed {seed}: winner {:?} on day {}; west {} men in the field, {} forts, {} deserted; east {}, {}, {}",
                sim.winner,
                sim.clock.minute / 1440,
                men(Side::West),
                forts(Side::West),
                deserted[0],
                men(Side::East),
                forts(Side::East),
                deserted[1]
            );
        }
    }

    #[test]
    #[ignore]
    fn rush_report() {
        for seed in 0..6 {
            let mut sim = Sim::new(seed);
            let r = sim.road_forts();
            let goal = sim.forts[r[5]].pos;
            // One great host: both banners together, the scouts left at home.
            sim.armies.retain(|a| a.id != 3);
            sim.armies[1].pos = sim.armies[0].pos;
            sim.apply(Side::West, Order::Join { army: 1 }).unwrap();
            for minute in 0..6 * 1440 {
                if minute % 30 == 0 {
                    let idle: Vec<&Army> = sim
                        .armies
                        .iter()
                        .filter(|a| a.side == Side::West && !a.routing() && a.target.is_none())
                        .collect();
                    let orders: Vec<Order> = idle
                        .iter()
                        .flat_map(|a| {
                            let mut o = vec![Order::SetStance { army: a.id, stance: Stance::Aggressive }];
                            if a.stamina < 150 && a.state != ArmyState::Resting {
                                o.push(Order::Rest { army: a.id });
                            } else if a.stamina >= 600 || a.state != ArmyState::Resting {
                                o.extend(march(&sim, a, goal));
                            }
                            o
                        })
                        .collect();
                    for o in orders {
                        let _ = sim.apply(Side::West, o);
                    }
                }
                for o in think(&sim, Side::East) {
                    let _ = sim.apply(Side::East, o);
                }
                sim.tick();
                for e in sim.log.iter().take_while(|e| e.minute == sim.clock.minute).filter(|e| !e.private) {
                    let t = &e.text;
                    if t.contains("taken") || t.contains("flees") || t.contains("war is over") || t.contains("scatters")
                    {
                        println!("  seed {seed} day {} {} {t}", e.minute / 1440, bivouac_sim::clock::hhmm(e.minute));
                    }
                }
                if sim.winner.is_some() {
                    break;
                }
            }
            if seed == 0 {
                for a in &sim.armies {
                    println!(
                        "  {} {:?} {} men at {:?} {:?} stamina {} food {:.1}d",
                        a.name,
                        a.side,
                        a.men,
                        a.pos,
                        a.state,
                        a.stamina,
                        a.food_days()
                    );
                }
                println!("  citadel garrison {}", sim.forts[r[5]].garrison);
            }
            let men = |side| sim.armies.iter().filter(|a| a.side == side).map(|a| a.men).sum::<u32>();
            println!(
                "seed {seed}: winner {:?} on day {}, field men west {} east {}",
                sim.winner,
                sim.clock.minute / 1440,
                men(Side::West),
                men(Side::East)
            );
        }
    }

    /// Plays the AI against an idle opponent and prints the log, to eyeball pacing:
    /// `cargo test -p bivouac-ai campaign_log -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn campaign_log() {
        let mut sim = Sim::new(0x0B1B_00AC);
        for _ in 0..4 * 1440 {
            for o in think(&sim, Side::East) {
                let _ = sim.apply(Side::East, o);
            }
            sim.tick();
            // Public news and the Host's own, as it happens; the idle West's scouting is noise.
            for e in sim.log.iter().take_while(|e| e.minute == sim.clock.minute).filter(|e| e.visible_to(Side::East)) {
                println!("day {} {} {}", e.minute / 1440, bivouac_sim::clock::hhmm(e.minute), e.text);
            }
        }
        for f in &sim.forts {
            println!("fort {} {:?} garrison {} stores {}", f.name, f.owner, f.garrison, f.total_stores());
        }
        for c in &sim.convoys {
            println!(
                "convoy {} {:?} {}→{} leg {:?} at {:?} delivered {}",
                c.name, c.side, c.from, c.to, c.leg, c.pos, c.delivered
            );
        }
        for a in &sim.armies {
            let path_end = a.path.back();
            println!(
                "army {} {:?} men {} pos {:?} state {:?} stamina {} target {:?} path_end {path_end:?}",
                a.name, a.side, a.men, a.pos, a.state, a.stamina, a.target
            );
        }
    }
}
