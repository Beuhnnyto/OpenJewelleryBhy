//! Mesh booleans and robust volume measurement.
//!
//! Boolean strategy ("split then classify"):
//! 1. every component's polygons are split by the planes of the *other*
//!    components' polygons, but only inside the region where their bounding
//!    boxes overlap, using a local BSP tree (so far-away geometry is never
//!    touched — a shank with 40 seat cutters only gets cut locally);
//! 2. each resulting fragment is kept iff the boolean region differs on its
//!    two sides, probed a hair inside and outside the fragment with
//!    ray-parity point-membership tests (grid-accelerated, jitter-voted).
//!    The fragment is flipped when the region lies on its outer side.
//!
//! This rule is exact for any boolean expression, works for overlapping
//! tools (pavé cutters routinely overlap) and needs no global BSP.

use crate::mesh::{Aabb, Mesh};
use glam::DVec3;
use rayon::prelude::*;
use std::collections::HashMap;

const SPLIT_EPS: f64 = 1e-7;
const PROBE_EPS: f64 = 2e-5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

/// Point-membership and vertical-ray queries for a closed triangle mesh.
pub struct InsideTester {
    tris: Vec<[DVec3; 3]>,
    pub bbox: Aabb,
    nx: usize,
    ny: usize,
    cell: (f64, f64),
    cells: Vec<Vec<u32>>,
}

impl InsideTester {
    pub fn new(mesh: &Mesh) -> InsideTester {
        let tris: Vec<[DVec3; 3]> = (0..mesh.triangles.len()).map(|i| mesh.tri(i)).collect();
        let bbox = mesh.bbox().expanded(1e-6);
        let n = tris.len().max(1);
        let res = ((n as f64 / 2.0).sqrt().ceil() as usize).clamp(1, 512);
        let size = bbox.size();
        let (nx, ny) = if size.x >= size.y {
            (res, ((res as f64 * size.y / size.x.max(1e-9)).ceil() as usize).max(1))
        } else {
            (((res as f64 * size.x / size.y.max(1e-9)).ceil() as usize).max(1), res)
        };
        let cell = (size.x.max(1e-9) / nx as f64, size.y.max(1e-9) / ny as f64);
        let mut cells = vec![Vec::new(); nx * ny];
        for (i, t) in tris.iter().enumerate() {
            let lo = t[0].min(t[1]).min(t[2]);
            let hi = t[0].max(t[1]).max(t[2]);
            let (x0, y0) = Self::cell_of(&bbox, cell, nx, ny, lo.x, lo.y);
            let (x1, y1) = Self::cell_of(&bbox, cell, nx, ny, hi.x, hi.y);
            for cy in y0..=y1 {
                for cx in x0..=x1 {
                    cells[cy * nx + cx].push(i as u32);
                }
            }
        }
        InsideTester { tris, bbox, nx, ny, cell, cells }
    }

    #[inline]
    fn cell_of(b: &Aabb, cell: (f64, f64), nx: usize, ny: usize, x: f64, y: f64) -> (usize, usize) {
        let cx = (((x - b.min.x) / cell.0) as isize).clamp(0, nx as isize - 1) as usize;
        let cy = (((y - b.min.y) / cell.1) as isize).clamp(0, ny as isize - 1) as usize;
        (cx, cy)
    }

    /// All z values where the vertical line through (x, y) crosses the mesh.
    pub fn z_crossings(&self, x: f64, y: f64, out: &mut Vec<f64>) {
        out.clear();
        if x < self.bbox.min.x || x > self.bbox.max.x || y < self.bbox.min.y || y > self.bbox.max.y {
            return;
        }
        let (cx, cy) = Self::cell_of(&self.bbox, self.cell, self.nx, self.ny, x, y);
        for &ti in &self.cells[cy * self.nx + cx] {
            let [a, b, c] = self.tris[ti as usize];
            // 2D barycentric test in XY
            let d = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
            if d.abs() < 1e-18 {
                continue;
            }
            let l1 = ((b.y - c.y) * (x - c.x) + (c.x - b.x) * (y - c.y)) / d;
            let l2 = ((c.y - a.y) * (x - c.x) + (a.x - c.x) * (y - c.y)) / d;
            let l3 = 1.0 - l1 - l2;
            if l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0 {
                out.push(l1 * a.z + l2 * b.z + l3 * c.z);
            }
        }
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }

