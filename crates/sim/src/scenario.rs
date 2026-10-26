use std::collections::VecDeque;

use crate::army::Army;
use crate::clock::{Clock, Weather};
use crate::fort::{Extra, Fort, FortKind};
use crate::map::{Map, Road};
use crate::rng::Rng;
use crate::{Side, Sim};

#[allow(clippy::too_many_arguments)] // a table-row constructor reads best flat
fn fort(
    id: u32,
    name: &'static str,
    kind: FortKind,
    x: i32,
    owner: Option<Side>,
    stores: [u32; 3],
    extras: Vec<Extra>,
    garrison: u32,
) -> Fort {
    let garrison_morale = crate::army::MORALE_MAX * 8 / 10;
    Fort {
        id,
        name,
        kind,
        pos: (x, 8),
        owner,
        stores,
        extras,
        garrison,
        garrison_morale,
        garrison_bleed: 0,
        capture: None,
    }
}

/// The opening scenario: one valley under a dying sun. The Azure Remnant holds
/// Lantern Gate in the west, the Vermilion Host the Cinnabar Citadel in the east,
/// and four neutral holdings lie along the Imperial Road between them.
pub(crate) fn ashford_valley(seed: u64) -> Sim {
    use Extra::*;
    use FortKind::*;
    let forts = vec![
        fort(0, "Lantern Gate", Main, 4, Some(Side::West), [3000, 200, 400], vec![Palisade, Granary], 300),
        fort(
            1,
            "Willow Ford Mill",
            Mill,
            48,
            None,
            [1240, 210, 60],
            vec![WaterMill, DairyHerd(12), Granary, Palisade],
            190,
        ),
        fort(2, "Nine Wells", Village, 96, None, [400, 80, 0], vec![], 60),
        fort(3, "Crow Pagoda", Keep, 144, None, [600, 0, 150], vec![Watchtower, Palisade], 120),
        fort(4, "Salt Terrace", Mill, 192, None, [900, 40, 300], vec![WaterMill, Smokehouse], 90),
        fort(5, "Cinnabar Citadel", Main, 235, Some(Side::East), [3000, 200, 400], vec![Palisade, Granary], 300),
    ];

    // The Imperial Road links every fort, wandering between them.
    let mut rng = Rng::new(seed ^ 0x6B69_6E67);
    let mut kings = Vec::new();
    for pair in forts.windows(2) {
        let (a, b) = (pair[0].pos.0 as f32, pair[1].pos.0 as f32);
        kings.push((a, 8.5));
        let span = b - a;
        kings.push((a + span / 3.0, rng.range(62, 98) as f32 / 10.0));
        kings.push((a + span * 2.0 / 3.0, rng.range(62, 98) as f32 / 10.0));
    }
    kings.push((forts[5].pos.0 as f32, 8.5));

    let roads = vec![
        Road { name: "Imperial Road", points: kings },
        Road {
            name: "Pilgrim Road",
            points: vec![(4.0, 8.5), (14.0, 5.2), (30.0, 3.6), (52.0, 3.4), (72.0, 4.6), (88.0, 6.5), (96.0, 8.5)],
        },
        Road {
            name: "Ridge Road",
            points: vec![
                (96.0, 8.5),
                (106.0, 5.5),
                (125.0, 3.8),
                (160.0, 4.2),
                (180.0, 3.6),
                (210.0, 5.0),
                (226.0, 7.0),
                (235.0, 8.5),
            ],
        },
        Road {
            name: "Ferry Road",
            points: vec![
                (48.0, 8.5),
                (60.0, 13.4),
                (80.0, 13.3),
                (100.0, 13.5),
                (125.0, 13.4),
                (137.0, 11.0),
                (144.0, 8.5),
            ],
        },
    ];

    let centers: Vec<_> = forts.iter().map(|f| f.pos).collect();
    let map = Map::generate(seed, &centers, roads);

    let on_road = |x| map.road_cell(0, x);
    let armies = vec![
        Army::new(1, Side::West, "First Banner", 1800, forts[0].pos),
        Army::new(2, Side::West, "Second Banner", 640, on_road(22)),
        Army::new(3, Side::West, "Grey Sparrows", 60, on_road(34)),
        Army::new(4, Side::East, "Cinnabar Host", 2000, forts[5].pos),
        Army::new(5, Side::East, "Ash Wolves", 400, on_road(214)),
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
        winner: None,
        rng: Rng::new(seed),
        next_army_id: 6,
        next_convoy_id: 1,
    }
}
