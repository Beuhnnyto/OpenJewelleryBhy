//! Solid text (gvTextObjects / gvTextOnCurve): glyph outlines → polygons
//! with holes → triangulated caps → closed extrusions, optionally wrapped on
//! the ring rail. Fonts are embedded (DejaVu, permissive licence).

use crate::mesh::{signed_area_2d, Mesh};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

static SANS: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
static SERIF: &[u8] = include_bytes!("../assets/fonts/DejaVuSerif-Bold.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Font {
    Sans,
    Serif,
}

impl Font {
    pub const ALL: [Font; 2] = [Font::Sans, Font::Serif];
    pub fn name(self) -> &'static str {
        match self {
            Font::Sans => "Sans bold",
            Font::Serif => "Serif bold",
        }
    }
    fn data(self) -> &'static [u8] {
        match self {
            Font::Sans => SANS,
            Font::Serif => SERIF,
        }
    }
}

/// One filled region: outer contour (CCW) and holes (CW).
#[derive(Clone, Debug)]
pub struct Region {
    pub outer: Vec<DVec2>,
    pub holes: Vec<Vec<DVec2>>,
}

struct Builder {
    contours: Vec<Vec<DVec2>>,
    cur: Vec<DVec2>,
    scale: f64,
    offset: DVec2,
    steps: usize,
}

impl Builder {
    fn p(&self, x: f32, y: f32) -> DVec2 {
        DVec2::new(x as f64, y as f64) * self.scale + self.offset
    }
}

