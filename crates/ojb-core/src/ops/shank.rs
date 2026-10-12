//! Shank builder (gvProfilePlacer + gvProfileSweep1 + gvOutsideRingRail):
//! profiles placed at stations around the rail, blended and swept.

use super::{smooth, OpResult, Output, Rail, Solid};
use crate::mesh::Mesh;
use crate::profile::{blend, Profile};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Station {
    /// Position on the rail, 0 = top (12 o'clock), 0.25 = 3 o'clock,
    /// 0.5 = bottom (6 o'clock).
    pub pos: f64,
    /// Band width along the finger (mm).
    pub width: f64,
    /// Metal thickness outward from the rail (mm).
    pub thickness: f64,
    pub profile: Profile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShankParams {
    pub rail: super::super::document::NodeId,
    pub stations: Vec<Station>,
    /// Mirror stations of the right half (0..0.5) onto the left half.
    pub symmetric: bool,
    pub samples: usize,
    pub profile_points: usize,
}

impl Default for ShankParams {
    fn default() -> Self {
        ShankParams {
            rail: Default::default(),
            stations: vec![
                Station { pos: 0.0, width: 2.2, thickness: 1.9, profile: Profile::Comfort },
                Station { pos: 0.25, width: 2.3, thickness: 1.7, profile: Profile::Comfort },
                Station { pos: 0.5, width: 2.5, thickness: 1.6, profile: Profile::Comfort },
            ],
            symmetric: true,
            samples: 192,
            profile_points: 32,
        }
    }
}

/// Resolved width/thickness/profile as a function of the rail position.
#[derive(Clone, Debug)]
pub struct ShankInfo {
    pub rail: Rail,
    stations: Vec<Station>,
}

impl ShankInfo {
    pub fn new(rail: Rail, p: &ShankParams) -> Result<ShankInfo, String> {
        if p.stations.is_empty() {
            return Err("shank needs at least one station".into());
        }
        let mut st: Vec<Station> = p
            .stations
            .iter()
            .map(|s| Station { pos: s.pos.rem_euclid(1.0), ..s.clone() })
            .collect();
        if p.symmetric {
            let mirrored: Vec<Station> = st
                .iter()
                .filter(|s| s.pos > 1e-9 && s.pos < 0.5 - 1e-9)
                .map(|s| Station { pos: 1.0 - s.pos, ..s.clone() })
                .collect();
            st.retain(|s| s.pos <= 0.5 + 1e-9);
            st.extend(mirrored);
        }
        st.sort_by(|a, b| a.pos.partial_cmp(&b.pos).unwrap());
        for s in &st {
            if s.width <= 0.0 || s.thickness <= 0.0 {
                return Err("station width and thickness must be positive".into());
            }
        }
        Ok(ShankInfo { rail, stations: st })
    }

    /// (index a, index b, blend factor) around the periodic station list.
    fn locate(&self, t: f64) -> (usize, usize, f64) {
        let t = t.rem_euclid(1.0);
        let n = self.stations.len();
        if n == 1 {
            return (0, 0, 0.0);
        }
        let i = self.stations.iter().rposition(|s| s.pos <= t).unwrap_or(n - 1);
        let j = (i + 1) % n;
        let a = self.stations[i].pos;
        let mut b = self.stations[j].pos;
        let mut tt = t;
        if b <= a {
            b += 1.0;
        }
        if tt < a {
            tt += 1.0;
        }
        (i, j, smooth((tt - a) / (b - a)))
    }

    pub fn width(&self, t: f64) -> f64 {
        let (a, b, f) = self.locate(t);
        self.stations[a].width + (self.stations[b].width - self.stations[a].width) * f
    }

    pub fn thickness(&self, t: f64) -> f64 {
        let (a, b, f) = self.locate(t);
        self.stations[a].thickness + (self.stations[b].thickness - self.stations[a].thickness) * f
    }

    pub fn profile(&self, t: f64, n: usize) -> Vec<DVec2> {
        let (a, b, f) = self.locate(t);
        let pa = self.stations[a].profile.points(n);
        if self.stations[a].profile == self.stations[b].profile {
            return pa;
        }
        blend(&pa, &self.stations[b].profile.points(n), f)
    }

    /// One cross-section ring in world space.
    pub fn ring(&self, t: f64, n: usize) -> Vec<DVec3> {
        let w = self.width(t);
        let h = self.thickness(t);
        let nrm = Rail::normal(t);
        let base = self.rail.point(t);
        self.profile(t, n)
            .iter()
            .map(|q| base + DVec3::Y * (q.x * w) + nrm * (q.y * h))
            .collect()
    }
}

pub fn eval(p: &ShankParams, rail_in: &Output) -> OpResult {
    let rail = rail_in.rail.ok_or("input is not a ring rail")?;
    let info = ShankInfo::new(rail, p)?;
    let samples = p.samples.clamp(16, 2048);
    let k = p.profile_points.clamp(8, 256);
    let rings: Vec<Vec<DVec3>> = (0..samples).map(|i| info.ring(i as f64 / samples as f64, k)).collect();
    let mesh = Mesh::loft(&rings, true, None, None);
    Ok(Output {
        solids: vec![Solid::metal(mesh)],
        rail: Some(rail),
        shank: Some(info),
        curves: vec![rail.polyline(128)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_band_volume() {
        // A square-profile band is an annulus: V = π (R² - r²) w
        let rail = Rail { radius: 8.0 };
        let p = ShankParams {
            stations: vec![Station { pos: 0.0, width: 3.0, thickness: 2.0, profile: Profile::Square }],
            samples: 1024,
            ..Default::default()
        };
        let out = eval(&p, &Output { rail: Some(rail), ..Default::default() }).unwrap();
        let v = out.solids[0].mesh.volume();
        let exact = std::f64::consts::PI * (10.0f64.powi(2) - 8.0f64.powi(2)) * 3.0;
        assert!((v - exact).abs() / exact < 0.002, "{v} vs {exact}");
    }

    #[test]
    fn station_interpolation_symmetric() {
        let info = ShankInfo::new(Rail { radius: 8.0 }, &ShankParams::default()).unwrap();
        assert!((info.thickness(0.0) - 1.9).abs() < 1e-9);
        assert!((info.thickness(0.5) - 1.6).abs() < 1e-9);
        assert!((info.width(0.1) - info.width(0.9)).abs() < 1e-9);
        assert!((info.thickness(0.97) - info.thickness(0.03)).abs() < 1e-9);
    }
}
