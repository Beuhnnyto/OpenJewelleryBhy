//! Parametric gem models: faceted (brilliant / step), cabochon and pearl,
//! with carat estimation both from the exact mesh volume and from the
//! industry rule-of-thumb formulas.

use crate::mesh::Mesh;
use crate::outline::Shape;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cut {
    /// Brilliant-style faceting (kite / star facets).
    Diamond,
    /// Step-cut faceting with a flat table.
    Gem,
    Cabochon,
    Pearl,
}

impl Cut {
    pub const ALL: [Cut; 4] = [Cut::Diamond, Cut::Gem, Cut::Cabochon, Cut::Pearl];
    pub fn name(self) -> &'static str {
        match self {
            Cut::Diamond => "Diamond",
            Cut::Gem => "Gem (step)",
            Cut::Cabochon => "Cabochon",
            Cut::Pearl => "Pearl",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GemMaterial {
    Diamond,
    Moissanite,
    CubicZirconia,
    Sapphire,
    Ruby,
    Emerald,
    Aquamarine,
    Amethyst,
    Topaz,
    Tanzanite,
    Garnet,
    Peridot,
    Opal,
    Pearl,
}

impl GemMaterial {
    pub const ALL: [GemMaterial; 14] = [
        GemMaterial::Diamond,
        GemMaterial::Moissanite,
        GemMaterial::CubicZirconia,
        GemMaterial::Sapphire,
        GemMaterial::Ruby,
        GemMaterial::Emerald,
        GemMaterial::Aquamarine,
        GemMaterial::Amethyst,
        GemMaterial::Topaz,
        GemMaterial::Tanzanite,
        GemMaterial::Garnet,
        GemMaterial::Peridot,
        GemMaterial::Opal,
        GemMaterial::Pearl,
    ];

    /// Specific gravity (g/cm³), typical reference values.
    pub fn sg(self) -> f64 {
        match self {
            GemMaterial::Diamond => 3.52,
            GemMaterial::Moissanite => 3.21,
            GemMaterial::CubicZirconia => 5.80,
            GemMaterial::Sapphire | GemMaterial::Ruby => 4.00,
            GemMaterial::Emerald => 2.72,
            GemMaterial::Aquamarine => 2.70,
            GemMaterial::Amethyst => 2.65,
            GemMaterial::Topaz => 3.53,
            GemMaterial::Tanzanite => 3.35,
            GemMaterial::Garnet => 3.85,
            GemMaterial::Peridot => 3.34,
            GemMaterial::Opal => 2.10,
            GemMaterial::Pearl => 2.71,
        }
    }

    /// Refractive index (yellow) and dispersion (B–G), 0 ior = opaque.
    pub fn optics(self) -> (f64, f64) {
        match self {
            GemMaterial::Diamond => (2.417, 0.044),
            GemMaterial::Moissanite => (2.65, 0.104),
            GemMaterial::CubicZirconia => (2.16, 0.060),
            GemMaterial::Sapphire | GemMaterial::Ruby => (1.77, 0.018),
            GemMaterial::Emerald | GemMaterial::Aquamarine => (1.58, 0.014),
            GemMaterial::Amethyst => (1.55, 0.013),
            GemMaterial::Topaz => (1.62, 0.014),
            GemMaterial::Tanzanite => (1.69, 0.030),
            GemMaterial::Garnet => (1.79, 0.027),
            GemMaterial::Peridot => (1.67, 0.020),
            GemMaterial::Opal => (1.45, 0.0),
            GemMaterial::Pearl => (0.0, 0.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            GemMaterial::Diamond => "Diamond",
            GemMaterial::Moissanite => "Moissanite",
            GemMaterial::CubicZirconia => "Cubic zirconia",
            GemMaterial::Sapphire => "Sapphire",
            GemMaterial::Ruby => "Ruby",
            GemMaterial::Emerald => "Emerald",
            GemMaterial::Aquamarine => "Aquamarine",
            GemMaterial::Amethyst => "Amethyst",
            GemMaterial::Topaz => "Topaz",
            GemMaterial::Tanzanite => "Tanzanite",
            GemMaterial::Garnet => "Garnet",
            GemMaterial::Peridot => "Peridot",
            GemMaterial::Opal => "Opal",
            GemMaterial::Pearl => "Pearl",
        }
    }

    /// Display colour (linear RGB).
    pub fn color(self) -> [f32; 3] {
        match self {
            GemMaterial::Diamond | GemMaterial::Moissanite | GemMaterial::CubicZirconia => [0.92, 0.95, 1.0],
            GemMaterial::Sapphire | GemMaterial::Tanzanite => [0.08, 0.18, 0.75],
            GemMaterial::Ruby | GemMaterial::Garnet => [0.75, 0.03, 0.08],
            GemMaterial::Emerald => [0.02, 0.55, 0.2],
            GemMaterial::Aquamarine => [0.45, 0.8, 0.95],
            GemMaterial::Amethyst => [0.5, 0.2, 0.7],
            GemMaterial::Topaz => [0.95, 0.65, 0.2],
            GemMaterial::Peridot => [0.55, 0.8, 0.1],
            GemMaterial::Opal => [0.85, 0.85, 0.9],
            GemMaterial::Pearl => [0.95, 0.92, 0.86],
        }
    }
}

/// Everything needed to build one gem, independent of where it is placed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GemSpec {
    pub shape: Shape,
    pub cut: Cut,
    pub material: GemMaterial,
    /// Length along local Y (mm).
    pub length: f64,
    /// Width along local X (mm).
    pub width: f64,
    /// Total depth table→culet as % of the smaller of length/width.
    pub depth_pct: f64,
    /// Table size as % of the outline.
    pub table_pct: f64,
    /// Crown height as % of the smaller dimension.
    pub crown_pct: f64,
    /// Girdle thickness as % of the smaller dimension.
    pub girdle_pct: f64,
}

impl Default for GemSpec {
    fn default() -> Self {
        GemSpec::round(6.5)
    }
}

impl GemSpec {
    pub fn round(d: f64) -> GemSpec {
        GemSpec::new(Shape::Round, d, d)
    }

    pub fn new(shape: Shape, width: f64, length: f64) -> GemSpec {
        let step = shape.prefers_step_cut();
        GemSpec {
            shape,
            cut: if step { Cut::Gem } else { Cut::Diamond },
            material: GemMaterial::Diamond,
            length,
            width,
            depth_pct: if step { 66.0 } else { 61.0 },
            table_pct: if step { 65.0 } else { 57.0 },
            crown_pct: if step { 12.0 } else { 15.0 },
            girdle_pct: 3.0,
        }
    }

    pub fn min_dim(&self) -> f64 {
        self.length.min(self.width)
    }

    pub fn depth(&self) -> f64 {
        match self.cut {
            Cut::Pearl => self.width,
            _ => self.min_dim() * self.depth_pct / 100.0,
        }
    }

    /// Key heights relative to the girdle centre: (table z, girdle half
    /// thickness, culet z).
    pub fn heights(&self) -> GemHeights {
        let m = self.min_dim();
        match self.cut {
            Cut::Pearl => {
                let r = self.width / 2.0;
                GemHeights { table: r, girdle_half: 0.0, culet: -r }
            }
            Cut::Cabochon => {
                let g = m * self.girdle_pct / 100.0;
                GemHeights { table: self.depth() - g / 2.0, girdle_half: g / 2.0, culet: -g / 2.0 }
            }
            _ => {
                let g = m * self.girdle_pct / 100.0;
                let crown = m * self.crown_pct / 100.0;
                let pav = (self.depth() - crown - g).max(0.05 * m);
                GemHeights { table: g / 2.0 + crown, girdle_half: g / 2.0, culet: -g / 2.0 - pav }
            }
        }
    }

    pub fn outline(&self, n: usize) -> Vec<DVec2> {
        let shape = if self.cut == Cut::Pearl { &Shape::Round } else { &self.shape };
        let w = self.width;
        let l = if self.cut == Cut::Pearl { self.width } else { self.length };
        shape.outline(w, l, n)
    }

    /// Outline resolution adapted to gem size: small melee needs few facets.
    pub fn resolution(&self) -> usize {
        let m = self.length.max(self.width);
        if m < 1.6 {
            16
        } else if m < 4.0 {
            32
        } else {
            48
        }
    }

    pub fn build_mesh(&self) -> Mesh {
        let n = self.resolution();
        match self.cut {
            Cut::Pearl => Mesh::uv_sphere(DVec3::ZERO, self.width / 2.0, 40),
            Cut::Cabochon => cabochon(self, n),
            Cut::Diamond if !self.shape.prefers_step_cut() => brilliant(self, n),
            _ => step(self, n),
        }
    }

    /// Cached shared mesh for this spec (gems are instanced a lot in pavé).
    pub fn mesh(&self) -> Arc<Mesh> {
        static CACHE: OnceLock<Mutex<HashMap<String, Arc<Mesh>>>> = OnceLock::new();
        let key = serde_json::to_string(self).unwrap_or_default();
        let cache = CACHE.get_or_init(Default::default);
        if let Some(m) = cache.lock().unwrap().get(&key) {
            return m.clone();
        }
        let m = Arc::new(self.build_mesh());
        let mut c = cache.lock().unwrap();
        if c.len() > 4096 {
            c.clear();
        }
        c.insert(key, m.clone());
        m
    }

    pub fn volume_mm3(&self) -> f64 {
        self.mesh().volume()
    }

    /// Carat weight from the exact model volume: mm³ × SG / 200.
    pub fn carat_from_volume(&self) -> f64 {
        self.volume_mm3() * self.material.sg() / 200.0
    }

    /// Industry estimation formulas (L × W × D × factor), scaled from the
    /// diamond SG to the gem material. Returns `None` when no standard
    /// formula exists for the shape.
    pub fn carat_formula(&self) -> Option<f64> {
        let (l, w) = (self.length.max(self.width), self.length.min(self.width));
        let d = self.depth();
        let ratio = l / w;
        let sg = self.material.sg() / 3.52;
        let lwd = l * w * d;
        let interp = |table: &[(f64, f64)]| -> f64 {
            if ratio <= table[0].0 {
                return table[0].1;
            }
            for win in table.windows(2) {
                let ((r0, f0), (r1, f1)) = (win[0], win[1]);
                if ratio <= r1 {
                    return f0 + (f1 - f0) * (ratio - r0) / (r1 - r0);
                }
            }
            table[table.len() - 1].1
        };
        let ct = match (self.cut, &self.shape) {
            (Cut::Pearl, _) => return None,
            (Cut::Cabochon, _) => lwd * 0.0026 * self.material.sg(),
            (_, Shape::Round) => w * w * d * 0.0061 * sg,
            (_, Shape::Oval) => ((l + w) / 2.0).powi(2) * d * 0.0062 * sg,
            (_, Shape::Emerald | Shape::Asscher | Shape::Radiant | Shape::Octagon) => {
                lwd * interp(&[(1.0, 0.0080), (1.5, 0.0092), (2.0, 0.0100), (2.5, 0.0106)]) * sg
            }
            (_, Shape::Marquise) => lwd * interp(&[(1.5, 0.00565), (2.0, 0.0058), (2.5, 0.00585), (3.0, 0.00595)]) * sg,
            (_, Shape::Pear) => lwd * interp(&[(1.25, 0.00615), (1.5, 0.0060), (1.66, 0.0059), (2.0, 0.00575)]) * sg,
            (_, Shape::Princess) => lwd * 0.0083 * sg,
            (_, Shape::Cushion) => lwd * 0.0081 * sg,
            (_, Shape::Heart) => lwd * 0.0059 * sg,
            (_, Shape::Trillion | Shape::TrillionCurved | Shape::Triangle) => lwd * 0.0057 * sg,
            (_, Shape::Baguette | Shape::TaperedBaguette) => lwd * 0.00915 * sg,
            _ => return None,
        };
        Some(ct)
    }

    pub fn label(&self) -> String {
        if self.cut == Cut::Pearl {
            return format!("Pearl {:.2} mm", self.width);
        }
        if (self.length - self.width).abs() < 1e-6 {
            format!("{} {} {:.2} mm", self.material.name(), self.shape.name(), self.width)
        } else {
            format!("{} {} {:.2}x{:.2} mm", self.material.name(), self.shape.name(), self.length, self.width)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GemHeights {
    pub table: f64,
    pub girdle_half: f64,
    pub culet: f64,
}

fn ring3(o: &[DVec2], scale: f64, z: f64) -> Vec<DVec3> {
    o.iter().map(|p| DVec3::new(p.x * scale, p.y * scale, z)).collect()
}

/// Outline points at odd indices only (half resolution, rotated half a step).
fn half_ring(o: &[DVec2], scale: f64, z: f64, odd: bool) -> Vec<DVec3> {
    o.iter()
        .enumerate()
        .filter(|(i, _)| (i % 2 == 1) == odd)
        .map(|(_, p)| DVec3::new(p.x * scale, p.y * scale, z))
        .collect()
}

/// Brilliant-style faceting: girdle (n) → table (n/2) via kite/star
/// triangles on the crown; girdle → mid pavilion ring (n/2) → culet.
fn brilliant(spec: &GemSpec, n: usize) -> Mesh {
    let o = spec.outline(n);
    let h = spec.heights();
    let t = spec.table_pct / 100.0;
    let mut m = Mesh::new();
    let g_top = ring3(&o, 1.0, h.girdle_half);
    let g_bot = ring3(&o, 1.0, -h.girdle_half);
    let table = half_ring(&o, t, h.table, false);
    let pav_frac = 0.55;
    let pav_mid = half_ring(&o, 1.0 - pav_frac, -h.girdle_half + (h.culet + h.girdle_half) * pav_frac, true);
    let culet = DVec3::new(0.0, 0.0, h.culet);
    let tc = DVec3::new(0.0, 0.0, h.table);

    let push = |m: &mut Mesh, pts: [DVec3; 3]| {
        let b = m.positions.len() as u32;
        m.positions.extend_from_slice(&pts);
        m.triangles.push([b, b + 1, b + 2]);
    };
    let k = n / 2;
    for i in 0..k {
        let g0 = g_top[2 * i];
        let g1 = g_top[2 * i + 1];
        let g2 = g_top[(2 * i + 2) % n];
        let t0 = table[i];
        let t1 = table[(i + 1) % k];
        // crown facets (outward = CCW seen from outside)
        push(&mut m, [g0, g1, t0]);
        push(&mut m, [g1, g2, t1]);
        push(&mut m, [g1, t1, t0]);
        // table fan
        push(&mut m, [tc, t0, t1]);
        // girdle band
        for j in [2 * i, 2 * i + 1] {
            let a = g_bot[j];
            let b = g_bot[(j + 1) % n];
            let c = g_top[(j + 1) % n];
            let d = g_top[j];
            push(&mut m, [a, b, c]);
            push(&mut m, [a, c, d]);
        }
        // pavilion: girdle bottom → mid ring (odd) → culet
        let b0 = g_bot[2 * i];
        let b1 = g_bot[2 * i + 1];
        let b2 = g_bot[(2 * i + 2) % n];
        let p_prev = pav_mid[(i + k - 1) % k];
        let p_cur = pav_mid[i];
        push(&mut m, [b1, b0, p_cur]);
        push(&mut m, [b0, p_prev, p_cur]);
        push(&mut m, [b2, b1, p_cur]);
        push(&mut m, [p_cur, p_prev, culet]);
    }
    m
}

/// Step cut: concentric rings with flat facets.
fn step(spec: &GemSpec, n: usize) -> Mesh {
    let o = spec.outline(n);
    let h = spec.heights();
    let t = spec.table_pct / 100.0;
    let crown_mid = (1.0 + t) / 2.0;
    let gh = h.girdle_half;
    let pav = h.culet + gh; // negative
    let rings = vec![
        ring3(&o, 0.12, h.culet - pav * 0.0),
        ring3(&o, 0.45, -gh + pav * 0.62),
        ring3(&o, 0.75, -gh + pav * 0.3),
        ring3(&o, 1.0, -gh),
        ring3(&o, 1.0, gh),
        ring3(&o, crown_mid, gh + (h.table - gh) * 0.55),
        ring3(&o, t, h.table),
    ];
    Mesh::loft(&rings, false, Some(DVec3::new(0.0, 0.0, h.culet)), Some(DVec3::new(0.0, 0.0, h.table)))
}

fn cabochon(spec: &GemSpec, n: usize) -> Mesh {
    let o = spec.outline(n);
    let h = spec.heights();
    let dome = h.table - h.girdle_half;
    let mut rings = vec![ring3(&o, 1.0, h.culet), ring3(&o, 1.0, h.girdle_half)];
    let steps = 10;
    for i in 1..steps {
        let a = std::f64::consts::FRAC_PI_2 * i as f64 / steps as f64;
        rings.push(ring3(&o, a.cos(), h.girdle_half + dome * a.sin()));
    }
    Mesh::loft(&rings, false, Some(DVec3::new(0.0, 0.0, h.culet)), Some(DVec3::new(0.0, 0.0, h.table)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_one_carat_close_to_formula() {
        let g = GemSpec::round(6.5);
        let f = g.carat_formula().unwrap();
        let v = g.carat_from_volume();
        assert!((f - 1.02).abs() < 0.02, "formula {f}");
        assert!((v - f).abs() / f < 0.08, "mesh {v} vs formula {f}");
    }

    #[test]
    fn all_shapes_build_closed_positive() {
        for s in Shape::BUILTIN.iter() {
            for cut in Cut::ALL {
                let mut g = GemSpec::new(s.clone(), 5.0, 5.0 * s.default_ratio());
                g.cut = cut;
                let v = g.build_mesh().volume();
                assert!(v > 0.0, "{:?} {:?} volume {v}", s, cut);
            }
        }
    }

    #[test]
    fn princess_formula_vs_mesh() {
        let g = GemSpec::new(Shape::Princess, 5.5, 5.5);
        let f = g.carat_formula().unwrap();
        let v = g.carat_from_volume();
        assert!((v - f).abs() / f < 0.2, "mesh {v} vs formula {f}");
    }
}
