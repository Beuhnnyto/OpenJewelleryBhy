//! Background evaluation worker and CPU-side scene baking.
//!
//! The UI thread never runs geometry: it sends document snapshots to a
//! worker that keeps the incremental evaluator, bakes GPU-ready vertex
//! arrays (smooth-shaded metal with crease detection, flat-faceted gems) and
//! builds the tech report. Unchanged node outputs reuse their baked arrays.

use bytemuck::{Pod, Zeroable};
use eframe::egui;
use glam::DVec3;
use ojb_core::document::{Document, EvalResult, NodeId, Op};
use ojb_core::mesh::{Aabb, Mesh};
use ojb_core::ops::{Output, Role};
use ojb_core::report::{self, TechReport};
use ojb_core::Evaluator;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    /// rgb + material (0 metal, 1 gem, 2 unlit)
    pub col: [f32; 4],
}

pub const MAT_METAL: f32 = 0.0;
pub const MAT_GEM: f32 = 1.0;
pub const MAT_UNLIT: f32 = 2.0;

#[derive(Default)]
pub struct SceneNode {
    pub id: NodeId,
    pub metal: Vec<Vertex>,
    pub gems: Vec<Vertex>,
    /// gem vertex ranges and the node that created each gem (for picking)
    pub gem_owner: Vec<(Range<usize>, NodeId)>,
    pub cutters: Vec<Vertex>,
    pub lines: Vec<Vertex>,
    pub in_final: bool,
    pub bbox: Option<Aabb>,
}

pub struct Scene {
    pub nodes: Vec<Arc<SceneNode>>,
    pub bbox: Aabb,
}

pub struct Job {
    pub doc: Document,
    pub preview: bool,
    pub version: u64,
}

pub struct Done {
    pub version: u64,
    pub preview: bool,
    pub doc: Document,
    pub scene: Arc<Scene>,
    pub result: Arc<EvalResult>,
    pub report: Option<TechReport>,
    pub millis: f64,
}

