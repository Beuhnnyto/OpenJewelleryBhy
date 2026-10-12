//! More builders: pearl cup, bail, split / bypass shank, rope, deformers
//! (twist, taper), gems on surface and imported meshes.

use super::shank::{ShankInfo, ShankParams, Station};
use super::transform::Axis;
use super::{deg, smooth, Chain, GemInstance, OpResult, Output, Rail, Role, Solid};
use crate::document::NodeId;
use crate::gem::GemSpec;
use crate::mesh::Mesh;
use crate::profile::Profile;
use glam::{DAffine3, DMat3, DQuat, DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

// ------------------------------------------------------------------ pearl cup

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PearlCupParams {
    pub gem: NodeId,
    pub gem_index: usize,
    /// How much of the pearl the cup covers (% of the diameter).
    pub cup_height: f64,
    pub thickness: f64,
    /// Peg inside the pearl (% of the diameter).
    pub peg_height: f64,
    pub peg_diameter: f64,
    /// Earring post below the cup.
    pub post: bool,
    pub post_length: f64,
    pub post_diameter: f64,
}

impl Default for PearlCupParams {
    fn default() -> Self {
        PearlCupParams {
            gem: NodeId::default(),
            gem_index: 0,
            cup_height: 28.0,
            thickness: 0.45,
            peg_height: 45.0,
            peg_diameter: 0.9,
            post: true,
            post_length: 10.0,
            post_diameter: 0.8,
        }
    }
}

/// gvPearlCup: a spherical cap under the pearl, a peg into it and an
/// optional post.
pub fn eval_pearl_cup(p: &PearlCupParams, input: &Output) -> OpResult {
    let g = input.gems.get(p.gem_index).ok_or("input has no gem at that index")?;
    let r = g.spec.width / 2.0;
    let h = (p.cup_height / 100.0 * 2.0 * r).clamp(0.2, 2.0 * r);
    let seg = 48;
    let ring = |rad: f64, z: f64| -> Vec<DVec3> {
        (0..seg).map(|i| DVec3::new((TAU * i as f64 / seg as f64).cos() * rad, (TAU * i as f64 / seg as f64).sin() * rad, z)).collect()
    };
    // inner surface follows the pearl, outer surface offset by thickness
    let steps = 10;
    let mut outer = Vec::new();
    let mut inner = Vec::new();
    for i in 0..=steps {
        let z = -r + h * i as f64 / steps as f64; // from bottom of pearl up
        let rin = (r * r - z * z).max(0.0).sqrt();
        let ro = ((r + p.thickness).powi(2) - z * z).max(0.0).sqrt();
        inner.push(ring(rin.max(1e-3), z));
        outer.push(ring(ro, z - p.thickness * (1.0 - i as f64 / steps as f64)));
    }
    let mut rings: Vec<Vec<DVec3>> = inner.into_iter().rev().collect();
    rings.extend(outer);
    let cup = Mesh::loft(&rings, false, Some(DVec3::new(0.0, 0.0, -r)), Some(DVec3::new(0.0, 0.0, -r - p.thickness)));
    let mut solids = vec![Solid::metal(cup.transformed(&g.xform))];
    let peg_top = -r + 2.0 * r * p.peg_height / 100.0;
    let peg = Mesh::tube(
        &[DVec3::new(0.0, 0.0, -r - p.thickness * 0.5), DVec3::new(0.0, 0.0, peg_top)],
        &[p.peg_diameter / 2.0; 2],
        16,
        0.0,
        0.6,
    );
    solids.push(Solid::metal(peg.transformed(&g.xform)));
    if p.post {
        let post = Mesh::tube(
            &[DVec3::new(0.0, 0.0, -r - p.thickness * 0.5), DVec3::new(0.0, 0.0, -r - p.thickness - p.post_length)],
            &[p.post_diameter / 2.0; 2],
            16,
            0.0,
            0.5,
        );
        solids.push(Solid::metal(post.transformed(&g.xform)));
    }
    Ok(Output { solids, ..Default::default() })
}

// ------------------------------------------------------------------- bail

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BailParams {
    /// Attachment point (bottom of the bail).
    pub position: [f64; 3],
    /// Euler rotation in degrees (X, Y, Z). The loop rises along +Z by default.
    pub rotation: [f64; 3],
    pub inner_diameter: f64,
    pub width: f64,
    pub thickness: f64,
    pub jump_ring: bool,
    pub jump_ring_inner: f64,
    pub jump_ring_wire: f64,
}

impl Default for BailParams {
    fn default() -> Self {
        BailParams {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0; 3],
            inner_diameter: 3.2,
            width: 2.2,
            thickness: 0.9,
            jump_ring: true,
            jump_ring_inner: 2.6,
            jump_ring_wire: 0.7,
        }
    }
}

