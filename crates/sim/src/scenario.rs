use std::collections::VecDeque;

use crate::army::Army;
use crate::clock::{Clock, Weather};
use crate::fort::{Extra, Fort, FortKind};
use crate::map::{LANES, LOWER_ROW, MAP_W, MIDDLE_ROW, Map, Road, Site, UPPER_ROW};
use crate::rng::Rng;
use crate::{Side, Sim};

/// Men behind each capital's walls at the start: enough that storming them is a
/// day's bloody work, not an afternoon's.
const CAPITAL_GARRISON: u32 = 800;
/// The west capital's column. The valley mirrors about its middle, so column `x`
/// on the west side answers to `MIRROR - x` on the east.
const WEST_CAPITAL: i32 = 4;
const MIRROR: i32 = MAP_W - 1;
/// Holdings on the west half stay at or left of this column, so they never crowd
/// their mirrored twins.
const HALF: i32 = 110;
/// A holding in the middle of the valley, owed to neither side.
const CENTER: i32 = MIRROR / 2;

const KEEPS: &[&str] = &[
    "Crow Pagoda",
    "Heron Tower",
    "Owl Watch",
    "Thunder Keep",
    "Jade Spire",
    "Kestrel Hold",
    "Snow Lion Keep",
    "Bell Tower",
];
const MILLS: &[&str] =
    &["Willow Ford Mill", "Salt Terrace", "Stone Wheel Mill", "Reed Mill", "Copper Weir", "Lotus Mill", "Eel Weir"];
const VILLAGES: &[&str] = &[
    "Nine Wells",
    "Plum Village",
    "Three Bridges",
    "Fox Hollow",
    "Lantern Market",
    "Turtle Hill",
    "Ash Village",
    "Silk Inn",
];
const FARMS: &[&str] = &[
    "Ox Farm",
    "Persimmon Farm",
    "Goose Pond",
    "Tea Terraces",
    "Bean Rows",
    "Duck Paddies",
    "Rice Terraces",
    "Sorghum Farm",
    "Melon Patch",
    "Pear Orchard",
    "Barley Farm",
    "Mulberry Farm",
    "Hemp Fields",
    "Old Wu's Farm",
    "Millstone Farm",
    "Crane Paddies",
];
const CLIMBS: &[&str] = &["Goat Stair", "Pilgrim Steps", "Charcoal Path", "Shrine Path"];
const CROSSINGS: &[&str] = &["Ferry Road", "Stone Bridge Road", "Ox Ford Road", "Willow Causeway"];

/// A holding on the west half (or in the middle), before it's named and mirrored.
struct Plan {
    kind: FortKind,
    x: i32,
    lane: usize,
    stores: [u32; 3],
    extras: Vec<Extra>,
    garrison: u32,
}

fn lane_row(lane: usize) -> i32 {
    [UPPER_ROW, MIDDLE_ROW, LOWER_ROW][lane]
}

/// Stores, extras and garrison for a neutral holding of `kind`: a little different
/// every game.
fn outfit(rng: &mut Rng, kind: FortKind, lane: usize) -> ([u32; 3], Vec<Extra>, u32) {
    use Extra::*;
    let mut extras = Vec::new();
    // Each extra in `num` games out of four.
    let maybe = |rng: &mut Rng, extras: &mut Vec<Extra>, num, e: Extra| {
        if rng.chance(num, 4) {
            extras.push(e);
        }
    };
    let (stores, garrison) = match kind {
        FortKind::Keep => {
            extras.push(Watchtower);
            maybe(rng, &mut extras, 3, Palisade);
            maybe(rng, &mut extras, 1, Granary);
            ([rng.range(400, 800) as u32, 0, rng.range(80, 200) as u32], rng.range(100, 160) as u32)
        }
        FortKind::Mill => {
            extras.push(WaterMill);
            maybe(rng, &mut extras, 2, Granary);
            maybe(rng, &mut extras, 2, Palisade);
            if rng.chance(1, 2) {
                extras.push(DairyHerd(rng.range(6, 15) as u32));
            }
            // Down on the flats the mills smoke the river's eels as well.
            if lane == 2 || rng.chance(1, 3) {
                extras.push(Smokehouse);
            }
            (
                [rng.range(800, 1300) as u32, rng.range(40, 220) as u32, rng.range(0, 300) as u32],
                rng.range(90, 200) as u32,
            )
        }
        FortKind::Village => {
            if rng.chance(1, 2) {
                extras.push(DairyHerd(rng.range(4, 10) as u32));
            }
            ([rng.range(300, 550) as u32, rng.range(0, 100) as u32, 0], rng.range(40, 90) as u32)
        }
        FortKind::Farm => {
            if rng.chance(1, 3) {
                extras.push(DairyHerd(rng.range(4, 10) as u32));
            }
            ([rng.range(100, 260) as u32, rng.range(0, 40) as u32, 0], 0)
        }
        FortKind::Main => unreachable!("capitals are set, not rolled"),
    };
    (stores, extras, garrison)
}

