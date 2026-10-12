//! Gem placement: single gem (gvGem / gvGemOnRingRail) and rows of gems on
//! the rail (gvGemsOnCurve applied to a ring rail: pavé, channel, eternity).

use super::{deg, Chain, GemInstance, OpResult, Output, Rail};
use crate::document::NodeId;
use crate::gem::GemSpec;
use glam::{DAffine3, DQuat, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Placement {
    Free {
        position: [f64; 3],
        /// Euler angles in degrees (X, Y, Z).
        rotation: [f64; 3],
    },
    OnRail {
        /// A ring rail or a shank node.
        rail: NodeId,
        /// Position on the rail (0 = top).
        t: f64,
        /// Girdle centre height: above the shank's outer surface when the
        /// input is a shank, above the rail otherwise.
        height: f64,
        /// Rotation about the gem axis (degrees).
        spin: f64,
        /// Tilt sideways about the rail tangent (degrees).
        tilt: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GemParams {
    pub spec: GemSpec,
    pub placement: Placement,
}

impl Default for GemParams {
    fn default() -> Self {
        GemParams { spec: GemSpec::default(), placement: Placement::Free { position: [0.0; 3], rotation: [0.0; 3] } }
    }
}

/// Height of the outer shank surface above the rail at `t`, 0 for a bare rail.
fn surface(o: &Output, t: f64) -> f64 {
    o.shank.as_ref().map(|s| s.thickness(t)).unwrap_or(0.0)
}

pub fn eval(p: &GemParams, rail_in: Option<&Output>) -> OpResult {
    if p.spec.width <= 0.0 || p.spec.length <= 0.0 {
        return Err("gem size must be positive".into());
    }
    let xform = match &p.placement {
        Placement::Free { position, rotation } => DAffine3::from_rotation_translation(
            DQuat::from_euler(glam::EulerRot::XYZ, deg(rotation[0]), deg(rotation[1]), deg(rotation[2])),
            DVec3::from(*position),
        ),
        Placement::OnRail { t, height, spin, tilt, .. } => {
            let o = rail_in.ok_or("missing rail input")?;
            let rail = o.rail.ok_or("input has no ring rail")?;
            rail.frame(*t, surface(o, *t) + height)
                * DAffine3::from_rotation_x(deg(*tilt))
                * DAffine3::from_rotation_z(deg(*spin))
        }
    };
    Ok(Output {
        gems: vec![GemInstance { spec: p.spec.clone(), xform }],
        chains: vec![Chain { indices: vec![0], closed: false }],
        ..Default::default()
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spacing {
    /// Spacing kept exactly, row starts at `start` (may not reach `end`).
    Fixed,
    /// At least `spacing`; leftover length is shared so the row fills the span.
    Minimum,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GemsOnRailParams {
    /// Ring rail or shank node.
    pub rail: NodeId,
    /// Template gem (size is overridden by start/end sizes).
    pub spec: GemSpec,
    pub start: f64,
    pub end: f64,
    pub size_start: f64,
    pub size_end: f64,
    pub spacing: f64,
    pub method: Spacing,
    /// Height of the girdle relative to the shank surface (negative = sunk).
    pub height: f64,
    /// Repeat the row on the other side of the ring (t → 1 - t).
    pub mirror: bool,
    /// Ignore start/end and go all around (eternity band).
    pub full_circle: bool,
    pub rows: usize,
    pub row_spacing: f64,
}

impl Default for GemsOnRailParams {
    fn default() -> Self {
        GemsOnRailParams {
            rail: NodeId::default(),
            spec: GemSpec::round(1.5),
            start: 0.04,
            end: 0.2,
            size_start: 1.5,
            size_end: 1.5,
            spacing: 0.1,
            method: Spacing::Minimum,
            height: -0.5,
            mirror: true,
            full_circle: false,
            rows: 1,
            row_spacing: 0.15,
        }
    }
}

pub fn eval_on_rail(p: &GemsOnRailParams, rail_in: &Output) -> OpResult {
    let rail = rail_in.rail.ok_or("input has no ring rail")?;
    if p.size_start <= 0.0 || p.size_end <= 0.0 {
        return Err("gem sizes must be positive".into());
    }
    let ref_h = surface(rail_in, (p.start + p.end) * 0.5) + p.height;
    let radius = rail.radius + ref_h.max(0.0);
    let circ = std::f64::consts::TAU * radius;
    let (t0, t1) = if p.full_circle { (0.0, 1.0) } else { (p.start.min(p.end), p.start.max(p.end)) };
    let span = (t1 - t0) * circ;
    let size_at = |f: f64| p.size_start + (p.size_end - p.size_start) * f;

    // Largest count that fits.
    let fits = |n: usize| -> (bool, f64) {
        let sizes: f64 = (0..n).map(|i| size_at(if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 })).sum();
        let gaps = if p.full_circle { n } else { n.saturating_sub(1) } as f64;
        let used = sizes + gaps * p.spacing;
        (used <= span + 1e-9, span - used)
    };
    let mut n = 0;
    while n < 2000 && fits(n + 1).0 {
        n += 1;
    }
    if n == 0 {
        return Err("no gem fits in the requested span".into());
    }
    let leftover = fits(n).1;
    let gaps = if p.full_circle { n } else { n.saturating_sub(1) };
    let gap = match p.method {
        Spacing::Fixed => p.spacing,
        Spacing::Minimum if gaps > 0 => p.spacing + leftover / gaps as f64,
        Spacing::Minimum => p.spacing,
    };
    let start_offset = if p.method == Spacing::Minimum && gaps == 0 { leftover / 2.0 } else { 0.0 };

    let mut out = Output::default();
    let rows = p.rows.clamp(1, 7);
    let sides: &[bool] = if p.mirror && !p.full_circle { &[false, true] } else { &[false] };
    for &mirrored in sides {
        for row in 0..rows {
            let lateral = (row as f64 - (rows - 1) as f64 / 2.0) * (p.size_start.max(p.size_end) + p.row_spacing);
            let mut idx = Vec::with_capacity(n);
            let mut s = start_offset;
            for i in 0..n {
                let size = size_at(if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 });
                let center = s + size / 2.0;
                s += size + gap;
                let mut t = t0 + center / circ;
                if mirrored {
                    t = 1.0 - t;
                }
                let mut spec = p.spec.clone();
                spec.width = size;
                spec.length = size * p.spec.length / p.spec.width;
                let h = surface(rail_in, t) + p.height;
                let xf = rail.frame(t, h) * DAffine3::from_translation(DVec3::new(0.0, lateral, 0.0));
                idx.push(out.gems.len());
                out.gems.push(GemInstance { spec, xform: xf });
            }
            out.chains.push(Chain { indices: idx, closed: p.full_circle });
        }
    }
    out.curves.push(
        (0..=64)
            .map(|i| Rail { radius: rail.radius + ref_h }.point(t0 + (t1 - t0) * i as f64 / 64.0))
            .collect(),
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eternity_fills_circle() {
        let rail = Output { rail: Some(Rail { radius: 8.66 }), ..Default::default() };
        let p = GemsOnRailParams { full_circle: true, height: 1.0, ..Default::default() };
        let o = eval_on_rail(&p, &rail).unwrap();
        let circ = std::f64::consts::TAU * (8.66 + 1.0);
        assert_eq!(o.gems.len(), (circ / 1.6).floor() as usize);
        // neighbours never overlap
        let g = &o.gems;
        for i in 0..g.len() {
            let d = g[i].center().distance(g[(i + 1) % g.len()].center());
            assert!(d >= 1.5, "{d}");
        }
    }

    #[test]
    fn taper_and_mirror() {
        let rail = Output { rail: Some(Rail { radius: 8.66 }), ..Default::default() };
        let p = GemsOnRailParams { size_start: 2.0, size_end: 1.2, ..Default::default() };
        let o = eval_on_rail(&p, &rail).unwrap();
        assert_eq!(o.chains.len(), 2);
        assert_eq!(o.gems.len() % 2, 0);
        assert!(o.gems[0].spec.width > o.gems[o.chains[0].indices.len() - 1].spec.width);
    }
}
