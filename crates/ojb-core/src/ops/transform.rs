//! Transform builders: polar array (gvPolarArray) and dynamic mirror
//! (gvDynamicMirror). Both keep full parametric history of their source.

use super::{deg, OpResult, Output};
use crate::document::NodeId;
use glam::{DAffine3, DMat3, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn vec(self) -> DVec3 {
        match self {
            Axis::X => DVec3::X,
            Axis::Y => DVec3::Y,
            Axis::Z => DVec3::Z,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PolarArrayParams {
    pub source: NodeId,
    pub count: usize,
    pub axis: Axis,
    /// Total sweep in degrees (360 = full circle, evenly spaced).
    pub angle: f64,
    pub start_angle: f64,
}

impl Default for PolarArrayParams {
    fn default() -> Self {
        PolarArrayParams { source: NodeId::default(), count: 6, axis: Axis::Z, angle: 360.0, start_angle: 0.0 }
    }
}

pub fn eval_polar(p: &PolarArrayParams, src: &Output) -> OpResult {
    if p.count == 0 || p.count > 512 {
        return Err("count must be between 1 and 512".into());
    }
    let full = (p.angle.abs() - 360.0).abs() < 1e-9;
    let step = if full || p.count == 1 { p.angle / p.count as f64 } else { p.angle / (p.count - 1) as f64 };
    let mut out = Output::default();
    for i in 0..p.count {
        let a = deg(p.start_angle + step * i as f64);
        let xf = DAffine3::from_axis_angle(p.axis.vec(), a);
        out.extend(&src.transformed(&xf));
    }
    out.rail = src.rail;
    out.shank = src.shank.clone();
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MirrorParams {
    pub source: NodeId,
    /// Mirror across the YZ plane (x → -x).
    pub x: bool,
    /// Mirror across the XZ plane (y → -y).
    pub y: bool,
    /// Mirror across the XY plane (z → -z).
    pub z: bool,
}

impl Default for MirrorParams {
    fn default() -> Self {
        MirrorParams { source: NodeId::default(), x: true, y: false, z: false }
    }
}

pub fn eval_mirror(p: &MirrorParams, src: &Output) -> OpResult {
    let mut out = src.clone();
    for (on, diag) in [(p.x, DVec3::new(-1.0, 1.0, 1.0)), (p.y, DVec3::new(1.0, -1.0, 1.0)), (p.z, DVec3::new(1.0, 1.0, -1.0))] {
        if on {
            let xf = DAffine3::from_mat3(DMat3::from_diagonal(diag));
            out.extend(&src.transformed(&xf));
        }
    }
    Ok(out)
}
