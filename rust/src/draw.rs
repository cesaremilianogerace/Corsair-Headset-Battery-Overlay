//! Glyph rasterizer built on signed distance fields: anti-aliased, crisp at
//! any size, no image assets. Pure Rust (no Win32) so build.rs can reuse it
//! to generate the .exe icon.

#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    Headset,
    Battery { level: u8, low: bool, charging: bool },
}

const DARK_FG: [f32; 3] = [0.10, 0.10, 0.10];
const LIGHT_FG: [f32; 3] = [1.0, 1.0, 1.0];
const RED: [f32; 3] = [0.91, 0.22, 0.22];
const GREEN: [f32; 3] = [0.25, 0.78, 0.35];
/// Battery percentage digits: yellow on a dark taskbar, amber on a light one.
const YELLOW: [f32; 3] = [1.0, 0.85, 0.10];
const AMBER: [f32; 3] = [0.62, 0.38, 0.0];
const HEADSET_OPACITY: f32 = 0.6;
/// Empty part of the level bar.
const TRACK_OPACITY: f32 = 0.3;
/// The .exe icon has no theme to follow: a gray readable on light and dark.
#[allow(dead_code)] // used by build.rs
const APP_ICON_GRAY: [f32; 3] = [0.55, 0.55, 0.55];

/// Seven-segment masks for 0-9; bit 0..6 = segments a..g.
const SEGMENTS: [u8; 10] = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F];
const DASH: u8 = 0x40; // segment g only

/// How the tray shows the battery level; pick one with `TRAY_STYLE`.
#[derive(Clone, Copy)]
#[allow(dead_code)] // the unused style stays available
pub enum TrayStyle {
    /// Percentage on top, small battery with the level underneath.
    NumberOverBattery,
    /// Two big digits ("00" = 100%, "--" = 0%) over a level bar.
    DigitsOverBar,
}

pub const TRAY_STYLE: TrayStyle = TrayStyle::NumberOverBattery;//DigitsOverBar; //TrayStyle::NumberOverBattery

/// Tray glyph; `light` = light taskbar (glyphs drawn dark).
pub fn draw(glyph: Glyph, size: i32, light: bool) -> Canvas {
    draw_styled(glyph, size, light, TRAY_STYLE)
}

fn draw_styled(glyph: Glyph, size: i32, light: bool, style: TrayStyle) -> Canvas {
    let s = size as f32;
    let fg = if light { DARK_FG } else { LIGHT_FG };
    let mut c = Canvas::new(size as usize);
    match glyph {
        Glyph::Headset => headset(&mut c, s, fg, HEADSET_OPACITY),
        Glyph::Battery { level, low, charging } => {
            let colors = Colors {
                fg,
                level: if charging { GREEN } else if low { RED } else { fg },
                digits: if light { AMBER } else { YELLOW },
            };
            match style {
                TrayStyle::NumberOverBattery => number_over_battery(&mut c, s, colors, level),
                TrayStyle::DigitsOverBar => digits_over_bar(&mut c, s, colors, level),
            }
        }
    }
    c
}

#[derive(Clone, Copy)]
struct Colors {
    fg: [f32; 3],
    /// Battery fill / bar: plain, low (red) or charging (green).
    level: [f32; 3],
    digits: [f32; 3],
}

/// The .exe icon: the headset with a half-full battery between the ear cups.
#[allow(dead_code)] // used by build.rs
pub fn draw_app_icon(size: i32) -> Canvas {
    let s = size as f32;
    let mut c = Canvas::new(size as usize);
    headset(&mut c, s, APP_ICON_GRAY, 1.0);
    let bs = s * 0.45; // battery glyph size
    battery(&mut c, (s * 0.5 - bs * 0.5).round(), (s * 0.67 - bs * 0.5).round(), bs, APP_ICON_GRAY, 50);
    c
}

