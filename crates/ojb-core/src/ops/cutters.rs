//! Cutters: gem seat cutter (gvGemCutter) and Scoop (metal-weight
//! reduction channel inside the shank, MatrixGold 4.0's "Scoop").

use super::{smooth, OpResult, Output, Solid};
use crate::document::NodeId;
use crate::mesh::Mesh;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GemCutterParams {
    pub gems: NodeId,
    /// Clear-out height above the table (mm).
    pub top_length: f64,
    /// Extra room around the girdle (mm).
    pub clearance: f64,
    /// How far the seat follows the pavilion before the hole (0..1).
    pub seat_depth: f64,
    /// Through-hole diameter relative to the gem (0..1).
    pub hole: f64,
    /// Hole length below the seat (mm).
    pub bottom_length: f64,
}

impl Default for GemCutterParams {
    fn default() -> Self {
        GemCutterParams {
            gems: NodeId::default(),
            top_length: 1.5,
            clearance: 0.03,
            seat_depth: 0.55,
            hole: 0.5,
            bottom_length: 3.0,
        }
    }
}

pub fn gem_cutter(spec: &crate::gem::GemSpec, p: &GemCutterParams) -> Mesh {
    let o = spec.outline(spec.resolution());
    let h = spec.heights();
    let size = spec.width.min(spec.length);
    let k = 1.0 + 2.0 * p.clearance / size;
    let ring = |s: f64, z: f64| -> Vec<DVec3> { o.iter().map(|q| DVec3::new(q.x * s, q.y * s, z)).collect() };
    let pav = h.culet + h.girdle_half; // negative
    let seat = p.seat_depth.clamp(0.05, 0.95);
    let seat_scale = (k * (1.0 - seat)).max(p.hole);
    let seat_z = -h.girdle_half + pav * (1.0 - seat_scale / k).min(seat);
    let hole = p.hole.clamp(0.05, seat_scale);
    let rings = vec![
        ring(hole, seat_z - p.bottom_length),
        ring(hole, seat_z),
        ring(seat_scale, seat_z),
        ring(k, -h.girdle_half),
        ring(k, h.table + p.top_length),
    ];
    Mesh::loft(
        &rings,
        false,
        Some(DVec3::new(0.0, 0.0, seat_z - p.bottom_length)),
        Some(DVec3::new(0.0, 0.0, h.table + p.top_length)),
    )
}

