//! Indexed triangle meshes and the lofting primitives every builder relies on.

use glam::{DAffine3, DVec2, DVec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb {
        min: DVec3::splat(f64::INFINITY),
        max: DVec3::splat(f64::NEG_INFINITY),
    };

    pub fn from_points<'a>(pts: impl IntoIterator<Item = &'a DVec3>) -> Aabb {
        let mut b = Aabb::EMPTY;
        for p in pts {
            b.grow(*p);
        }
        b
    }
    #[inline]
    pub fn grow(&mut self, p: DVec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }
    #[inline]
    pub fn intersects(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x
            && self.max.x >= o.min.x
            && self.min.y <= o.max.y
            && self.max.y >= o.min.y
            && self.min.z <= o.max.z
            && self.max.z >= o.min.z
    }
    #[inline]
    pub fn contains(&self, p: DVec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
    pub fn expanded(&self, d: f64) -> Aabb {
        Aabb { min: self.min - DVec3::splat(d), max: self.max + DVec3::splat(d) }
    }
    pub fn size(&self) -> DVec3 {
        if self.is_empty() {
            DVec3::ZERO
        } else {
            self.max - self.min
        }
    }
    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }
}

#[derive(Clone, Default, Debug)]
pub struct Mesh {
    pub positions: Vec<DVec3>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    pub fn append(&mut self, other: &Mesh) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.triangles
            .extend(other.triangles.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
    }

