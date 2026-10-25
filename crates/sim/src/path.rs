use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::Cell;
use crate::clock::Weather;
use crate::map::{Map, Terrain};

/// Most men that can push through a forest cell. The real number is an open design question.
pub const FOREST_CAP: u32 = 150;

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
        Terrain::Forest if men <= FOREST_CAP => 50,
        Terrain::Forest | Terrain::Mountain | Terrain::River => return None,
    };
    Some(match (weather, road) {
        (Weather::Rain, true) => base * 5 / 4,
        (Weather::Rain, false) => base * 3 / 2,
        _ => base,
    })
}

/// Stamina spent entering a cell.
pub fn stamina_cost(map: &Map, (x, y): Cell, weather: Weather) -> u16 {
    let base = match map.terrain(x, y) {
        _ if map.is_road(x, y) => 6,
        Terrain::Fort => 6,
        Terrain::Forest => 20,
        _ => 15,
    };
    if weather == Weather::Rain { base * 3 / 2 } else { base }
}

const NEIGHBORS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// Cheapest route from `from` to `to` (excluding `from`), avoiding cells where `blocked` is true.
pub fn find_path(
    map: &Map,
    from: Cell,
    to: Cell,
    men: u32,
    weather: Weather,
    blocked: impl Fn(Cell) -> bool,
) -> Option<Vec<Cell>> {
    enter_cost(map, to, men, weather)?;
    if from == to {
        return Some(Vec::new());
    }
    let idx = |(x, y): Cell| (y * map.w + x) as usize;
    let mut best = vec![u32::MAX; (map.w * map.h) as usize];
    let mut came_from = vec![usize::MAX; best.len()];
    let mut heap = BinaryHeap::new();
    best[idx(from)] = 0;
    heap.push(Reverse((0u32, from.0, from.1)));

    while let Some(Reverse((cost, x, y))) = heap.pop() {
        if (x, y) == to {
            let mut path = vec![to];
            let mut i = idx(to);
            while came_from[i] != idx(from) {
                i = came_from[i];
                path.push(((i as i32) % map.w, (i as i32) / map.w));
            }
            path.reverse();
            return Some(path);
        }
        if cost > best[idx((x, y))] {
            continue;
        }
        for (dx, dy) in NEIGHBORS {
            let next = (x + dx, y + dy);
            let Some(step) = enter_cost(map, next, men, weather) else { continue };
            if blocked(next) {
                continue;
            }
            let step = if dx != 0 && dy != 0 { step * 14 / 10 } else { step };
            let c = cost + step;
            if c < best[idx(next)] {
                best[idx(next)] = c;
                came_from[idx(next)] = idx((x, y));
                heap.push(Reverse((c, next.0, next.1)));
            }
        }
    }
    None
}
