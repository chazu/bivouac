use bivouac_sim::clock::{LUNAR_CYCLE_DAYS, MINUTES_PER_DAY};
use bivouac_sim::{Sim, Weather};
use macroquad::prelude::Color;

use crate::palette::{mix, rgb, smoothstep};

/// A sun or moon: `x` runs 0 (west, left) to 1 (east, right); `alt` is negative below the horizon.
#[derive(Clone, Copy)]
pub struct Body {
    pub x: f32,
    pub alt: f32,
}

impl Body {
    /// Rises in the east at `rise` (minute of day) and sets in the west 12 hours later.
    fn at(minute_of_day: f32, rise: f32) -> Body {
        let p = ((minute_of_day - rise).rem_euclid(MINUTES_PER_DAY as f32)) / 720.0;
        Body { x: 1.0 - p, alt: (p * std::f32::consts::PI).sin() }
    }

    pub fn up(&self) -> bool {
        self.alt > 0.0
    }
}

/// Weather visuals ease toward the sim's weather instead of snapping.
pub struct WeatherFx {
    pub cloud: f32,
    pub fog: f32,
    pub rain: f32,
}

fn targets(w: Weather) -> (f32, f32, f32) {
    match w {
        Weather::Clear => (0.12, 0.0, 0.0),
        Weather::Cloudy => (0.65, 0.0, 0.0),
        Weather::Rain => (0.9, 0.15, 1.0),
        Weather::Fog => (0.35, 1.0, 0.0),
    }
}

impl WeatherFx {
    pub fn new(sim: &Sim) -> WeatherFx {
        let (cloud, fog, rain) = targets(sim.weather);
        WeatherFx { cloud, fog, rain }
    }

    pub fn update(&mut self, sim: &Sim, dt: f32) {
        let (c, f, r) = targets(sim.weather);
        let k = (dt * 0.4).min(1.0);
        self.cloud += (c - self.cloud) * k;
        self.fog += (f - self.fog) * k;
        self.rain += (r - self.rain) * k;
    }
}

/// Everything the renderer needs to know about light and sky for one frame.
pub struct Light {
    pub sun: Body,
    pub moon: Body,
    /// 0 at night, 1 in full day.
    pub day: f32,
    /// Continuous lunar phase, 0..28 (14 = full).
    pub phase: f32,
    pub illum: f32,
    pub blood: bool,
    /// Strength of the blood moon's red light on the land, 0..1.
    pub red: f32,
    /// Overall brightness of the land, 0..1.
    pub level: f32,
    pub cloud: f32,
    pub fog: f32,
    pub rain: f32,
}

impl Light {
    /// `minute` is fractional so the sun and moon glide between ticks.
    pub fn compute(sim: &Sim, minute: f32, fx: &WeatherFx) -> Light {
        let mod_ = minute.rem_euclid(MINUTES_PER_DAY as f32);
        let sun = Body::at(mod_, 360.0);
        let phase = (minute / MINUTES_PER_DAY as f32).rem_euclid(LUNAR_CYCLE_DAYS as f32);
        let moon_rise =
            (360.0 + phase * MINUTES_PER_DAY as f32 / LUNAR_CYCLE_DAYS as f32).rem_euclid(MINUTES_PER_DAY as f32);
        let moon = Body::at(mod_, moon_rise);
        let illum = (1.0 - (std::f32::consts::TAU * phase / LUNAR_CYCLE_DAYS as f32).cos()) / 2.0;
        let day = smoothstep(-0.12, 0.22, sun.alt);
        let moonlight = if moon.up() { illum * smoothstep(0.0, 0.3, moon.alt) * (1.0 - fx.cloud * 0.85) } else { 0.0 };
        // Night stays readable: darkness is mood here, not a gameplay handicap (yet).
        let level = day * (1.0 - fx.cloud * 0.25) + (1.0 - day) * (0.6 + 0.3 * moonlight);
        let blood = sim.clock.is_blood_moon();
        let red = if blood && moon.up() {
            (1.0 - day) * smoothstep(0.0, 0.25, moon.alt) * (1.0 - fx.cloud * 0.6)
        } else {
            0.0
        };
        Light { sun, moon, day, phase, illum, blood, red, level, cloud: fx.cloud, fog: fx.fog, rain: fx.rain }
    }

    /// Lights a land color: darkens and blues it at night, washes it red under a
    /// blood moon. `depth` runs 0 (far) to 1 (near); far rows sit in a little haze.
    pub fn lit(&self, c: Color, depth: f32) -> Color {
        let night = 1.0 - self.day;
        let k = self.level * (0.75 + 0.25 * depth);
        let mut out = Color { r: c.r * k, g: c.g * k, b: c.b * k, a: c.a };
        out = mix(out, Color { r: out.r * 0.6, g: out.g * 0.8, b: out.b * 1.25 + 0.02, a: c.a }, night * 0.6);
        out = mix(out, Color { r: out.r * 1.4 + 0.05, g: out.g * 0.6, b: out.b * 0.55, a: c.a }, self.red * 0.5);
        let grey = (out.r + out.g + out.b) / 3.0;
        mix(out, Color { r: grey, g: grey, b: grey, a: c.a }, self.rain * 0.2)
    }

    /// Like [`Light::lit`] but keeps important things (troops, forts) readable at night.
    pub fn lit_bright(&self, c: Color) -> Color {
        let l = self.lit(c, 1.0);
        mix(l, c, 0.55)
    }

    pub fn sky(&self, v: f32) -> Color {
        let day = mix(rgb(0.32, 0.52, 0.82), rgb(0.64, 0.77, 0.90), v);
        let night = mix(rgb(0.01, 0.01, 0.04), rgb(0.05, 0.04, 0.10), v);
        let dusk = mix(rgb(0.16, 0.10, 0.28), rgb(0.95, 0.45, 0.22), v * v);
        let twilight = (1.0 - self.sun.alt.abs() / 0.28).clamp(0.0, 1.0);
        let mut c = mix(mix(night, day, self.day), dusk, twilight * 0.85);
        c = mix(c, mix(rgb(0.07, 0.0, 0.01), rgb(0.30, 0.02, 0.03), v), self.red * 0.8);
        let grey = (c.r + c.g + c.b) / 3.0;
        c = mix(c, rgb(grey, grey, grey * 1.05), self.cloud * 0.55);
        crate::palette::scale(c, 1.0 - self.rain * 0.35)
    }
}
