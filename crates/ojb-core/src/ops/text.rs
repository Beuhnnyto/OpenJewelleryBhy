//! Text builders: engraved / embossed text on the ring (inside or outside)
//! and free solid text.

use super::{deg, OpResult, Output, Rail, Solid};
use crate::document::NodeId;
use crate::mesh::Mesh;
use crate::text::{extrude, layout, Font};
use glam::{DAffine3, DQuat, DVec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextMode {
    /// Produces a cutter (use with a Boolean difference).
    Engrave,
    /// Produces metal.
    Emboss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingSurface {
    Inside,
    Outside,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextOnRailParams {
    /// Ring rail or shank.
    pub rail: NodeId,
    pub text: String,
    pub font: Font,
    /// Cap height (mm).
    pub size: f64,
    pub depth: f64,
    pub spacing: f64,
    /// Centre of the text on the rail (0 = top, 0.5 = bottom).
    pub t: f64,
    pub surface: RingSurface,
    pub mode: TextMode,
    /// Shift across the band (mm).
    pub offset: f64,
    /// Mirror the reading direction.
    pub flip: bool,
}

impl Default for TextOnRailParams {
    fn default() -> Self {
        TextOnRailParams {
            rail: NodeId::default(),
            text: "Forever".into(),
            font: Font::Serif,
            size: 1.2,
            depth: 0.25,
            spacing: 0.08,
            t: 0.5,
            surface: RingSurface::Inside,
            mode: TextMode::Engrave,
            offset: 0.0,
            flip: false,
        }
    }
}

pub fn eval_on_rail(p: &TextOnRailParams, input: &Output) -> OpResult {
    let rail = input.rail.ok_or("input has no ring rail")?;
    if p.text.trim().is_empty() {
        return Err("empty text".into());
    }
    let regions = layout(&p.text, p.font, p.size, p.spacing);
    let (z0, z1) = match (p.surface, p.mode) {
        (RingSurface::Inside, TextMode::Engrave) => (-0.3, p.depth),
        (RingSurface::Inside, TextMode::Emboss) => (-p.depth, 0.15),
        (RingSurface::Outside, TextMode::Engrave) => (-p.depth, 0.3),
        (RingSurface::Outside, TextMode::Emboss) => (-0.15, p.depth),
    };
    let mut flat = extrude(&regions, z1 - z0, 0.12);
    let base_r = match p.surface {
        RingSurface::Inside => rail.radius,
        RingSurface::Outside => rail.radius + input.shank.as_ref().map(|s| s.thickness(p.t)).unwrap_or(0.0),
    };
    let circ = std::f64::consts::TAU * base_r;
    if regions.iter().flat_map(|r| r.outer.iter()).map(|q| q.x.abs()).fold(0.0, f64::max) * 2.0 > circ * 0.95 {
        return Err("text is longer than the ring".into());
    }
    let mut sgn = if p.surface == RingSurface::Inside { -1.0 } else { 1.0 };
    if p.flip {
        sgn = -sgn;
    }
    for v in &mut flat.positions {
        let t = p.t + sgn * v.x / circ;
        // inner surface: metal is outward (+N); outer surface: metal is inward
        let r = match p.surface {
            RingSurface::Inside => base_r + z0 + v.z,
            RingSurface::Outside => base_r + z0 + v.z,
        };
        *v = Rail::normal(t) * r + DVec3::Y * (v.y + p.offset);
    }
    flat.ensure_outward();
    let solid = match p.mode {
        TextMode::Engrave => Solid::cutter(flat),
        TextMode::Emboss => Solid::metal(flat),
    };
    Ok(Output { solids: vec![solid], ..Default::default() })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Text3dParams {
    pub text: String,
    pub font: Font,
    pub size: f64,
    pub depth: f64,
    pub spacing: f64,
    pub position: [f64; 3],
    pub rotation: [f64; 3],
    pub mode: TextMode,
}

impl Default for Text3dParams {
    fn default() -> Self {
        Text3dParams {
            text: "OJB".into(),
            font: Font::Sans,
            size: 4.0,
            depth: 1.0,
            spacing: 0.2,
            position: [0.0; 3],
            rotation: [0.0; 3],
            mode: TextMode::Emboss,
        }
    }
}

pub fn eval_3d(p: &Text3dParams) -> OpResult {
    if p.text.trim().is_empty() {
        return Err("empty text".into());
    }
    let m = text_mesh(&p.text, p.font, p.size, p.depth, p.spacing);
    let xf = DAffine3::from_rotation_translation(
        DQuat::from_euler(glam::EulerRot::XYZ, deg(p.rotation[0]), deg(p.rotation[1]), deg(p.rotation[2])),
        DVec3::from(p.position),
    );
    let m = m.transformed(&xf);
    Ok(Output {
        solids: vec![match p.mode {
            TextMode::Engrave => Solid::cutter(m),
            TextMode::Emboss => Solid::metal(m),
        }],
        ..Default::default()
    })
}

pub fn text_mesh(text: &str, font: Font, size: f64, depth: f64, spacing: f64) -> Mesh {
    extrude(&layout(text, font, size, spacing), depth, 0.0)
}
