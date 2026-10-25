use macroquad::prelude::*;

/// A fixed-pitch character grid. Most characters come from the font; braille and
/// block elements are drawn as shapes so they tile seamlessly at any size.
pub struct Grid {
    font: Font,
    pub cw: f32,
    pub ch: f32,
    font_px: u16,
    font_scale: f32,
    baseline: f32,
}

impl Grid {
    pub fn new(font: Font) -> Grid {
        Grid { font, cw: 0.0, ch: 0.0, font_px: 0, font_scale: 1.0, baseline: 0.0 }
    }

    /// Sizes cells so `rows` rows fill the window height.
    pub fn fit(&mut self, rows: f32) {
        let ch = (screen_height() / rows).floor().max(10.0);
        if ch == self.ch {
            return;
        }
        let dpi = screen_dpi_scale();
        self.ch = ch;
        self.font_px = (ch * 0.78 * dpi).round() as u16;
        self.font_scale = 1.0 / dpi;
        self.cw = measure_text("M", Some(&self.font), self.font_px, self.font_scale).width.round().max(1.0);
        self.baseline = (ch * 0.76).round();
    }

    pub fn cols(&self) -> i32 {
        (screen_width() / self.cw).floor() as i32
    }

    pub fn rows(&self) -> i32 {
        (screen_height() / self.ch).floor() as i32
    }

    pub fn put(&self, col: i32, row: i32, c: char, color: Color) {
        self.put_px(col as f32 * self.cw, row as f32 * self.ch, c, color);
    }

    /// Draws `c` in the cell whose top-left corner is at pixel (x, y).
    pub fn put_px(&self, x: f32, y: f32, c: char, color: Color) {
        if c == ' ' {
            return;
        }
        if let Some(bits) = braille_bits(c) {
            self.braille(x, y, bits, color);
        } else if let Some(mask) = quadrant_mask(c) {
            self.quadrants(x, y, mask, color);
        } else if let Some(level) = shade_level(c) {
            self.shade(x, y, level, color);
        } else {
            let mut buf = [0u8; 4];
            let params = TextParams {
                font: Some(&self.font),
                font_size: self.font_px,
                font_scale: self.font_scale,
                color,
                ..Default::default()
            };
            draw_text_ex(c.encode_utf8(&mut buf), x, y + self.baseline, params);
        }
    }

    /// Draws at most `width` characters; returns how many columns were used.
    pub fn text_clip(&self, col: i32, row: i32, s: &str, width: i32, color: Color) -> i32 {
        let mut n = 0;
        for c in s.chars().take(width.max(0) as usize) {
            self.put(col + n, row, c, color);
            n += 1;
        }
        n
    }

    pub fn text(&self, col: i32, row: i32, s: &str, color: Color) -> i32 {
        self.text_clip(col, row, s, i32::MAX, color)
    }

    pub fn fill(&self, col: i32, row: i32, w: i32, h: i32, color: Color) {
        draw_rectangle(col as f32 * self.cw, row as f32 * self.ch, w as f32 * self.cw, h as f32 * self.ch, color);
    }

    fn braille(&self, x: f32, y: f32, bits: u8, color: Color) {
        // Unicode braille dot numbering: dots 1-3 and 7 down the left column, 4-6 and 8 down the right.
        const DOTS: [(f32, f32); 8] =
            [(0.0, 0.0), (0.0, 1.0), (0.0, 2.0), (1.0, 0.0), (1.0, 1.0), (1.0, 2.0), (0.0, 3.0), (1.0, 3.0)];
        let r = (self.cw * 0.17).max(1.3);
        for (i, (dx, dy)) in DOTS.iter().enumerate() {
            if bits & (1 << i) != 0 {
                draw_circle(x + self.cw * (0.28 + dx * 0.44), y + self.ch * (0.17 + dy * 0.22), r, color);
            }
        }
    }

