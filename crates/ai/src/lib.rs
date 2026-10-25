//! The computer opponent. It plays through the same [`Order`]s a human sends, sees
//! only what its own side has detected, and is asked for orders once per tick.

use bivouac_sim::{ArmyState, Order, Seen, Side, Sim, Stance};

/// Placeholder strategy: every half hour, keep armies aggressive, rest the tired ones
/// and march idle ones on the nearest fort their side doesn't hold. Real planning (front strategy, raids,
/// supply) comes later; it should only ever see what its own armies can detect.
const MUSTER_AT: u32 = 600;
const MUSTER_MEN: u32 = 400;

pub fn think(sim: &Sim, side: Side) -> Vec<Order> {
    if !sim.clock.minute.is_multiple_of(30) {
        return Vec::new();
    }
    let mut orders = Vec::new();
    // Raise new cohorts from swelling garrisons.
    for f in sim.forts.iter().filter(|f| f.owner == Some(side) && f.garrison >= MUSTER_AT) {
        orders.push(Order::Muster { fort: f.id, men: MUSTER_MEN });
    }
    for a in sim.armies.iter().filter(|a| a.side == side && !a.routing()) {
        if a.stance != Stance::Aggressive {
            orders.push(Order::SetStance { army: a.id, stance: Stance::Aggressive });
        }
        if a.target.is_some() {
            continue; // already in a fight or chasing one
        }
        match a.state {
            ArmyState::Marching => {}
            ArmyState::Resting if a.stamina < 900 => {}
            _ if a.stamina < 250 => orders.push(Order::Rest { army: a.id }),
            _ => {
                let target =
                    sim.forts.iter().filter(|f| f.owner != Some(side)).min_by_key(|f| (f.pos.0 - a.pos.0).abs());
                match target {
                    Some(f) if f.contains(a.pos) => {
                        if a.state != ArmyState::Holding {
                            orders.push(Order::Hold { army: a.id });
                        }
                    }
                    Some(f) => {
                        // Defenders standing at the fort have to be beaten first.
                        let defender = sim.intel(side).reports.iter().find(|r| {
                            r.seen >= Seen::Sighted
                                && f.within_reach(r.cell)
                                && sim.army(r.army).is_some_and(|b| !b.routing())
                        });
                        orders.push(match defender {
                            Some(d) => Order::Attack { army: a.id, target: d.army },
                            None => Order::March { army: a.id, to: f.pos },
                        });
                    }
                    None => {}
                }
            }
        }
    }
    orders
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
        for e in sim.log.iter().rev() {
            println!("day {} {} {}", e.minute / 1440, bivouac_sim::clock::hhmm(e.minute), e.text);
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