/// gvBail: a loop with a rounded rectangular section, plus a jump ring.
pub fn eval_bail(p: &BailParams) -> OpResult {
    let ri = p.inner_diameter / 2.0;
    let rc = ri + p.thickness / 2.0;
    // loop in the YZ plane (chain runs along X), open at the bottom
    let gap = 0.35;
    let n = 64;
    let sec = rounded_rect(p.thickness, p.width, 12);
    let centre = DVec3::new(0.0, 0.0, rc + p.thickness * 0.2);
    let mut rings = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let a = -PI / 2.0 + gap + (TAU - 2.0 * gap) * i as f64 / n as f64;
        let nrm = DVec3::new(0.0, a.cos(), a.sin());
        let c = centre + nrm * rc;
        rings.push(sec.iter().map(|q| c + nrm * q.x + DVec3::X * q.y).collect::<Vec<_>>());
    }
    let c0 = rings[0].iter().copied().sum::<DVec3>() / sec.len() as f64;
    let c1 = rings[n].iter().copied().sum::<DVec3>() / sec.len() as f64;
    let bail = Mesh::loft(&rings, false, Some(c0), Some(c1));
    let xf = DAffine3::from_rotation_translation(
        DQuat::from_euler(glam::EulerRot::XYZ, deg(p.rotation[0]), deg(p.rotation[1]), deg(p.rotation[2])),
        DVec3::from(p.position),
    );
    let mut solids = vec![Solid::metal(bail.transformed(&xf))];
    // foot joining the loop ends to the piece
    let foot = Mesh::boxed(DVec3::new(-p.width / 2.0, -rc * 0.5, -0.4), DVec3::new(p.width / 2.0, rc * 0.5, p.thickness * 0.8));
    solids.push(Solid::metal(foot.transformed(&xf)));
    if p.jump_ring {
        let jr = p.jump_ring_inner / 2.0 + p.jump_ring_wire / 2.0;
        let jc = centre + DVec3::Z * (rc + p.thickness / 2.0 - jr * 0.15);
        let path: Vec<DVec2> = (0..96).map(|i| DVec2::from_angle(TAU * i as f64 / 96.0) * jr).collect();
        let sec: Vec<DVec2> = (0..16).map(|i| DVec2::from_angle(TAU * i as f64 / 16.0) * (p.jump_ring_wire / 2.0)).collect();
        // torus in the XZ plane, threaded through the bail
        let t = Mesh::sweep_closed_planar(&path, &sec);
        let rot = DAffine3::from_translation(jc + DVec3::Z * jr) * DAffine3::from_rotation_x(PI / 2.0);
        solids.push(Solid::metal(t.transformed(&(xf * rot))));
    }
    Ok(Output { solids, ..Default::default() })
}

