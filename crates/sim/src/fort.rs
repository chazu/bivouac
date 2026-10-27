use crate::{Cell, Side};

/// Game minutes an army must hold an undefended fort to take it.
pub const CAPTURE_MINUTES: u32 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FortKind {
    Main,
    Keep,
    Mill,
    Village,
}

impl FortKind {
    pub fn label(self) -> &'static str {
        match self {
            FortKind::Main => "main fort",
            FortKind::Keep => "keep",
            FortKind::Mill => "supply depot",
            FortKind::Village => "village",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    WaterMill,
    DairyHerd(u32),
    Granary,
    Smokehouse,
    Watchtower,
    Palisade,
}

impl Extra {
    pub fn label(self) -> String {
        match self {
            Extra::WaterMill => "water mill".into(),
            Extra::DairyHerd(n) => format!("dairy herd ({n} cows)"),
            Extra::Granary => "granary".into(),
            Extra::Smokehouse => "smokehouse".into(),
            Extra::Watchtower => "watchtower".into(),
            Extra::Palisade => "palisade".into(),
        }
    }
}

pub const GOODS: [&str; 3] = ["grain", "cheese", "salt pork"];
const GRAIN: usize = 0;
const CHEESE: usize = 1;
const PORK: usize = 2;

#[derive(Clone, Debug)]
pub struct Fort {
    pub id: u32,
    pub name: &'static str,
    pub kind: FortKind,
    /// Center cell. The fort covers 5×3 cells around it.
    pub pos: Cell,
    pub owner: Option<Side>,
    /// Rations of grain, cheese and salt pork.
    pub stores: [u32; 3],
    pub extras: Vec<Extra>,
    pub garrison: u32,
    pub garrison_morale: u32,
    pub(crate) garrison_bleed: u32,
    pub(crate) capture: Option<(Side, u32)>,
}

impl Fort {
    pub fn contains(&self, (x, y): Cell) -> bool {
        (x - self.pos.0).abs() <= 2 && (y - self.pos.1).abs() <= 1
    }

    /// Inside the walls or right up against them: where the garrison can fight.
    pub fn within_reach(&self, (x, y): Cell) -> bool {
        (x - self.pos.0).abs() <= 3 && (y - self.pos.1).abs() <= 2
    }

    /// Whether the fort's gates are shut to `side`: a standing garrison keeps out
    /// everyone but its own.
    pub fn bars(&self, side: Side) -> bool {
        self.garrison > 0 && self.owner != Some(side)
    }

    /// Who is taking the fort and how many minutes they've held it.
    pub fn capture_progress(&self) -> Option<(Side, u32)> {
        self.capture
    }

    pub fn capacity(&self) -> u32 {
        let base = match self.kind {
            FortKind::Main => 4000,
            FortKind::Keep => 1500,
            FortKind::Mill => 1000,
            FortKind::Village => 800,
        };
        let granary = if self.extras.contains(&Extra::Granary) { 1000 } else { 0 };
        base + granary
    }

    pub fn total_stores(&self) -> u32 {
        self.stores.iter().sum()
    }

    pub fn daily_production(&self) -> [u32; 3] {
        let mut p = [0; 3];
        match self.kind {
            // A capital draws on the whole province behind it.
            FortKind::Main => p[GRAIN] += 2500,
            FortKind::Village => p[GRAIN] += 60,
            _ => {}
        }
        for e in &self.extras {
            match *e {
                Extra::WaterMill => p[GRAIN] += 180,
                Extra::DairyHerd(cows) => p[CHEESE] += cows * 5 / 2,
                Extra::Smokehouse => p[PORK] += 20,
                _ => {}
            }
        }
        p
    }

    /// Takes up to `want` rations, using what spoils soonest first.
    pub(crate) fn take(&mut self, mut want: u32) -> u32 {
        let mut got = 0;
        for g in [CHEESE, GRAIN, PORK] {
            let n = want.min(self.stores[g]);
            self.stores[g] -= n;
            want -= n;
            got += n;
        }
        got
    }

    /// Like [`Fort::take`], but keeps track of which goods were taken.
    pub(crate) fn take_goods(&mut self, mut want: u32) -> [u32; 3] {
        let mut got = [0; 3];
        for g in [CHEESE, GRAIN, PORK] {
            got[g] = want.min(self.stores[g]);
            self.stores[g] -= got[g];
            want -= got[g];
        }
        got
    }

    pub fn room(&self) -> u32 {
        self.capacity().saturating_sub(self.total_stores())
    }

    /// Stores what fits and hands back what doesn't.
    pub(crate) fn add(&mut self, goods: [u32; 3]) -> [u32; 3] {
        let mut left = [0; 3];
        for (g, n) in goods.into_iter().enumerate() {
            let fits = n.min(self.room());
            self.stores[g] += fits;
            left[g] = n - fits;
        }
        left
    }
}
