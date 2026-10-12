//! Headless software renderer: PNG previews / job-bag thumbnails without a
//! GPU (orthographic, z-buffered, 2× supersampled).

use crate::document::{Document, EvalResult};
use crate::mesh::Mesh;
use crate::ops::Role;
use glam::DVec3;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shading {
    Metal,
    Gem,
}

pub struct Item {
    pub mesh: Mesh,
    pub color: [f32; 3],
    pub shading: Shading,
    /// (refractive index, dispersion) for gems; ior 0 = opaque (pearl).
    pub optics: (f64, f64),
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Rotation around Z in degrees (0 = looking from the front, -Y).
    pub yaw: f64,
    /// Elevation in degrees (90 = top view).
    pub pitch: f64,
}

impl Default for View {
    fn default() -> Self {
        View { yaw: 35.0, pitch: 25.0 }
    }
}

impl View {
    pub fn basis(&self) -> (DVec3, DVec3, DVec3) {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        let eye = DVec3::new(y.sin() * p.cos(), -y.cos() * p.cos(), p.sin());
        let right = DVec3::Z.cross(eye).normalize_or(DVec3::X);
        let up = eye.cross(right).normalize();
        (eye, right, up)
    }
}

/// Collects the final model of a document as render items.
pub fn items(doc: &Document, r: &EvalResult) -> Vec<Item> {
    let metal = doc.alloy.color();
    let mut out = Vec::new();
    for (_, o) in r.final_outputs(doc) {
        let m = Mesh::merged(o.solids.iter().filter(|s| s.role == Role::Metal).map(|s| s.mesh.as_ref()));
        if !m.is_empty() {
            out.push(Item { mesh: m, color: metal, shading: Shading::Metal, optics: (0.0, 0.0) });
        }
        for g in &o.gems {
            out.push(Item { mesh: g.world_mesh(), color: g.spec.material.color(), shading: Shading::Gem, optics: g.spec.material.optics() });
        }
    }
    out
}

/// Renders to RGBA8.
pub fn render(items: &[Item], w: usize, h: usize, view: View) -> Vec<u8> {
    let ss = 2;
    let (sw, sh) = (w * ss, h * ss);
    let (eye, right, up) = view.basis();
    // fit
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for it in items {
        for p in &it.mesh.positions {
            let (x, y) = (p.dot(right), p.dot(up));
            lo = [lo[0].min(x), lo[1].min(y)];
            hi = [hi[0].max(x), hi[1].max(y)];
        }
    }
    let mut color = vec![[0f32; 3]; sw * sh];
    for yy in 0..sh {
        let t = yy as f32 / sh as f32;
        let c = [0.93 - 0.12 * t, 0.94 - 0.12 * t, 0.96 - 0.1 * t];
        for xx in 0..sw {
            color[yy * sw + xx] = c;
        }
    }
    if lo[0].is_finite() {
        let margin = 0.08;
        let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-6);
        let scale = (sw.min(sh) as f64) * (1.0 - 2.0 * margin) / span;
        let cx = (lo[0] + hi[0]) / 2.0;
        let cy = (lo[1] + hi[1]) / 2.0;
        let to_screen = |p: DVec3| -> (f64, f64, f64) {
            (
                sw as f64 / 2.0 + (p.dot(right) - cx) * scale,
                sh as f64 / 2.0 - (p.dot(up) - cy) * scale,
                p.dot(eye),
            )
        };
        let mut depth = vec![f64::NEG_INFINITY; sw * sh];
        let light = (eye * 0.6 + right * -0.35 + up * 0.75).normalize();
        let half = (light + eye).normalize();
        for it in items {
            let m = &it.mesh;
            let base = it.color;
            for ti in 0..m.triangles.len() {
                let [a, b, c] = m.tri(ti);
                let n = (b - a).cross(c - a);
                let nl = n.length();
                if nl < 1e-15 {
                    continue;
                }
                let mut n = n / nl;
                if n.dot(eye) < 0.0 {
                    if it.shading == Shading::Metal {
                        continue; // back face of a closed solid
                    }
                    n = -n;
                }
                let shade = shade(n, light, half, up, base, it.shading);
                let (p0, p1, p2) = (to_screen(a), to_screen(b), to_screen(c));
                let minx = p0.0.min(p1.0).min(p2.0).floor().max(0.0) as usize;
                let maxx = (p0.0.max(p1.0).max(p2.0).ceil() as isize).min(sw as isize - 1);
                let miny = p0.1.min(p1.1).min(p2.1).floor().max(0.0) as usize;
                let maxy = (p0.1.max(p1.1).max(p2.1).ceil() as isize).min(sh as isize - 1);
                if maxx < 0 || maxy < 0 {
                    continue;
                }
                let area = (p1.0 - p0.0) * (p2.1 - p0.1) - (p2.0 - p0.0) * (p1.1 - p0.1);
                if area.abs() < 1e-12 {
                    continue;
                }
                for y in miny..=maxy as usize {
                    for x in minx..=maxx as usize {
                        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                        let w0 = ((p1.0 - px) * (p2.1 - py) - (p2.0 - px) * (p1.1 - py)) / area;
                        let w1 = ((p2.0 - px) * (p0.1 - py) - (p0.0 - px) * (p2.1 - py)) / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                            continue;
                        }
                        let z = w0 * p0.2 + w1 * p1.2 + w2 * p2.2;
                        let i = y * sw + x;
                        if z > depth[i] {
                            depth[i] = z;
                            color[i] = shade;
                        }
                    }
                }
            }
        }
    }
    // downsample + gamma
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f32; 3];
            for dy in 0..ss {
                for dx in 0..ss {
                    let c = color[(y * ss + dy) * sw + x * ss + dx];
                    for k in 0..3 {
                        acc[k] += c[k];
                    }
                }
            }
            let o = (y * w + x) * 4;
            for k in 0..3 {
                let v = (acc[k] / (ss * ss) as f32).clamp(0.0, 1.0).powf(1.0 / 1.6);
                out[o + k] = (v * 255.0 + 0.5) as u8;
            }
            out[o + 3] = 255;
        }
    }
    out
}

