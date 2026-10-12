//! Girdle outlines of gem shapes (closed CCW polygons in the gem's XY plane).

use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Round,
    Oval,
    Cushion,
    Princess,
    Emerald,
    Asscher,
    Radiant,
    Octagon,
    Baguette,
    TaperedBaguette,
    Marquise,
    Pear,
    Heart,
    Trillion,
    TrillionCurved,
    Triangle,
    HalfMoon,
    Hexagon,
    Lozenge,
    Trapezoid,
    Coffin,
    /// Custom gem builder: any closed curve (points in arbitrary units,
    /// normalised to the requested L x W).
    Custom(Vec<[f64; 2]>),
}

impl Shape {
    pub const BUILTIN: [Shape; 21] = [
        Shape::Round,
        Shape::Oval,
        Shape::Cushion,
        Shape::Princess,
        Shape::Emerald,
        Shape::Asscher,
        Shape::Radiant,
        Shape::Octagon,
        Shape::Baguette,
        Shape::TaperedBaguette,
        Shape::Marquise,
        Shape::Pear,
        Shape::Heart,
        Shape::Trillion,
        Shape::TrillionCurved,
        Shape::Triangle,
        Shape::HalfMoon,
        Shape::Hexagon,
        Shape::Lozenge,
        Shape::Trapezoid,
        Shape::Coffin,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Shape::Round => "Round",
            Shape::Oval => "Oval",
            Shape::Cushion => "Cushion",
            Shape::Princess => "Princess",
            Shape::Emerald => "Emerald",
            Shape::Asscher => "Asscher",
            Shape::Radiant => "Radiant",
            Shape::Octagon => "Octagon",
            Shape::Baguette => "Baguette",
            Shape::TaperedBaguette => "Tapered baguette",
            Shape::Marquise => "Marquise",
            Shape::Pear => "Pear",
            Shape::Heart => "Heart",
            Shape::Trillion => "Trillion",
            Shape::TrillionCurved => "Trillion (curved)",
            Shape::Triangle => "Triangle",
            Shape::HalfMoon => "Half moon",
            Shape::Hexagon => "Hexagon",
            Shape::Lozenge => "Lozenge",
            Shape::Trapezoid => "Trapezoid",
            Shape::Coffin => "Coffin",
            Shape::Custom(_) => "Custom",
        }
    }

    pub fn from_name(s: &str) -> Option<Shape> {
        let k = s.to_ascii_lowercase().replace([' ', '-', '_'], "");
        Shape::BUILTIN.iter().find(|sh| sh.name().to_ascii_lowercase().replace([' ', '(', ')'], "") == k
            || format!("{:?}", sh).to_ascii_lowercase() == k).cloned()
    }

    /// Step-cut shapes look wrong with brilliant faceting.
    pub fn prefers_step_cut(&self) -> bool {
        matches!(
            self,
            Shape::Emerald | Shape::Asscher | Shape::Baguette | Shape::TaperedBaguette | Shape::Trapezoid | Shape::Lozenge
        )
    }

    /// Default length / width ratio for a new gem of this shape.
    pub fn default_ratio(&self) -> f64 {
        match self {
            Shape::Oval => 1.4,
            Shape::Emerald | Shape::Radiant => 1.4,
            Shape::Baguette | Shape::TaperedBaguette => 2.5,
            Shape::Marquise => 2.0,
            Shape::Pear => 1.5,
            Shape::Heart => 0.95,
            Shape::Trillion | Shape::TrillionCurved | Shape::Triangle => 0.9,
            Shape::HalfMoon => 0.5,
            Shape::Hexagon => 1.15,
            Shape::Lozenge => 1.6,
            Shape::Trapezoid => 0.6,
            Shape::Coffin => 1.5,
            _ => 1.0,
        }
    }

    /// Outline with exactly `n` points (rounded up to an even number),
    /// CCW, scaled to width `w` (X) and length `l` (Y), centred on origin.
    pub fn outline(&self, w: f64, l: f64, n: usize) -> Vec<DVec2> {
        let n = (n.max(8) + 1) & !1;
        let raw = match self {
            Shape::Round | Shape::Oval => ellipse(n),
            Shape::Cushion => superellipse(n, 3.6),
            Shape::Princess | Shape::Baguette => polygon(&rect_cut(0.0), n),
            Shape::Emerald | Shape::Asscher => polygon(&rect_cut(0.28), n),
            Shape::Radiant => polygon(&rect_cut(0.18), n),
            Shape::Octagon => polygon(&rect_cut(0.41), n),
            Shape::TaperedBaguette => polygon(&[[-0.65, -1.0], [0.65, -1.0], [1.0, 1.0], [-1.0, 1.0]], n),
            Shape::Trapezoid => polygon(&[[-0.6, 1.0], [-1.0, -1.0], [1.0, -1.0], [0.6, 1.0]], n),
            Shape::Lozenge => polygon(&[[0.0, -1.0], [1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]], n),
            Shape::Hexagon => polygon(&regular(6, PI / 2.0), n),
            Shape::Coffin => polygon(&[[-0.55, -1.0], [0.55, -1.0], [1.0, 0.45], [0.7, 1.0], [-0.7, 1.0], [-1.0, 0.45]], n),
            Shape::Triangle => polygon(&regular(3, PI / 2.0), n),
            Shape::Trillion => polygon(&truncated_triangle(0.12), n),
            Shape::TrillionCurved => bowed_polygon(&regular(3, PI / 2.0), 0.18, n),
            Shape::Marquise => lens(n, 2.5),
            Shape::Pear => pear(n, 2.5),
            Shape::Heart => heart(n),
            Shape::HalfMoon => half_moon(n),
            Shape::Custom(pts) => {
                let p: Vec<DVec2> = pts.iter().map(|p| DVec2::new(p[0], p[1])).collect();
                let p = if crate::mesh::signed_area_2d(&p) < 0.0 { p.into_iter().rev().collect() } else { p };
                crate::mesh::resample_closed_2d(&p, n)
            }
        };
        normalise(raw, w, l)
    }
}

