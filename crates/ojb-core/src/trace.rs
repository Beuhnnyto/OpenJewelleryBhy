//! Photorealistic progressive path tracer ("Render Studio"): BVH-accelerated,
//! polished metal with Fresnel reflections, gems as dispersive dielectrics
//! (per-channel IOR → fire), studio softbox environment, ground plane.

use crate::render::{Item, Shading};
use glam::{DVec3, Vec3};
use rayon::prelude::*;

#[derive(Clone, Copy)]
enum Mat {
    Metal { color: Vec3, rough: f32 },
    Gem { color: Vec3, ior: f32, disp: f32 },
    Pearl { color: Vec3 },
    Ground,
}

#[derive(Clone, Copy)]
struct Tri {
    a: Vec3,
    e1: Vec3,
    e2: Vec3,
    n: [Vec3; 3],
    mat: u32,
}

#[derive(Clone, Copy)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// leaf: first triangle, inner: left child (right = left + 1)
    start: u32,
    count: u32,
}

pub struct Tracer {
    tris: Vec<Tri>,
    nodes: Vec<Node>,
    mats: Vec<Mat>,
    ground_z: f32,
    scale: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: DVec3,
    pub target: DVec3,
    pub up: DVec3,
    pub fov_deg: f64,
}

impl Camera {
    /// Frames a bounding box from a yaw/pitch direction (like render::View).
    pub fn framing(b: &crate::mesh::Aabb, yaw: f64, pitch: f64) -> Camera {
        let (y, p) = (yaw.to_radians(), pitch.to_radians());
        let dir = DVec3::new(y.sin() * p.cos(), -y.cos() * p.cos(), p.sin());
        let r = b.size().length() * 0.5;
        let fov: f64 = 30.0;
        let dist = r / (fov.to_radians() * 0.5).sin() * 1.05;
        Camera { eye: b.center() + dir * dist, target: b.center(), up: DVec3::Z, fov_deg: fov }
    }
}

struct Rng(u64);
impl Rng {
    #[inline]
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

impl Tracer {
    pub fn new(items: &[Item]) -> Tracer {
        let mut tris = Vec::new();
        let mut mats = Vec::new();
        let mut zmin = f64::INFINITY;
        let mut bb = crate::mesh::Aabb::EMPTY;
        for it in items {
            let c = Vec3::from(it.color);
            let mat = match it.shading {
                Shading::Metal => Mat::Metal { color: c, rough: 0.035 },
                Shading::Gem if it.optics.0 <= 0.0 => Mat::Pearl { color: c },
                Shading::Gem => Mat::Gem { color: c, ior: it.optics.0 as f32, disp: it.optics.1 as f32 },
            };
            let mi = mats.len() as u32;
            mats.push(mat);
            let normals = if it.shading == Shading::Metal { it.mesh.corner_normals(32.0) } else { vec![] };
            for i in 0..it.mesh.triangles.len() {
                let [a, b, c] = it.mesh.tri(i);
                zmin = zmin.min(a.z).min(b.z).min(c.z);
                bb.grow(a);
                bb.grow(b);
                bb.grow(c);
                let fn_ = (b - a).cross(c - a).normalize_or_zero().as_vec3();
                let n = if normals.is_empty() { [fn_; 3] } else { normals[i].map(|v| v.as_vec3()) };
                tris.push(Tri { a: a.as_vec3(), e1: (b - a).as_vec3(), e2: (c - a).as_vec3(), n, mat: mi });
            }
        }
        mats.push(Mat::Ground);
        let mut t = Tracer { tris, nodes: vec![], mats, ground_z: zmin as f32 - 0.02, scale: bb.size().length().max(1.0) as f32 };
        t.build();
        t
    }