    fn parity_up(&self, p: DVec3, buf: &mut Vec<f64>) -> bool {
        self.z_crossings(p.x, p.y, buf);
        buf.iter().filter(|z| **z > p.z).count() % 2 == 1
    }

    /// Majority vote over three slightly jittered vertical rays, which makes
    /// the test robust against rays grazing edges / vertices and small cracks.
    pub fn contains(&self, p: DVec3) -> bool {
        if !self.bbox.contains(p) {
            return false;
        }
        let mut buf = Vec::new();
        let j = 3.1e-7;
        let votes = [DVec3::new(j, 0.7 * j, 0.0), DVec3::new(-0.6 * j, j, 0.0), DVec3::new(0.3 * j, -1.3 * j, 0.0)]
            .iter()
            .filter(|o| self.parity_up(p + **o, &mut buf))
            .count();
        votes >= 2
    }
}

/// Volume of the union of several closed meshes, measured by vertical ray
/// intervals on a grid (handles overlapping parts — e.g. a head welded into a
/// shank — without double counting). `cells` is the resolution along the
/// longest XY side.
pub fn union_volume(meshes: &[&Mesh], cells: usize) -> f64 {
    let testers: Vec<InsideTester> = meshes.iter().filter(|m| !m.is_empty()).map(|m| InsideTester::new(m)).collect();
    if testers.is_empty() {
        return 0.0;
    }
    let bbox = testers.iter().fold(Aabb::EMPTY, |b, t| b.union(&t.bbox));
    let size = bbox.size();
    let h = size.x.max(size.y) / cells as f64;
    let nx = (size.x / h).ceil() as usize;
    let ny = (size.y / h).ceil() as usize;
    let rows: f64 = (0..ny)
        .into_par_iter()
        .map(|iy| {
            let y = bbox.min.y + (iy as f64 + 0.5) * h + 1.7e-9;
            let mut buf = Vec::new();
            let mut iv: Vec<(f64, f64)> = Vec::new();
            let mut row = 0.0;
            for ix in 0..nx {
                let x = bbox.min.x + (ix as f64 + 0.5) * h + 2.3e-9;
                iv.clear();
                for t in &testers {
                    t.z_crossings(x, y, &mut buf);
                    for pair in buf.chunks_exact(2) {
                        iv.push((pair[0], pair[1]));
                    }
                }
                if iv.is_empty() {
                    continue;
                }
                iv.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let (mut s, mut e) = iv[0];
                for &(a, b) in &iv[1..] {
                    if a > e {
                        row += e - s;
                        s = a;
                        e = b;
                    } else {
                        e = e.max(b);
                    }
                }
                row += e - s;
            }
            row
        })
        .sum();
    rows * h * h
}

#[derive(Clone)]
struct Poly {
    v: Vec<DVec3>,
    n: DVec3,
    w: f64,
}

impl Poly {
    fn from_tri(t: [DVec3; 3]) -> Option<Poly> {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let l = n.length();
        if l < 1e-14 {
            return None;
        }
        let n = n / l;
        Some(Poly { v: t.to_vec(), n, w: n.dot(t[0]) })
    }
    fn bbox(&self) -> Aabb {
        Aabb::from_points(&self.v)
    }
    fn centroid(&self) -> DVec3 {
        // area-weighted centroid of a convex polygon
        let o = self.v[0];
        let mut c = DVec3::ZERO;
        let mut a = 0.0;
        for i in 1..self.v.len() - 1 {
            let ar = (self.v[i] - o).cross(self.v[i + 1] - o).length();
            c += (o + self.v[i] + self.v[i + 1]) / 3.0 * ar;
            a += ar;
        }
        if a > 0.0 {
            c / a
        } else {
            self.v.iter().copied().sum::<DVec3>() / self.v.len() as f64
        }
    }
}

