//! Glyph rasterizer built on signed distance fields: anti-aliased, crisp at
//! any size, no image assets. Pure Rust (no Win32) so build.rs can reuse it
//! to generate the .exe icon.

#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    Headset,
    Battery { level: u8, low: bool, charging: bool },
}

pub const DARK_FG: [f32; 3] = [0.10, 0.10, 0.10];
pub const LIGHT_FG: [f32; 3] = [1.0, 1.0, 1.0];
const RED: [f32; 3] = [0.91, 0.22, 0.22];
const GREEN: [f32; 3] = [0.25, 0.78, 0.35];
const HEADSET_OPACITY: f32 = 0.6;
/// The .exe icon has no theme to follow: a gray readable on light and dark.
#[allow(dead_code)] // used by build.rs
const APP_ICON_GRAY: [f32; 3] = [0.55, 0.55, 0.55];

/// Lightning bolt on a 32x32 design grid.
const BOLT: [(f32, f32); 6] = [(16.0, 7.5), (10.5, 17.0), (14.0, 17.0), (12.5, 24.5), (18.5, 15.0), (15.0, 15.0)];

pub fn draw(glyph: Glyph, size: i32, fg: [f32; 3]) -> Canvas {
    let s = size as f32;
    let mut c = Canvas::new(size as usize);
    match glyph {
        Glyph::Headset => headset(&mut c, s, fg, HEADSET_OPACITY),
        Glyph::Battery { level, low, charging } => battery(&mut c, 0.0, 0.0, s, fg, level, low, charging),
    }
    c
}

/// The .exe icon: the headset with a half-full battery between the ear cups.
#[allow(dead_code)] // used by build.rs
pub fn draw_app_icon(size: i32) -> Canvas {
    let s = size as f32;
    let mut c = Canvas::new(size as usize);
    headset(&mut c, s, APP_ICON_GRAY, 1.0);
    let bs = s * 0.45; // battery glyph size
    battery(&mut c, (s * 0.5 - bs * 0.5).round(), (s * 0.67 - bs * 0.5).round(), bs, APP_ICON_GRAY, 50, false, false);
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
#[allow(clippy::too_many_arguments)]
fn battery(c: &mut Canvas, ox: f32, oy: f32, s: f32, fg: [f32; 3], level: u8, low: bool, charging: bool) {
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
        let color = if charging { GREEN } else if low { RED } else { fg };
        c.fill(color, 1.0, |x, y| sd_box(x, y, il, it, fill_r, ib, h));
    }

    if charging {
        let bolt = BOLT.map(|(x, y)| (ox + x * s / 32.0, oy + y * s / 32.0));
        c.erase(|x, y| sd_polygon(x, y, &bolt) - t);
        c.fill(fg, 1.0, |x, y| sd_polygon(x, y, &bolt));
    }
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

/// Signed distance to a simple polygon (Inigo Quilez).
fn sd_polygon(x: f32, y: f32, v: &[(f32, f32)]) -> f32 {
    let mut d = (x - v[0].0).powi(2) + (y - v[0].1).powi(2);
    let mut sign = 1.0;
    let mut j = v.len() - 1;
    for i in 0..v.len() {
        let (ex, ey) = (v[j].0 - v[i].0, v[j].1 - v[i].1);
        let (wx, wy) = (x - v[i].0, y - v[i].1);
        let k = ((wx * ex + wy * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        let (bx, by) = (wx - ex * k, wy - ey * k);
        d = d.min(bx * bx + by * by);
        let c = (y >= v[i].1, y < v[j].1, ex * wy > ey * wx);
        if (c.0 && c.1 && c.2) || (!c.0 && !c.1 && !c.2) {
            sign = -sign;
        }
        j = i;
    }
    sign * d.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes every tray glyph (dark and light taskbar) plus the .exe icon at
    /// common sizes to `target/icon_preview.bgra` (header: width, height as
    /// u32 LE) for visual inspection.
    #[test]
    fn preview_sheet() {
        let glyphs = [
            Some(Glyph::Headset),
            Some(Glyph::Battery { level: 100, low: false, charging: false }),
            Some(Glyph::Battery { level: 60, low: false, charging: false }),
            Some(Glyph::Battery { level: 30, low: false, charging: false }),
            Some(Glyph::Battery { level: 10, low: true, charging: false }),
            Some(Glyph::Battery { level: 50, low: false, charging: true }),
            None, // .exe icon
        ];
        let sizes = [16, 20, 24, 32, 40, 48];
        let (cell, pad) = (48usize, 8usize);
        let (w, h) = (glyphs.len() * (cell + pad), sizes.len() * 2 * (cell + pad));
        let mut sheet = vec![0u32; w * h];
        for (row, fg) in [LIGHT_FG, DARK_FG].into_iter().enumerate() {
            for (si, &size) in sizes.iter().enumerate() {
                for (gi, glyph) in glyphs.iter().enumerate() {
                    let px = match glyph {
                        Some(g) => draw(*g, size, fg).bgra(),
                        None => draw_app_icon(size).bgra(),
                    };
                    let (ox, oy) = (gi * (cell + pad), (row * sizes.len() + si) * (cell + pad));
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
