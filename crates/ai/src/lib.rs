//! The computer opponent. It plays through the same [`Order`]s a human sends,
//! and is asked for orders once per tick, before the sim advances.

use bivouac_sim::{ArmyState, Order, Side, Sim};

/// Placeholder strategy: every half hour, tired armies rest and idle ones march on
/// the nearest fort their side doesn't hold. Real planning (front strategy, raids,
/// supply) comes later; it should only ever see what its own armies can detect.
pub fn think(sim: &Sim, side: Side) -> Vec<Order> {
    if !sim.clock.minute.is_multiple_of(30) {
        return Vec::new();
    }
    let mut orders = Vec::new();
    for a in sim.armies.iter().filter(|a| a.side == side) {
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
                    Some(f) => orders.push(Order::March { army: a.id, to: f.pos }),
                    None => {}
                }
            }
        }
    }
    orders
}