/// A column in `lo..=hi` for something `half` columns wide in `lane`, clear of
/// everything already `taken` there (center column, lane, half-width).
fn place(rng: &mut Rng, lane: usize, (lo, hi): (i32, i32), half: i32, taken: &[(i32, usize, i32)]) -> Option<i32> {
    (0..60)
        .map(|_| rng.range(lo, hi + 1))
        .find(|&x| taken.iter().all(|&(tx, tl, th)| tl != lane || (x - tx).abs() > half + th + 5))
}

/// Control points along a lane through `anchors` (west half, sorted), wandering
/// up to `wander` rows off the lane between them, mirrored onto the east half.
fn lane_road(rng: &mut Rng, name: &'static str, lane: usize, anchors: &[i32], wander: f32) -> Road {
    let y0 = lane_row(lane) as f32 + 0.5;
    let mut west: Vec<(f32, f32)> = Vec::new();
    let mut prev: Option<i32> = None;
    for &x in anchors {
        if let Some(p) = prev {
            let gap = x - p;
            if gap >= 12 {
                for k in 1..=2 {
                    let y = y0 + rng.range(-10, 11) as f32 / 10.0 * wander;
                    west.push((p as f32 + gap as f32 * k as f32 / 3.0, y));
                }
            }
        }
        west.push((x as f32, y0));
        prev = Some(x);
    }
    // One more bend before the middle of the valley, unless a holding sits there.
    let last = prev.unwrap_or(0);
    if last < CENTER - 8 {
        west.push(((last + CENTER) as f32 / 2.0, y0 + rng.range(-10, 11) as f32 / 10.0 * wander));
    }
    let mut points = west.clone();
    if last == CENTER {
        // The middle holding is its own mirror: the road runs straight through it.
        points.pop();
        points.push((CENTER as f32 + 0.5, y0));
        west.pop();
    }
    points.extend(west.iter().rev().map(|&(x, y)| (MIRROR as f32 - x, y)));
    Road { name, points }
}

/// A short road from column `from` on road `a` to column `to` on road `b`,
/// meeting each exactly where it runs.
fn link(name: &'static str, a: &Road, from: i32, b: &Road, to: i32) -> Road {
    let (lo, hi) = if from < to { ((from, a), (to, b)) } else { ((to, b), (from, a)) };
    Road { name, points: vec![(lo.0 as f32, lo.1.y_at(lo.0 as f32)), (hi.0 as f32, hi.1.y_at(hi.0 as f32))] }
}

fn pick(rng: &mut Rng, names: &'static [&'static str], used: &mut Vec<&'static str>) -> &'static str {
    let free: Vec<_> = names.iter().copied().filter(|n| !used.contains(n)).collect();
    let name = free[rng.range(0, free.len() as i32) as usize];
    used.push(name);
    name
}

