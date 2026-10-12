//! Halo builder (gvHalo): melee around a centre gem, with a metal frame and
//! shared prongs.

use super::prongs::prong;
use super::{Chain, GemInstance, OpResult, Output, Solid};
use crate::document::NodeId;
use crate::gem::{GemMaterial, GemSpec};
use crate::mesh::{offset_polygon, polyline_length_2d, resample_closed_2d, Mesh};
use glam::{DAffine3, DVec2, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HaloParams {
    pub gem: NodeId,
    pub gem_index: usize,
    /// Melee diameter (mm).
    pub stone: f64,
    pub material: GemMaterial,
    /// Gap between the centre girdle and the melee girdles (mm).
    pub spacing_from_center: f64,
    /// Minimum gap between melee (mm).
    pub gem_spacing: f64,
    /// Melee girdle height relative to the centre girdle (mm).
    pub z_offset: f64,
    pub metal: bool,
    /// Metal frame height under the melee girdle (mm).
    pub metal_height: f64,
    /// Frame wall beyond the melee girdle on each side (mm).
    pub wall: f64,
    pub prongs: bool,
    pub prong_diameter: f64,
}

impl Default for HaloParams {
    fn default() -> Self {
        HaloParams {
            gem: NodeId::default(),
            gem_index: 0,
            stone: 1.3,
            material: GemMaterial::Diamond,
            spacing_from_center: 0.25,
            gem_spacing: 0.12,
            z_offset: -0.9,
            metal: true,
            metal_height: 1.2,
            wall: 0.25,
            prongs: true,
            prong_diameter: 0.45,
        }
    }
}

pub fn eval(p: &HaloParams, input: &Output) -> OpResult {
    let c = input.gems.get(p.gem_index).ok_or("input has no gem at that index")?;
    if p.stone <= 0.2 {
        return Err("melee size too small".into());
    }
    let r = p.stone / 2.0;
    let outline = c.spec.outline(192);
    let path = offset_polygon(&outline, p.spacing_from_center + r);
    let len = polyline_length_2d(&path, true);
    let count = (len / (p.stone + p.gem_spacing)).floor() as usize;
    if count < 3 {
        return Err("halo too small for that melee size".into());
    }
    let centers = resample_closed_2d(&path, count);
    let mut spec = GemSpec::round(p.stone);
    spec.material = p.material;
    let sh = spec.heights();
    let gems: Vec<GemInstance> = centers
        .iter()
        .map(|q| GemInstance {
            spec: spec.clone(),
            xform: c.xform * DAffine3::from_translation(DVec3::new(q.x, q.y, p.z_offset)),
        })
        .collect();

    let mut solids = Vec::new();
    if p.metal {
        let frame_path = resample_closed_2d(&path, 256);
        let half = r + p.wall;
        let top = p.z_offset - sh.girdle_half;
        let bottom = top - p.metal_height;
        let section = vec![
            DVec2::new(-half, bottom),
            DVec2::new(half, bottom),
            DVec2::new(half, top),
            DVec2::new(-half, top),
        ];
        let m = Mesh::sweep_closed_planar(&frame_path, &section);
        solids.push(Solid::metal(m.transformed(&c.xform)));
    }
    if p.prongs {
        // shared prongs on the inner and outer side between neighbours
        let normals = crate::mesh::outward_normals_2d(&centers);
        for i in 0..count {
            let a = centers[i];
            let b = centers[(i + 1) % count];
            let mid = (a + b) / 2.0;
            let n = (normals[i] + normals[(i + 1) % count]).normalize_or(DVec2::X);
            let side = r * 0.78;
            for s in [-1.0, 1.0] {
                let q = mid + n * side * s;
                let base = DVec3::new(q.x, q.y, p.z_offset - sh.girdle_half - p.metal_height * 0.5);
                let tip = DVec3::new(q.x, q.y, p.z_offset + sh.girdle_half + p.stone * 0.18);
                let m = prong(base, tip, p.prong_diameter / 2.0, p.prong_diameter / 2.0, 1.0);
                solids.push(Solid::metal(m.transformed(&c.xform)));
            }
        }
    }
    Ok(Output {
        gems,
        chains: vec![Chain { indices: (0..count).collect(), closed: true }],
        solids,
        ..Default::default()
    })
}