enum Side {
    Coplanar,
    Front,
    Back,
    Spanning,
}

fn split(n: DVec3, w: f64, p: &Poly) -> (Side, Option<Poly>, Option<Poly>) {
    let d: Vec<f64> = p.v.iter().map(|v| n.dot(*v) - w).collect();
    let (mut f, mut b) = (false, false);
    for x in &d {
        if *x > SPLIT_EPS {
            f = true
        } else if *x < -SPLIT_EPS {
            b = true
        }
    }
    match (f, b) {
        (false, false) => (Side::Coplanar, None, None),
        (true, false) => (Side::Front, None, None),
        (false, true) => (Side::Back, None, None),
        (true, true) => {
            let mut fv = Vec::with_capacity(p.v.len() + 1);
            let mut bv = Vec::with_capacity(p.v.len() + 1);
            let k = p.v.len();
            for i in 0..k {
                let j = (i + 1) % k;
                let (vi, vj) = (p.v[i], p.v[j]);
                let (di, dj) = (d[i], d[j]);
                if di >= -SPLIT_EPS {
                    fv.push(vi);
                }
                if di <= SPLIT_EPS {
                    bv.push(vi);
                }
                if (di > SPLIT_EPS && dj < -SPLIT_EPS) || (di < -SPLIT_EPS && dj > SPLIT_EPS) {
                    let t = di / (di - dj);
                    let x = vi.lerp(vj, t);
                    fv.push(x);
                    bv.push(x);
                }
            }
            let mk = |v: Vec<DVec3>| if v.len() >= 3 { Some(Poly { v, n: p.n, w: p.w }) } else { None };
            (Side::Spanning, mk(fv), mk(bv))
        }
    }
}

struct BspNode {
    n: DVec3,
    w: f64,
    front: Option<u32>,
    back: Option<u32>,
}

/// Plane-only BSP used to split foreign polygons along this surface.
struct Bsp {
    nodes: Vec<BspNode>,
}

impl Bsp {
    fn build(polys: Vec<Poly>) -> Bsp {
        let mut nodes: Vec<BspNode> = Vec::new();
        if polys.is_empty() {
            return Bsp { nodes };
        }
        // (parent, is_front, polys)
        let mut stack: Vec<(Option<(u32, bool)>, Vec<Poly>)> = vec![(None, polys)];
        while let Some((parent, mut list)) = stack.pop() {
            let pick = list.len() / 2;
            let pivot = list.swap_remove(pick);
            let idx = nodes.len() as u32;
            nodes.push(BspNode { n: pivot.n, w: pivot.w, front: None, back: None });
            if let Some((p, is_front)) = parent {
                if is_front {
                    nodes[p as usize].front = Some(idx);
                } else {
                    nodes[p as usize].back = Some(idx);
                }
            }
            let (mut fl, mut bl) = (Vec::new(), Vec::new());
            for q in list {
                match split(pivot.n, pivot.w, &q) {
                    (Side::Coplanar, ..) => {}
                    (Side::Front, ..) => fl.push(q),
                    (Side::Back, ..) => bl.push(q),
                    (Side::Spanning, f, b) => {
                        fl.extend(f);
                        bl.extend(b);
                    }
                }
            }
            if !fl.is_empty() {
                stack.push((Some((idx, true)), fl));
            }
            if !bl.is_empty() {
                stack.push((Some((idx, false)), bl));
            }
        }
        Bsp { nodes }
    }