    /// Light/medium/dark shade as an ordered dither of small square dots on a
    /// screen-wide grid, so neighbouring shaded cells join up without seams.
    fn shade(&self, x: f32, y: f32, level: u8, color: Color) {
        const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
        let s = (self.ch / 6.0).round().max(2.0);
        let (x1, y1) = (x + self.cw, y + self.ch);
        for j in (y / s).floor() as i32..(y1 / s).ceil() as i32 {
            for i in (x / s).floor() as i32..(x1 / s).ceil() as i32 {
                if BAYER[((j & 3) * 4 + (i & 3)) as usize] >= level * 4 {
                    continue;
                }
                let (rx0, ry0) = ((i as f32 * s).max(x), (j as f32 * s).max(y));
                let (rx1, ry1) = (((i + 1) as f32 * s).min(x1), ((j + 1) as f32 * s).min(y1));
                if rx1 > rx0 && ry1 > ry0 {
                    draw_rectangle(rx0, ry0, rx1 - rx0, ry1 - ry0, color);
                }
            }
        }
    }

    /// Small free-floating text centered on pixel `cx`, e.g. place names on the map.
    pub fn label(&self, cx: f32, y: f32, s: &str, color: Color) {
        let size = (self.font_px as f32 * 0.68).round() as u16;
        let dims = measure_text(s, Some(&self.font), size, self.font_scale);
        let x = cx - dims.width / 2.0;
        draw_rectangle(x - 3.0, y, dims.width + 6.0, dims.height + 4.0, Color::new(0.03, 0.03, 0.04, 0.7));
        let params = TextParams {
            font: Some(&self.font),
            font_size: size,
            font_scale: self.font_scale,
            color,
            ..Default::default()
        };
        draw_text_ex(s, x, y + dims.offset_y + 2.0, params);
    }

    fn quadrants(&self, x: f32, y: f32, mask: u8, color: Color) {
        let (w, h) = (self.cw, self.ch);
        let (hw, hh) = ((w / 2.0).round(), (h / 2.0).round());
        match mask {
            15 => draw_rectangle(x, y, w, h, color),
            3 => draw_rectangle(x, y, w, hh, color),
            12 => draw_rectangle(x, y + hh, w, h - hh, color),
            5 => draw_rectangle(x, y, hw, h, color),
            10 => draw_rectangle(x + hw, y, w - hw, h, color),
            _ => {
                let quads = [
                    (x, y, hw, hh),
                    (x + hw, y, w - hw, hh),
                    (x, y + hh, hw, h - hh),
                    (x + hw, y + hh, w - hw, h - hh),
                ];
                for (i, (qx, qy, qw, qh)) in quads.into_iter().enumerate() {
                    if mask & (1 << i) != 0 {
                        draw_rectangle(qx, qy, qw, qh, color);
                    }
                }
            }
        }
    }
}

pub fn braille_bits(c: char) -> Option<u8> {
    let u = c as u32;
    (0x2800..=0x28FF).contains(&u).then(|| (u - 0x2800) as u8)
}

pub fn braille_char(bits: u8) -> char {
    char::from_u32(0x2800 + bits as u32).unwrap()
}

/// Quadrant masks: 1 = top-left, 2 = top-right, 4 = bottom-left, 8 = bottom-right.
const QUADRANTS: [char; 16] = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];

fn quadrant_mask(c: char) -> Option<u8> {
    QUADRANTS.iter().position(|&q| q == c && q != ' ').map(|i| i as u8)
}

fn shade_level(c: char) -> Option<u8> {
    match c {
        '░' => Some(1),
        '▒' => Some(2),
        '▓' => Some(3),
        _ => None,
    }
}

pub fn quadrant_char(mask: u8) -> char {
    QUADRANTS[(mask & 15) as usize]
}

/// Braille patterns for 1..=8 companies, filling the cell from the top-left.
pub const DENSITY: [u8; 8] = [0x01, 0x03, 0x07, 0x47, 0xC7, 0xE7, 0xF7, 0xFF];
