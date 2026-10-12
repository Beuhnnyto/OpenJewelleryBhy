//! Prong placer (gvProngPlacer): prongs for any set of gems, shared between
//! neighbours along a chain or individual per gem.

use super::{GemInstance, OpResult, Output, Solid};
use crate::document::NodeId;
use crate::mesh::Mesh;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProngLayout {
    /// One prong on each side between neighbouring gems (shared), two at
    /// each end of an open row.
    Shared,
    /// Two prongs per gem, across the row.
    TwoPerGem,
    /// Four prongs per gem on the diagonals.
    FourPerGem,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProngsParams {
    pub gems: NodeId,
    pub layout: ProngLayout,
    /// Prong diameter as a fraction of the (average) gem size.
    pub diameter_pct: f64,
    /// Minimum diameter in mm (castability).
    pub min_diameter: f64,
    /// Tip height above the girdle, as a fraction of gem size.
    pub above_girdle_pct: f64,
    /// Depth below the girdle (mm).
    pub below_girdle: f64,
    /// 0 = prong tangent to the girdle, 1 = prong centre on the girdle.
    pub nudge: f64,
    pub dome: f64,
}

impl Default for ProngsParams {
    fn default() -> Self {
        ProngsParams {
            gems: NodeId::default(),
            layout: ProngLayout::Shared,
            diameter_pct: 28.0,
            min_diameter: 0.45,
            above_girdle_pct: 22.0,
            below_girdle: 0.9,
            nudge: 0.35,
            dome: 1.0,
        }
    }
}

/// A vertical (gem-axis) prong.
pub fn prong(base: DVec3, tip: DVec3, r_base: f64, r_tip: f64, dome: f64) -> Mesh {
    let path: Vec<DVec3> = (0..4).map(|i| base.lerp(tip, i as f64 / 3.0)).collect();
    let radii: Vec<f64> = (0..4).map(|i| r_base + (r_tip - r_base) * i as f64 / 3.0).collect();
    Mesh::tube(&path, &radii, 16, 0.0, dome)
}

fn gem_prong(g: &GemInstance, local_xy: DVec2, d: f64, p: &ProngsParams) -> Mesh {
    let size = g.spec.width.min(g.spec.length);
    let h = g.spec.heights();
    let base = g.xform.transform_point3(DVec3::new(local_xy.x, local_xy.y, -h.girdle_half - p.below_girdle));
    let tip = g.xform.transform_point3(DVec3::new(local_xy.x, local_xy.y, h.girdle_half + size * p.above_girdle_pct / 100.0));
    prong(base, tip, d / 2.0 * 1.1, d / 2.0, p.dome)
}

pub fn eval(p: &ProngsParams, input: &Output) -> OpResult {
    if input.gems.is_empty() {
        return Err("input has no gems".into());
    }
    let mut solids = Vec::new();
    let mut covered = vec![false; input.gems.len()];
    let dia = |g: &GemInstance| (g.spec.width.min(g.spec.length) * p.diameter_pct / 100.0).max(p.min_diameter);

    if p.layout == ProngLayout::Shared {
        for ch in &input.chains {
            let n = ch.indices.len();
            if n < 2 {
                continue;
            }
            let pairs = if ch.closed { n } else { n - 1 };
            for k in 0..pairs {
                let a = &input.gems[ch.indices[k]];
                let b = &input.gems[ch.indices[(k + 1) % n]];
                // Express b's centre in a's local frame to find the row direction.
                let inv = a.xform.inverse();
                let bl = inv.transform_point3(b.center());
                let dir = DVec2::new(bl.x, bl.y).normalize_or(DVec2::X);
                let lat = DVec2::new(-dir.y, dir.x);
                let ra = a.spec.width.min(a.spec.length) / 2.0;
                let rb = b.spec.width.min(b.spec.length) / 2.0;
                let d = (dia(a) + dia(b)) / 2.0;
                let dist = DVec2::new(bl.x, bl.y).length();
                // midpoint between girdles, pushed sideways to touch both gems
                let mid = dir * (ra + (dist - ra - rb) / 2.0);
                let off = ((ra + rb) / 2.0 + d / 2.0 * (1.0 - 2.0 * p.nudge)).max(0.0);
                // on the bisector, `off` from both centres; never closer to
                // the row axis than ~0.6 r so the prong sits in the gap
                let side = (off * off - mid.length_squared()).max(0.0).sqrt().max(0.6 * (ra + rb) / 2.0);
                for s in [-1.0, 1.0] {
                    solids.push(Solid::metal(gem_prong(a, mid + lat * side * s, d, p)));
                }
            }
            if !ch.closed {
                // end prongs: two at the outer side of each end gem
                for (end, nb) in [(ch.indices[0], ch.indices[1]), (ch.indices[n - 1], ch.indices[n - 2])] {
                    let g = &input.gems[end];
                    let nl = g.xform.inverse().transform_point3(input.gems[nb].center());
                    let away = -DVec2::new(nl.x, nl.y).normalize_or(DVec2::X);
                    let lat = DVec2::new(-away.y, away.x);
                    let r = g.spec.width.min(g.spec.length) / 2.0;
                    let d = dia(g);
                    let rr = r + d / 2.0 * (1.0 - 2.0 * p.nudge);
                    for s in [-1.0, 1.0] {
                        let dir = (away + lat * s).normalize();
                        solids.push(Solid::metal(gem_prong(g, dir * rr, d, p)));
                    }
                }
            }
            for &i in &ch.indices {
                covered[i] = true;
            }
        }
    }
    for (i, g) in input.gems.iter().enumerate() {
        if covered[i] {
            continue;
        }
        let d = dia(g);
        let dirs: Vec<DVec2> = match p.layout {
            ProngLayout::TwoPerGem => vec![DVec2::Y, -DVec2::Y],
            _ => {
                let a = (g.spec.length / 2.0).atan2(g.spec.width / 2.0);
                [a, std::f64::consts::PI - a, std::f64::consts::PI + a, -a]
                    .iter()
                    .map(|a| DVec2::new(a.cos(), a.sin()))
                    .collect()
            }
        };
        let outline = g.spec.outline(64);
        for dir in dirs {
            let r = crate::outline::radius_at_angle(&outline, dir.y.atan2(dir.x));
            solids.push(Solid::metal(gem_prong(g, dir * (r + d / 2.0 * (1.0 - 2.0 * p.nudge)), d, p)));
        }
    }
    Ok(Output { solids, ..Default::default() })
}
