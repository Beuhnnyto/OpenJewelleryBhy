//! Head builder (gvHead): prongs around a gem plus gallery rails.

use super::{deg, GemInstance, OpResult, Output, Solid};
use crate::document::NodeId;
use crate::mesh::{offset_polygon, Mesh};
use crate::outline::radius_at_angle;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GalleryRail {
    /// 0 = at the prong base, 1 = at the girdle.
    pub at: f64,
    /// Wire diameter (mm).
    pub wire: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeadParams {
    pub gem: NodeId,
    /// Which gem of the input node (0 = first).
    pub gem_index: usize,
    pub prongs: usize,
    /// Extra rotation of the prong layout (degrees).
    pub rotation: f64,
    pub tip_diameter: f64,
    pub base_diameter: f64,
    /// Prong tip height above the girdle (mm).
    pub above_girdle: f64,
    /// Overall depth from the girdle to the prong base (mm).
    pub depth: f64,
    /// Base footprint relative to the girdle (0 = prongs meet in a point).
    pub base_scale: f64,
    /// Outward bow of the prongs (mm).
    pub bend: f64,
    /// 0 = prong tangent to the girdle, 1 = prong centre on the girdle.
    pub nudge: f64,
    pub dome: f64,
    pub rails: Vec<GalleryRail>,
}

impl Default for HeadParams {
    fn default() -> Self {
        HeadParams {
            gem: NodeId::default(),
            gem_index: 0,
            prongs: 4,
            rotation: 0.0,
            tip_diameter: 1.0,
            base_diameter: 1.15,
            above_girdle: 1.0,
            depth: 4.2,
            base_scale: 0.55,
            bend: 0.25,
            nudge: 0.3,
            dome: 0.9,
            rails: vec![GalleryRail { at: 0.12, wire: 0.9 }, GalleryRail { at: 0.62, wire: 0.8 }],
        }
    }
}

/// Prong directions (radians in the gem plane). Four prongs go to the
/// "corners" of the L×W box (princess/emerald corners, oval diagonals);
/// other counts are spread evenly starting at the gem's tip (+Y), which puts
/// prongs on the points of pears, marquises and trillions.
pub fn prong_angles(g: &GemInstance, count: usize, rotation: f64) -> Vec<f64> {
    let rot = deg(rotation);
    if count == 4 {
        let a = (g.spec.length / 2.0).atan2(g.spec.width / 2.0);
        return [a, PI - a, PI + a, TAU - a].iter().map(|x| x + rot).collect();
    }
    (0..count).map(|i| FRAC_PI_2 + TAU * i as f64 / count as f64 + rot).collect()
}

fn cubic(p0: DVec3, p1: DVec3, p2: DVec3, p3: DVec3, t: f64) -> DVec3 {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

pub fn eval(p: &HeadParams, input: &Output) -> OpResult {
    let g = input.gems.get(p.gem_index).ok_or("input has no gem at that index")?;
    if p.prongs < 2 || p.prongs > 12 {
        return Err("prong count must be between 2 and 12".into());
    }
    let h = g.spec.heights();
    let outline = g.spec.outline(96);
    let rt = p.tip_diameter / 2.0;
    let rb = p.base_diameter / 2.0;
    let base_z = -h.girdle_half - p.depth;
    let mut solids = Vec::new();

    for a in prong_angles(g, p.prongs, p.rotation) {
        let dir = DVec3::new(a.cos(), a.sin(), 0.0);
        let rg = radius_at_angle(&outline, a);
        let r_top = rg + rt * (1.0 - 2.0 * p.nudge);
        let girdle_pt = dir * r_top;
        let base_pt = dir * (rg * p.base_scale + rb * (1.0 - 2.0 * p.nudge).max(0.0) * p.base_scale.min(1.0))
            + DVec3::Z * base_z;
        let tip = girdle_pt + DVec3::Z * (h.girdle_half + p.above_girdle);
        let span = -base_z;
        let c1 = base_pt + DVec3::Z * span * 0.45 + dir * p.bend;
        let c2 = girdle_pt - DVec3::Z * span * 0.35 + dir * p.bend * 0.5;
        let mut path: Vec<DVec3> = (0..=14).map(|i| cubic(base_pt, c1, c2, girdle_pt, i as f64 / 14.0)).collect();
        path.push(girdle_pt.lerp(tip, 0.5));
        path.push(tip);
        let n = path.len();
        let radii: Vec<f64> = (0..n).map(|i| rb + (rt - rb) * (i as f64 / 14.0).min(1.0)).collect();
        let m = Mesh::tube(&path, &radii, 18, 0.0, p.dome);
        solids.push(Solid::metal(m.transformed(&g.xform)));
    }

    // Gallery rails follow the prong centres at their height.
    for rail in &p.rails {
        let at = rail.at.clamp(0.0, 1.0);
        let z = base_z + (0.0 - base_z) * at;
        let scale = p.base_scale + (1.0 - p.base_scale) * at;
        let path: Vec<DVec2> = offset_polygon(&outline, 0.0).iter().map(|q| *q * scale).collect();
        let off = rt * (1.0 - 2.0 * p.nudge) * at + rb * (1.0 - at) * p.base_scale.min(1.0);
        let path = offset_polygon(&path, off.max(0.0));
        let r = rail.wire / 2.0;
        let section: Vec<DVec2> =
            (0..14).map(|i| DVec2::from_angle(TAU * i as f64 / 14.0) * r + DVec2::new(0.0, z)).collect();
        if path.iter().map(|q| q.length()).fold(0.0, f64::max) <= r {
            continue; // collapsed rail
        }
        let m = Mesh::sweep_closed_planar(&path, &section);
        solids.push(Solid::metal(m.transformed(&g.xform)));
    }
    Ok(Output { solids, ..Default::default() })
}
