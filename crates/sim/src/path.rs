use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::Cell;
use crate::clock::Weather;
use crate::map::{Map, NEIGHBORS, Terrain};

/// Most men that fit through a forest cell at once. Bigger forces can push into
/// the woods, but they string out and go slower, the bigger the slower.
pub const FOREST_CAP: u32 = 150;
/// Most men that can climb into the mountains: scouts and lookouts, not armies.
pub const MOUNTAIN_CAP: u32 = 100;

/// Game minutes to enter a cell, or None if an army of `men` can't go there.
pub fn enter_cost(map: &Map, (x, y): Cell, men: u32, weather: Weather) -> Option<u32> {
    if !map.in_bounds(x, y) {
        return None;
    }
    let road = map.is_road(x, y);
    let base = match map.terrain(x, y) {
        _ if road => 15,
        Terrain::Fort => 15,
        Terrain::Field => 35,
        Terrain::Hill => 45,
        Terrain::Forest => 50 + crowding(men, 8).min(100),
        Terrain::Mountain if men <= MOUNTAIN_CAP => 90,
        Terrain::Mountain | Terrain::River => return None,
    };
    Some(match (weather, road) {
        (Weather::Rain, true) => base * 5 / 4,
        (Weather::Rain, false) => base * 3 / 2,
        _ => base,
    })
}

/// Game minutes for a wagon to enter a cell: roads and forts only, and mud makes
/// them crawl.
pub fn wagon_cost(map: &Map, (x, y): Cell, weather: Weather) -> Option<u32> {
    if !map.in_bounds(x, y) || !(map.is_road(x, y) || map.terrain(x, y) == Terrain::Fort) {
        return None;
    }
    Some(if weather == Weather::Rain { 30 } else { 20 })
}

/// Extra minutes (or stamina) per forest cell for an army of `men`: `per` for each
/// forest cell's worth of men beyond the first, as the column waits on itself.
fn crowding(men: u32, per: u32) -> u32 {
    (men.div_ceil(FOREST_CAP).max(1) - 1) * per
}

/// Stamina an army of `men` spends entering a cell.
pub fn stamina_cost(map: &Map, (x, y): Cell, men: u32, weather: Weather) -> u16 {
    let base = match map.terrain(x, y) {
        _ if map.is_road(x, y) => 6,
        Terrain::Fort => 6,
        Terrain::Forest => 20 + crowding(men, 2).min(25) as u16,
        Terrain::Hill => 18,
        Terrain::Mountain => 35,
        _ => 15,
    };
    if weather == Weather::Rain { base * 3 / 2 } else { base }
}

/// Cheapest route from `from` to `to` (excluding `from`), avoiding cells where `blocked` is true.
pub fn find_path(
    map: &Map,
    from: Cell,
    to: Cell,
    men: u32,
    weather: Weather,
    blocked: impl Fn(Cell) -> bool,
) -> Option<Vec<Cell>> {
    find_route(map, from, to, |c| enter_cost(map, c, men, weather), blocked)
}

/// Cheapest route from `from` to `to` (excluding `from`) under any entry cost.
/// Fort walls are passed only at their gates.
pub fn find_route(
    map: &Map,
    from: Cell,
    to: Cell,
    cost: impl Fn(Cell) -> Option<u32>,
    blocked: impl Fn(Cell) -> bool,
) -> Option<Vec<Cell>> {
    cost(to)?;
    let came_from = flood(map, from, Some(to), cost, blocked);
    trace(map, &came_from, from, to)
}

/// Like [`find_route`], but if `to` can't be reached, goes as close to it as the
/// ground allows. None if nowhere closer than `from` can be reached.
pub fn find_route_near(
    map: &Map,
    from: Cell,
    to: Cell,
    cost: impl Fn(Cell) -> Option<u32>,
    blocked: impl Fn(Cell) -> bool,
) -> Option<Vec<Cell>> {
    let came_from = flood(map, from, None, cost, blocked);
    let gap = |(x, y): Cell| (x - to.0).abs().max((y - to.1).abs());
    let reached = (0..map.h).flat_map(|y| (0..map.w).map(move |x| (x, y)));
    let best = reached
        .filter(|&c| c == from || came_from[(c.1 * map.w + c.0) as usize] != usize::MAX)
        .min_by_key(|&c| (gap(c), (c.0 - from.0).abs() + (c.1 - from.1).abs()))?;
    if best == from && from != to {
        return None;
    }
    trace(map, &came_from, from, best)
}

/// Dijkstra from `from`, stopping early once `stop` is settled. Returns each
/// cell's predecessor on its cheapest route (usize::MAX if unreached).
fn flood(
    map: &Map,
    from: Cell,
    stop: Option<Cell>,
    cost: impl Fn(Cell) -> Option<u32>,
    blocked: impl Fn(Cell) -> bool,
) -> Vec<usize> {
    let idx = |(x, y): Cell| (y * map.w + x) as usize;
    let mut best = vec![u32::MAX; (map.w * map.h) as usize];
    let mut came_from = vec![usize::MAX; best.len()];
    let mut heap = BinaryHeap::new();
    best[idx(from)] = 0;
    heap.push(Reverse((0u32, from.0, from.1)));

    while let Some(Reverse((spent, x, y))) = heap.pop() {
        if Some((x, y)) == stop {
            break;
        }
        if spent > best[idx((x, y))] {
            continue;
        }
        for (dx, dy) in NEIGHBORS {
            let next = (x + dx, y + dy);
            let Some(step) = cost(next) else { continue };
            if blocked(next) || map.crosses_wall((x, y), next) {
                continue;
            }
            let step = if dx != 0 && dy != 0 { step * 14 / 10 } else { step };
            let c = spent + step;
            if c < best[idx(next)] {
                best[idx(next)] = c;
                came_from[idx(next)] = idx((x, y));
                heap.push(Reverse((c, next.0, next.1)));
            }
        }
    }
    came_from
}

/// Walks the predecessors back from `to`, giving the route excluding `from`.
fn trace(map: &Map, came_from: &[usize], from: Cell, to: Cell) -> Option<Vec<Cell>> {
    if from == to {
        return Some(Vec::new());
    }
    let idx = |(x, y): Cell| (y * map.w + x) as usize;
    if came_from[idx(to)] == usize::MAX {
        return None;
    }
    let mut path = vec![to];
    let mut i = idx(to);
    while came_from[i] != idx(from) {
        i = came_from[i];
        path.push(((i as i32) % map.w, (i as i32) / map.w));
    }
    path.reverse();
    Some(path)
}