fn normalise(pts: Vec<DVec2>, w: f64, l: f64) -> Vec<DVec2> {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for p in &pts {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let c = (lo + hi) * 0.5;
    let s = DVec2::new(w / (hi.x - lo.x), l / (hi.y - lo.y));
    pts.into_iter().map(|p| (p - c) * s).collect()
}

fn ellipse(n: usize) -> Vec<DVec2> {
    (0..n).map(|i| {
        let a = TAU * i as f64 / n as f64 + PI / 2.0;
        DVec2::new(a.cos(), a.sin())
    }).collect()
}

fn superellipse(n: usize, p: f64) -> Vec<DVec2> {
    (0..n).map(|i| {
        let a = TAU * i as f64 / n as f64 + PI / 4.0;
        let (s, c) = a.sin_cos();
        DVec2::new(c.signum() * c.abs().powf(2.0 / p), s.signum() * s.abs().powf(2.0 / p))
    }).collect()
}

fn rect_cut(c: f64) -> Vec<[f64; 2]> {
    if c <= 0.0 {
        return vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    }
    let d = 1.0 - c;
    vec![[-d, -1.0], [d, -1.0], [1.0, -d], [1.0, d], [d, 1.0], [-d, 1.0], [-1.0, d], [-1.0, -d]]
}

fn regular(k: usize, start: f64) -> Vec<[f64; 2]> {
    (0..k).map(|i| {
        let a = start + TAU * i as f64 / k as f64;
        [a.cos(), a.sin()]
    }).collect()
}

fn truncated_triangle(t: f64) -> Vec<[f64; 2]> {
    let v = regular(3, PI / 2.0);
    let mut out = Vec::new();
    for i in 0..3 {
        let a = DVec2::from(v[i]);
        let b = DVec2::from(v[(i + 1) % 3]);
        let p = a.lerp(b, t);
        let q = a.lerp(b, 1.0 - t);
        out.push([p.x, p.y]);
        out.push([q.x, q.y]);
    }
    out
}

/// Samples a polygon so that every vertex is kept exactly (sharp corners)
/// and edges get points proportional to their length.
fn polygon(v: &[[f64; 2]], n: usize) -> Vec<DVec2> {
    let v: Vec<DVec2> = v.iter().map(|p| DVec2::from(*p)).collect();
    let k = v.len();
    let lens: Vec<f64> = (0..k).map(|i| v[i].distance(v[(i + 1) % k])).collect();
    let total: f64 = lens.iter().sum();
    let mut counts: Vec<usize> = lens.iter().map(|l| ((l / total) * n as f64).round().max(1.0) as usize).collect();
    loop {
        let s: usize = counts.iter().sum();
        if s == n {
            break;
        }
        let i = (0..k).max_by(|a, b| lens[*a].partial_cmp(&lens[*b]).unwrap()).unwrap();
        let j = (0..k).filter(|&j| counts[j] > 1).max_by_key(|&j| counts[j]).unwrap_or(i);
        if s < n { counts[i] += 1 } else { counts[j] -= 1 }
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..k {
        for s in 0..counts[i] {
            out.push(v[i].lerp(v[(i + 1) % k], s as f64 / counts[i] as f64));
        }
    }
    out
}

fn bowed_polygon(v: &[[f64; 2]], bulge: f64, n: usize) -> Vec<DVec2> {
    let base = polygon(v, n);
    let v: Vec<DVec2> = v.iter().map(|p| DVec2::from(*p)).collect();
    let k = v.len();
    // Push each point outward proportionally to sin(pi * t) along its edge.
    base.into_iter().map(|p| {
        let mut best = (f64::INFINITY, DVec2::ZERO, 0.0);
        for i in 0..k {
            let a = v[i];
            let b = v[(i + 1) % k];
            let e = b - a;
            let t = ((p - a).dot(e) / e.length_squared()).clamp(0.0, 1.0);
            let d = p.distance(a + e * t);
            if d < best.0 {
                best = (d, DVec2::new(e.y, -e.x).normalize() * e.length(), t);
            }
        }
        p + best.1 * bulge * (PI * best.2).sin()
    }).collect()
}

fn lens(n: usize, r: f64) -> Vec<DVec2> {
    // Two circular arcs of radius r meeting at sharp tips on the Y axis.
    let c = r - 1.0;
    let tip = (2.0 * r - 1.0).sqrt();
    let amax = (tip / r).asin();
    let half = n / 2;
    let mut out = Vec::with_capacity(n);
    for i in 0..half {
        let a = -amax + 2.0 * amax * i as f64 / half as f64;
        out.push(DVec2::new(-c + r * a.cos(), r * a.sin()));
    }
    for i in 0..half {
        let a = -amax + 2.0 * amax * i as f64 / half as f64;
        out.push(DVec2::new(c - r * a.cos(), -r * a.sin()));
    }
    out
}

fn pear(n: usize, r: f64) -> Vec<DVec2> {
    let c = r - 1.0;
    let tip = (2.0 * r - 1.0).sqrt();
    let amax = (tip / r).asin();
    let arc_n = n / 4;
    let round_n = n - 2 * arc_n;
    let mut out = Vec::with_capacity(n);
    // right flank: from (1,0) up to the tip
    for i in 0..arc_n {
        let a = amax * i as f64 / arc_n as f64;
        out.push(DVec2::new(-c + r * a.cos(), r * a.sin()));
    }
    // left flank: from tip down to (-1,0)
    for i in 0..arc_n {
        let a = amax * (1.0 - i as f64 / arc_n as f64);
        out.push(DVec2::new(c - r * a.cos(), r * a.sin()));
    }
    // round bottom
    for i in 0..round_n {
        let a = PI + PI * i as f64 / round_n as f64;
        out.push(DVec2::new(a.cos(), a.sin()));
    }
    out
}

fn heart(n: usize) -> Vec<DVec2> {
    // Classic heart curve; t = 0 is the top cusp, point at the bottom.
    (0..n).map(|i| {
        let t = -TAU * i as f64 / n as f64;
        let x = 16.0 * t.sin().powi(3);
        let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
        DVec2::new(x, y)
    }).collect()
}

fn half_moon(n: usize) -> Vec<DVec2> {
    let arc = n * 3 / 4;
    let flat = n - arc;
    let mut out = Vec::with_capacity(n);
    for i in 0..arc {
        let a = PI * i as f64 / arc as f64;
        out.push(DVec2::new(a.cos(), a.sin()));
    }
    for i in 0..flat {
        out.push(DVec2::new(-1.0 + 2.0 * i as f64 / flat as f64, 0.0));
    }
    out
}

/// Distance from the centre to the outline along direction `angle` (radians).
pub fn radius_at_angle(outline: &[DVec2], angle: f64) -> f64 {
    let d = DVec2::new(angle.cos(), angle.sin());
    let n = outline.len();
    let mut best: f64 = 0.0;
    for i in 0..n {
        let a = outline[i];
        let b = outline[(i + 1) % n];
        let e = b - a;
        let den = d.perp_dot(e);
        if den.abs() < 1e-12 {
            continue;
        }
        let t = a.perp_dot(e) / den; // distance along ray
        let u = a.perp_dot(d) / den; // parameter along edge
        if t > 0.0 && (-1e-9..=1.0 + 1e-9).contains(&u) {
            best = best.max(t);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::signed_area_2d;

    #[test]
    fn all_outlines_ccw_and_sized() {
        for s in Shape::BUILTIN.iter() {
            let o = s.outline(6.0, 8.0, 48);
            assert_eq!(o.len(), 48, "{:?}", s);
            assert!(signed_area_2d(&o) > 0.0, "{:?} not CCW", s);
            let w = o.iter().map(|p| p.x).fold(f64::MIN, f64::max) - o.iter().map(|p| p.x).fold(f64::MAX, f64::min);
            let l = o.iter().map(|p| p.y).fold(f64::MIN, f64::max) - o.iter().map(|p| p.y).fold(f64::MAX, f64::min);
            assert!((w - 6.0).abs() < 1e-9 && (l - 8.0).abs() < 1e-9, "{:?} {w} {l}", s);
        }
    }

    #[test]
    fn round_radius() {
        let o = Shape::Round.outline(6.0, 6.0, 64);
        assert!((radius_at_angle(&o, 0.3) - 3.0).abs() < 0.01);
    }
}