pub fn eval_gem_cutter(p: &GemCutterParams, input: &Output) -> OpResult {
    if input.gems.is_empty() {
        return Err("input has no gems".into());
    }
    // one cutter mesh per distinct spec, instanced
    let mut cache: Vec<(crate::gem::GemSpec, Mesh)> = Vec::new();
    let mut solids = Vec::with_capacity(input.gems.len());
    for g in &input.gems {
        let m = match cache.iter().find(|(s, _)| *s == g.spec) {
            Some((_, m)) => m,
            None => {
                cache.push((g.spec.clone(), gem_cutter(&g.spec, p)));
                &cache.last().unwrap().1
            }
        };
        solids.push(Solid::cutter(m.transformed(&g.xform)));
    }
    Ok(Output { solids, ..Default::default() })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoopParams {
    pub shank: NodeId,
    /// Rail positions where the scoop starts and ends (0.5 = bottom).
    pub start: f64,
    pub end: f64,
    /// Metal left on each side and on the outside (mm).
    pub wall: f64,
    /// Fraction of the remaining thickness removed.
    pub depth: f64,
}

impl Default for ScoopParams {
    fn default() -> Self {
        ScoopParams { shank: NodeId::default(), start: 0.2, end: 0.8, wall: 0.55, depth: 0.85 }
    }
}

pub fn eval_scoop(p: &ScoopParams, input: &Output) -> OpResult {
    let info = input.shank.as_ref().ok_or("input is not a shank")?;
    let (t0, t1) = (p.start.min(p.end), p.start.max(p.end));
    if t1 - t0 < 0.02 {
        return Err("scoop span too short".into());
    }
    let n = ((t1 - t0) * 400.0) as usize + 8;
    let ramp = 0.12;
    let mut rings = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let f = i as f64 / n as f64;
        let t = t0 + (t1 - t0) * f;
        let e = smooth((f / ramp).min((1.0 - f) / ramp).min(1.0));
        // At the ends the section shrinks to a sliver entirely below the inner
        // surface (outside the metal) so the cut fades out smoothly.
        let w = ((info.width(t) / 2.0 - p.wall).max(0.0) * e.sqrt()).max(0.03);
        let top = (info.thickness(t) - p.wall).max(0.0) * p.depth * e - 0.08 * (1.0 - e);
        let bottom = -0.4;
        let nrm = super::Rail::normal(t);
        let base = info.rail.point(t);
        // rounded-rectangle section (u across finger, v outward)
        let rr = (w.min(top - bottom) * 0.35).max(1e-4);
        let mut sec: Vec<DVec2> = Vec::new();
        for (cx, cy, a0) in [(w - rr, bottom + rr, -90.0f64), (w - rr, top - rr, 0.0), (-w + rr, top - rr, 90.0), (-w + rr, bottom + rr, 180.0)] {
            for k in 0..4 {
                let a = (a0 + 30.0 * k as f64).to_radians();
                sec.push(DVec2::new(cx + rr * a.cos(), cy + rr * a.sin()));
            }
        }
        rings.push(sec.iter().map(|q| base + DVec3::Y * q.x + nrm * q.y).collect::<Vec<_>>());
    }
    let tip = |t: f64| info.rail.point(t) + super::Rail::normal(t) * -0.2;
    let m = Mesh::loft(&rings, false, Some(tip(t0)), Some(tip(t1)));
    Ok(Output { solids: vec![Solid::cutter(m)], ..Default::default() })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelCutterParams {
    pub gems: NodeId,
    /// Channel width as % of the gem girdle.
    pub width_pct: f64,
    /// Height above the table (mm).
    pub top: f64,
    /// Depth below the girdle as % of the pavilion.
    pub depth_pct: f64,
    /// Extension past the end gems of open rows (mm).
    pub end_extension: f64,
}

impl Default for ChannelCutterParams {
    fn default() -> Self {
        ChannelCutterParams { gems: NodeId::default(), width_pct: 92.0, top: 1.5, depth_pct: 35.0, end_extension: 0.0 }
    }
}

/// gvChannelCutter: a swept box following each row of gems, so the stones
/// sit in a channel held by the side walls.
pub fn eval_channel(p: &ChannelCutterParams, input: &Output) -> OpResult {
    if input.chains.is_empty() {
        return Err("input has no gem rows".into());
    }
    let mut solids = Vec::new();
    for ch in &input.chains {
        if ch.indices.len() < 2 {
            continue;
        }
        let section = |g: &super::GemInstance| -> Vec<DVec3> {
            let h = g.spec.heights();
            let w = g.spec.width.min(g.spec.length) * p.width_pct / 100.0 / 2.0;
            let z0 = -h.girdle_half + (h.culet + h.girdle_half) * p.depth_pct / 100.0;
            let z1 = h.table + p.top;
            [(-w, z0), (w, z0), (w, z1), (-w, z1)]
                .iter()
                .map(|(y, z)| g.xform.transform_point3(DVec3::new(0.0, *y, *z)))
                .collect()
        };
        let mut rings: Vec<Vec<DVec3>> = ch.indices.iter().map(|&i| section(&input.gems[i])).collect();
        let solid = if ch.closed {
            Mesh::loft(&rings, true, None, None)
        } else {
            // extend past both ends along the row direction
            let ext = |ring: &Vec<DVec3>, from: DVec3, to: DVec3, g: &super::GemInstance| -> Vec<DVec3> {
                let d = (to - from).normalize_or_zero() * (g.spec.width / 2.0 + p.end_extension);
                ring.iter().map(|q| *q + d).collect()
            };
            let (f, s) = (&input.gems[ch.indices[0]], &input.gems[ch.indices[1]]);
            let n = ch.indices.len();
            let (l, pl) = (&input.gems[ch.indices[n - 1]], &input.gems[ch.indices[n - 2]]);
            let first = ext(&rings[0], s.center(), f.center(), f);
            let last = ext(&rings[n - 1], pl.center(), l.center(), l);
            rings.insert(0, first);
            rings.push(last);
            let c0 = rings[0].iter().copied().sum::<DVec3>() / 4.0;
            let c1 = rings[rings.len() - 1].iter().copied().sum::<DVec3>() / 4.0;
            Mesh::loft(&rings, false, Some(c0), Some(c1))
        };
        solids.push(Solid::cutter(solid));
    }
    if solids.is_empty() {
        return Err("rows need at least two gems".into());
    }
    Ok(Output { solids, ..Default::default() })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScallopParams {
    pub gem: NodeId,
    pub gem_index: usize,
    pub count: usize,
    /// Radius of each scallop (mm).
    pub radius: f64,
    /// Height of the scallop centres above the girdle (mm).
    pub height: f64,
    /// Rotation of the pattern (degrees).
    pub rotation: f64,
}

impl Default for ScallopParams {
    fn default() -> Self {
        ScallopParams { gem: NodeId::default(), gem_index: 0, count: 6, radius: 0.9, height: 1.1, rotation: 0.0 }
    }
}

/// gvScallopCutter: radial cylinders that scallop a bezel's top edge.
pub fn eval_scallop(p: &ScallopParams, input: &Output) -> OpResult {
    let g = input.gems.get(p.gem_index).ok_or("input has no gem at that index")?;
    if p.count == 0 || p.count > 64 {
        return Err("count must be 1..64".into());
    }
    let outline = g.spec.outline(96);
    let mut solids = Vec::new();
    for k in 0..p.count {
        let a = (p.rotation.to_radians()) + std::f64::consts::TAU * (k as f64 + 0.5) / p.count as f64;
        let r = crate::outline::radius_at_angle(&outline, a);
        let dir = DVec3::new(a.cos(), a.sin(), 0.0);
        let z = g.spec.heights().girdle_half + p.height;
        let from = dir * (r * 0.6) + DVec3::Z * z;
        let to = dir * (r + 3.0) + DVec3::Z * z;
        let path: Vec<DVec3> = (0..4).map(|i| from.lerp(to, i as f64 / 3.0)).collect();
        let m = Mesh::tube(&path, &[p.radius; 4], 20, 0.0, 0.0);
        solids.push(Solid::cutter(m.transformed(&g.xform)));
    }
    Ok(Output { solids, ..Default::default() })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AzureParams {
    pub shank: NodeId,
    pub start: f64,
    pub end: f64,
    /// Hexagon size across flats (mm).
    pub size: f64,
    /// Metal between cells (mm).
    pub wall: f64,
    /// Metal kept at the inner and outer surfaces (mm).
    pub skin: f64,
}

impl Default for AzureParams {
    fn default() -> Self {
        AzureParams { shank: NodeId::default(), start: 0.06, end: 0.2, size: 0.9, wall: 0.3, skin: 0.4 }
    }
}

/// Azure / honeycomb cutters (MatrixGold 3.10): hexagonal windows through
/// the sides of the shank, mirrored on both sides of the ring.
pub fn eval_azure(p: &AzureParams, input: &Output) -> OpResult {
    let info = input.shank.as_ref().ok_or("input is not a shank")?;
    let (t0, t1) = (p.start.min(p.end), p.start.max(p.end));
    let pitch = p.size + p.wall;
    let r_hex = p.size / 3f64.sqrt(); // circumradius from flat-to-flat
    let mut solids = Vec::new();
    let mid_t = (t0 + t1) / 2.0;
    let thick = info.thickness(mid_t);
    let rows = (((thick - 2.0 * p.skin) / pitch).floor() as usize).max(1);
    let rail_r = info.rail.radius;
    for row in 0..rows {
        let v = p.skin + p.size / 2.0 + row as f64 * pitch + ((thick - 2.0 * p.skin) - rows as f64 * pitch + p.wall).max(0.0) / 2.0;
        let rad = rail_r + v;
        let circ = std::f64::consts::TAU * rad;
        let stagger = if row % 2 == 1 { 0.5 } else { 0.0 };
        let n = (((t1 - t0) * circ) / pitch).floor() as usize;
        for k in 0..n {
            let t = t0 + ((k as f64 + 0.5 + stagger) * pitch) / circ;
            if t > t1 || v + p.size / 2.0 > info.thickness(t) - p.skin + 1e-9 {
                continue;
            }
            for side in [1.0, -1.0] {
                let tt = if side > 0.0 { t } else { 1.0 - t };
                let nrm = Rail::normal(tt);
                let tan = Rail::tangent(tt);
                let c = nrm * rad;
                let w = info.width(tt) / 2.0 + 0.5;
                let hex = |y: f64| -> Vec<DVec3> {
                    (0..6)
                        .map(|i| {
                            let a = std::f64::consts::TAU * i as f64 / 6.0;
                            c + tan * (r_hex * a.cos()) + nrm * (r_hex * a.sin()) + DVec3::Y * y
                        })
                        .collect()
                };
                let m = Mesh::loft(&[hex(-w), hex(w)], false, Some(c - DVec3::Y * w), Some(c + DVec3::Y * w));
                solids.push(Solid::cutter(m));
            }
        }
    }
    if solids.is_empty() {
        return Err("shank too thin for that cell size".into());
    }
    Ok(Output { solids, ..Default::default() })
}

use super::Rail;