    /// Splits `p` by every plane on its path down the tree.
    fn split_into(&self, p: Poly, out: &mut Vec<Poly>) {
        if self.nodes.is_empty() {
            out.push(p);
            return;
        }
        let mut stack = vec![(0u32, p)];
        while let Some((ni, poly)) = stack.pop() {
            let node = &self.nodes[ni as usize];
            let route = |child: Option<u32>, q: Poly, stack: &mut Vec<(u32, Poly)>, out: &mut Vec<Poly>| match child {
                Some(c) => stack.push((c, q)),
                None => out.push(q),
            };
            match split(node.n, node.w, &poly) {
                (Side::Coplanar, ..) => {
                    let c = if node.n.dot(poly.n) > 0.0 { node.front } else { node.back };
                    route(c, poly, &mut stack, out)
                }
                (Side::Front, ..) => route(node.front, poly, &mut stack, out),
                (Side::Back, ..) => route(node.back, poly, &mut stack, out),
                (Side::Spanning, f, b) => {
                    if let Some(f) = f {
                        route(node.front, f, &mut stack, out);
                    }
                    if let Some(b) = b {
                        route(node.back, b, &mut stack, out);
                    }
                }
            }
        }
    }
}

struct Component<'a> {
    mesh: &'a Mesh,
    tester: InsideTester,
    in_a: bool,
}

/// Boolean of the union of `a` with the union of `b`. Inputs must be closed,
/// outward-oriented meshes (they may overlap each other freely).
pub fn boolean(a: &[&Mesh], b: &[&Mesh], op: BoolOp) -> Mesh {
    let comps: Vec<Component> = a
        .iter()
        .map(|m| (m, true))
        .chain(b.iter().map(|m| (m, false)))
        .filter(|(m, _)| !m.is_empty())
        .map(|(m, in_a)| Component { mesh: m, tester: InsideTester::new(m), in_a })
        .collect();
    let region = |ina: bool, inb: bool| match op {
        BoolOp::Union => ina || inb,
        BoolOp::Difference => ina && !inb,
        BoolOp::Intersection => ina && inb,
    };

    let kept: Vec<Vec<Poly>> = (0..comps.len())
        .into_par_iter()
        .map(|xi| {
            let x = &comps[xi];
            let mut polys: Vec<Poly> =
                (0..x.mesh.triangles.len()).filter_map(|i| Poly::from_tri(x.mesh.tri(i))).collect();
            // 1. local splitting against every overlapping component
            for (yi, y) in comps.iter().enumerate() {
                if yi == xi || !x.tester.bbox.intersects(&y.tester.bbox) {
                    continue;
                }
                let zone = Aabb {
                    min: x.tester.bbox.min.max(y.tester.bbox.min),
                    max: x.tester.bbox.max.min(y.tester.bbox.max),
                }
                .expanded(1e-5);
                let local: Vec<Poly> = (0..y.mesh.triangles.len())
                    .filter_map(|i| Poly::from_tri(y.mesh.tri(i)))
                    .filter(|p| p.bbox().intersects(&zone))
                    .collect();
                if local.is_empty() {
                    continue;
                }
                let bsp = Bsp::build(local);
                let mut next = Vec::with_capacity(polys.len());
                for p in polys {
                    if p.bbox().intersects(&zone) {
                        bsp.split_into(p, &mut next);
                    } else {
                        next.push(p);
                    }
                }
                polys = next;
            }
            // 2. classification
            let member = |p: DVec3, self_in: bool| -> bool {
                let mut ina = x.in_a && self_in;
                let mut inb = !x.in_a && self_in;
                for (ci, c) in comps.iter().enumerate() {
                    if ci == xi || (c.in_a && ina) || (!c.in_a && inb) {
                        continue;
                    }
                    if c.tester.contains(p) {
                        if c.in_a {
                            ina = true
                        } else {
                            inb = true
                        }
                    }
                }
                region(ina, inb)
            };
            let mut out = Vec::with_capacity(polys.len());
            for mut p in polys {
                let c = p.centroid();
                let r_in = member(c - p.n * PROBE_EPS, true);
                let r_out = member(c + p.n * PROBE_EPS, false);
                if r_in && !r_out {
                    out.push(p);
                } else if !r_in && r_out {
                    p.v.reverse();
                    p.n = -p.n;
                    p.w = -p.w;
                    out.push(p);
                }
            }
            out
        })
        .collect();

    polys_to_mesh(kept.iter().flatten())
}

