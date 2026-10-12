//! Shank cross-section profiles.
//!
//! A profile is a closed CCW polygon in (u, v): u ∈ [-0.5, 0.5] across the
//! band (finger axis), v ∈ [0, 1] from the inner surface (on the ring rail)
//! outwards. All profiles resample to the same point count, starting at the
//! bottom centre, so they can be blended between stations.

use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    /// Flat inside, domed outside (classic "D" / half-round wedding band).
    HalfRound,
    /// Comfort fit: slightly domed inside and outside.
    Comfort,
    /// Flat band with softened edges.
    Flat,
    /// Square / rectangular wire.
    Square,
    /// Round wire.
    Round,
    /// Knife edge: rises to a ridge.
    KnifeEdge,
    /// Court: domed outside, domed inside, thinner edges.
    Court,
}

impl Profile {
    pub const ALL: [Profile; 7] = [
        Profile::HalfRound,
        Profile::Comfort,
        Profile::Flat,
        Profile::Square,
        Profile::Round,
        Profile::KnifeEdge,
        Profile::Court,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Profile::HalfRound => "Half round (D)",
            Profile::Comfort => "Comfort fit",
            Profile::Flat => "Flat",
            Profile::Square => "Square",
            Profile::Round => "Round wire",
            Profile::KnifeEdge => "Knife edge",
            Profile::Court => "Court",
        }
    }

    fn raw(self) -> Vec<DVec2> {
        let arc = |n: usize, cx: f64, cy: f64, rx: f64, ry: f64, a0: f64, a1: f64| -> Vec<DVec2> {
            (0..=n)
                .map(|i| {
                    let a = a0 + (a1 - a0) * i as f64 / n as f64;
                    DVec2::new(cx + rx * a.cos(), cy + ry * a.sin())
                })
                .collect::<Vec<_>>()
        };
        let mut p = Vec::new();
        match self {
            Profile::HalfRound => {
                p.push(DVec2::new(0.0, 0.0));
                p.push(DVec2::new(0.5, 0.0));
                p.extend(arc(24, 0.0, 0.0, 0.5, 1.0, 0.0, PI));
                p.push(DVec2::new(-0.5, 0.0));
            }
            Profile::Comfort => {
                p.extend(arc(12, 0.0, 0.18, 0.5, 0.18, -PI / 2.0, 0.0));
                p.extend(arc(24, 0.0, 0.18, 0.5, 0.82, 0.0, PI));
                p.extend(arc(12, 0.0, 0.18, 0.5, 0.18, PI, 1.5 * PI));
            }
            Profile::Court => {
                p.extend(arc(12, 0.0, 0.3, 0.5, 0.3, -PI / 2.0, 0.0));
                p.extend(arc(24, 0.0, 0.3, 0.5, 0.7, 0.0, PI));
                p.extend(arc(12, 0.0, 0.3, 0.5, 0.3, PI, 1.5 * PI));
            }
            Profile::Flat => {
                let r = 0.08;
                p.push(DVec2::new(0.0, 0.0));
                p.extend(arc(4, 0.5 - r, r, r, r, -PI / 2.0, 0.0));
                p.extend(arc(4, 0.5 - r, 1.0 - r, r, r, 0.0, PI / 2.0));
                p.extend(arc(4, -0.5 + r, 1.0 - r, r, r, PI / 2.0, PI));
                p.extend(arc(4, -0.5 + r, r, r, r, PI, 1.5 * PI));
            }
            Profile::Square => {
                p.push(DVec2::new(0.0, 0.0));
                p.push(DVec2::new(0.5, 0.0));
                p.push(DVec2::new(0.5, 1.0));
                p.push(DVec2::new(-0.5, 1.0));
                p.push(DVec2::new(-0.5, 0.0));
            }
            Profile::Round => {
                p.extend(arc(48, 0.0, 0.5, 0.5, 0.5, -PI / 2.0, 1.5 * PI));
            }
            Profile::KnifeEdge => {
                p.push(DVec2::new(0.0, 0.0));
                p.push(DVec2::new(0.5, 0.0));
                p.push(DVec2::new(0.5, 0.35));
                p.push(DVec2::new(0.0, 1.0));
                p.push(DVec2::new(-0.5, 0.35));
                p.push(DVec2::new(-0.5, 0.0));
            }
        }
        // drop duplicate closing points
        p.dedup_by(|a, b| a.distance(*b) < 1e-9);
        if p.len() > 1 && p[0].distance(*p.last().unwrap()) < 1e-9 {
            p.pop();
        }
        p
    }

    /// Profile resampled to `n` points, starting at the bottom centre.
    pub fn points(self, n: usize) -> Vec<DVec2> {
        let raw = self.raw();
        let mut pts = crate::mesh::resample_closed_2d(&raw, n);
        // rotate so index 0 is the point closest to (0, 0)
        let start = pts
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.length_squared().partial_cmp(&b.1.length_squared()).unwrap())
            .map(|x| x.0)
            .unwrap_or(0);
        pts.rotate_left(start);
        pts
    }
}

/// Blend two equally sampled profiles.
pub fn blend(a: &[DVec2], b: &[DVec2], t: f64) -> Vec<DVec2> {
    a.iter().zip(b).map(|(p, q)| p.lerp(*q, t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_ccw_and_bounded() {
        for p in Profile::ALL {
            let pts = p.points(32);
            assert_eq!(pts.len(), 32);
            assert!(crate::mesh::signed_area_2d(&pts) > 0.0, "{:?}", p);
            for q in &pts {
                assert!(q.x >= -0.5 - 1e-9 && q.x <= 0.5 + 1e-9 && q.y >= -1e-9 && q.y <= 1.0 + 1e-9, "{:?} {q}", p);
            }
        }
    }
}
