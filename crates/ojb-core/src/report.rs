//! Technical report: metal weights (all alloys, g and dwt), gem list with
//! carat estimates, dimensions, finger size conversions.

use crate::csg::union_volume;
use crate::document::{Document, EvalResult, Op};
use crate::materials::{Alloy, GRAMS_PER_DWT};
use crate::mesh::{Aabb, Mesh};
use crate::ops::Role;
use crate::sizes::Region;
use serde::Serialize;
use std::fmt::Write;

#[derive(Clone, Debug, Serialize)]
pub struct MetalLine {
    pub alloy: String,
    pub density: f64,
    pub grams: f64,
    pub dwt: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct GemLine {
    pub description: String,
    pub count: usize,
    pub carat_each_model: f64,
    pub carat_each_formula: Option<f64>,
    pub carat_total: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TechReport {
    pub title: String,
    pub selected_alloy: String,
    pub metal_volume_mm3: f64,
    pub metal_grams: f64,
    pub metal_dwt: f64,
    pub surface_mm2: f64,
    pub metals: Vec<MetalLine>,
    pub gems: Vec<GemLine>,
    pub gem_count: usize,
    pub total_carat: f64,
    pub size_mm: [f64; 3],
    pub finger_sizes: Vec<(String, String)>,
    pub triangles: usize,
}

/// Metal meshes of the final model.
pub fn metal_meshes<'a>(doc: &'a Document, r: &'a EvalResult) -> Vec<&'a Mesh> {
    r.final_outputs(doc)
        .flat_map(|(_, o)| o.solids.iter())
        .filter(|s| s.role == Role::Metal)
        .map(|s| s.mesh.as_ref())
        .collect()
}

pub fn build(doc: &Document, r: &EvalResult) -> TechReport {
    let metal = metal_meshes(doc, r);
    let volume = union_volume(&metal, 500);
    let surface: f64 = metal.iter().map(|m| m.area()).sum();
    let mut bbox = Aabb::EMPTY;
    for m in &metal {
        bbox = bbox.union(&m.bbox());
    }
    let mut lines: Vec<GemLine> = Vec::new();
    let mut total_ct = 0.0;
    let mut count = 0;
    for (_, o) in r.final_outputs(doc) {
        for g in &o.gems {
            let label = g.spec.label();
            let ct = g.spec.carat_from_volume();
            total_ct += ct;
            count += 1;
            match lines.iter_mut().find(|l| l.description == label) {
                Some(l) => {
                    l.count += 1;
                    l.carat_total += ct;
                }
                None => lines.push(GemLine {
                    description: label,
                    count: 1,
                    carat_each_model: ct,
                    carat_each_formula: g.spec.carat_formula(),
                    carat_total: ct,
                }),
            }
        }
    }
    let metals = Alloy::ALL
        .iter()
        .map(|a| {
            let g = a.grams(volume);
            MetalLine { alloy: a.name().into(), density: a.density(), grams: g, dwt: g / GRAMS_PER_DWT }
        })
        .collect();
    let finger_sizes = doc
        .nodes
        .iter()
        .find_map(|n| match &n.op {
            Op::RingRail(p) => Some(p.size),
            _ => None,
        })
        .map(|s| Region::ALL.iter().map(|r| (r.name().to_string(), r.format(s.convert(*r).value))).collect())
        .unwrap_or_default();
    let grams = doc.alloy.grams(volume);
    let triangles = r
        .final_outputs(doc)
        .map(|(_, o)| {
            o.solids.iter().filter(|s| s.role == Role::Metal).map(|s| s.mesh.triangles.len()).sum::<usize>()
                + o.gems.iter().map(|g| g.spec.mesh().triangles.len()).sum::<usize>()
        })
        .sum();
    TechReport {
        title: doc.meta.title.clone(),
        selected_alloy: doc.alloy.name().into(),
        metal_volume_mm3: volume,
        metal_grams: grams,
        metal_dwt: grams / GRAMS_PER_DWT,
        surface_mm2: surface,
        metals,
        gems: lines,
        gem_count: count,
        total_carat: total_ct,
        size_mm: bbox.size().to_array(),
        finger_sizes,
        triangles,
    }
}

impl TechReport {
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "# Tech report — {}\n", if self.title.is_empty() { "untitled" } else { &self.title });
        let _ = writeln!(
            s,
            "**Metal ({}):** {:.2} g / {:.2} dwt — volume {:.1} mm³, surface {:.0} mm²  ",
            self.selected_alloy, self.metal_grams, self.metal_dwt, self.metal_volume_mm3, self.surface_mm2
        );
        let _ = writeln!(s, "**Overall size:** {:.2} × {:.2} × {:.2} mm  ", self.size_mm[0], self.size_mm[1], self.size_mm[2]);
        let _ = writeln!(s, "**Gems:** {} stones, {:.3} ct total\n", self.gem_count, self.total_carat);
        if !self.gems.is_empty() {
            let _ = writeln!(s, "| Gem | Qty | ct each (model) | ct each (formula) | ct total |\n|---|---:|---:|---:|---:|");
            for g in &self.gems {
                let f = g.carat_each_formula.map(|f| format!("{f:.3}")).unwrap_or_else(|| "—".into());
                let _ = writeln!(s, "| {} | {} | {:.3} | {} | {:.3} |", g.description, g.count, g.carat_each_model, f, g.carat_total);
            }
            s.push('\n');
        }
        let _ = writeln!(s, "| Alloy | g/cm³ | grams | dwt |\n|---|---:|---:|---:|");
        for m in &self.metals {
            let _ = writeln!(s, "| {} | {:.2} | {:.2} | {:.2} |", m.alloy, m.density, m.grams, m.dwt);
        }
        if !self.finger_sizes.is_empty() {
            let _ = writeln!(s, "\n| Size system | Size |\n|---|---|");
            for (r, v) in &self.finger_sizes {
                let _ = writeln!(s, "| {r} | {v} |");
            }
        }
        s
    }
}