fn headset(c: &mut Canvas, s: f32, fg: [f32; 3], opacity: f32) {
    let t = stroke(s);
    let u = s / 32.0;
    let (cx, cy, r) = (16.0 * u, 17.0 * u, 10.0 * u);
    c.fill(fg, opacity, |x, y| {
        let band = if y <= cy { ((x - cx).hypot(y - cy) - r).abs() - t / 2.0 } else { f32::MAX };
        let left = sd_box(x, y, 3.0 * u, 16.0 * u, 9.0 * u, 27.0 * u, 2.0 * u);
        let right = sd_box(x, y, 23.0 * u, 16.0 * u, 29.0 * u, 27.0 * u, 2.0 * u);
        band.min(left).min(right)
    });
}

/// Battery glyph in the `s`-sized square at (`ox`, `oy`). Anything already
/// on the canvas around it is cleared so it stays readable when overlaid.
#[allow(dead_code)] // used by build.rs
fn battery(c: &mut Canvas, ox: f32, oy: f32, s: f32, fg: [f32; 3], level: u8) {
    let t = stroke(s);
    let h = t / 2.0;
    // Edges snapped to whole pixels so the outline stays sharp at 16px.
    let (l, r) = (ox + (s * 0.06).round(), ox + (s * 0.84).round());
    let (top, bot) = (oy + (s * 0.27).round(), oy + (s * 0.73).round());
    let nub_r = (ox + (s * 0.94).round()).max(r + 1.0);
    let (nub_t, nub_b) = (oy + (s * 0.40).round(), oy + (s * 0.60).round());

    c.erase(|x, y| sd_box(x, y, l, top, nub_r, bot, t) - t);
    c.fill(fg, 1.0, |x, y| sd_box(x, y, l + h, top + h, r - h, bot - h, t).abs() - h);
    c.fill(fg, 1.0, |x, y| sd_box(x, y, r, nub_t, nub_r, nub_b, h));

    if level > 0 {
        let (il, it, ir, ib) = (l + 2.0 * t, top + 2.0 * t, r - 2.0 * t, bot - 2.0 * t);
        // Never thinner than one stroke, so low levels stay visible.
        let fill_r = il + ((ir - il) * level as f32 / 100.0).max(t);
        c.fill(fg, 1.0, |x, y| sd_box(x, y, il, it, fill_r, ib, h));
    }
}

/// Percentage across the top, a short battery along the bottom edge.
fn number_over_battery(c: &mut Canvas, s: f32, colors: Colors, level: u8) {
    let t = stroke(s);
    let h = t / 2.0;
    let nub_w = (s / 16.0).round().max(1.0);
    let (l, r) = (0.0, s - nub_w);
    let (top, bot) = (s - (s * 0.375).round(), s);
    let inset = (bot - top) * 0.3;
    let (nub_t, nub_b) = ((top + inset).round(), (bot - inset).round());

    c.fill(colors.fg, 1.0, |x, y| sd_box(x, y, l + h, top + h, r - h, bot - h, t).abs() - h);
    c.fill(colors.fg, 1.0, |x, y| sd_box(x, y, r, nub_t, s, nub_b, 0.0));
    if level > 0 {
        // Gap between outline and fill only when there is room for it.
        let gap = if bot - top - 4.0 * t >= 2.0 { t } else { 0.0 };
        let (il, it, ir, ib) = (l + t + gap, top + t + gap, r - t - gap, bot - t - gap);
        // Never thinner than one stroke, so low levels stay visible.
        let fill_r = il + ((ir - il) * level as f32 / 100.0).max(t);
        c.fill(colors.level, 1.0, |x, y| sd_box(x, y, il, it, fill_r, ib, 0.0));
    }

    let room = top - t;
    let (ds, dh) = digit_metrics(room, 8.0);
    let dw = (dh * 0.6).round();
    // A narrow "1" so "100" fits at 16px.
    let cells: Vec<(u8, f32)> = level
        .to_string()
        .bytes()
        .map(|b| (b - b'0') as usize)
        .map(|d| (SEGMENTS[d], if d == 1 { ds } else { dw }))
        .collect();
    let y0 = ((room - dh) / 2.0).floor();
    draw_digits(c, colors.digits, &cells, s, y0, dh, ds, ds);
}