fn polys_to_mesh<'a>(polys: impl Iterator<Item = &'a Poly>) -> Mesh {
    let mut m = Mesh::new();
    let mut map: HashMap<[i64; 3], u32> = HashMap::new();
    let q = 1e6;
    let mut id = |m: &mut Mesh, p: DVec3| -> u32 {
        let k = [(p.x * q).round() as i64, (p.y * q).round() as i64, (p.z * q).round() as i64];
        *map.entry(k).or_insert_with(|| {
            m.positions.push(p);
            (m.positions.len() - 1) as u32
        })
    };
    for p in polys {
        let ids: Vec<u32> = p.v.iter().map(|v| id(&mut m, *v)).collect();
        for i in 1..ids.len() - 1 {
            let t = [ids[0], ids[i], ids[i + 1]];
            if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
                m.triangles.push(t);
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(min: f64, max: f64) -> Mesh {
        Mesh::boxed(DVec3::splat(min), DVec3::splat(max))
    }

    #[test]
    fn contains_basic() {
        let m = Mesh::uv_sphere(DVec3::ZERO, 1.0, 24);
        let t = InsideTester::new(&m);
        assert!(t.contains(DVec3::new(0.1, 0.2, 0.3)));
        assert!(!t.contains(DVec3::new(0.9, 0.9, 0.0)));
    }

    #[test]
    fn union_volume_overlap() {
        let a = cube(0.0, 2.0);
        let b = Mesh::boxed(DVec3::new(1.0, 0.0, 0.0), DVec3::new(3.0, 2.0, 2.0));
        let v = union_volume(&[&a, &b], 300);
        assert!((v - 12.0).abs() < 0.05, "{v}");
    }

    #[test]
    fn difference_corner() {
        let a = cube(0.0, 2.0);
        let b = cube(1.0, 3.0);
        let r = boolean(&[&a], &[&b], BoolOp::Difference);
        assert!((r.volume() - 7.0).abs() < 1e-6, "{}", r.volume());
    }

    #[test]
    fn union_and_intersection() {
        let a = cube(0.0, 2.0);
        let b = cube(1.0, 3.0);
        let u = boolean(&[&a], &[&b], BoolOp::Union);
        assert!((u.volume() - 15.0).abs() < 1e-6, "{}", u.volume());
        let i = boolean(&[&a], &[&b], BoolOp::Intersection);
        assert!((i.volume() - 1.0).abs() < 1e-6, "{}", i.volume());
    }

    #[test]
    fn difference_with_overlapping_tools_and_sphere() {
        let a = cube(-2.0, 2.0);
        let s1 = Mesh::uv_sphere(DVec3::new(0.0, 0.0, 2.0), 1.0, 24);
        let s2 = Mesh::uv_sphere(DVec3::new(0.5, 0.0, 2.0), 1.0, 24);
        let r = boolean(&[&a], &[&s1, &s2], BoolOp::Difference);
        // expected: cube minus union of the two half-spheres inside the cube
        let lens = union_volume(&[&s1, &s2], 400) / 2.0;
        let expect = 64.0 - lens;
        assert!((r.volume() - expect).abs() / expect < 0.01, "{} vs {}", r.volume(), expect);
        // the result must itself be a valid solid for further booleans
        let r2 = boolean(&[&r], &[&cube(1.5, 3.0)], BoolOp::Difference);
        assert!((r2.volume() - (r.volume() - 0.125)).abs() < 0.02, "{} {}", r2.volume(), r.volume());
    }
}
