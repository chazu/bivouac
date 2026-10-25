pub const MINUTES_PER_DAY: u32 = 1440;
/// Short enough that the moon visibly changes from one night to the next.
pub const LUNAR_CYCLE_DAYS: u32 = 8;
pub const FULL_MOON: u32 = LUNAR_CYCLE_DAYS / 2;
/// The campaign opens on the night of a full moon.
pub const MOON_OFFSET_DAYS: u32 = FULL_MOON;
/// The full moon of every other lunar cycle is a blood moon.
const BLOOD_MOON_EVERY: u32 = 2;

/// Game time. One sim tick is one game minute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    pub minute: u32,
}

impl Clock {
    /// Days since the campaign began, from 0. Shown to players counting from 1.
    pub fn day(&self) -> u32 {
        self.minute / MINUTES_PER_DAY
    }

    fn lunar_day(&self) -> u32 {
        self.day() + MOON_OFFSET_DAYS
    }

    pub fn minute_of_day(&self) -> u32 {
        self.minute % MINUTES_PER_DAY
    }

    /// 0 = new moon, [`FULL_MOON`] = full; one step a day.
    pub fn moon_phase(&self) -> u32 {
        self.lunar_day() % LUNAR_CYCLE_DAYS
    }

    pub fn is_blood_moon(&self) -> bool {
        self.moon_phase() == FULL_MOON && (self.lunar_day() / LUNAR_CYCLE_DAYS).is_multiple_of(BLOOD_MOON_EVERY)
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
        1 => "waxing crescent",
        2 => "first quarter",
        3 => "waxing gibbous",
        4 => "full moon",
        5 => "waning gibbous",
        6 => "last quarter",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_moon_changes_every_night_and_bleeds_every_other_cycle() {
        let at = |day: u32| Clock { minute: day * MINUTES_PER_DAY + 22 * 60 };
        let names: Vec<_> = (0..LUNAR_CYCLE_DAYS).map(|d| moon_phase_name(at(d).moon_phase())).collect();
        assert!(names.windows(2).all(|w| w[0] != w[1]));
        let blood: Vec<u32> = (0..40).filter(|&d| at(d).is_blood_moon()).collect();
        assert_eq!(blood, vec![0, 16, 32]);
    }
}
