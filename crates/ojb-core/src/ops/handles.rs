//! Viewport control handles (MatrixGold "VPCH"): points the user drags in
//! the 3D view. Each handle moves along one world axis and maps the drag
//! distance (mm) onto one parameter.

use super::{Output, Rail};
use crate::document::Op;
use crate::ops::gems::Placement;
use glam::DVec3;
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub struct Handle {
    pub id: u32,
    pub pos: DVec3,
    /// Unit drag direction in world space.
    pub axis: DVec3,
    pub label: &'static str,
}

fn h(id: u32, pos: DVec3, axis: DVec3, label: &'static str) -> Handle {
    Handle { id, pos, axis: axis.normalize_or(DVec3::Z), label }
}

fn xyz(id0: u32, at: DVec3, len: f64) -> [Handle; 3] {
    [
        h(id0, at + DVec3::X * len, DVec3::X, "X"),
        h(id0 + 1, at + DVec3::Y * len, DVec3::Y, "Y"),
        h(id0 + 2, at + DVec3::Z * len, DVec3::Z, "Z"),
    ]
}

/// `out` is the node's own output, `input` the output of its first input.
pub fn handles(op: &Op, out: &Output, input: Option<&Output>) -> Vec<Handle> {
    let rail_in = input.and_then(|i| i.rail);
    match op {
        Op::RingRail(_) => out.rail.map(|r| vec![h(0, r.point(0.0), DVec3::Z, "finger size")]).unwrap_or_default(),
        Op::Shank(p) => {
            let Some(info) = &out.shank else { return vec![] };
            let r = info.rail;
            p.stations
                .iter()
                .enumerate()
                .flat_map(|(i, s)| {
                    let n = Rail::normal(s.pos);
                    let base = r.point(s.pos);
                    let i = i as u32;
                    [
                        h(i * 3, base + n * s.thickness, n, "thickness"),
                        h(i * 3 + 1, base + n * (s.thickness / 2.0) + DVec3::Y * (s.width / 2.0), DVec3::Y, "width"),
                        h(i * 3 + 2, base - n * 0.4, Rail::tangent(s.pos), "position"),
                    ]
                })
                .collect()
        }
        Op::Gem(p) => {
            let Some(g) = out.gems.first() else { return vec![] };
            let s = g.spec.width.max(g.spec.length) * 0.75;
            match p.placement {
                Placement::OnRail { .. } => vec![
                    h(0, g.center() + g.up() * (g.spec.heights().table + 0.6), g.up(), "height"),
                    h(1, g.center() + g.xform.matrix3.x_axis.normalize() * s, g.xform.matrix3.x_axis, "slide on rail"),
                ],
                Placement::Free { .. } => xyz(0, g.center(), s).to_vec(),
            }
        }
        Op::GemsOnRail(_) => {
            let (Some(ch), Some(r)) = (out.chains.first(), out.rail.or(rail_in)) else { return vec![] };
            let _ = r;
            let first = &out.gems[ch.indices[0]];
            let last = &out.gems[*ch.indices.last().unwrap()];
            vec![
                h(0, first.center() - first.xform.matrix3.x_axis.normalize() * first.spec.width, first.xform.matrix3.x_axis, "start"),
                h(1, last.center() + last.xform.matrix3.x_axis.normalize() * last.spec.width, last.xform.matrix3.x_axis, "end"),
                h(2, first.center() + first.up() * 1.2, first.up(), "height"),
            ]
        }
        Op::Head(p) => {
            let Some(g) = input.and_then(|i| i.gems.get(p.gem_index)) else { return vec![] };
            let hh = g.spec.heights();
            let side = g.xform.matrix3.y_axis.normalize() * (g.spec.length / 2.0 + p.tip_diameter);
            vec![
                h(0, g.center() + side + g.up() * (hh.girdle_half + p.above_girdle), g.up(), "prong height"),
                h(1, g.center() - g.up() * (hh.girdle_half + p.depth), -g.up(), "head depth"),
            ]
        }
        Op::Bezel(p) => {
            let Some(g) = input.and_then(|i| i.gems.get(p.gem_index)) else { return vec![] };
            let side = g.xform.matrix3.y_axis.normalize() * (g.spec.length / 2.0 + p.top_thickness);
            vec![h(0, g.center() + side + g.up() * (g.spec.heights().girdle_half + p.above_girdle), g.up(), "bezel height")]
        }
        Op::Halo(_) => {
            let Some(m) = out.gems.first() else { return vec![] };
            let Some(c) = input.and_then(|i| i.gems.first()) else { return vec![] };
            let radial = (m.center() - c.center()).normalize_or_zero();
            vec![h(0, m.center() + radial * m.spec.width, radial, "spacing"), h(1, m.center() + c.up() * 1.0, c.up(), "height")]
        }
        Op::Scoop(super::cutters::ScoopParams { start, end, .. })
        | Op::Azure(super::cutters::AzureParams { start, end, .. }) => {
            let Some(info) = input.and_then(|i| i.shank.as_ref()) else { return vec![] };
            [(*start, 0u32), (*end, 1)]
                .iter()
                .map(|(t, id)| h(*id, info.rail.point(*t) - Rail::normal(*t) * 0.5, Rail::tangent(*t), if *id == 0 { "start" } else { "end" }))
                .collect()
        }
        Op::TextOnRail(p) => {
            let Some(r) = rail_in else { return vec![] };
            vec![h(0, r.point(p.t) - Rail::normal(p.t) * 0.6, Rail::tangent(p.t), "slide")]
        }
        Op::Signet(p) => {
            let Some(r) = rail_in else { return vec![] };
            let top = DVec3::Z * (r.radius + p.height + p.dome);
            vec![
                h(0, top + DVec3::Z * 0.5, DVec3::Z, "height"),
                h(1, top + DVec3::X * (p.top_width / 2.0), DVec3::X, "width"),
                h(2, top + DVec3::Y * (p.top_length / 2.0), DVec3::Y, "length"),
            ]
        }
        Op::Text3d(p) => xyz(0, DVec3::from(p.position), 2.0).to_vec(),
        Op::Bail(p) => xyz(0, DVec3::from(p.position), 2.0).to_vec(),
        Op::Import(p) => xyz(0, DVec3::from(p.position), 3.0).to_vec(),
        _ => vec![],
    }
}