/// Two digits as large as possible over a full-width level bar.
fn digits_over_bar(c: &mut Canvas, s: f32, colors: Colors, level: u8) {
    let bar_h = (s / 8.0).round().max(2.0);
    let bar_t = s - bar_h;
    c.fill(colors.fg, TRACK_OPACITY, |x, y| sd_box(x, y, 0.0, bar_t, s, s, 0.0));
    if level > 0 {
        let fill_r = (s * level as f32 / 100.0).max(1.0);
        c.fill(colors.level, 1.0, |x, y| sd_box(x, y, 0.0, bar_t, fill_r, s, 0.0));
    }

    let room = bar_t - stroke(s);
    let (ds, dh) = digit_metrics(room, 7.0);
    let spacing = (ds - 1.0).max(1.0);
    let dw = (dh * 0.6).round().min(((s - spacing) / 2.0).floor());
    let masks = match level {
        0 => [DASH, DASH],
        100.. => [SEGMENTS[0], SEGMENTS[0]],
        n => [SEGMENTS[(n / 10) as usize], SEGMENTS[(n % 10) as usize]],
    };
    let y0 = ((room - dh) / 2.0).floor();
    draw_digits(c, colors.digits, &masks.map(|m| (m, dw)), s, y0, dh, ds, spacing);
}

/// Stroke and height for digits filling `room` pixels: the height is
/// adjusted so the middle bar lands on whole pixels.
fn digit_metrics(room: f32, stroke_ratio: f32) -> (f32, f32) {
    let ds = (room / stroke_ratio).round().max(1.0);
    let dh = if (room - ds) % 2.0 == 0.0 { room } else { room - 1.0 };
    (ds, dh)
}

/// Seven-segment `cells` (mask, width), centered horizontally in the icon.
#[allow(clippy::too_many_arguments)]
fn draw_digits(c: &mut Canvas, color: [f32; 3], cells: &[(u8, f32)], s: f32, y0: f32, dh: f32, ds: f32, spacing: f32) {
    let total = cells.iter().map(|&(_, w)| w).sum::<f32>() + spacing * (cells.len() - 1) as f32;
    let mut x0 = ((s - total) / 2.0).round();
    let placed: Vec<(u8, f32, f32)> = cells
        .iter()
        .map(|&(mask, w)| {
            let cell = (mask, x0, w);
            x0 += w + spacing;
            cell
        })
        .collect();
    c.fill(color, 1.0, |x, y| {
        placed.iter().fold(f32::MAX, |m, &(mask, x0, w)| m.min(sd_segments(x, y, mask, x0, y0, w, dh, ds)))
    });
}

/// Signed distance to the seven-segment `mask` with its top-left at (`x0`, `y0`).
#[allow(clippy::too_many_arguments)]
fn sd_segments(x: f32, y: f32, mask: u8, x0: f32, y0: f32, w: f32, h: f32, ds: f32) -> f32 {
    let (x1, y1) = (x0 + w, y0 + h);
    let ym = y0 + (h - ds) / 2.0; // top of the middle bar
    let segments = [
        (x0, y0, x1, y0 + ds),      // a: top
        (x1 - ds, y0, x1, ym + ds), // b: upper right
        (x1 - ds, ym, x1, y1),      // c: lower right
        (x0, y1 - ds, x1, y1),      // d: bottom
        (x0, ym, x0 + ds, y1),      // e: lower left
        (x0, y0, x0 + ds, ym + ds), // f: upper left
        (x0, ym, x1, ym + ds),      // g: middle
    ];
    segments
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & (1 << i) != 0)
        .fold(f32::MAX, |m, (_, &(l, t, r, b))| m.min(sd_box(x, y, l, t, r, b, 0.0)))
}

/// Stroke width in whole pixels.
fn stroke(s: f32) -> f32 {
    (s / 16.0).round().max(1.0)
}

/// Premultiplied RGBA float canvas.
pub struct Canvas {
    pub size: usize,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(size: usize) -> Self {
        Self { size, px: vec![[0.0; 4]; size * size] }
    }

    fn for_each(&mut self, sdf: impl Fn(f32, f32) -> f32, mut f: impl FnMut(&mut [f32; 4], f32)) {
        for y in 0..self.size {
            for x in 0..self.size {
                let a = coverage(sdf(x as f32 + 0.5, y as f32 + 0.5));
                if a > 0.0 {
                    f(&mut self.px[y * self.size + x], a);
                }
            }
        }
    }