impl ttf_parser::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cur = vec![self.p(x, y)];
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cur.push(self.p(x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let a = *self.cur.last().unwrap();
        let (b, c) = (self.p(x1, y1), self.p(x, y));
        for i in 1..=self.steps {
            let t = i as f64 / self.steps as f64;
            self.cur.push(a * (1.0 - t).powi(2) + b * (2.0 * t * (1.0 - t)) + c * (t * t));
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let a = *self.cur.last().unwrap();
        let (b, c, d) = (self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        for i in 1..=self.steps {
            let t = i as f64 / self.steps as f64;
            let u = 1.0 - t;
            self.cur.push(a * u * u * u + b * 3.0 * u * u * t + c * 3.0 * u * t * t + d * t * t * t);
        }
    }
    fn close(&mut self) {
        let mut c = std::mem::take(&mut self.cur);
        if c.len() > 1 && c[0].distance(*c.last().unwrap()) < 1e-9 {
            c.pop();
        }
        c.dedup_by(|a, b| a.distance(*b) < 1e-9);
        if c.len() >= 3 {
            self.contours.push(c);
        }
    }
}

fn point_in_poly(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Lays out `text` (single line, centred on the origin, baseline at y = 0
/// shifted so the cap height is centred). `size` is the cap height in mm.
pub fn layout(text: &str, font: Font, size: f64, spacing: f64) -> Vec<Region> {
    let face = match ttf_parser::Face::parse(font.data(), 0) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let cap = face.capital_height().map(|c| c as f64).unwrap_or(face.units_per_em() as f64 * 0.72);
    let scale = size / cap;
    let steps = 6;
    let mut contours: Vec<Vec<Vec<DVec2>>> = Vec::new();
    let mut x = 0.0;
    for ch in text.chars() {
        let gid = face.glyph_index(ch).or_else(|| face.glyph_index('?'));
        let Some(gid) = gid else { continue };
        let mut b = Builder { contours: vec![], cur: vec![], scale, offset: DVec2::new(x, 0.0), steps };
        face.outline_glyph(gid, &mut b);
        contours.push(b.contours);
        x += face.glyph_hor_advance(gid).unwrap_or(0) as f64 * scale + spacing;
    }
    let width = (x - spacing).max(0.0);
    let shift = DVec2::new(-width / 2.0, -size / 2.0);
    let mut regions = Vec::new();
    // group per glyph: holes are contours contained in an odd number of others
    for glyph in contours {
        let glyph: Vec<Vec<DVec2>> = glyph.into_iter().map(|c| c.into_iter().map(|p| p + shift).collect()).collect();
        let depth: Vec<usize> = (0..glyph.len())
            .map(|i| (0..glyph.len()).filter(|&j| j != i && point_in_poly(glyph[i][0], &glyph[j])).count())
            .collect();
        let mut outers: Vec<Region> = Vec::new();
        let mut outer_idx = Vec::new();
        for (i, c) in glyph.iter().enumerate() {
            if depth[i] % 2 == 0 {
                let mut c = c.clone();
                if signed_area_2d(&c) < 0.0 {
                    c.reverse();
                }
                outer_idx.push(i);
                outers.push(Region { outer: c, holes: vec![] });
            }
        }
        for (i, c) in glyph.iter().enumerate() {
            if depth[i] % 2 == 1 {
                // innermost outer that contains it
                let owner = outer_idx
                    .iter()
                    .enumerate()
                    .filter(|(_, &oi)| point_in_poly(c[0], &glyph[oi]))
                    .max_by_key(|(_, &oi)| depth[oi])
                    .map(|(k, _)| k);
                if let Some(k) = owner {
                    let mut c = c.clone();
                    if signed_area_2d(&c) > 0.0 {
                        c.reverse();
                    }
                    outers[k].holes.push(c);
                }
            }
        }
        regions.extend(outers);
    }
    regions
}

/// Splits every contour edge longer than `max` (so wrapped text bends).
fn densify(c: &[DVec2], max: f64) -> Vec<DVec2> {
    let mut out = Vec::with_capacity(c.len() * 2);
    for i in 0..c.len() {
        let a = c[i];
        let b = c[(i + 1) % c.len()];
        let n = (a.distance(b) / max).ceil().max(1.0) as usize;
        for k in 0..n {
            out.push(a.lerp(b, k as f64 / n as f64));
        }
    }
    out
}

/// Conforming longest-edge refinement of a triangle soup sharing indices.
fn refine(pts: &mut Vec<DVec2>, tris: &mut Vec<[u32; 3]>, max: f64) {
    for _ in 0..12 {
        let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_default().push(ti);
            }
        }
        let mut long: Vec<((u32, u32), f64)> = edges
            .keys()
            .map(|e| (*e, pts[e.0 as usize].distance(pts[e.1 as usize])))
            .filter(|(_, l)| *l > max)
            .collect();
        if long.is_empty() {
            return;
        }
        long.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let mut touched = vec![false; tris.len()];
        let mut new_tris = Vec::new();
        for (e, _) in long {
            let owners = &edges[&e];
            if owners.iter().any(|t| touched[*t]) {
                continue;
            }
            let m = pts.len() as u32;
            pts.push((pts[e.0 as usize] + pts[e.1 as usize]) / 2.0);
            for &ti in owners {
                touched[ti] = true;
                let t = tris[ti];
                // rotate so that the split edge is (t0, t1)
                let k = (0..3).find(|&k| {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    (a.min(b), a.max(b)) == e
                });
                let Some(k) = k else { continue };
                let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                new_tris.push([a, m, c]);
                new_tris.push([m, b, c]);
            }
        }
        let mut kept: Vec<[u32; 3]> = tris.iter().enumerate().filter(|(i, _)| !touched[*i]).map(|(_, t)| *t).collect();
        kept.extend(new_tris);
        *tris = kept;
    }
}

/// Closed extrusion of text regions from z = 0 to z = depth.
/// `max_edge` > 0 densifies the mesh so it can later be bent.
pub fn extrude(regions: &[Region], depth: f64, max_edge: f64) -> Mesh {
    let mut m = Mesh::new();
    for r in regions {
        let dens = |c: &Vec<DVec2>| if max_edge > 0.0 { densify(c, max_edge) } else { c.clone() };
        let outer = dens(&r.outer);
        let holes: Vec<Vec<DVec2>> = r.holes.iter().map(dens).collect();
        let mut pts: Vec<DVec2> = outer.clone();
        let mut hole_idx = Vec::new();
        for h in &holes {
            hole_idx.push(pts.len());
            pts.extend_from_slice(h);
        }
        let flat: Vec<f64> = pts.iter().flat_map(|p| [p.x, p.y]).collect();
        let Ok(idx) = earcutr::earcut(&flat, &hole_idx, 2) else { continue };
        let mut tris: Vec<[u32; 3]> = idx.chunks_exact(3).map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect();
        // make caps CCW
        for t in &mut tris {
            let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
            if (b - a).perp_dot(c - a) < 0.0 {
                t.swap(1, 2);
            }
        }
        let boundary = pts.len();
        if max_edge > 0.0 {
            refine(&mut pts, &mut tris, max_edge * 1.5);
        }
        let base = m.positions.len() as u32;
        let n = pts.len() as u32;
        for p in &pts {
            m.positions.push(DVec3::new(p.x, p.y, 0.0));
        }
        for p in &pts {
            m.positions.push(DVec3::new(p.x, p.y, depth));
        }
        for t in &tris {
            m.triangles.push([base + n + t[0], base + n + t[1], base + n + t[2]]); // top, up
            m.triangles.push([base + t[0], base + t[2], base + t[1]]); // bottom, down
        }
        // side walls along every (original boundary) contour
        let mut start = 0usize;
        let mut ranges = vec![(0usize, outer.len())];
        start += outer.len();
        for h in &holes {
            ranges.push((start, h.len()));
            start += h.len();
        }
        debug_assert!(start == boundary);
        for (s, len) in ranges {
            for k in 0..len {
                let a = (s + k) as u32;
                let b = (s + (k + 1) % len) as u32;
                // contour direction keeps the material on the left: outward wall
                m.triangles.push([base + a, base + b, base + n + b]);
                m.triangles.push([base + a, base + n + b, base + n + a]);
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_are_closed_solids() {
        let r = layout("OB8", Font::Sans, 3.0, 0.1);
        assert!(r.iter().any(|x| !x.holes.is_empty()), "O / B / 8 have holes");
        let m = extrude(&r, 0.5, 0.0);
        let area: f64 = r
            .iter()
            .map(|x| signed_area_2d(&x.outer) + x.holes.iter().map(|h| signed_area_2d(h)).sum::<f64>())
            .sum();
        assert!((m.volume() - area * 0.5).abs() / (area * 0.5) < 1e-6, "{} vs {}", m.volume(), area * 0.5);
        // refined version has the same volume
        let m2 = extrude(&r, 0.5, 0.15);
        assert!((m2.volume() - m.volume()).abs() / m.volume() < 1e-6);
        assert!(m2.triangles.len() > m.triangles.len());
    }
}