/// Applies a drag of `d` mm along handle `id`.
pub fn apply(op: &mut Op, id: u32, d: f64, out: &Output, input: Option<&Output>) {
    let rail_r = input.and_then(|i| i.rail).or(out.rail).map(|r| r.radius).unwrap_or(8.0);
    let circ = TAU * rail_r.max(1.0);
    match op {
        Op::RingRail(p) => {
            let dia = (p.size.diameter() + 2.0 * d).clamp(8.0, 35.0);
            p.size.value = p.size.region.from_diameter(dia);
        }
        Op::Shank(p) => {
            let (i, k) = ((id / 3) as usize, id % 3);
            if let Some(s) = p.stations.get_mut(i) {
                match k {
                    0 => s.thickness = (s.thickness + d).clamp(0.3, 15.0),
                    1 => s.width = (s.width + 2.0 * d).clamp(0.3, 30.0),
                    _ => s.pos = (s.pos + d / circ).rem_euclid(1.0),
                }
            }
        }
        Op::Gem(p) => match &mut p.placement {
            Placement::OnRail { t, height, .. } => match id {
                0 => *height += d,
                _ => *t = (*t + d / (TAU * (rail_r + *height).max(1.0))).rem_euclid(1.0),
            },
            Placement::Free { position, .. } => position[id.min(2) as usize] += d,
        },
        Op::GemsOnRail(p) => match id {
            0 => p.start = (p.start - d / circ).clamp(0.0, 1.0),
            1 => p.end = (p.end + d / circ).clamp(0.0, 1.0),
            _ => p.height += d,
        },
        Op::Head(p) => match id {
            0 => p.above_girdle = (p.above_girdle + d).max(0.0),
            _ => p.depth = (p.depth + d).max(0.5),
        },
        Op::Bezel(p) => p.above_girdle = (p.above_girdle + d).max(0.0),
        Op::Halo(p) => match id {
            0 => p.spacing_from_center += d,
            _ => p.z_offset += d,
        },
        Op::Scoop(super::cutters::ScoopParams { start, end, .. }) | Op::Azure(super::cutters::AzureParams { start, end, .. }) => {
            let t = if id == 0 { start } else { end };
            *t = (*t + d / circ).clamp(0.0, 1.0);
        }
        Op::TextOnRail(p) => p.t = (p.t + d / circ).rem_euclid(1.0),
        Op::Signet(p) => match id {
            0 => p.height = (p.height + d).max(0.6),
            1 => p.top_width = (p.top_width + 2.0 * d).max(1.5),
            _ => p.top_length = (p.top_length + 2.0 * d).max(1.5),
        },
        Op::Text3d(p) => p.position[id.min(2) as usize] += d,
        Op::Bail(p) => p.position[id.min(2) as usize] += d,
        Op::Import(p) => p.position[id.min(2) as usize] += d,
        _ => {}
    }
}