    pub fn merged<'a>(meshes: impl IntoIterator<Item = &'a Mesh>) -> Mesh {
        let mut m = Mesh::new();
        for x in meshes {
            m.append(x);
        }
        m
    }

    /// Applies an affine transform; mirroring transforms keep normals outward.
    pub fn transform(&mut self, xf: &DAffine3) {
        for p in &mut self.positions {
            *p = xf.transform_point3(*p);
        }
        if xf.matrix3.determinant() < 0.0 {
            self.flip();
        }
    }

    pub fn transformed(&self, xf: &DAffine3) -> Mesh {
        let mut m = self.clone();
        m.transform(xf);
        m
    }

    pub fn flip(&mut self) {
        for t in &mut self.triangles {
            t.swap(1, 2);
        }
    }

    #[inline]
    pub fn tri(&self, i: usize) -> [DVec3; 3] {
        let t = self.triangles[i];
        [self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]]
    }

    /// Signed volume (divergence theorem). Positive for closed outward meshes.
    pub fn volume(&self) -> f64 {
        self.triangles
            .iter()
            .map(|t| {
                let a = self.positions[t[0] as usize];
                let b = self.positions[t[1] as usize];
                let c = self.positions[t[2] as usize];
                a.dot(b.cross(c))
            })
            .sum::<f64>()
            / 6.0
    }

    pub fn area(&self) -> f64 {
        (0..self.triangles.len())
            .map(|i| {
                let [a, b, c] = self.tri(i);
                (b - a).cross(c - a).length() * 0.5
            })
            .sum()
    }

    pub fn bbox(&self) -> Aabb {
        Aabb::from_points(&self.positions)
    }

    /// Flips the whole mesh if it encloses negative volume.
    pub fn ensure_outward(&mut self) {
        if self.volume() < 0.0 {
            self.flip();
        }
    }

    /// Lofts a sequence of rings that all have the same number of points.
    ///
    /// * `closed_along`: connect the last ring back to the first (tori, shanks).
    /// * `start_tip` / `end_tip`: close the ends with a fan to that point
    ///   (use the ring centroid for a flat cap, an offset point for a cone).
    ///
    /// The result is oriented outward when it is closed.
    pub fn loft(
        rings: &[Vec<DVec3>],
        closed_along: bool,
        start_tip: Option<DVec3>,
        end_tip: Option<DVec3>,
    ) -> Mesh {
        let mut m = Mesh::new();
        let nr = rings.len();
        if nr == 0 {
            return m;
        }
        let k = rings[0].len();
        debug_assert!(rings.iter().all(|r| r.len() == k));
        m.positions.reserve(nr * k + 2);
        for r in rings {
            m.positions.extend_from_slice(r);
        }
        let idx = |i: usize, j: usize| (i * k + (j % k)) as u32;
        let segs = if closed_along { nr } else { nr - 1 };
        m.triangles.reserve(segs * k * 2 + 2 * k);
        for i in 0..segs {
            let i2 = (i + 1) % nr;
            for j in 0..k {
                let (a, b, c, d) = (idx(i, j), idx(i2, j), idx(i2, j + 1), idx(i, j + 1));
                m.triangles.push([a, b, c]);
                m.triangles.push([a, c, d]);
            }
        }
        if !closed_along {
            if let Some(tip) = start_tip {
                let ti = m.positions.len() as u32;
                m.positions.push(tip);
                for j in 0..k {
                    m.triangles.push([ti, idx(0, j), idx(0, j + 1)]);
                }
            }
            if let Some(tip) = end_tip {
                let ti = m.positions.len() as u32;
                m.positions.push(tip);
                for j in 0..k {
                    m.triangles.push([ti, idx(nr - 1, j + 1), idx(nr - 1, j)]);
                }
            }
        }
        m.ensure_outward();
        m
    }

    /// Tube of circular section along a path, radii per path point.
    /// `dome_start`/`dome_end` add hemispherical-ish caps scaled by the factor
    /// (0 = flat cap).
    pub fn tube(path: &[DVec3], radii: &[f64], segments: usize, dome_start: f64, dome_end: f64) -> Mesh {
        assert!(path.len() >= 2 && radii.len() == path.len());
        let frames = parallel_transport_frames(path);
        let ring = |c: DVec3, n: DVec3, b: DVec3, r: f64| -> Vec<DVec3> {
            (0..segments)
                .map(|j| {
                    let a = std::f64::consts::TAU * j as f64 / segments as f64;
                    c + (n * a.cos() + b * a.sin()) * r
                })
                .collect()
        };
        let dome_rings = 5usize;
        let mut rings = Vec::with_capacity(path.len() + 2 * dome_rings);
        let (t0, n0, b0) = frames[0];
        let mut start_tip = path[0];
        if dome_start > 0.0 {
            for k in (1..dome_rings).rev() {
                let phi = std::f64::consts::FRAC_PI_2 * k as f64 / dome_rings as f64;
                let c = path[0] - t0 * radii[0] * dome_start * phi.sin();
                rings.push(ring(c, n0, b0, radii[0] * phi.cos()));
            }
            start_tip = path[0] - t0 * radii[0] * dome_start;
        }
        for (i, p) in path.iter().enumerate() {
            let (_, n, b) = frames[i];
            rings.push(ring(*p, n, b, radii[i]));
        }
        let last = path.len() - 1;
        let (tl, nl, bl) = frames[last];
        let mut end_tip = path[last];
        if dome_end > 0.0 {
            for k in 1..dome_rings {
                let phi = std::f64::consts::FRAC_PI_2 * k as f64 / dome_rings as f64;
                let c = path[last] + tl * radii[last] * dome_end * phi.sin();
                rings.push(ring(c, nl, bl, radii[last] * phi.cos()));
            }
            end_tip = path[last] + tl * radii[last] * dome_end;
        }
        Mesh::loft(&rings, false, Some(start_tip), Some(end_tip))
    }

    /// Sweeps a closed 2D cross-section along a closed planar path lying in
    /// the local XY plane. Section coordinates are (outward offset, z).
    pub fn sweep_closed_planar(path: &[DVec2], section: &[DVec2]) -> Mesh {
        let normals = outward_normals_2d(path);
        let rings: Vec<Vec<DVec3>> = path
            .iter()
            .zip(&normals)
            .map(|(p, n)| section.iter().map(|s| DVec3::new(p.x + n.x * s.x, p.y + n.y * s.x, s.y)).collect())
            .collect();
        Mesh::loft(&rings, true, None, None)
    }

    pub fn uv_sphere(center: DVec3, radius: f64, seg: usize) -> Mesh {
        let rings: Vec<Vec<DVec3>> = (1..seg)
            .map(|i| {
                let phi = std::f64::consts::PI * i as f64 / seg as f64;
                (0..seg * 2)
                    .map(|j| {
                        let th = std::f64::consts::PI * j as f64 / seg as f64;
                        center + DVec3::new(phi.sin() * th.cos(), phi.sin() * th.sin(), -phi.cos()) * radius
                    })
                    .collect()
            })
            .collect();
        Mesh::loft(&rings, false, Some(center - DVec3::Z * radius), Some(center + DVec3::Z * radius))
    }

    pub fn boxed(min: DVec3, max: DVec3) -> Mesh {
        let c = [
            DVec3::new(min.x, min.y, min.z),
            DVec3::new(max.x, min.y, min.z),
            DVec3::new(max.x, max.y, min.z),
            DVec3::new(min.x, max.y, min.z),
        ];
        let lo: Vec<DVec3> = c.to_vec();
        let hi: Vec<DVec3> = c.iter().map(|p| DVec3::new(p.x, p.y, max.z)).collect();
        let cz = |z| DVec3::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0, z);
        Mesh::loft(&[lo, hi], false, Some(cz(min.z)), Some(cz(max.z)))
    }
}

