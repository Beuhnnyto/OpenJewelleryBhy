//! Builders ("dynamic commands"). Each builder is a pure function of its
//! parameters and of the outputs of the nodes it references.

pub mod bezel;
pub mod boolean;
pub mod cutters;
pub mod extras;
pub mod gems;
pub mod handles;
pub mod halo;
pub mod head;
pub mod prongs;
pub mod rail;
pub mod shank;
pub mod signet;
pub mod text;
pub mod transform;

use crate::gem::GemSpec;
use crate::mesh::{Aabb, Mesh};
use glam::{DAffine3, DMat3, DVec3};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Metal,
    Cutter,
}

#[derive(Clone, Debug)]
pub struct Solid {
    pub mesh: Arc<Mesh>,
    pub role: Role,
}

impl Solid {
    pub fn metal(m: Mesh) -> Solid {
        Solid { mesh: Arc::new(m), role: Role::Metal }
    }
    pub fn cutter(m: Mesh) -> Solid {
        Solid { mesh: Arc::new(m), role: Role::Cutter }
    }
}

#[derive(Clone, Debug)]
pub struct GemInstance {
    pub spec: GemSpec,
    /// Local gem frame → world. Local: girdle centre at origin, table +Z.
    pub xform: DAffine3,
}

impl GemInstance {
    pub fn center(&self) -> DVec3 {
        self.xform.translation
    }
    pub fn up(&self) -> DVec3 {
        self.xform.matrix3.z_axis.normalize()
    }
    pub fn world_mesh(&self) -> Mesh {
        self.spec.mesh().transformed(&self.xform)
    }
}

/// Ordered run of gems (used for shared prongs and cutters).
#[derive(Clone, Debug)]
pub struct Chain {
    pub indices: Vec<usize>,
    pub closed: bool,
}

/// The ring rail: a circle of inner radius `radius` in the XZ plane.
#[derive(Clone, Copy, Debug)]
pub struct Rail {
    pub radius: f64,
}

impl Rail {
    /// t = 0 at 12 o'clock (+Z), increasing clockwise seen from the front.
    pub fn normal(t: f64) -> DVec3 {
        let a = std::f64::consts::TAU * t;
        DVec3::new(a.sin(), 0.0, a.cos())
    }
    pub fn tangent(t: f64) -> DVec3 {
        let a = std::f64::consts::TAU * t;
        DVec3::new(a.cos(), 0.0, -a.sin())
    }
    pub fn point(&self, t: f64) -> DVec3 {
        Rail::normal(t) * self.radius
    }
    /// Frame at `t`, lifted `height` mm above the inner surface:
    /// x = tangent, y = finger axis, z = outward normal.
    pub fn frame(&self, t: f64, height: f64) -> DAffine3 {
        let n = Rail::normal(t);
        DAffine3::from_mat3_translation(DMat3::from_cols(Rail::tangent(t), DVec3::Y, n), n * (self.radius + height))
    }
    pub fn polyline(&self, n: usize) -> Vec<DVec3> {
        (0..=n).map(|i| self.point(i as f64 / n as f64)).collect()
    }
}

/// Everything a node produces.
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub solids: Vec<Solid>,
    pub gems: Vec<GemInstance>,
    pub chains: Vec<Chain>,
    pub rail: Option<Rail>,
    pub shank: Option<shank::ShankInfo>,
    /// Helper curves for display (rails, guides).
    pub curves: Vec<Vec<DVec3>>,
}

impl Output {
    pub fn bbox(&self) -> Aabb {
        let mut b = Aabb::EMPTY;
        for s in &self.solids {
            b = b.union(&s.mesh.bbox());
        }
        for g in &self.gems {
            let r = g.spec.length.max(g.spec.width) * 0.6;
            b = b.union(&Aabb { min: g.center() - DVec3::splat(r), max: g.center() + DVec3::splat(r) });
        }
        b
    }

    pub fn transformed(&self, xf: &DAffine3) -> Output {
        Output {
            solids: self
                .solids
                .iter()
                .map(|s| Solid { mesh: Arc::new(s.mesh.transformed(xf)), role: s.role })
                .collect(),
            gems: self
                .gems
                .iter()
                .map(|g| {
                    let mut x = *xf * g.xform;
                    // keep gem frames right-handed when mirrored (flip local X)
                    if x.matrix3.determinant() < 0.0 {
                        x.matrix3.x_axis = -x.matrix3.x_axis;
                    }
                    GemInstance { spec: g.spec.clone(), xform: x }
                })
                .collect(),
            chains: self.chains.clone(),
            rail: self.rail,
            shank: self.shank.clone(),
            curves: self.curves.iter().map(|c| c.iter().map(|p| xf.transform_point3(*p)).collect()).collect(),
        }
    }

    /// Appends another output, re-indexing chains.
    pub fn extend(&mut self, o: &Output) {
        let base = self.gems.len();
        self.solids.extend(o.solids.iter().cloned());
        self.gems.extend(o.gems.iter().cloned());
        self.chains.extend(o.chains.iter().map(|c| Chain {
            indices: c.indices.iter().map(|i| i + base).collect(),
            closed: c.closed,
        }));
        self.curves.extend(o.curves.iter().cloned());
        if self.rail.is_none() {
            self.rail = o.rail;
        }
        if self.shank.is_none() {
            self.shank = o.shank.clone();
        }
    }
}

pub type OpResult = Result<Output, String>;

/// Smoothstep used to blend station values.
pub(crate) fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn deg(v: f64) -> f64 {
    v.to_radians()
}