fn rounded_rect(w: f64, h: f64, n_corner: usize) -> Vec<DVec2> {
    let r = (w.min(h) * 0.35).max(1e-3);
    let mut out = Vec::new();
    for (cx, cy, a0) in [(w / 2.0 - r, -h / 2.0 + r, -90.0f64), (w / 2.0 - r, h / 2.0 - r, 0.0), (-w / 2.0 + r, h / 2.0 - r, 90.0), (-w / 2.0 + r, -h / 2.0 + r, 180.0)] {
        for k in 0..n_corner {
            let a = (a0 + 90.0 * k as f64 / (n_corner - 1) as f64).to_radians();
            out.push(DVec2::new(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    out
}

// ------------------------------------------------------------- split shank

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitMode {
    /// Two strands that separate towards the top (gvSplitShank).
    Split,
    /// Ends that pass each other at the top (gvBypassRingRail).
    Bypass,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SplitShankParams {
    pub rail: NodeId,
    pub mode: SplitMode,
    pub width: f64,
    pub thickness: f64,
    pub bottom_width: f64,
    pub bottom_thickness: f64,
    /// Total separation at the top (mm).
    pub gap: f64,
    /// Portion of the rail over which the strands separate (0..0.5).
    pub span: f64,
    pub profile: Profile,
}

impl Default for SplitShankParams {
    fn default() -> Self {
        SplitShankParams {
            rail: NodeId::default(),
            mode: SplitMode::Split,
            width: 1.6,
            thickness: 1.7,
            bottom_width: 2.6,
            bottom_thickness: 1.6,
            gap: 2.4,
            span: 0.18,
            profile: Profile::Comfort,
        }
    }
}

pub fn eval_split(p: &SplitShankParams, input: &Output) -> OpResult {
    let rail = input.rail.ok_or("input has no ring rail")?;
    let sp = ShankParams {
        rail: p.rail,
        stations: vec![
            Station { pos: 0.0, width: p.width, thickness: p.thickness, profile: p.profile },
            Station { pos: p.span.clamp(0.02, 0.45), width: p.width, thickness: (p.thickness + p.bottom_thickness) / 2.0, profile: p.profile },
            Station { pos: 0.5, width: p.bottom_width, thickness: p.bottom_thickness, profile: p.profile },
        ],
        symmetric: true,
        samples: 256,
        profile_points: 32,
    };
    let info = ShankInfo::new(rail, &sp)?;
    let k = 32;
    let span = p.span.clamp(0.02, 0.45);
    let separation = |t: f64| -> f64 {
        let d = t.min(1.0 - t); // distance from the top
        smooth(1.0 - d / span)
    };
    let strand = |sign: f64, t0: f64, t1: f64, closed: bool, bypass: bool| -> Mesh {
        let samples = 256usize;
        let rings: Vec<Vec<DVec3>> = (0..if closed { samples } else { samples + 1 })
            .map(|i| {
                let t = t0 + (t1 - t0) * i as f64 / samples as f64;
                let tw = t.rem_euclid(1.0);
                let s = if bypass {
                    // lateral offset ramps across the overlap around the top
                    let u = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
                    let near = separation(tw);
                    (u * 2.0 - 1.0).signum() * near
                } else {
                    sign * separation(tw)
                };
                let shift = s * (p.gap / 2.0 + p.width / 2.0);
                let mut ring = info.ring(tw, k);
                for q in &mut ring {
                    q.y += shift;
                }
                ring
            })
            .collect();
        if closed {
            Mesh::loft(&rings, true, None, None)
        } else {
            let c0 = rings[0].iter().copied().sum::<DVec3>() / k as f64;
            let c1 = rings[rings.len() - 1].iter().copied().sum::<DVec3>() / k as f64;
            Mesh::tube_like_caps(rings, c0, c1)
        }
    };
    let solids = match p.mode {
        SplitMode::Split => vec![Solid::metal(strand(1.0, 0.0, 1.0, true, false)), Solid::metal(strand(-1.0, 0.0, 1.0, true, false))],
        SplitMode::Bypass => {
            // one strand running a little more than a full turn: its two
            // ends pass each other side by side at the top
            let ov = span * 0.6;
            vec![Solid::metal(strand(1.0, -ov, 1.0 + ov, false, true))]
        }
    };
    Ok(Output { solids, rail: Some(rail), shank: Some(info), curves: vec![rail.polyline(128)], ..Default::default() })
}

impl Mesh {
    /// Open loft with domed ends (used for bypass ends).
    pub fn tube_like_caps(rings: Vec<Vec<DVec3>>, c0: DVec3, c1: DVec3) -> Mesh {
        let n = rings.len();
        let d0 = (rings[0][0] - rings[1][0]).normalize_or_zero();
        let d1 = (rings[n - 1][0] - rings[n - 2][0]).normalize_or_zero();
        let mut all = Vec::with_capacity(n + 6);
        let r0: f64 = rings[0].iter().map(|q| q.distance(c0)).sum::<f64>() / rings[0].len() as f64;
        let r1: f64 = rings[n - 1].iter().map(|q| q.distance(c1)).sum::<f64>() / rings[n - 1].len() as f64;
        for k in (1..4).rev() {
            let phi = PI / 2.0 * k as f64 / 4.0;
            all.push(rings[0].iter().map(|q| c0 + (*q - c0) * phi.cos() + d0 * r0 * 0.6 * phi.sin()).collect());
        }
        all.extend(rings.iter().cloned());
        for k in 1..4 {
            let phi = PI / 2.0 * k as f64 / 4.0;
            all.push(rings[n - 1].iter().map(|q| c1 + (*q - c1) * phi.cos() + d1 * r1 * 0.6 * phi.sin()).collect());
        }
        Mesh::loft(&all, false, Some(c0 + d0 * r0 * 0.6), Some(c1 + d1 * r1 * 0.6))
    }
}

// ------------------------------------------------------------------- rope

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RopeParams {
    pub rail: NodeId,
    pub strands: usize,
    pub wire: f64,
    /// Length of one full twist (mm).
    pub pitch: f64,
    /// Rope centre height above the shank surface / rail (mm).
    pub height: f64,
    pub full_circle: bool,
    pub start: f64,
    pub end: f64,
}

impl Default for RopeParams {
    fn default() -> Self {
        RopeParams { rail: NodeId::default(), strands: 3, wire: 0.7, pitch: 4.0, height: 0.0, full_circle: true, start: 0.1, end: 0.4 }
    }
}

/// gvRope: twisted strands following the rail.
pub fn eval_rope(p: &RopeParams, input: &Output) -> OpResult {
    let rail = input.rail.ok_or("input has no ring rail")?;
    if p.strands == 0 || p.strands > 12 || p.wire <= 0.05 || p.pitch <= 0.2 {
        return Err("invalid rope parameters".into());
    }
    let (t0, t1) = if p.full_circle { (0.0, 1.0) } else { (p.start.min(p.end), p.start.max(p.end)) };
    let rr = rail.radius + p.wire + p.height + if p.full_circle { 0.0 } else { 0.0 };
    let len = TAU * rr * (t1 - t0);
    // whole number of twists on a full circle so the strands close up
    let turns = (len / p.pitch).round().max(1.0);
    let n = (len / 0.15).ceil() as usize;
    let off = p.wire * 0.55;
    let mut solids = Vec::new();
    for s in 0..p.strands {
        let ph = TAU * s as f64 / p.strands as f64;
        let path: Vec<DVec3> = (0..=n)
            .map(|i| {
                let u = i as f64 / n as f64;
                let t = t0 + (t1 - t0) * u;
                let a = ph + TAU * turns * u;
                let nrm = Rail::normal(t);
                nrm * (rr + off * a.cos()) + DVec3::Y * (off * a.sin())
            })
            .collect();
        let m = if p.full_circle {
            let frames = crate::mesh::parallel_transport_frames(&path[..n]);
            let rings: Vec<Vec<DVec3>> = (0..n)
                .map(|i| {
                    let (_, a, b) = frames[i];
                    (0..12).map(|j| path[i] + (a * (TAU * j as f64 / 12.0).cos() + b * (TAU * j as f64 / 12.0).sin()) * (p.wire / 2.0)).collect()
                })
                .collect();
            Mesh::loft(&rings, true, None, None)
        } else {
            Mesh::tube(&path, &vec![p.wire / 2.0; path.len()], 12, 1.0, 1.0)
        };
        solids.push(Solid::metal(m));
    }
    Ok(Output { solids, ..Default::default() })
}

// ---------------------------------------------------------------- deformers

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeformKind {
    /// gvTwist: rotation about the axis growing along it.
    Twist,
    /// gvTaper: scale perpendicular to the axis growing along it.
    Taper,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeformParams {
    pub source: NodeId,
    pub kind: DeformKind,
    pub axis: Axis,
    /// Twist: total angle in degrees. Taper: scale at the far end (1 = none).
    pub amount: f64,
}

impl Default for DeformParams {
    fn default() -> Self {
        DeformParams { source: NodeId::default(), kind: DeformKind::Twist, axis: Axis::Y, amount: 90.0 }
    }
}

pub fn eval_deform(p: &DeformParams, src: &Output) -> OpResult {
    let b = src.bbox();
    if b.is_empty() {
        return Err("nothing to deform".into());
    }
    let ax = p.axis.vec();
    let (lo, hi) = (b.min.dot(ax), b.max.dot(ax));
    let span = (hi - lo).max(1e-9);
    let c = b.center();
    let f = |q: DVec3| -> DVec3 {
        let u = (q.dot(ax) - lo) / span; // 0..1 along the axis
        let rel = q - c;
        let along = ax * rel.dot(ax);
        let perp = rel - along;
        match p.kind {
            DeformKind::Twist => c + along + DQuat::from_axis_angle(ax, deg(p.amount) * (u - 0.5)) * perp,
            DeformKind::Taper => c + along + perp * (1.0 + (p.amount - 1.0) * u),
        }
    };
    let mut out = src.clone();
    out.solids = src
        .solids
        .iter()
        .map(|s| {
            let mut m = (*s.mesh).clone();
            for v in &mut m.positions {
                *v = f(*v);
            }
            m.ensure_outward();
            Solid { mesh: Arc::new(m), role: s.role }
        })
        .collect();
    for g in &mut out.gems {
        g.xform.translation = f(g.xform.translation);
    }
    Ok(out)
}

// --------------------------------------------------------- gems on surface

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoFill {
    pub center: [f64; 3],
    pub radius: f64,
    /// Only surfaces facing within this angle of the average normal (deg).
    pub max_angle: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GemsOnSurfaceParams {
    pub target: NodeId,
    pub spec: GemSpec,
    /// Seed points (snapped to the surface). Added by clicking in the viewport.
    pub points: Vec<[f64; 3]>,
    /// Girdle offset along the surface normal (negative = sunk).
    pub height: f64,
    pub spacing: f64,
    pub fill: Option<AutoFill>,
}

impl Default for GemsOnSurfaceParams {
    fn default() -> Self {
        GemsOnSurfaceParams { target: NodeId::default(), spec: GemSpec::round(1.3), points: vec![], height: -0.15, spacing: 0.12, fill: None }
    }
}

struct Hit {
    p: DVec3,
    n: DVec3,
}

fn closest_on_tri(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
    // Ericson, Real-Time Collision Detection 5.1.5
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let den = 1.0 / (va + vb + vc);
    a + ab * (vb * den) + ac * (vc * den)
}

fn snap(meshes: &[&Mesh], p: DVec3) -> Option<Hit> {
    let mut best: Option<(f64, Hit)> = None;
    for m in meshes {
        for i in 0..m.triangles.len() {
            let [a, b, c] = m.tri(i);
            let q = closest_on_tri(p, a, b, c);
            let d = q.distance_squared(p);
            if best.as_ref().is_none_or(|x| d < x.0) {
                let n = (b - a).cross(c - a).normalize_or_zero();
                best = Some((d, Hit { p: q, n }));
            }
        }
    }
    best.map(|b| b.1)
}

fn frame(p: DVec3, n: DVec3) -> DAffine3 {
    let helper = if n.dot(DVec3::Y).abs() < 0.9 { DVec3::Y } else { DVec3::X };
    let x = helper.cross(n).normalize();
    let y = n.cross(x);
    DAffine3::from_mat3_translation(DMat3::from_cols(x, y, n), p)
}

/// gvGemOnSurface: gems snapped to any metal surface, by points or by an
/// automatic Poisson-disk fill of a region.
pub fn eval_on_surface(p: &GemsOnSurfaceParams, input: &Output) -> OpResult {
    let meshes: Vec<&Mesh> = input.solids.iter().filter(|s| s.role == Role::Metal).map(|s| s.mesh.as_ref()).collect();
    if meshes.is_empty() {
        return Err("target has no surface".into());
    }
    let size = p.spec.width.max(p.spec.length);
    let min_d = size + p.spacing;
    let mut placed: Vec<(DVec3, DVec3)> = Vec::new();
    let try_add = |q: DVec3, n: DVec3, placed: &mut Vec<(DVec3, DVec3)>| {
        if placed.iter().all(|(o, _)| o.distance(q) >= min_d - 1e-9) {
            placed.push((q, n));
        }
    };
    for s in &p.points {
        if let Some(h) = snap(&meshes, DVec3::from(*s)) {
            try_add(h.p, h.n, &mut placed);
        }
    }
    if let Some(f) = &p.fill {
        let c = DVec3::from(f.center);
        // candidate triangles in the sphere
        let mut tris: Vec<([DVec3; 3], f64)> = Vec::new();
        for m in &meshes {
            for i in 0..m.triangles.len() {
                let t = m.tri(i);
                // any triangle touching the region (large faces included)
                if closest_on_tri(c, t[0], t[1], t[2]).distance(c) <= f.radius {
                    tris.push((t, (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5));
                }
            }
        }
        // reference orientation: the surface nearest to the region centre
        let avg = snap(&meshes, c).map(|h| h.n).unwrap_or(DVec3::Z);
        let cos_max = f.max_angle.to_radians().cos();
        tris.retain(|(t, _)| (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero().dot(avg) >= cos_max);
        let total: f64 = tris.iter().map(|x| x.1).sum();
        if total > 0.0 {
            // deterministic dart throwing (LCG), roughly 30 attempts per slot
            let mut seed: u64 = 0x9E3779B97F4A7C15;
            let mut rnd = || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (seed >> 11) as f64 / (1u64 << 53) as f64
            };
            let cdf: Vec<f64> = tris.iter().scan(0.0, |acc, t| { *acc += t.1; Some(*acc) }).collect();
            let attempts = ((total / (min_d * min_d)) * 30.0) as usize + 100;
            for _ in 0..attempts.min(200_000) {
                let r = rnd() * total;
                let i = cdf.partition_point(|x| *x < r).min(tris.len() - 1);
                let (t, _) = tris[i];
                let (mut u, mut v) = (rnd(), rnd());
                if u + v > 1.0 {
                    u = 1.0 - u;
                    v = 1.0 - v;
                }
                let q = t[0] + (t[1] - t[0]) * u + (t[2] - t[0]) * v;
                // keep the whole gem inside the region
                if q.distance(c) > f.radius - size / 2.0 {
                    continue;
                }
                let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
                try_add(q, n, &mut placed);
            }
        }
    }
    if placed.is_empty() {
        return Err("no gems placed yet: click on the surface or enable auto fill".into());
    }
    let gems: Vec<GemInstance> = placed
        .iter()
        .map(|(q, n)| GemInstance { spec: p.spec.clone(), xform: frame(*q + *n * p.height, *n) })
        .collect();
    let chains = (0..gems.len()).map(|i| Chain { indices: vec![i], closed: false }).collect();
    Ok(Output { gems, chains, ..Default::default() })
}

// ----------------------------------------------------------- imported mesh

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportParams {
    pub name: String,
    /// Mesh payload: little-endian f32 xyz positions then u32 triangles,
    /// base64 encoded (see `encode_mesh`).
    pub data: String,
    pub role: Role,
    pub position: [f64; 3],
    pub rotation: [f64; 3],
    pub scale: f64,
}

impl Default for ImportParams {
    fn default() -> Self {
        ImportParams { name: String::new(), data: String::new(), role: Role::Metal, position: [0.0; 3], rotation: [0.0; 3], scale: 1.0 }
    }
}

pub fn eval_import(p: &ImportParams) -> OpResult {
    let mut m = decode_mesh(&p.data).ok_or("corrupt mesh data")?;
    let xf = DAffine3::from_scale_rotation_translation(
        DVec3::splat(p.scale),
        DQuat::from_euler(glam::EulerRot::XYZ, deg(p.rotation[0]), deg(p.rotation[1]), deg(p.rotation[2])),
        DVec3::from(p.position),
    );
    m.transform(&xf);
    m.ensure_outward();
    Ok(Output { solids: vec![Solid { mesh: Arc::new(m), role: p.role }], ..Default::default() })
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode_mesh(m: &Mesh) -> String {
    let mut bytes = Vec::with_capacity(8 + m.positions.len() * 12 + m.triangles.len() * 12);
    bytes.extend((m.positions.len() as u32).to_le_bytes());
    bytes.extend((m.triangles.len() as u32).to_le_bytes());
    for p in &m.positions {
        for x in [p.x, p.y, p.z] {
            bytes.extend((x as f32).to_le_bytes());
        }
    }
    for t in &m.triangles {
        for i in t {
            bytes.extend(i.to_le_bytes());
        }
    }
    let mut s = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

pub fn decode_mesh(s: &str) -> Option<Mesh> {
    let mut lut = [255u8; 256];
    for (i, c) in B64.iter().enumerate() {
        lut[*c as usize] = i as u8;
    }
    let mut bytes = Vec::with_capacity(s.len() * 3 / 4);
    let clean: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    for c in clean.chunks(4) {
        let mut n = 0u32;
        let mut valid: usize = 0;
        for (k, b) in c.iter().enumerate() {
            if *b == b'=' {
                break;
            }
            let v = lut[*b as usize];
            if v == 255 {
                return None;
            }
            n |= (v as u32) << (18 - 6 * k);
            valid += 1;
        }
        for k in 0..valid.saturating_sub(1) {
            bytes.push((n >> (16 - 8 * k) & 255) as u8);
        }
    }
    let rd = |o: usize| -> Option<u32> { Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?)) };
    let nv = rd(0)? as usize;
    let nt = rd(4)? as usize;
    let mut m = Mesh::new();
    let mut o = 8;
    for _ in 0..nv {
        let mut v = [0f64; 3];
        for x in &mut v {
            *x = f32::from_bits(rd(o)?) as f64;
            o += 4;
        }
        m.positions.push(DVec3::from(v));
    }
    for _ in 0..nt {
        let t = [rd(o)?, rd(o + 4)?, rd(o + 8)?];
        o += 12;
        if t.iter().any(|i| *i as usize >= nv) {
            return None;
        }
        m.triangles.push(t);
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_payload_roundtrip() {
        let m = Mesh::uv_sphere(DVec3::new(1.0, 2.0, 3.0), 2.0, 12);
        let d = decode_mesh(&encode_mesh(&m)).unwrap();
        assert_eq!(d.triangles, m.triangles);
        assert!((d.volume() - m.volume()).abs() < 1e-3);
    }

    #[test]
    fn surface_fill_respects_spacing() {
        let plate = Mesh::boxed(DVec3::new(-10.0, -10.0, -1.0), DVec3::new(10.0, 10.0, 0.0));
        let o = Output { solids: vec![Solid::metal(plate)], ..Default::default() };
        let p = GemsOnSurfaceParams {
            fill: Some(AutoFill { center: [0.0, 0.0, 0.0], radius: 6.0, max_angle: 10.0 }),
            ..Default::default()
        };
        let r = eval_on_surface(&p, &o).unwrap();
        assert!(r.gems.len() > 20, "{}", r.gems.len());
        for (i, a) in r.gems.iter().enumerate() {
            assert!((a.up().z - 1.0).abs() < 1e-9);
            for b in &r.gems[i + 1..] {
                assert!(a.center().distance(b.center()) >= 1.42 - 1e-6);
            }
        }
    }
}
