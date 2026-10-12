//! Signet / award ring builder (MatrixGold 4.0 "Award Ring Builder"):
//! a shank rising into a shaped top plate, with optional engraved or
//! embossed text and an engraved border.

use super::shank::{ShankInfo, ShankParams, Station};
use super::text::TextMode;
use super::{OpResult, Output, Rail, Solid};
use crate::csg::{boolean, BoolOp};
use crate::document::NodeId;
use crate::mesh::{offset_polygon, Mesh};
use crate::outline::Shape;
use crate::profile::Profile;
use crate::text::{extrude, layout, Font};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SignetParams {
    pub rail: NodeId,
    pub shape: Shape,
    /// Top size across the finger direction... X (along the rail tangent).
    pub top_width: f64,
    /// Top size along the finger (Y).
    pub top_length: f64,
    /// Height of the top face above the rail (mm).
    pub height: f64,
    /// Dome of the top face (mm).
    pub dome: f64,
    pub shank_width: f64,
    pub shank_thickness: f64,
    pub text: String,
    pub font: Font,
    pub text_size: f64,
    pub text_depth: f64,
    pub text_mode: TextMode,
    /// Engraved border inset from the edge (mm, 0 = none).
    pub border: f64,
}

impl Default for SignetParams {
    fn default() -> Self {
        SignetParams {
            rail: NodeId::default(),
            shape: Shape::Cushion,
            top_width: 11.0,
            top_length: 12.0,
            height: 3.2,
            dome: 0.2,
            shank_width: 3.0,
            shank_thickness: 1.8,
            text: "AB".into(),
            font: Font::Serif,
            text_size: 4.5,
            text_depth: 0.4,
            text_mode: TextMode::Engrave,
            border: 0.6,
        }
    }
}

pub fn eval(p: &SignetParams, input: &Output) -> OpResult {
    let rail = input.rail.ok_or("input has no ring rail")?;
    if p.top_width <= 1.0 || p.top_length <= 1.0 || p.height <= 0.5 {
        return Err("signet top too small".into());
    }
    let r = rail.radius;
    // shank that widens and thickens into the top
    let half_ang = (p.top_width / 2.0 / (r + p.height)).asin() / std::f64::consts::TAU;
    let sp = ShankParams {
        rail: p.rail,
        stations: vec![
            Station { pos: 0.0, width: p.top_length * 0.8, thickness: p.height * 0.9, profile: Profile::Flat },
            Station { pos: half_ang * 1.1, width: p.top_length * 0.7, thickness: p.height * 0.7, profile: Profile::Comfort },
            Station { pos: (half_ang * 2.6).min(0.3), width: p.shank_width, thickness: p.shank_thickness, profile: Profile::Comfort },
            Station { pos: 0.5, width: p.shank_width * 0.9, thickness: p.shank_thickness * 0.85, profile: Profile::Comfort },
        ],
        symmetric: true,
        samples: 256,
        profile_points: 32,
    };
    let info = ShankInfo::new(rail, &sp)?;
    let rings: Vec<Vec<DVec3>> = (0..sp.samples).map(|i| info.ring(i as f64 / sp.samples as f64, 32)).collect();
    let shank = Mesh::loft(&rings, true, None, None);

    // top plate: outline in the XY plane at the top of the ring
    let outline = p.shape.outline(p.top_width, p.top_length, 128);
    let top_z = r + p.height;
    let ring = |s: f64, z: f64| -> Vec<DVec3> { outline.iter().map(|q| DVec3::new(q.x * s, q.y * s, z)).collect() };
    let bottom_z = r + 0.35;
    let plate = Mesh::loft(
        &[ring(0.8, bottom_z), ring(0.97, top_z - p.height * 0.35), ring(1.0, top_z - 0.25), ring(0.985, top_z - 0.05)],
        false,
        Some(DVec3::new(0.0, 0.0, bottom_z)),
        Some(DVec3::new(0.0, 0.0, top_z + p.dome)),
    );
    let mut metal = boolean(&[&shank, &plate], &[], BoolOp::Union);

    let mut cut: Vec<Mesh> = Vec::new();
    let mut add: Vec<Mesh> = Vec::new();
    if p.border > 0.0 {
        let outer = offset_polygon(&outline, -p.border);
        let inner = offset_polygon(&outline, -p.border - 0.35);
        let groove = ring_band(&outer, &inner, top_z - 0.25, top_z + p.dome + 0.5);
        cut.push(groove);
    }
    if !p.text.trim().is_empty() {
        let regions = layout(&p.text, p.font, p.text_size, p.text_size * 0.06);
        let (z0, depth) = match p.text_mode {
            TextMode::Engrave => (top_z + p.dome * 0.5 - p.text_depth, p.text_depth + 1.0),
            TextMode::Emboss => (top_z - 0.1, p.text_depth + p.dome + 0.1),
        };
        let mut m = extrude(&regions, depth, 0.0);
        for v in &mut m.positions {
            v.z += z0;
        }
        match p.text_mode {
            TextMode::Engrave => cut.push(m),
            TextMode::Emboss => add.push(m),
        }
    }
    if !add.is_empty() {
        let mut a: Vec<&Mesh> = vec![&metal];
        a.extend(add.iter());
        metal = boolean(&a, &[], BoolOp::Union);
    }
    if !cut.is_empty() {
        let b: Vec<&Mesh> = cut.iter().collect();
        metal = boolean(&[&metal], &b, BoolOp::Difference);
    }
    Ok(Output {
        solids: vec![Solid::metal(metal)],
        rail: Some(rail),
        shank: Some(info),
        curves: vec![Rail { radius: r }.polyline(128)],
        ..Default::default()
    })
}

/// Closed band between two nested outlines, from z0 to z1.
fn ring_band(outer: &[DVec2], inner: &[DVec2], z0: f64, z1: f64) -> Mesh {
    let n = outer.len();
    let mut m = Mesh::new();
    let at = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
    for (k, (pts, z)) in [(outer, z0), (outer, z1), (inner, z1), (inner, z0)].into_iter().enumerate() {
        let _ = k;
        m.positions.extend(pts.iter().map(|p| at(*p, z)));
    }
    let idx = |ring: usize, i: usize| (ring * n + i % n) as u32;
    for ring in 0..4 {
        let next = (ring + 1) % 4;
        for i in 0..n {
            m.triangles.push([idx(ring, i), idx(ring, i + 1), idx(next, i + 1)]);
            m.triangles.push([idx(ring, i), idx(next, i + 1), idx(next, i)]);
        }
    }
    m.ensure_outward();
    m
}