impl Mesh {
    /// Per-corner normals: smooth across edges sharper than `crease_deg`
    /// are kept hard. Returns one normal per triangle corner.
    pub fn corner_normals(&self, crease_deg: f64) -> Vec<[DVec3; 3]> {
        let nt = self.triangles.len();
        let fnorm: Vec<DVec3> = (0..nt)
            .map(|i| {
                let [a, b, c] = self.tri(i);
                (b - a).cross(c - a)
            })
            .collect();
        let funit: Vec<DVec3> = fnorm.iter().map(|n| n.normalize_or_zero()).collect();
        let nv = self.positions.len();
        let mut start = vec![0u32; nv + 1];
        for t in &self.triangles {
            for &v in t {
                start[v as usize + 1] += 1;
            }
        }
        for i in 0..nv {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut adj = vec![0u32; nt * 3];
        for (fi, t) in self.triangles.iter().enumerate() {
            for &v in t {
                adj[fill[v as usize] as usize] = fi as u32;
                fill[v as usize] += 1;
            }
        }
        let crease = crease_deg.to_radians().cos();
        (0..nt)
            .map(|fi| {
                let fu = funit[fi];
                let t = self.triangles[fi];
                let mut out = [DVec3::ZERO; 3];
                for (k, &v) in t.iter().enumerate() {
                    let mut acc = DVec3::ZERO;
                    for &g in &adj[start[v as usize] as usize..start[v as usize + 1] as usize] {
                        if funit[g as usize].dot(fu) >= crease {
                            acc += fnorm[g as usize];
                        }
                    }
                    out[k] = acc.normalize_or(fu);
                }
                out
            })
            .collect()
    }
}

/// Rotation-minimising frames (tangent, normal, binormal) along a polyline.
pub fn parallel_transport_frames(path: &[DVec3]) -> Vec<(DVec3, DVec3, DVec3)> {
    let n = path.len();
    let tangent = |i: usize| -> DVec3 {
        let a = path[i.saturating_sub(1)];
        let b = path[(i + 1).min(n - 1)];
        (b - a).normalize_or(DVec3::Z)
    };
    let t0 = tangent(0);
    let helper = if t0.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let mut nrm = t0.cross(helper).normalize();
    let mut out = Vec::with_capacity(n);
    let mut prev_t = t0;
    for i in 0..n {
        let t = tangent(i);
        let axis = prev_t.cross(t);
        let s = axis.length();
        if s > 1e-12 {
            let ang = s.atan2(prev_t.dot(t));
            nrm = glam::DQuat::from_axis_angle(axis / s, ang) * nrm;
        }
        nrm = (nrm - t * nrm.dot(t)).normalize_or(nrm);
        out.push((t, nrm, t.cross(nrm)));
        prev_t = t;
    }
    out
}

/// Outward unit normals of a closed 2D polygon (works for either winding),
/// with mitre compensation clamped so that sharp corners stay sharp but finite.
pub fn outward_normals_2d(path: &[DVec2]) -> Vec<DVec2> {
    let n = path.len();
    let ccw = signed_area_2d(path) >= 0.0;
    (0..n)
        .map(|i| {
            let p = path[(i + n - 1) % n];
            let c = path[i];
            let q = path[(i + 1) % n];
            let e1 = (c - p).normalize_or_zero();
            let e2 = (q - c).normalize_or_zero();
            let mut n1 = DVec2::new(e1.y, -e1.x);
            let mut n2 = DVec2::new(e2.y, -e2.x);
            if !ccw {
                n1 = -n1;
                n2 = -n2;
            }
            let avg = (n1 + n2).normalize_or(n1);
            let cos_half = avg.dot(n1).max(0.35);
            avg / cos_half
        })
        .collect()
}

pub fn signed_area_2d(path: &[DVec2]) -> f64 {
    let n = path.len();
    (0..n).map(|i| path[i].perp_dot(path[(i + 1) % n])).sum::<f64>() * 0.5
}

/// Offsets a closed polygon outward by `d` (mitred, see [`outward_normals_2d`]).
pub fn offset_polygon(path: &[DVec2], d: f64) -> Vec<DVec2> {
    outward_normals_2d(path).iter().zip(path).map(|(n, p)| *p + *n * d).collect()
}

pub fn polyline_length_2d(path: &[DVec2], closed: bool) -> f64 {
    let n = path.len();
    let segs = if closed { n } else { n - 1 };
    (0..segs).map(|i| path[i].distance(path[(i + 1) % n])).sum()
}

/// Resamples a closed polygon to `count` points evenly spaced by arc length.
pub fn resample_closed_2d(path: &[DVec2], count: usize) -> Vec<DVec2> {
    let n = path.len();
    let total = polyline_length_2d(path, true);
    let mut out = Vec::with_capacity(count);
    let mut seg = 0usize;
    let mut acc = 0.0;
    for k in 0..count {
        let target = total * k as f64 / count as f64;
        loop {
            let l = path[seg % n].distance(path[(seg + 1) % n]);
            if acc + l >= target || seg >= n {
                let t = if l > 0.0 { (target - acc) / l } else { 0.0 };
                out.push(path[seg % n].lerp(path[(seg + 1) % n], t.clamp(0.0, 1.0)));
                break;
            }
            acc += l;
            seg += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_volume() {
        let m = Mesh::boxed(DVec3::ZERO, DVec3::new(2.0, 3.0, 4.0));
        assert!((m.volume() - 24.0).abs() < 1e-9);
    }

    #[test]
    fn sphere_volume_close() {
        let m = Mesh::uv_sphere(DVec3::new(1.0, 2.0, 3.0), 2.0, 48);
        let exact = 4.0 / 3.0 * std::f64::consts::PI * 8.0;
        assert!((m.volume() - exact).abs() / exact < 0.01, "{}", m.volume());
    }

    #[test]
    fn tube_is_outward() {
        let path: Vec<DVec3> = (0..10).map(|i| DVec3::new(0.0, 0.0, i as f64)).collect();
        let m = Mesh::tube(&path, &[0.5; 10], 24, 1.0, 0.0);
        let cyl = std::f64::consts::PI * 0.25 * 9.0;
        assert!(m.volume() > cyl * 0.97);
    }

    #[test]
    fn torus_sweep_volume() {
        let r = 5.0;
        let path: Vec<DVec2> = (0..256)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 256.0;
                DVec2::new(a.cos(), a.sin()) * r
            })
            .collect();
        let sec: Vec<DVec2> = (0..64)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 64.0;
                DVec2::new(a.cos(), a.sin())
            })
            .collect();
        let m = Mesh::sweep_closed_planar(&path, &sec);
        let exact = 2.0 * std::f64::consts::PI.powi(2) * r * 1.0;
        assert!((m.volume() - exact).abs() / exact < 0.01, "{}", m.volume());
    }
}
