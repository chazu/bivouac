mod app;
mod grid;
mod hud;
mod light;
mod mapview;
mod palette;
mod sky;

use bivouac_sim::{Side, Sim, Weather};
use macroquad::prelude::*;

use app::{App, Layout, Selection};
use grid::Grid;
use light::{Light, WeatherFx};

const SEED: u64 = 0x0B1B_00AC;
/// Sim ticks (game minutes) per real second at ×1, so a game hour takes 10 seconds.
/// The real time scale is still an open design question.
const TICKS_PER_SECOND: f32 = 6.0;
/// Most ticks run in one frame, so a slow frame at ×8 can't spiral.
const MAX_TICKS_PER_FRAME: u32 = 200;
const FONT: &[u8] = include_bytes!("../../../assets/fonts/DejaVuSansMono.ttf");

fn window_conf() -> Conf {
    Conf {
        window_title: "Bivouac".into(),
        window_width: 1600,
        window_height: 1000,
        high_dpi: true,
        sample_count: 4,
        ..Default::default()
    }
}

/// Command-line options, mostly for development:
/// `--screenshot out.png` renders a few frames, saves the window and exits;
/// `--advance N` runs N game minutes first; `--weather rain|fog|cloudy|clear`;
/// `--scroll X`; `--select ID`; `--collapse-sky`.
#[derive(Default)]
struct Args {
    screenshot: Option<String>,
    advance: u32,
    weather: Option<Weather>,
    scroll: Option<f32>,
    select: Option<u32>,
    collapse_sky: bool,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_default();
        match flag.as_str() {
            "--screenshot" => args.screenshot = Some(value()),
            "--advance" => args.advance = value().parse().unwrap_or(0),
            "--weather" => args.weather = Weather::parse(&value()),
            "--scroll" => args.scroll = value().parse().ok(),
            "--select" => args.select = value().parse().ok(),
            "--collapse-sky" => args.collapse_sky = true,
            other => eprintln!("ignoring unknown argument {other}"),
        }
    }
    args
}

/// One game minute: the AI gets to issue orders, then the sim advances.
fn step(sim: &mut Sim) {
    for order in bivouac_ai::think(sim, Side::East) {
        // A refused AI order (say, no route) is simply dropped; it will try again.
        let _ = sim.apply(Side::East, order);
    }
    sim.tick();
}

#[macroquad::main(window_conf)]
async fn main() {
    let args = parse_args();
    let mut font = load_ttf_font_from_bytes(FONT).expect("bundled font loads");
    font.set_filter(FilterMode::Linear);
    let mut grid = Grid::new(font);

    let mut sim = Sim::new(SEED);
    for _ in 0..args.advance {
        step(&mut sim);
    }
    if let Some(w) = args.weather {
        sim.set_weather(w);
    }
    let mut app = App::new(Side::West);
    app.sky_collapsed = args.collapse_sky;
    app.selected = args.select.map(Selection::Army);
    let mut fx = WeatherFx::new(&sim);

    grid.fit(app::rows_to_fit(app.sky_collapsed));
    let layout = Layout::new(&grid, app.sky_collapsed);
    match args.scroll {
        Some(x) => app.scroll = x,
        None => app.center_on(sim.forts[1].pos.0 - 10, &layout),
    }

    let mut pending = 0.0f32;
    let mut frame = 0u32;
    loop {
        grid.fit(app::rows_to_fit(app.sky_collapsed));
        let layout = Layout::new(&grid, app.sky_collapsed);
        app.handle_input(&mut sim, &grid, &layout);

        let dt = get_frame_time().min(0.1);
        if !app.paused && args.screenshot.is_none() {
            pending += dt * app.speed() as f32 * TICKS_PER_SECOND;
            let mut n = 0;
            while pending >= 1.0 && n < MAX_TICKS_PER_FRAME {
                step(&mut sim);
                pending -= 1.0;
                n += 1;
            }
            pending = pending.min(1.0);
        }
        fx.update(&sim, dt);

        let t = get_time() as f32;
        let light = Light::compute(&sim, sim.clock.minute as f32 + pending, &fx);
        clear_background(BLACK);
        sky::draw(&grid, &layout, &light, &sim, t);
        mapview::draw(&grid, &layout, &app, &sim, &light, t);
        sky::draw_overlays(&grid, &layout, &light, &sim, t);
        hud::draw(&grid, &layout, &app, &sim, &light);

        frame += 1;
        if let Some(path) = &args.screenshot
            && frame == 10
        {
            get_screen_data().export_png(path);
            break;
        }
        next_frame().await;
    }
}
