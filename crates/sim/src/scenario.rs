use std::collections::VecDeque;

use crate::army::Army;
use crate::clock::{Clock, MINUTES_PER_DAY, Weather};
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
    Fort { id, name, kind, pos: (x, 8), owner, stores, extras, garrison, capture: None }
}

/// The opening scenario: one valley, two main forts, four neutral holdings between.
pub(crate) fn ashford_valley(seed: u64) -> Sim {
    use Extra::*;
    use FortKind::*;
    let forts = vec![
        fort(0, "Greywall Keep", Main, 4, Some(Side::West), [3000, 200, 400], vec![Palisade, Granary], 300),
        fort(
            1,
            "Ashford Mill",
            Mill,
            48,
            None,
            [1240, 210, 60],
            vec![WaterMill, DairyHerd(12), Granary, Palisade],
            190,
        ),
        fort(2, "Thornby", Village, 96, None, [400, 80, 0], vec![], 60),
        fort(3, "Crow's Watch", Keep, 144, None, [600, 0, 150], vec![Watchtower, Palisade], 120),
        fort(4, "Saltmarsh", Mill, 192, None, [900, 40, 300], vec![WaterMill, Smokehouse], 90),
        fort(5, "Redhold", Main, 235, Some(Side::East), [3000, 200, 400], vec![Palisade, Granary], 300),
    ];

    // The King's Road links every fort, wandering between them.
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
        Road { name: "King's Road", points: kings },
        Road {
            name: "North Road",
            points: vec![(4.0, 8.5), (14.0, 5.2), (30.0, 3.6), (52.0, 3.4), (72.0, 4.6), (88.0, 6.5), (96.0, 8.5)],
        },
        Road {
            name: "High Road",
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
            name: "River Road",
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

    let on_kings = |x| map.road_cell(0, x);
    let armies = vec![
        Army::new(1, Side::West, "1st Army", 1800, forts[0].pos),
        Army::new(2, Side::West, "2nd Column", 640, on_kings(22)),
        Army::new(3, Side::West, "Scouts", 60, on_kings(34)),
        Army::new(4, Side::East, "Red Host", 2000, forts[5].pos),
        Army::new(5, Side::East, "Raiders", 400, on_kings(214)),
    ];

    Sim {
        map,
        forts,
        armies,
        // Day 14, late afternoon: a blood moon rises at dusk.
        clock: Clock { minute: 14 * MINUTES_PER_DAY + 17 * 60 },
        weather: Weather::Clear,
        wind: -1,
        log: VecDeque::new(),
        rng: Rng::new(seed),
        next_army_id: 6,
    }
}
