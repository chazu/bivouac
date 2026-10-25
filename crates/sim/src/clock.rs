pub const MINUTES_PER_DAY: u32 = 1440;
pub const LUNAR_CYCLE_DAYS: u32 = 28;
/// The full moon (phase 14) of every other lunar cycle is a blood moon.
const BLOOD_MOON_EVERY: u32 = 2;

/// Game time. One sim tick is one game minute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    pub minute: u32,
}

impl Clock {
    pub fn day(&self) -> u32 {
        self.minute / MINUTES_PER_DAY
    }

    pub fn minute_of_day(&self) -> u32 {
        self.minute % MINUTES_PER_DAY
    }

    /// 0 = new moon, 14 = full.
    pub fn moon_phase(&self) -> u32 {
        self.day() % LUNAR_CYCLE_DAYS
    }

    pub fn is_blood_moon(&self) -> bool {
        (13..=15).contains(&self.moon_phase()) && (self.day() / LUNAR_CYCLE_DAYS).is_multiple_of(BLOOD_MOON_EVERY)
    }

    /// Minute of the day the moon rises: the new moon rises with the sun (06:00),
    /// the full moon at sunset (18:00), about 51 minutes later each day.
    pub fn moon_rise(&self) -> u32 {
        (360 + self.moon_phase() * MINUTES_PER_DAY / LUNAR_CYCLE_DAYS) % MINUTES_PER_DAY
    }

    pub fn moon_up(&self) -> bool {
        (self.minute_of_day() + MINUTES_PER_DAY - self.moon_rise()) % MINUTES_PER_DAY < 720
    }

    pub fn is_night(&self) -> bool {
        let m = self.minute_of_day();
        !(360..1080).contains(&m)
    }
}

pub fn hhmm(minute: u32) -> String {
    let m = minute % MINUTES_PER_DAY;
    format!("{:02}:{:02}", m / 60, m % 60)
}

pub fn moon_phase_name(phase: u32) -> &'static str {
    match phase {
        0 => "new moon",
        1..=6 => "waxing crescent",
        7 => "first quarter",
        8..=13 => "waxing gibbous",
        14 => "full moon",
        15..=20 => "waning gibbous",
        21 => "last quarter",
        _ => "waning crescent",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Cloudy,
    Rain,
    Fog,
}

impl Weather {
    pub fn label(self) -> &'static str {
        match self {
            Weather::Clear => "clear",
            Weather::Cloudy => "overcast",
            Weather::Rain => "rain",
            Weather::Fog => "fog",
        }
    }

    pub fn parse(s: &str) -> Option<Weather> {
        Some(match s {
            "clear" => Weather::Clear,
            "cloudy" | "overcast" => Weather::Cloudy,
            "rain" => Weather::Rain,
            "fog" => Weather::Fog,
            _ => return None,
        })
    }
}