    fn build(&mut self) {
        let n = self.tris.len();
        if n == 0 {
            return;
        }
        let bounds = |t: &Tri| -> (Vec3, Vec3) {
            let b = t.a + t.e1;
            let c = t.a + t.e2;
            (t.a.min(b).min(c), t.a.max(b).max(c))
        };
        let cent = |t: &Tri| t.a + (t.e1 + t.e2) / 3.0;
        self.nodes.reserve(2 * n / 3 + 1);
        self.nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, start: 0, count: n as u32 });
        let mut stack = vec![0usize];
        while let Some(ni) = stack.pop() {
            let (s, c) = (self.nodes[ni].start as usize, self.nodes[ni].count as usize);
            let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let (mut clo, mut chi) = (lo, hi);
            for t in &self.tris[s..s + c] {
                let (a, b) = bounds(t);
                lo = lo.min(a);
                hi = hi.max(b);
                let ce = cent(t);
                clo = clo.min(ce);
                chi = chi.max(ce);
            }
            self.nodes[ni].lo = lo;
            self.nodes[ni].hi = hi;
            if c <= 4 {
                continue;
            }
            let ext = chi - clo;
            let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
            let mid = c / 2;
            self.tris[s..s + c].select_nth_unstable_by(mid, |p, q| cent(p)[axis].partial_cmp(&cent(q)[axis]).unwrap());
            let left = self.nodes.len();
            self.nodes.push(Node { lo, hi, start: s as u32, count: mid as u32 });
            self.nodes.push(Node { lo, hi, start: (s + mid) as u32, count: (c - mid) as u32 });
            self.nodes[ni].start = left as u32;
            self.nodes[ni].count = 0;
            stack.push(left);
            stack.push(left + 1);
        }
    }

    #[inline]
    fn slab(o: Vec3, inv: Vec3, lo: Vec3, hi: Vec3, tmax: f32) -> Option<f32> {
        let t1 = (lo - o) * inv;
        let t2 = (hi - o) * inv;
        let tn = t1.min(t2).max_element().max(0.0);
        let tf = t1.max(t2).min_element().min(tmax);
        (tn <= tf).then_some(tn)
    }

    /// Closest hit: (t, triangle, u, v).
    fn intersect(&self, o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, usize, f32, f32)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = d.recip();
        let mut best: Option<(f32, usize, f32, f32)> = None;
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let node = self.nodes[stack[sp] as usize];
            let lim = best.map_or(tmax, |b| b.0);
            if Self::slab(o, inv, node.lo, node.hi, lim).is_none() {
                continue;
            }
            if node.count > 0 {
                for i in node.start..node.start + node.count {
                    let t = &self.tris[i as usize];
                    let p = d.cross(t.e2);
                    let det = t.e1.dot(p);
                    if det.abs() < 1e-12 {
                        continue;
                    }
                    let id = 1.0 / det;
                    let s = o - t.a;
                    let u = s.dot(p) * id;
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let q = s.cross(t.e1);
                    let v = d.dot(q) * id;
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let tt = t.e2.dot(q) * id;
                    if tt > 1e-5 && tt < best.map_or(tmax, |b| b.0) {
                        best = Some((tt, i as usize, u, v));
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = node.start;
                stack[sp + 1] = node.start + 1;
                sp += 2;
            }
        }
        best
    }

    /// Studio lighting: soft gradient + three softboxes.
    fn env(&self, d: Vec3) -> Vec3 {
        let up = d.z.clamp(-1.0, 1.0);
        let mut c = Vec3::new(0.55, 0.57, 0.62) * (0.35 + 0.65 * (up * 0.5 + 0.5)).powf(1.5);
        let boxes = [
            (Vec3::new(0.0, 0.0, 1.0), 0.55f32, 5.0f32),
            (Vec3::new(-0.8, -0.4, 0.45).normalize(), 0.35, 3.5),
            (Vec3::new(0.85, 0.25, 0.35).normalize(), 0.3, 2.5),
        ];
        for (dir, size, power) in boxes {
            let cos = d.dot(dir);
            let edge = (1.0 - size * size * 0.5).max(0.0);
            if cos > edge {
                c += Vec3::splat(power) * ((cos - edge) / (1.0 - edge)).sqrt().min(1.0).powf(0.25);
            }
        }
        c
    }

    fn radiance(&self, mut o: Vec3, mut d: Vec3, rng: &mut Rng, channel: usize) -> Vec3 {
        let mut through = Vec3::ONE;
        let mut inside_gem: Option<Vec3> = None;
        for depth in 0..14 {
            let ground_t = if d.z < -1e-6 { (self.ground_z - o.z) / d.z } else { f32::INFINITY };
            let hit = self.intersect(o, d, ground_t.min(1e6));
            match hit {
                None if ground_t.is_finite() && ground_t > 0.0 => {
                    // diffuse ground: cosine sample the environment
                    let p = o + d * ground_t;
                    let (r1, r2) = (rng.next(), rng.next());
                    let phi = std::f32::consts::TAU * r1;
                    let r = r2.sqrt();
                    let nd = Vec3::new(r * phi.cos(), r * phi.sin(), (1.0 - r2).sqrt());
                    through *= Vec3::splat(0.82);
                    // fade the floor into the background far away
                    let fade = (-(p - o).length() / (self.scale * 6.0)).exp();
                    if depth > 3 || fade < 0.05 {
                        return through * self.env(nd);
                    }
                    o = p + Vec3::Z * 1e-4;
                    d = nd;
                }
                None => return through * self.env(d),
                Some((t, ti, u, v)) => {
                    let tri = &self.tris[ti];
                    let p = o + d * t;
                    let ng = tri.e1.cross(tri.e2).normalize();
                    let mut n = (tri.n[0] * (1.0 - u - v) + tri.n[1] * u + tri.n[2] * v).normalize();
                    if let Some(col) = inside_gem {
                        // Beer–Lambert body colour
                        let absorb = (Vec3::ONE - col).max(Vec3::ZERO) * 0.35;
                        through *= Vec3::new((-absorb.x * t).exp(), (-absorb.y * t).exp(), (-absorb.z * t).exp());
                    }
                    let front = d.dot(ng) < 0.0;
                    if !front {
                        n = -n;
                    }
                    match self.mats[tri.mat as usize] {
                        Mat::Metal { color, rough } => {
                            let cos = (-d.dot(n)).clamp(0.0, 1.0);
                            let f = color + (Vec3::ONE - color) * (1.0 - cos).powi(5);
                            through *= f;
                            let mut r = d - n * 2.0 * d.dot(n);
                            r = (r + random_unit(rng) * rough).normalize();
                            if r.dot(n) <= 0.0 {
                                r = d - n * 2.0 * d.dot(n);
                            }
                            o = p + n * 1e-4;
                            d = r;
                        }
                        Mat::Pearl { color } => {
                            let cos = (-d.dot(n)).clamp(0.0, 1.0);
                            let fr = 0.04 + 0.96 * (1.0 - cos).powi(5);
                            o = p + n * 1e-4;
                            if rng.next() < fr {
                                d = (d - n * 2.0 * d.dot(n) + random_unit(rng) * 0.08).normalize();
                            } else {
                                through *= color * 0.95;
                                d = (n + random_unit(rng)).normalize();
                            }
                        }
                        Mat::Gem { color, ior, disp } => {
                            // channel-dependent IOR gives dispersion ("fire")
                            let ior_c = ior + disp * [-0.5, 0.0, 0.6][channel];
                            let (eta, entering) = if front { (1.0 / ior_c, true) } else { (ior_c, false) };
                            let cosi = (-d.dot(n)).clamp(-1.0, 1.0);
                            let k = 1.0 - eta * eta * (1.0 - cosi * cosi);
                            let fres = if k < 0.0 {
                                1.0
                            } else {
                                let r0 = ((1.0 - ior_c) / (1.0 + ior_c)).powi(2);
                                r0 + (1.0 - r0) * (1.0 - cosi).powi(5)
                            };
                            if rng.next() < fres {
                                d = d - n * 2.0 * d.dot(n);
                                o = p + n * 1e-4;
                            } else {
                                d = (d * eta + n * (eta * cosi - k.sqrt())).normalize();
                                o = p - n * 1e-4;
                                inside_gem = if entering { Some(color) } else { None };
                            }
                        }
                        Mat::Ground => unreachable!(),
                    }
                }
            }
            if depth > 5 {
                let q = through.max_element().clamp(0.05, 0.95);
                if rng.next() > q {
                    return Vec3::ZERO;
                }
                through /= q;
            }
        }
        Vec3::ZERO
    }

    /// Adds one sample per pixel to `accum` (linear RGB, w*h*3).
    pub fn render_pass(&self, cam: &Camera, w: usize, h: usize, accum: &mut [f32], pass: u32) {
        let eye = cam.eye.as_vec3();
        let fwd = (cam.target - cam.eye).normalize().as_vec3();
        let right = fwd.cross(cam.up.as_vec3()).normalize();
        let up = right.cross(fwd);
        let tan = (cam.fov_deg.to_radians() as f32 * 0.5).tan();
        let aspect = w as f32 / h as f32;
        accum.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ ((y as u64) << 32) ^ (pass as u64).wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
            for x in 0..w {
                let sx = ((x as f32 + rng.next()) / w as f32 * 2.0 - 1.0) * tan * aspect;
                let sy = (1.0 - (y as f32 + rng.next()) / h as f32 * 2.0) * tan;
                let d = (fwd + right * sx + up * sy).normalize();
                let ch = (rng.next() * 3.0).min(2.999) as usize;
                let l = self.radiance(eye, d, &mut rng, ch);
                // spectral-by-channel estimator: only the sampled channel, ×3
                row[x * 3 + ch] += l[ch] * 3.0;
            }
        });
    }
}