/// The opening scenario: one valley under a dying sun. The Azure Remnant holds
/// Lantern Gate in the west, the Vermilion Host the Cinnabar Citadel in the east.
/// Between them three lanes run the length of the valley: the high road under the
/// ridge, the Imperial Road down the middle, and the river road on the flats
/// beyond the river, tied together by climbs and bridges. Keeps, mills, villages
/// and farms are scattered along them, laid out in mirrored pairs so neither side
/// starts with the better half.
pub(crate) fn ashford_valley(seed: u64) -> Sim {
    let mut rng = Rng::new(seed ^ 0x6B69_6E67);

    // Where the lanes meet: a climb to the high road and a crossing to the river
    // road near each capital, and another of each farther out.
    let mut joins = Vec::new(); // (lane, column on the Imperial Road, column on the side lane)
    for lane in [0, 2] {
        let near = rng.range(14, 24);
        let far = rng.range(58, 92);
        for x in [near, far] {
            joins.push((lane, x, x + rng.range(6, 10)));
        }
    }
    let mut taken: Vec<(i32, usize, i32)> = vec![(WEST_CAPITAL, 1, 2)];
    for &(lane, mid, side) in &joins {
        taken.push((mid, 1, 1));
        taken.push((side, lane, 1));
    }
    let start = |lane: usize| joins.iter().filter(|j| j.0 == lane).map(|j| j.2).min().unwrap_or(20);

    // The west half's holdings.
    let mut plans: Vec<Plan> = Vec::new();
    let mut add = |rng: &mut Rng, kind: FortKind, lane: usize, span: (i32, i32), taken: &mut Vec<(i32, usize, i32)>| {
        let half = if kind == FortKind::Farm { 1 } else { 2 };
        let x = place(rng, lane, span, half, taken)?;
        taken.push((x, lane, half));
        let (stores, extras, garrison) = outfit(rng, kind, lane);
        plans.push(Plan { kind, x, lane, stores, extras, garrison });
        Some(())
    };
    add(&mut rng, FortKind::Mill, 1, (38, 58), &mut taken);
    let second = if rng.chance(1, 2) { FortKind::Keep } else { FortKind::Village };
    add(&mut rng, second, 1, (72, 104), &mut taken);
    let high = if rng.chance(2, 3) { FortKind::Keep } else { FortKind::Village };
    add(&mut rng, high, 0, (start(0) + 10, 104), &mut taken);
    let low = if rng.chance(1, 2) { FortKind::Mill } else { FortKind::Village };
    add(&mut rng, low, 2, (start(2) + 10, 104), &mut taken);
    // Farms, at least one in each side lane.
    let farms = rng.range(4, 7);
    for k in 0..farms {
        let lane = match k {
            0 => 0,
            1 => 2,
            _ => rng.range(0, LANES as i32) as usize,
        };
        let lo = if lane == 1 { 40 } else { start(lane) + 4 };
        add(&mut rng, FortKind::Farm, lane, (lo, HALF), &mut taken);
    }
    // Sometimes a keep in the middle of the valley, up under the ridge or down on
    // the flats, for both sides to fight over.
    let center = rng.chance(1, 2).then(|| if rng.chance(1, 2) { 0 } else { 2 });
    if let Some(lane) = center {
        let (stores, extras, garrison) = outfit(&mut rng, FortKind::Keep, lane);
        plans.push(Plan { kind: FortKind::Keep, x: CENTER, lane, stores, extras, garrison });
    }

    // Roads: one down each lane through its holdings, then the links between lanes.
    let anchors = |lane: usize, from: i32| {
        let mut xs: Vec<i32> = plans.iter().filter(|p| p.lane == lane).map(|p| p.x).collect();
        xs.push(from);
        xs.sort();
        xs
    };
    let imperial = lane_road(&mut rng, "Imperial Road", 1, &anchors(1, WEST_CAPITAL), 1.1);
    let high_name = if rng.chance(1, 2) { "High Road" } else { "Ridge Road" };
    let high_road = lane_road(&mut rng, high_name, 0, &anchors(0, start(0)), 0.9);
    let river_road = lane_road(&mut rng, "River Road", 2, &anchors(2, start(2)), 0.9);
    let mut used = Vec::new();
    let mut links = Vec::new();
    for &(lane, mid, side) in &joins {
        let other = if lane == 0 { &high_road } else { &river_road };
        let names = if lane == 0 { CLIMBS } else { CROSSINGS };
        links.push(link(pick(&mut rng, names, &mut used), &imperial, mid, other, side));
        links.push(link(pick(&mut rng, names, &mut used), &imperial, MIRROR - mid, other, MIRROR - side));
    }
    let mut roads = vec![imperial, high_road, river_road];
    roads.extend(links);

    // Name the holdings and mirror them east; the capitals stand at either end.
    let capital = |name, x, owner| Fort {
        id: 0,
        name,
        kind: FortKind::Main,
        pos: (x, MIDDLE_ROW),
        owner: Some(owner),
        stores: [3000, 200, 400],
        extras: vec![Extra::Palisade, Extra::Granary],
        garrison: CAPITAL_GARRISON,
        garrison_morale: crate::army::MORALE_MAX * 8 / 10,
        garrison_bleed: 0,
        capture: None,
    };
    let mut forts = vec![
        capital("Lantern Gate", WEST_CAPITAL, Side::West),
        capital("Cinnabar Citadel", MIRROR - WEST_CAPITAL, Side::East),
    ];
    let mut names = Vec::new();
    for p in &plans {
        let pool = match p.kind {
            FortKind::Keep => KEEPS,
            FortKind::Mill => MILLS,
            FortKind::Village => VILLAGES,
            _ => FARMS,
        };
        let twins = if p.x == CENTER { vec![p.x] } else { vec![p.x, MIRROR - p.x] };
        for x in twins {
            forts.push(Fort {
                name: pick(&mut rng, pool, &mut names),
                kind: p.kind,
                pos: (x, lane_row(p.lane)),
                owner: None,
                stores: p.stores,
                extras: p.extras.clone(),
                garrison: p.garrison,
                ..forts[0].clone()
            });
        }
    }
    forts.sort_by_key(|f| (f.pos.0, f.pos.1));
    for (id, f) in forts.iter_mut().enumerate() {
        f.id = id as u32;
    }

    let sites: Vec<Site> = forts.iter().map(|f| Site { pos: f.pos, walled: f.walled() }).collect();
    let mut middle: Vec<i32> = forts.iter().filter(|f| f.walled() && f.pos.1 == MIDDLE_ROW).map(|f| f.pos.0).collect();
    middle.sort();
    let map = Map::generate(seed, &sites, &middle, roads);

    let on_road = |x| map.road_cell(0, x);
    let west = forts[0].pos;
    let east = forts[forts.len() - 1].pos;
    let armies = vec![
        Army::new(1, Side::West, "First Banner", 1800, west),
        Army::new(2, Side::West, "Second Banner", 640, on_road(28)),
        Army::new(3, Side::West, "Grey Sparrows", 60, on_road(34)),
        Army::new(4, Side::East, "Cinnabar Host", 2000, east),
        Army::new(5, Side::East, "Ash Wolves", 400, on_road(MIRROR - 28)),
    ];

    Sim {
        map,
        forts,
        armies,
        // Day 1, late afternoon: a blood moon rises at dusk.
        clock: Clock { minute: 17 * 60 },
        weather: Weather::Clear,
        wind: -1,
        log: VecDeque::new(),
        clashes: Vec::new(),
        engaged: Vec::new(),
        battles: Vec::new(),
        garrison_fights: Vec::new(),
        announced: Vec::new(),
        raised: [2, 0],
        intel: Default::default(),
        tracks: Vec::new(),
        convoys: Vec::new(),
        forage: vec![crate::FORAGE_MAX * crate::army::MILLI as u32; LANES * MAP_W as usize],
        winner: None,
        rng: Rng::new(seed),
        next_army_id: 6,
        next_convoy_id: 1,
    }
}
