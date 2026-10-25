use crate::Cell;
use crate::rng::Rng;

pub const MAP_W: i32 = 240;
/// Rows of the three-quarter view: row 0 is the far ridge, the last row is nearest.
pub const MAP_H: i32 = 14;

/// Road heights snap to quarter rows so the line the client draws and the
/// cells the sim walks are derived from the same numbers.
pub const ROAD_STEPS_PER_ROW: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Field,
    Forest,
    Mountain,
    River,
    Fort,
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

    /// Topmost road cell in column `x` of road `road`, for placing units on it.
    pub fn road_cell(&self, road: usize, x: i32) -> Cell {
        (x, *self.roads[road].rows_in_column(x).start())
    }

    /// Builds a valley: a mountain ridge along the back, forest stands, a river
    /// winding along the front, forts at `forts` (centers), and `roads` laid on top.
    pub fn generate(seed: u64, forts: &[Cell], roads: Vec<Road>) -> Map {
        let (w, h) = (MAP_W, MAP_H);
        let mut rng = Rng::new(seed);
        let mut map =
            Map { w, h, terrain: vec![Terrain::Field; (w * h) as usize], road: vec![false; (w * h) as usize], roads };

        // The far ridge, with spurs reaching down into the valley.
        let mut ridge = 2;
        let mut ridge_at = vec![0; w as usize];
        for x in 0..w {
            if rng.chance(1, 4) {
                ridge = (ridge + rng.range(-1, 2)).clamp(1, 3);
            }
            ridge_at[x as usize] = ridge;
        }
        for _ in 0..14 {
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
        }

        // Forest stands.
        for _ in 0..30 {
            let (cx, cy) = (rng.range(0, w), rng.range(3, h));
            let (rx, ry) = (rng.range(3, 11), rng.range(1, 3));
            for y in cy - ry..=cy + ry {
                for x in cx - rx..=cx + rx {
                    let (dx, dy) = ((x - cx) as f32 / rx as f32, (y - cy) as f32 / ry as f32);
                    let ragged = rng.range(0, 30) as f32 / 100.0;
                    if dx * dx + dy * dy <= 1.0 + ragged && map.in_bounds(x, y) && map.terrain(x, y) == Terrain::Field {
                        map.set(x, y, Terrain::Forest);
                    }
                }
            }
        }

        // The river. Where its row changes it widens at the corner, so armies can't
        // slip diagonally between two water cells.
        let mut prev_y: Option<i32> = None;
        for x in 0..w {
            let xf = x as f32;
            let y = (11.0 + 1.2 * (xf * 0.07).sin() + 0.5 * (xf * 0.19 + 1.3).sin()).round() as i32;
            let y = y.clamp(10, 12);
            map.set(x, y, Terrain::River);
            if let Some(py) = prev_y.filter(|&py| py != y) {
                map.set(x, py, Terrain::River);
            }
            prev_y = Some(y);
        }

        // Forts sit in cleared ground.
        for &(fx, fy) in forts {
            for y in fy - 2..=fy + 2 {
                for x in fx - 3..=fx + 3 {
                    if map.in_bounds(x, y) && map.terrain(x, y) != Terrain::River {
                        map.set(x, y, Terrain::Field);
                    }
                }
            }
            for y in fy - 1..=fy + 1 {
                for x in fx - 2..=fx + 2 {
                    map.set(x, y, Terrain::Fort);
                }
            }
        }

        // Roads cut through forest and over passes; over water they become bridges.
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
