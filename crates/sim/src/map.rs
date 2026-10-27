use crate::Cell;
use crate::rng::Rng;

pub const MAP_W: i32 = 240;
/// Rows of the three-quarter view: row 0 is the far ridge, the last row is nearest.
pub const MAP_H: i32 = 20;

/// The rows the valley's three lanes run along: the high road under the ridge,
/// the Imperial Road down the middle, and the river road on the flats beyond.
pub const UPPER_ROW: i32 = 5;
pub const MIDDLE_ROW: i32 = 10;
pub const LOWER_ROW: i32 = 17;
/// The rows the river winds between.
pub const RIVER_TOP: i32 = 13;
pub const RIVER_BOTTOM: i32 = 15;
pub const LANES: usize = 3;

/// Which lane row `y` lies in: 0 under the ridge, 1 down the middle, 2 beyond
/// the river.
pub fn lane(y: i32) -> usize {
    if y < MIDDLE_ROW - 2 {
        0
    } else if y < RIVER_TOP {
        1
    } else {
        2
    }
}

/// Where a holding stands, for clearing its ground and raising its walls.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub pos: Cell,
    pub walled: bool,
}

/// Road heights snap to quarter rows so the line the client draws and the
/// cells the sim walks are derived from the same numbers.
pub const ROAD_STEPS_PER_ROW: f32 = 4.0;

pub(crate) const NEIGHBORS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Field,
    Forest,
    /// High ground: slow going, but it sees farther and defends better.
    Hill,
    Mountain,
    River,
    Fort,
}

/// The lay of the land along one stretch of the valley.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Country {
    /// Deep woods: cover for raiders, closed to armies off the road.
    Woodland,
    /// Mountain spurs pinch the valley down to a defile around the road.
    Narrows,
    /// Rolling hills crowned with crags: high ground to fight over.
    Highlands,
    /// Open fields cut by hedgerows: room to maneuver, little to hide in.
    Farmland,
}

impl Country {
    pub fn label(self) -> &'static str {
        match self {
            Country::Woodland => "deep woods",
            Country::Narrows => "a mountain pass",
            Country::Highlands => "hill country",
            Country::Farmland => "open farmland",
        }
    }

    fn names(self) -> &'static [&'static str] {
        match self {
            Country::Woodland => {
                &["the Weeping Pines", "Moth Lantern Wood", "the Old Cedar Deep", "Ghost Bamboo", "the Black Maples"]
            }
            Country::Narrows => {
                &["the Crane's Neck", "Iron Gate Gorge", "the Dragon's Teeth", "Nine Turn Defile", "Whisper Gap"]
            }
            Country::Highlands => {
                &["the Tiger Hills", "Kite Heights", "the Tomb Hills", "Cloud Step Downs", "the Bell Hills"]
            }
            Country::Farmland => {
                &["the Millet Plain", "Mulberry Fields", "the Red Paddies", "Ox Yoke Meadows", "the Plum Orchards"]
            }
        }
    }
}

/// A named stretch of valley between two forts, columns `x0..=x1`.
#[derive(Clone, Debug)]
pub struct Region {
    pub name: &'static str,
    pub country: Country,
    pub x0: i32,
    pub x1: i32,
}

/// A smooth road y = f(x) through control points, in cell units
/// (y = 8.5 runs along the middle of row 8). Points must have increasing x.
#[derive(Clone, Debug)]
pub struct Road {
    pub name: &'static str,
    pub points: Vec<(f32, f32)>,
}

impl Road {
    pub fn x_range(&self) -> (i32, i32) {
        (self.points[0].0 as i32, self.points[self.points.len() - 1].0 as i32)
    }

    /// Cubic Hermite interpolation with Catmull-Rom tangents.
    pub fn y_at(&self, x: f32) -> f32 {
        let p = &self.points;
        let last = p.len() - 1;
        if x <= p[0].0 {
            return p[0].1;
        }
        if x >= p[last].0 {
            return p[last].1;
        }
        let i = p.windows(2).position(|w| x <= w[1].0).unwrap_or(last - 1);
        let ((x0, y0), (x1, y1)) = (p[i], p[i + 1]);
        let h = x1 - x0;
        let t = (x - x0) / h;
        let m0 = if i == 0 { (y1 - y0) / h } else { (y1 - p[i - 1].1) / (x1 - p[i - 1].0) };
        let m1 = if i + 2 > last { (y1 - y0) / h } else { (p[i + 2].1 - y0) / (p[i + 2].0 - x0) };
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * h * m0
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * h * m1
    }