fn random_unit(rng: &mut Rng) -> Vec3 {
    loop {
        let v = Vec3::new(rng.next() * 2.0 - 1.0, rng.next() * 2.0 - 1.0, rng.next() * 2.0 - 1.0);
        let l = v.length_squared();
        if l > 1e-6 && l <= 1.0 {
            return v / l.sqrt();
        }
    }
}

/// Tone-maps an accumulation buffer to RGBA8 (ACES-like curve).
pub fn tonemap(accum: &[f32], passes: u32, out: &mut [u8]) {
    let inv = 1.0 / passes.max(1) as f32;
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        for k in 0..3 {
            let v = accum[i * 3 + k] * inv * 0.8;
            let m = (v * (2.51 * v + 0.03)) / (v * (2.43 * v + 0.59) + 0.14);
            px[k] = (m.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8;
        }
        px[3] = 255;
    }
}

/// Convenience: full render of a document view to RGBA.
pub fn render(items: &[Item], cam: &Camera, w: usize, h: usize, spp: u32) -> Vec<u8> {
    let tr = Tracer::new(items);
    let mut acc = vec![0f32; w * h * 3];
    for s in 0..spp {
        tr.render_pass(cam, w, h, &mut acc, s);
    }
    let mut out = vec![0u8; w * h * 4];
    tonemap(&acc, spp, &mut out);
    out
}