    fn fill(&mut self, rgb: [f32; 3], opacity: f32, sdf: impl Fn(f32, f32) -> f32) {
        self.for_each(sdf, |p, a| {
            let a = a * opacity;
            for i in 0..3 {
                p[i] = rgb[i] * a + p[i] * (1.0 - a);
            }
            p[3] = a + p[3] * (1.0 - a);
        });
    }

    fn erase(&mut self, sdf: impl Fn(f32, f32) -> f32) {
        self.for_each(sdf, |p, a| p.iter_mut().for_each(|v| *v *= 1.0 - a));
    }

    /// Straight (non-premultiplied) pixels as [r, g, b, a] bytes.
    pub fn rgba(&self) -> Vec<[u8; 4]> {
        let to_u8 = |v: f32| (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        self.px
            .iter()
            .map(|&[r, g, b, a]| {
                let ch = |v: f32| if a > 0.0 { to_u8(v / a) } else { 0 };
                [ch(r), ch(g), ch(b), to_u8(a)]
            })
            .collect()
    }

    /// Straight BGRA packed as 0xAARRGGBB, as Win32 icons expect.
    pub fn bgra(&self) -> Vec<u32> {
        self.rgba().iter().map(|&[r, g, b, a]| u32::from_le_bytes([b, g, r, a])).collect()
    }
}

fn coverage(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// Signed distance to a rounded rectangle given by its edges.
fn sd_box(x: f32, y: f32, l: f32, t: f32, r: f32, b: f32, radius: f32) -> f32 {
    let (hx, hy) = ((r - l) / 2.0, (b - t) / 2.0);
    let radius = radius.min(hx).min(hy);
    let qx = (x - (l + r) / 2.0).abs() - hx + radius;
    let qy = (y - (t + b) / 2.0).abs() - hy + radius;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes the tray glyphs in both styles (dark and light taskbar) plus the
    /// .exe icon at common sizes to `target/icon_preview.bgra` (header:
    /// width, height as u32 LE) for visual inspection.
    #[test]
    fn preview_sheet() {
        let levels = [
            Glyph::Battery { level: 100, low: false, charging: false },
            Glyph::Battery { level: 87, low: false, charging: false },
            Glyph::Battery { level: 42, low: false, charging: false },
            Glyph::Battery { level: 9, low: true, charging: false },
            Glyph::Battery { level: 56, low: false, charging: true },
            Glyph::Battery { level: 0, low: true, charging: false },
        ];
        let mut columns: Vec<Option<(Glyph, TrayStyle)>> = vec![Some((Glyph::Headset, TRAY_STYLE))];
        for style in [TrayStyle::NumberOverBattery, TrayStyle::DigitsOverBar] {
            columns.extend(levels.iter().map(|&g| Some((g, style))));
        }
        columns.push(None); // .exe icon

        let sizes = [16, 20, 24, 32, 40, 48];
        let (cell, pad) = (48usize, 8usize);
        let (w, h) = (columns.len() * (cell + pad), sizes.len() * 2 * (cell + pad));
        let mut sheet = vec![0u32; w * h];
        for (row, light) in [false, true].into_iter().enumerate() {
            for (si, &size) in sizes.iter().enumerate() {
                for (ci, column) in columns.iter().enumerate() {
                    let px = match column {
                        Some((g, style)) => draw_styled(*g, size, light, *style).bgra(),
                        None => draw_app_icon(size).bgra(),
                    };
                    let (ox, oy) = (ci * (cell + pad), (row * sizes.len() + si) * (cell + pad));
                    for y in 0..size as usize {
                        for x in 0..size as usize {
                            sheet[(oy + y) * w + ox + x] = px[y * size as usize + x];
                        }
                    }
                }
            }
        }
        let mut out = Vec::with_capacity(8 + sheet.len() * 4);
        out.extend((w as u32).to_le_bytes());
        out.extend((h as u32).to_le_bytes());
        sheet.iter().for_each(|p| out.extend(p.to_le_bytes()));
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/target/icon_preview.bgra"), out).unwrap();
    }
}