pub fn spawn(ctx: egui::Context) -> (Sender<Job>, Receiver<Done>) {
    let (jtx, jrx) = channel::<Job>();
    let (dtx, drx) = channel::<Done>();
    std::thread::Builder::new()
        .name("ojb-eval".into())
        .spawn(move || {
            let mut ev = Evaluator::new();
            let mut baked: HashMap<NodeId, (usize, [f32; 3], Arc<SceneNode>)> = HashMap::new();
            while let Ok(mut job) = jrx.recv() {
                while let Ok(j) = jrx.try_recv() {
                    job = j; // only the latest snapshot matters
                }
                ev.preview = job.preview;
                let t = std::time::Instant::now();
                let r = ev.evaluate(&job.doc);
                let scene = bake(&job.doc, &r, &mut baked);
                let report = if job.preview || r.error.is_some() { None } else { Some(report::build(&job.doc, &r)) };
                let done = Done {
                    version: job.version,
                    preview: job.preview,
                    doc: job.doc,
                    scene: Arc::new(scene),
                    result: Arc::new(r),
                    report,
                    millis: t.elapsed().as_secs_f64() * 1e3,
                };
                if dtx.send(done).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        })
        .expect("spawn worker");
    (jtx, drx)
}

fn bake(doc: &Document, r: &EvalResult, cache: &mut HashMap<NodeId, (usize, [f32; 3], Arc<SceneNode>)>) -> Scene {
    let consumed = &r.consumed;
    let metal_color = doc.alloy.color();
    // gem centre → node that created it
    let mut owners: HashMap<[i64; 3], NodeId> = HashMap::new();
    for n in &doc.nodes {
        if matches!(n.op, Op::Gem(_) | Op::GemsOnRail(_) | Op::Halo(_)) {
            if let Some(Ok(o)) = r.outputs.get(&n.id) {
                for g in &o.gems {
                    owners.entry(qkey(g.center())).or_insert(n.id);
                }
            }
        }
    }
    let mut nodes = Vec::new();
    let mut bbox = Aabb::EMPTY;
    for n in &doc.nodes {
        let Some(Ok(o)) = r.outputs.get(&n.id) else { continue };
        let in_final = n.visible && !consumed.contains(&n.id);
        let key = Arc::as_ptr(o) as usize;
        let sn = match cache.get(&n.id) {
            Some((k, c, sn)) if *k == key && *c == metal_color && sn.in_final == in_final => sn.clone(),
            _ => {
                let mut sn = bake_output(o, metal_color, &owners, n.id);
                sn.id = n.id;
                sn.in_final = in_final;
                let sn = Arc::new(sn);
                cache.insert(n.id, (key, metal_color, sn.clone()));
                sn
            }
        };
        if in_final {
            if let Some(b) = sn.bbox {
                bbox = bbox.union(&b);
            }
        }
        nodes.push(sn);
    }
    cache.retain(|id, _| doc.get(*id).is_some());
    Scene { nodes, bbox }
}

fn qkey(p: DVec3) -> [i64; 3] {
    [(p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64, (p.z * 1e4).round() as i64]
}

fn bake_output(o: &Output, metal: [f32; 3], owners: &HashMap<[i64; 3], NodeId>, me: NodeId) -> SceneNode {
    let mut sn = SceneNode::default();
    for s in &o.solids {
        match s.role {
            Role::Metal => smooth_vertices(&s.mesh, metal, MAT_METAL, &mut sn.metal),
            Role::Cutter => flat_vertices(&s.mesh, [0.9, 0.15, 0.1], MAT_UNLIT, &mut sn.cutters),
        }
    }
    let mut gem_cache: HashMap<String, Arc<Mesh>> = HashMap::new();
    for g in &o.gems {
        let key = format!("{:?}", g.spec);
        let m = gem_cache.entry(key).or_insert_with(|| g.spec.mesh()).clone();
        let start = sn.gems.len();
        if g.spec.cut == ojb_core::gem::Cut::Pearl {
            smooth_vertices(&m.transformed(&g.xform), g.spec.material.color(), MAT_GEM, &mut sn.gems);
        } else {
            flat_vertices(&m.transformed(&g.xform), g.spec.material.color(), MAT_GEM, &mut sn.gems);
        }
        let owner = owners.get(&qkey(g.center())).copied().unwrap_or(me);
        sn.gem_owner.push((start..sn.gems.len(), owner));
    }
    for c in &o.curves {
        for w in c.windows(2) {
            for p in w {
                sn.lines.push(Vertex { pos: p.as_vec3().to_array(), nrm: [0.0; 3], col: [0.1, 0.55, 0.95, MAT_UNLIT] });
            }
        }
    }
    let ob = o.bbox();
    sn.bbox = if ob.is_empty() { None } else { Some(ob) };
    sn
}

fn flat_vertices(m: &Mesh, color: [f32; 3], mat: f32, out: &mut Vec<Vertex>) {
    out.reserve(m.triangles.len() * 3);
    for i in 0..m.triangles.len() {
        let [a, b, c] = m.tri(i);
        let n = (b - a).cross(c - a).normalize_or_zero().as_vec3().to_array();
        for p in [a, b, c] {
            out.push(Vertex { pos: p.as_vec3().to_array(), nrm: n, col: [color[0], color[1], color[2], mat] });
        }
    }
}

/// Smooth normals, but faces meeting at more than ~35° keep a hard edge.
fn smooth_vertices(m: &Mesh, color: [f32; 3], mat: f32, out: &mut Vec<Vertex>) {
    let normals = m.corner_normals(35.0);
    out.reserve(m.triangles.len() * 3);
    for (t, ns) in m.triangles.iter().zip(&normals) {
        for k in 0..3 {
            out.push(Vertex {
                pos: m.positions[t[k] as usize].as_vec3().to_array(),
                nrm: ns[k].as_vec3().to_array(),
                col: [color[0], color[1], color[2], mat],
            });
        }
    }
}
