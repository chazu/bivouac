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
/// Columns an army can count on covering in a day, allowing for rests.
const COLUMNS_PER_DAY: f32 = 48.0;
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
    let Some(home) = sim.forts.iter().find(|f| f.kind == FortKind::Main && f.owner == Some(side)) else {
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
    let theirs = sim.forts.iter().find(|f| f.kind == FortKind::Main && f.owner == Some(side.other()));
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

    let objective =
        sim.forts.iter().filter(|f| f.owner != Some(side)).min_by_key(|f| ((f.pos.0 - home.pos.0).abs(), f.id));
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
        // Hungry men head for the nearest stores (starving ones can't rest it off),
        // and anyone passing a well-stocked fort tops up.
        // Inside a friendly fort they wait for the wagons instead, and they don't
        // set out for stores they'd starve before reaching.
        let in_own_fort = sim.forts.iter().any(|f| f.owner == Some(side) && f.contains(a.pos));
        let reach = (a.food_days() * COLUMNS_PER_DAY) as i32;
        let larder = sim
            .forts
            .iter()
            .filter(|f| f.owner == Some(side) && f.total_stores() >= a.men / 4 && near(a.pos, f.pos, reach))
            .min_by_key(|f| ((f.pos.0 - a.pos.0).abs(), f.id));
        let hungry = |f: &Fort| a.food_days() < 1.5 || (a.food_days() < 3.0 && near(a.pos, f.pos, TOP_UP_RANGE));
        if let Some(f) = larder.filter(|&f| !in_own_fort && hungry(f)) {
            orders.extend(march(sim, a, f.pos));
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
        let group = |at: Cell| field.iter().filter(|b| near(b.pos, at, GATHER_RANGE)).map(|b| b.men).sum::<u32>();
        if threat {
            // Too far to save the capital: hit theirs, or whatever they left thinly held.
            let strike = sim
                .forts
                .iter()
                .filter(|f| f.owner == Some(side.other()))
                .filter(|f| group(a.pos) >= needed(sim, side, f, &enemies))
                .min_by_key(|f| (f.kind != FortKind::Main, (f.pos.0 - a.pos.0).abs()));
            if let Some(f) = strike {
                orders.extend(go_take(sim, side, a, f));
            }
            continue;
        }
        let Some(f) = objective else { continue };
        let need = needed(sim, side, f, &enemies);
        let fighting = sim.in_battle(a.id) && group(a.pos) >= need / 2;
        if group(a.pos) >= need || fighting {
            orders.extend(go_take(sim, side, a, f));
            continue;
        }
        // Not enough here: gather at the friendly fort nearest the objective and
        // fold into one army there.
        let Some(rally) =
            sim.forts.iter().filter(|r| r.owner == Some(side)).min_by_key(|r| ((r.pos.0 - f.pos.0).abs(), r.id))
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
/// through each held fort to the next one closer to the enemy.
fn supply(sim: &Sim, side: Side) -> Vec<Order> {
    let Some(capital) = sim.forts.iter().find(|f| f.kind == FortKind::Main && f.owner == Some(side)) else {
        return Vec::new();
    };
    let mut held: Vec<_> = sim.forts.iter().filter(|f| f.owner == Some(side)).collect();
    held.sort_by_key(|f| ((f.pos.0 - capital.pos.0).abs(), f.id));
    let mut orders = Vec::new();
    for pair in held.windows(2) {
        let (from, to) = (pair[0].id, pair[1].id);
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
        assert!(sim.intel(Side::East).report(1).is_some(), "the rush should have been seen");
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 4), Some(sim.forts[5].pos), "the Host should march home: {orders:?}");
        assert!(orders.iter().any(|o| matches!(o, Order::SetPace { army: 4, pace: Pace::Fast })), "at the double");
    }

    #[test]
    fn armies_too_far_to_help_strike_back() {
        let mut sim = valley(&[(1, 2440, 205), (5, 400, 214), (4, 2000, 60)]);
        sim.forts[1].owner = Some(Side::West);
        sim.forts[1].garrison = 15;
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 4), Some(sim.forts[1].pos), "the Host should fall on Willow Ford: {orders:?}");
    }

    #[test]
    fn a_small_army_waits_for_help_before_storming() {
        let mut sim = valley(&[(5, 400, 112)]);
        for f in &mut sim.forts[2..5] {
            f.owner = Some(Side::East);
            f.garrison = 0;
        }
        // Willow Ford's 190 behind a palisade want more than 400.
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 5), Some(sim.forts[2].pos), "should fall back on Nine Wells: {orders:?}");
        let mut sim = valley(&[(5, 400, 112), (4, 2000, 114)]);
        for f in &mut sim.forts[2..5] {
            f.owner = Some(Side::East);
            f.garrison = 0;
        }
        let orders = think(&sim, Side::East);
        assert_eq!(marches_to(&orders, 5), Some(sim.forts[1].pos), "together they're enough: {orders:?}");
    }

    #[test]
    fn the_capital_keeps_its_guard() {
        let mut sim = valley(&[(4, 2000, 200)]);
        let musters = |sim: &Sim| think(sim, Side::East).iter().any(|o| matches!(o, Order::Muster { fort: 5, .. }));
        sim.forts[5].garrison = HOME_GUARD + MUSTER_MEN - 1;
        assert!(!musters(&sim));
        sim.forts[5].garrison = HOME_GUARD + MUSTER_MEN;
        assert!(musters(&sim));
    }

    /// Every West man thrown down the road at Cinnabar Citadel, against the AI:
    /// `cargo test -p bivouac-ai rush_report -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn rush_report() {
        for seed in 0..6 {
            let mut sim = Sim::new(seed);
            let goal = sim.forts[5].pos;
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
                println!("  citadel garrison {}", sim.forts[5].garrison);
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
