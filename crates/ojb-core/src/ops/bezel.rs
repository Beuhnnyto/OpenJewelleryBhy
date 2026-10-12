//! Bezel builder (gvBezel): a closed wall around the girdle with a seat.

use super::{OpResult, Output, Solid};
use crate::document::NodeId;
use crate::mesh::Mesh;
use glam::DVec2;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BezelParams {
    pub gem: NodeId,
    pub gem_index: usize,
    /// Clearance between girdle and inner wall (mm).
    pub girdle_offset: f64,
    /// Wall thickness at the top (mm).
    pub top_thickness: f64,
    /// Wall thickness at the bottom (mm).
    pub bottom_thickness: f64,
    /// Height of the top edge above the girdle (mm).
    pub above_girdle: f64,
    /// Total height of the bezel (mm).
    pub height: f64,
    /// Width of the seat ledge under the girdle (mm).
    pub seat: f64,
    /// Rounding of the top edge (mm).
    pub dome: f64,
}

impl Default for BezelParams {
    fn default() -> Self {
        BezelParams {
            gem: NodeId::default(),
            gem_index: 0,
            girdle_offset: 0.02,
            top_thickness: 0.6,
            bottom_thickness: 0.8,
            above_girdle: 0.7,
            height: 3.0,
            seat: 0.45,
            dome: 0.15,
        }
    }
}

pub fn eval(p: &BezelParams, input: &Output) -> OpResult {
    let g = input.gems.get(p.gem_index).ok_or("input has no gem at that index")?;
    let h = g.spec.heights();
    let top = h.girdle_half + p.above_girdle;
    let bottom = top - p.height.max(h.girdle_half * 2.0 + p.above_girdle + 0.2);
    let seat_z = -h.girdle_half;
    let ledge_z = (seat_z - p.seat * 0.8).max(bottom + 0.05);
    let go = p.girdle_offset;
    let section = vec![
        DVec2::new(go, top),
        DVec2::new(go, seat_z),
        DVec2::new(go - p.seat, ledge_z),
        DVec2::new(go - p.seat, bottom),
        DVec2::new(go + p.bottom_thickness, bottom),
        DVec2::new(go + p.top_thickness, top),
        DVec2::new(go + p.top_thickness * 0.5, top + p.dome),
    ];
    let outline = g.spec.outline(160);
    let mesh = Mesh::sweep_closed_planar(&outline, &section);
    Ok(Output { solids: vec![Solid::metal(mesh.transformed(&g.xform))], ..Default::default() })
}