    /// Quantized road height at the left edge of column `x`.
    pub fn edge_y(&self, x: i32) -> f32 {
        let y = (self.y_at(x as f32) * ROAD_STEPS_PER_ROW).round() / ROAD_STEPS_PER_ROW;
        y.clamp(0.25, MAP_H as f32 - 0.25)
    }

    /// Rows the road passes through in column `x`, top to bottom.
    pub fn rows_in_column(&self, x: i32) -> std::ops::RangeInclusive<i32> {
        let (a, b) = (self.edge_y(x), self.edge_y(x + 1));
        let (lo, hi) = (a.min(b), a.max(b));
        let top = lo.floor() as i32;
        let bottom = ((hi - 0.001).floor() as i32).max(top);
        top.min(MAP_H - 1)..=bottom.min(MAP_H - 1)
    }
}

#[derive(Clone, Debug)]
pub struct Map {
    pub w: i32,
    pub h: i32,
    terrain: Vec<Terrain>,
    road: Vec<bool>,
    pub roads: Vec<Road>,
    pub regions: Vec<Region>,
}

impl Map {
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }

    pub fn terrain(&self, x: i32, y: i32) -> Terrain {
        self.terrain[self.idx(x, y)]
    }

    pub fn is_road(&self, x: i32, y: i32) -> bool {
        self.road[self.idx(x, y)]
    }

    pub fn is_bridge(&self, x: i32, y: i32) -> bool {
        self.is_road(x, y) && self.terrain(x, y) == Terrain::River
    }

    /// The named stretch of valley column `x` lies in, if any.
    pub fn region_at(&self, x: i32) -> Option<&Region> {
        self.regions.iter().find(|r| (r.x0..=r.x1).contains(&x))
    }

    fn walled(&self, (x, y): Cell) -> bool {
        self.in_bounds(x, y) && self.terrain(x, y) == Terrain::Fort
    }

    /// Whether a step between neighboring cells `a` and `b` would climb a fort
    /// wall. Walls are crossed only at gates, where a road runs through them.
    pub fn crosses_wall(&self, a: Cell, b: Cell) -> bool {
        self.walled(a) != self.walled(b) && !(self.is_road(a.0, a.1) && self.is_road(b.0, b.1))
    }

    /// A fort cell where a road passes out through the wall.
    pub fn is_gate(&self, x: i32, y: i32) -> bool {
        self.walled((x, y))
            && self.is_road(x, y)
            && NEIGHBORS.iter().any(|&(dx, dy)| {
                let c = (x + dx, y + dy);
                self.in_bounds(c.0, c.1) && !self.walled(c) && self.is_road(c.0, c.1)
            })
    }

    /// A ragged ellipse of terrain `t` laid over whatever cells are one of `onto`.
    fn blob(&mut self, rng: &mut Rng, (cx, cy): Cell, (rx, ry): (i32, i32), t: Terrain, onto: &[Terrain]) {
        for y in cy - ry..=cy + ry {
            for x in cx - rx..=cx + rx {
                let (dx, dy) = ((x - cx) as f32 / rx.max(1) as f32, (y - cy) as f32 / (ry as f32 + 0.5));
                let ragged = rng.range(0, 30) as f32 / 100.0;
                if dx * dx + dy * dy <= 1.0 + ragged && self.in_bounds(x, y) && onto.contains(&self.terrain(x, y)) {
                    self.set(x, y, t);
                }
            }
        }
    }

    /// One great wood filling most of the valley, a belt of trees running from the
    /// ridge down to the river so small parties can cross unseen, and a few glades.
    fn woodland(&mut self, rng: &mut Rng, x0: i32, x1: i32) {
        let open = [Terrain::Field, Terrain::Hill];
        for _ in 0..rng.range(6, 9) {
            let c = (rng.range(x0 + 4, x1 - 3), rng.range(3, 13));
            let r = (rng.range(6, 13), rng.range(1, 4));
            self.blob(rng, c, r, Terrain::Forest, &open);
        }
        let mut bx = rng.range(x0 + 6, x1 - 6);
        for y in 2..RIVER_TOP {
            bx += rng.range(-1, 2);
            for x in bx - rng.range(1, 3)..=bx + rng.range(1, 3) {
                if self.in_bounds(x, y) && open.contains(&self.terrain(x, y)) {
                    self.set(x, y, Terrain::Forest);
                }
            }
        }
        for _ in 0..rng.range(1, 3) {
            let c = (rng.range(x0 + 4, x1 - 3), rng.range(5, 12));
            let r = (rng.range(2, 4), 0);
            self.blob(rng, c, r, Terrain::Field, &[Terrain::Forest]);
        }
    }

    /// Mountain spurs reaching down from the ridge to pinch the valley into a
    /// defile around the Imperial Road, with rough ground at their feet.
    fn narrows(&mut self, rng: &mut Rng, x0: i32, x1: i32) {
        let spurs = if x1 - x0 >= 34 && rng.chance(1, 2) { 2 } else { 1 };
        for k in 0..spurs {
            let span = (x1 - x0) / spurs;
            let cx = x0 + span * k + rng.range(span / 3, span * 2 / 3 + 1);
            let bottom = rng.range(MIDDLE_ROW - 1, MIDDLE_ROW + 1);
            let top_half = rng.range(6, 11);
            for y in 0..=bottom {
                let half = top_half * (bottom - y + 1) / (bottom + 1) + rng.range(0, 2);
                for x in cx - half..=cx + half {
                    if self.in_bounds(x, y) && self.terrain(x, y) != Terrain::River {
                        self.set(x, y, Terrain::Mountain);
                    }
                }
            }
        }
        // Scree and foothills around the spurs.
        for y in 0..RIVER_TOP {
            for x in x0..=x1 {
                let rocky = NEIGHBORS.iter().any(|&(dx, dy)| {
                    self.in_bounds(x + dx, y + dy) && self.terrain(x + dx, y + dy) == Terrain::Mountain
                });
                if rocky && self.terrain(x, y) == Terrain::Field && rng.chance(1, 2) {
                    self.set(x, y, Terrain::Hill);
                }
            }
        }
        for _ in 0..rng.range(2, 4) {
            let c = (rng.range(x0 + 2, x1 - 1), rng.range(4, 12));
            let r = (rng.range(1, 4), 1);
            self.blob(rng, c, r, Terrain::Forest, &[Terrain::Field, Terrain::Hill]);
        }
    }

    fn highlands(&mut self, rng: &mut Rng, x0: i32, x1: i32) {
        for _ in 0..rng.range(4, 7) {
            let (cx, cy) = (rng.range(x0 + 4, x1 - 3), rng.range(4, 12));
            let r = (rng.range(3, 8), rng.range(1, 3));
            self.blob(rng, (cx, cy), r, Terrain::Hill, &[Terrain::Field]);
            // Crags on the crowns.
            if rng.chance(1, 2) {
                for x in cx..cx + rng.range(1, 3) {
                    self.set(x, cy, Terrain::Mountain);
                }
            }
        }
        for _ in 0..rng.range(2, 5) {
            let c = (rng.range(x0 + 2, x1 - 1), rng.range(4, 13));
            let r = (rng.range(1, 4), rng.range(0, 2));
            self.blob(rng, c, r, Terrain::Forest, &[Terrain::Field, Terrain::Hill]);
        }
    }

    fn farmland(&mut self, rng: &mut Rng, x0: i32, x1: i32) {
        // Hedgerows: thin lines of trees along field edges.
        for _ in 0..rng.range(4, 7) {
            let (x, y) = (rng.range(x0, x1 - 4), rng.range(4, 13));
            for hx in x..(x + rng.range(3, 9)).min(x1) {
                if self.terrain(hx, y) == Terrain::Field {
                    self.set(hx, y, Terrain::Forest);
                }
            }
        }
        for _ in 0..rng.range(1, 3) {
            let c = (rng.range(x0 + 2, x1 - 1), rng.range(4, 13));
            let r = (rng.range(1, 3), 0);
            self.blob(rng, c, r, Terrain::Forest, &[Terrain::Field]);
        }
        if rng.chance(1, 2) {
            let c = (rng.range(x0 + 4, x1 - 3), rng.range(4, 12));
            let r = (rng.range(2, 5), 1);
            self.blob(rng, c, r, Terrain::Hill, &[Terrain::Field]);
        }
    }

    /// The flats beyond the river, in the same country as the valley above them
    /// but gentler: copses in the woods, knolls in the hills, hedges on the farms.
    fn flats(&mut self, rng: &mut Rng, x0: i32, x1: i32, country: Country) {
        let rows = (RIVER_BOTTOM + 1, MAP_H);
        let open = [Terrain::Field, Terrain::Hill];
        match country {
            Country::Woodland => {
                for _ in 0..rng.range(2, 5) {
                    let c = (rng.range(x0 + 3, x1 - 2), rng.range(rows.0 + 1, rows.1));
                    let r = (rng.range(3, 9), 1);
                    self.blob(rng, c, r, Terrain::Forest, &open);
                }
            }
            Country::Narrows | Country::Highlands => {
                for _ in 0..rng.range(2, 4) {
                    let c = (rng.range(x0 + 3, x1 - 2), rng.range(rows.0 + 1, rows.1));
                    let r = (rng.range(2, 6), 1);
                    self.blob(rng, c, r, Terrain::Hill, &[Terrain::Field]);
                }
                let c = (rng.range(x0 + 2, x1 - 1), rng.range(rows.0, rows.1));
                let r = (rng.range(1, 4), 0);
                self.blob(rng, c, r, Terrain::Forest, &open);
            }
            Country::Farmland => {
                for _ in 0..rng.range(2, 5) {
                    let (x, y) = (rng.range(x0, x1 - 4), rng.range(rows.0, rows.1));
                    for hx in x..(x + rng.range(3, 9)).min(x1) {
                        if self.terrain(hx, y) == Terrain::Field {
                            self.set(hx, y, Terrain::Forest);
                        }
                    }
                }
            }
        }
    }

    fn set(&mut self, x: i32, y: i32, t: Terrain) {
        if self.in_bounds(x, y) {
            let i = self.idx(x, y);
            self.terrain[i] = t;
        }
    }

    /// Height of the road line through cell (x, y) at the column's center, kept
    /// within the cell: where to draw anything that travels on the road.
    pub fn road_line_y(&self, x: i32, y: i32) -> Option<f32> {
        let road = self.roads.iter().find(|r| r.rows_in_column(x).contains(&y))?;
        let mid = (road.edge_y(x) + road.edge_y(x + 1)) / 2.0;
        Some(mid.clamp(y as f32, y as f32 + 1.0))
    }

    /// The stretch of road line inside cell (x, y), as two points in cell units,
    /// left to right: where troops on the road should stand.
    pub fn road_span(&self, x: i32, y: i32) -> Option<((f32, f32), (f32, f32))> {
        let road = self.roads.iter().find(|r| r.rows_in_column(x).contains(&y))?;
        let (a, b) = (road.edge_y(x), road.edge_y(x + 1));
        let (top, bottom) = (y as f32, y as f32 + 1.0);
        // Clip the line y = a + (b - a) t to this row; a steep road crosses several.
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        if (b - a).abs() > f32::EPSILON {
            let (ta, tb) = ((top - a) / (b - a), (bottom - a) / (b - a));
            t0 = ta.min(tb).max(0.0);
            t1 = ta.max(tb).min(1.0);
            if t1 <= t0 {
                (t0, t1) = (0.0, 1.0);
            }
        }
        let at = |t: f32| (x as f32 + t, (a + (b - a) * t).clamp(top, bottom));
        Some((at(t0), at(t1)))
    }

    /// The road cell under a point (in cell units) lying within `slack` rows of a
    /// road line. Lines run along cell edges, so a click on one is ambiguous.
    pub fn road_near(&self, fx: f32, fy: f32, slack: f32) -> Option<Cell> {
        let x = fx.floor() as i32;
        let t = fx - x as f32;
        self.roads.iter().find_map(|r| {
            let (x0, x1) = r.x_range();
            if x < x0 || x >= x1 {
                return None;
            }
            let ly = r.edge_y(x) * (1.0 - t) + r.edge_y(x + 1) * t;
            if (fy - ly).abs() > slack {
                return None;
            }
            let rows = r.rows_in_column(x);
            let y = (ly.floor() as i32).clamp(*rows.start(), *rows.end());
            Some((x, y))
        })
    }

    /// Roughly how many columns of marching it takes to get from `a` to `b` by road:
    /// along a lane is straight down it, and between lanes goes by whichever climbs
    /// and bridges make it shortest.
    pub fn road_distance(&self, a: Cell, b: Cell) -> i32 {
        // Junctions where links between lanes meet them: (lane, column) pairs,
        // two to a link, plus where we start and where we're going.
        let mut nodes: Vec<(usize, i32)> = vec![(lane(a.1), a.0), (lane(b.1), b.0)];
        let mut links = Vec::new();
        for r in &self.roads {
            let (x0, x1) = r.x_range();
            let (l0, l1) = (lane(r.y_at(x0 as f32) as i32), lane(r.y_at(x1 as f32) as i32));
            if l0 != l1 {
                let rise = (r.y_at(x1 as f32) - r.y_at(x0 as f32)).abs() as i32;
                links.push((nodes.len(), nodes.len() + 1, (x1 - x0).max(rise)));
                nodes.push((l0, x0));
                nodes.push((l1, x1));
            }
        }
        let n = nodes.len();
        let mut best = vec![i32::MAX; n];
        let mut done = vec![false; n];
        best[0] = 0;
        while let Some(i) = (0..n).filter(|&i| !done[i] && best[i] < i32::MAX).min_by_key(|&i| best[i]) {
            done[i] = true;
            if i == 1 {
                break;
            }
            for j in 0..n {
                let along = (nodes[j].0 == nodes[i].0).then(|| (nodes[j].1 - nodes[i].1).abs());
                let across = links.iter().find(|&&(p, q, _)| (p, q) == (i, j) || (q, p) == (i, j)).map(|l| l.2);
                if let Some(d) = [along, across].into_iter().flatten().min() {
                    best[j] = best[j].min(best[i] + d);
                }
            }
        }
        best[1]
    }

    /// Topmost road cell in column `x` of road `road`, for placing units on it.
    pub fn road_cell(&self, road: usize, x: i32) -> Cell {
        (x, *self.roads[road].rows_in_column(x).start())
    }

    /// Builds a valley: a mountain ridge along the back with foothills at its feet,
    /// a winding river across the front with flats beyond it, holdings at `sites`,
    /// and `roads` laid on top. Each stretch between two of the `anchors` (columns
    /// of the forts on the Imperial Road) gets its own country and name: deep
    /// woods, a mountain pass, hill country or open farmland. The country mirrors
    /// about the middle of the valley, so neither side's half is easier going.
    pub fn generate(seed: u64, sites: &[Site], anchors: &[i32], roads: Vec<Road>) -> Map {
        let (w, h) = (MAP_W, MAP_H);
        let mut rng = Rng::new(seed);
        let mut map = Map {
            w,
            h,
            terrain: vec![Terrain::Field; (w * h) as usize],
            road: vec![false; (w * h) as usize],
            roads,
            regions: Vec::new(),
        };

        // The ridge: a ragged wall of mountains along the back rows, with peaks
        // pushing down here and there and foothills at its foot.
        let mut ridge = 2;
        let mut ridge_at = vec![0; w as usize];
        for x in 0..w {
            if rng.chance(1, 4) {
                ridge = (ridge + rng.range(-1, 2)).clamp(1, 3);
            }
            ridge_at[x as usize] = ridge;
        }
        for _ in 0..8 {
            let cx = rng.range(0, w);
            let half = rng.range(2, 6);
            let depth = rng.range(1, 3);
            for dx in -half..=half {
                let x = cx + dx;
                if (0..w).contains(&x) {
                    let extra = depth * (half - dx.abs()) / half;
                    ridge_at[x as usize] += extra;
                }
            }
        }
        for x in 0..w {
            for y in 0..ridge_at[x as usize] {
                map.set(x, y, Terrain::Mountain);
            }
            let foot = ridge_at[x as usize];
            if rng.chance(1, 2) {
                map.set(x, foot, Terrain::Hill);
            }
            if rng.chance(1, 5) {
                map.set(x, foot + 1, Terrain::Hill);
            }
        }

        // Countries between the forts on the Imperial Road: each kind once before
        // any repeats, then mirrored onto the east half.
        let mut deck = [Country::Woodland, Country::Narrows, Country::Highlands, Country::Farmland];
        for k in (1..deck.len()).rev() {
            deck.swap(k, rng.range(0, k as i32 + 1) as usize);
        }
        let mut xs = anchors.to_vec();
        xs.sort();
        let bands = xs.len().saturating_sub(1);
        let mut countries: Vec<Country> = Vec::new();
        for k in 0..bands {
            let mirror = bands - 1 - k;
            let country = if mirror < k {
                countries[mirror]
            } else {
                match deck.get(k) {
                    Some(&c) => c,
                    None => loop {
                        let c = deck[rng.range(0, deck.len() as i32) as usize];
                        if Some(&c) != countries.last() {
                            break c;
                        }
                    },
                }
            };
            countries.push(country);
        }
        for (k, pair) in xs.windows(2).enumerate() {
            let (x0, x1) = (pair[0] + 4, pair[1] - 4);
            let country = countries[k];
            let names = country.names();
            let name = loop {
                let n = names[rng.range(0, names.len() as i32) as usize];
                if !map.regions.iter().any(|r| r.name == n) {
                    break n;
                }
            };
            map.regions.push(Region { name, country, x0, x1 });
            match country {
                Country::Woodland => map.woodland(&mut rng, x0, x1),
                Country::Narrows => map.narrows(&mut rng, x0, x1),
                Country::Highlands => map.highlands(&mut rng, x0, x1),
                Country::Farmland => map.farmland(&mut rng, x0, x1),
            }
            map.flats(&mut rng, x0, x1, country);
            // Copses within a few rows of the Imperial Road: somewhere to lie in
            // wait, even out in the open.
            if let Some(high_road) = map.roads.first().cloned() {
                let copses = if country == Country::Farmland { 1 } else { 2 };
                for _ in 0..copses {
                    let x = rng.range(x0 + 3, x1 - 2);
                    let row = *high_road.rows_in_column(x).start();
                    let cy = if rng.chance(1, 2) { row - 2 } else { row + 2 };
                    let (rx, ry) = (rng.range(2, 6), 1);
                    map.blob(&mut rng, (x, cy), (rx, ry), Terrain::Forest, &[Terrain::Field, Terrain::Hill]);
                }
            }
        }

        // The river: a gentle meander across the front of the valley, two cells
        // thick where it bends so it never leaves a diagonal gap.
        let mut prev_y: Option<i32> = None;
        for x in 0..w {
            let xf = x as f32;
            let mid = (RIVER_TOP + RIVER_BOTTOM) as f32 / 2.0;
            let y = (mid + 1.2 * (xf * 0.07).sin() + 0.5 * (xf * 0.19 + 1.3).sin()).round() as i32;
            let y = y.clamp(RIVER_TOP, RIVER_BOTTOM);
            map.set(x, y, Terrain::River);
            if let Some(py) = prev_y.filter(|&py| py != y) {
                map.set(x, py, Terrain::River);
            }
            prev_y = Some(y);
        }

        // Clear ground around each holding; forts get their 5×3 of walls.
        for site in sites {
            let (fx, fy) = site.pos;
            let (rx, ry) = if site.walled { (3, 2) } else { (2, 1) };
            for y in fy - ry..=fy + ry {
                for x in fx - rx..=fx + rx {
                    if map.in_bounds(x, y) && map.terrain(x, y) != Terrain::River {
                        map.set(x, y, Terrain::Field);
                    }
                }
            }
            if site.walled {
                for y in fy - 1..=fy + 1 {
                    for x in fx - 2..=fx + 2 {
                        map.set(x, y, Terrain::Fort);
                    }
                }
            }
        }

        // Roads: they clear the way through woods and rock, and bridge the river.
        for r in 0..map.roads.len() {
            let (x0, x1) = map.roads[r].x_range();
            for x in x0..x1 {
                for y in map.roads[r].rows_in_column(x) {
                    let i = map.idx(x, y);
                    map.road[i] = true;
                    if matches!(map.terrain[i], Terrain::Forest | Terrain::Mountain) {
                        map.terrain[i] = Terrain::Field;
                    }
                }
            }
        }
        map
    }
}