fn shade(n: DVec3, light: DVec3, half: DVec3, up: DVec3, base: [f32; 3], s: Shading) -> [f32; 3] {
    let diff = n.dot(light).max(0.0) as f32;
    let env = (0.5 + 0.5 * n.dot(up)) as f32;
    match s {
        Shading::Metal => {
            let spec = n.dot(half).max(0.0).powf(60.0) as f32;
            let k = 0.18 + 0.5 * diff + 0.45 * env * env;
            [base[0] * k + spec * 0.8, base[1] * k + spec * 0.8, base[2] * k + spec * 0.75]
        }
        Shading::Gem => {
            let spec = n.dot(half).max(0.0).powf(25.0) as f32;
            // facet sparkle: alternate brightness by facet orientation
            let facet = ((n.x * 7.3 + n.y * 5.1 + n.z * 3.7).sin() * 0.5 + 0.5) as f32;
            let k = 0.25 + 0.35 * diff + 0.35 * facet;
            [base[0] * k + spec, base[1] * k + spec, base[2] * k + spec]
        }
    }
}

pub fn write_png(path: &Path, w: usize, h: usize, rgba: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())
}

/// The application icon (gold ring with a diamond), RGBA, `n`×`n` pixels.
pub fn app_icon(n: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; n * n * 4];
    let s = n as f32 / 64.0;
    let ss = 4; // supersampling for smooth edges
    for y in 0..n {
        for x in 0..n {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let fx = (x as f32 + (sx as f32 + 0.5) / ss as f32) / s;
                    let fy = (y as f32 + (sy as f32 + 0.5) / ss as f32) / s;
                    let r = ((fx - 32.0).powi(2) + (fy - 39.0).powi(2)).sqrt();
                    let (dx, dy) = (fx - 32.0, fy - 14.0);
                    let c = if dx.abs() / 12.0 + dy.abs() / 10.0 <= 1.0 {
                        if dy < -2.0 { [250.0, 252.0, 255.0] } else if dx < 0.0 { [185.0, 205.0, 240.0] } else { [215.0, 228.0, 252.0] }
                    } else if (r - 19.0).abs() < 4.8 {
                        let k = 1.0 - ((r - 19.0) / 4.8).abs() * 0.4 + if fy < 39.0 { 0.0 } else { -0.08 };
                        [245.0 * k, 182.0 * k, 58.0 * k]
                    } else {
                        continue;
                    };
                    acc[0] += c[0];
                    acc[1] += c[1];
                    acc[2] += c[2];
                    acc[3] += 255.0;
                }
            }
            let o = (y * n + x) * 4;
            let cov = acc[3] / 255.0;
            if cov > 0.0 {
                for k in 0..3 {
                    rgba[o + k] = (acc[k] / cov) as u8;
                }
                rgba[o + 3] = (acc[3] / (ss * ss) as f32) as u8;
            }
        }
    }
    rgba
}

/// Writes a Windows .ico with PNG-compressed images of several sizes.
pub fn write_ico(path: &Path, sizes: &[usize]) -> Result<(), String> {
    let mut pngs = Vec::new();
    for &n in sizes {
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, n as u32, n as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().map_err(|e| e.to_string())?;
            w.write_image_data(&app_icon(n)).map_err(|e| e.to_string())?;
        }
        pngs.push((n, buf));
    }
    let mut out = Vec::new();
    out.extend([0u8, 0, 1, 0]);
    out.extend((pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len() as u32;
    for (n, data) in &pngs {
        let d = if *n >= 256 { 0u8 } else { *n as u8 };
        out.extend([d, d, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &pngs {
        out.extend(data);
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}
