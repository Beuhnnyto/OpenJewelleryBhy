//! Parametric history ("dynamic commands"): a document is a DAG of nodes.
//! Each node is a builder with parameters and references to earlier nodes.
//!
//! The evaluator caches every node's output under a content hash of its
//! parameters and of its inputs' hashes, so editing one parameter only
//! re-runs that node and its descendants. Independent nodes of the same
//! dependency level are evaluated in parallel.

use crate::materials::Alloy;
use crate::ops::{self, Output};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u32);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "params", rename_all = "snake_case")]
pub enum Op {
    RingRail(ops::rail::RingRailParams),
    Shank(ops::shank::ShankParams),
    Gem(ops::gems::GemParams),
    GemsOnRail(ops::gems::GemsOnRailParams),
    Head(ops::head::HeadParams),
    Bezel(ops::bezel::BezelParams),
    Halo(ops::halo::HaloParams),
    Prongs(ops::prongs::ProngsParams),
    GemCutter(ops::cutters::GemCutterParams),
    Scoop(ops::cutters::ScoopParams),
    Boolean(ops::boolean::BooleanParams),
    PolarArray(ops::transform::PolarArrayParams),
    Mirror(ops::transform::MirrorParams),
    TextOnRail(ops::text::TextOnRailParams),
    Text3d(ops::text::Text3dParams),
    Signet(ops::signet::SignetParams),
    ChannelCutter(ops::cutters::ChannelCutterParams),
    ScallopCutter(ops::cutters::ScallopParams),
    Azure(ops::cutters::AzureParams),
    PearlCup(ops::extras::PearlCupParams),
    Bail(ops::extras::BailParams),
    SplitShank(ops::extras::SplitShankParams),
    Rope(ops::extras::RopeParams),
    Deform(ops::extras::DeformParams),
    GemsOnSurface(ops::extras::GemsOnSurfaceParams),
    Import(ops::extras::ImportParams),
}

/// Builder categories, as on MatrixGold's ribbon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Tools,
    Gems,
    Settings,
    Cutters,
    Solid,
    Transform,
}

impl Op {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Op::RingRail(_) => "Ring Rail",
            Op::Shank(_) => "Shank",
            Op::Gem(_) => "Gem",
            Op::GemsOnRail(_) => "Gems On Rail",
            Op::Head(_) => "Head",
            Op::Bezel(_) => "Bezel",
            Op::Halo(_) => "Halo",
            Op::Prongs(_) => "Prong Placer",
            Op::GemCutter(_) => "Gem Cutter",
            Op::Scoop(_) => "Scoop",
            Op::Boolean(_) => "Boolean",
            Op::PolarArray(_) => "Polar Array",
            Op::Mirror(_) => "Mirror",
            Op::TextOnRail(_) => "Text On Ring",
            Op::Text3d(_) => "Text Object",
            Op::Signet(_) => "Signet Ring",
            Op::ChannelCutter(_) => "Channel Cutter",
            Op::ScallopCutter(_) => "Scallop Cutter",
            Op::Azure(_) => "Azure Cutter",
            Op::PearlCup(_) => "Pearl Cup",
            Op::Bail(_) => "Bail",
            Op::SplitShank(_) => "Split / Bypass Shank",
            Op::Rope(_) => "Rope",
            Op::Deform(_) => "Twist / Taper",
            Op::GemsOnSurface(_) => "Gems On Surface",
            Op::Import(_) => "Imported Mesh",
        }
    }

    pub fn category(&self) -> Category {
        match self {
            Op::RingRail(_) | Op::Shank(_) | Op::Signet(_) | Op::SplitShank(_) | Op::TextOnRail(_) | Op::Text3d(_) => Category::Tools,
            Op::Gem(_) | Op::GemsOnRail(_) | Op::Halo(_) | Op::GemsOnSurface(_) => Category::Gems,
            Op::Head(_) | Op::Bezel(_) | Op::Prongs(_) | Op::PearlCup(_) | Op::Bail(_) => Category::Settings,
            Op::GemCutter(_) | Op::Scoop(_) | Op::ChannelCutter(_) | Op::ScallopCutter(_) | Op::Azure(_) => Category::Cutters,
            Op::Boolean(_) | Op::Rope(_) | Op::Import(_) => Category::Solid,
            Op::PolarArray(_) | Op::Mirror(_) | Op::Deform(_) => Category::Transform,
        }
    }

    /// Nodes this op reads.
    pub fn inputs(&self) -> Vec<NodeId> {
        match self {
            Op::RingRail(_) => vec![],
            Op::Shank(p) => vec![p.rail],
            Op::Gem(p) => match &p.placement {
                ops::gems::Placement::OnRail { rail, .. } => vec![*rail],
                _ => vec![],
            },
            Op::GemsOnRail(p) => vec![p.rail],
            Op::Head(p) => vec![p.gem],
            Op::Bezel(p) => vec![p.gem],
            Op::Halo(p) => vec![p.gem],
            Op::Prongs(p) => vec![p.gems],
            Op::GemCutter(p) => vec![p.gems],
            Op::Scoop(p) => vec![p.shank],
            Op::Boolean(p) => p.targets.iter().chain(p.tools.iter()).copied().collect(),
            Op::PolarArray(p) => vec![p.source],
            Op::Mirror(p) => vec![p.source],
            Op::TextOnRail(p) => vec![p.rail],
            Op::Text3d(_) | Op::Bail(_) | Op::Import(_) => vec![],
            Op::Signet(p) => vec![p.rail],
            Op::ChannelCutter(p) => vec![p.gems],
            Op::ScallopCutter(p) => vec![p.gem],
            Op::Azure(p) => vec![p.shank],
            Op::PearlCup(p) => vec![p.gem],
            Op::SplitShank(p) => vec![p.rail],
            Op::Rope(p) => vec![p.rail],
            Op::Deform(p) => vec![p.source],
            Op::GemsOnSurface(p) => vec![p.target],
        }
    }

    /// Inputs whose geometry this op absorbs (hidden from the final model).
    pub fn consumes(&self) -> Vec<NodeId> {
        match self {
            Op::Boolean(p) => p.targets.iter().chain(p.tools.iter()).copied().collect(),
            Op::PolarArray(p) => vec![p.source],
            Op::Mirror(p) => vec![p.source],
            Op::Deform(p) => vec![p.source],
            _ => vec![],
        }
    }

    /// Rewrites references (used when duplicating / remapping templates).
    pub fn remap(&mut self, f: &dyn Fn(NodeId) -> NodeId) {
        match self {
            Op::RingRail(_) => {}
            Op::Shank(p) => p.rail = f(p.rail),
            Op::Gem(p) => {
                if let ops::gems::Placement::OnRail { rail, .. } = &mut p.placement {
                    *rail = f(*rail)
                }
            }
            Op::GemsOnRail(p) => p.rail = f(p.rail),
            Op::Head(p) => p.gem = f(p.gem),
            Op::Bezel(p) => p.gem = f(p.gem),
            Op::Halo(p) => p.gem = f(p.gem),
            Op::Prongs(p) => p.gems = f(p.gems),
            Op::GemCutter(p) => p.gems = f(p.gems),
            Op::Scoop(p) => p.shank = f(p.shank),
            Op::Boolean(p) => {
                p.targets.iter_mut().for_each(|x| *x = f(*x));
                p.tools.iter_mut().for_each(|x| *x = f(*x));
            }
            Op::PolarArray(p) => p.source = f(p.source),
            Op::Mirror(p) => p.source = f(p.source),
            Op::TextOnRail(p) => p.rail = f(p.rail),
            Op::Text3d(_) | Op::Bail(_) | Op::Import(_) => {}
            Op::Signet(p) => p.rail = f(p.rail),
            Op::ChannelCutter(p) => p.gems = f(p.gems),
            Op::ScallopCutter(p) => p.gem = f(p.gem),
            Op::Azure(p) => p.shank = f(p.shank),
            Op::PearlCup(p) => p.gem = f(p.gem),
            Op::SplitShank(p) => p.rail = f(p.rail),
            Op::Rope(p) => p.rail = f(p.rail),
            Op::Deform(p) => p.source = f(p.source),
            Op::GemsOnSurface(p) => p.target = f(p.target),
        }
    }

    fn eval(&self, get: &dyn Fn(NodeId) -> Result<Arc<Output>, String>, preview: bool) -> Result<Output, String> {
        let one = |id: NodeId| get(id);
        match self {
            Op::RingRail(p) => ops::rail::eval(p),
            Op::Shank(p) => ops::shank::eval(p, &*one(p.rail)?),
            Op::Gem(p) => match &p.placement {
                ops::gems::Placement::OnRail { rail, .. } => ops::gems::eval(p, Some(&*one(*rail)?)),
                _ => ops::gems::eval(p, None),
            },
            Op::GemsOnRail(p) => ops::gems::eval_on_rail(p, &*one(p.rail)?),
            Op::Head(p) => ops::head::eval(p, &*one(p.gem)?),
            Op::Bezel(p) => ops::bezel::eval(p, &*one(p.gem)?),
            Op::Halo(p) => ops::halo::eval(p, &*one(p.gem)?),
            Op::Prongs(p) => ops::prongs::eval(p, &*one(p.gems)?),
            Op::GemCutter(p) => ops::cutters::eval_gem_cutter(p, &*one(p.gems)?),
            Op::Scoop(p) => ops::cutters::eval_scoop(p, &*one(p.shank)?),
            Op::Boolean(p) => {
                let t: Vec<Arc<Output>> = p.targets.iter().map(|i| one(*i)).collect::<Result<_, _>>()?;
                let k: Vec<Arc<Output>> = p.tools.iter().map(|i| one(*i)).collect::<Result<_, _>>()?;
                let tr: Vec<&Output> = t.iter().map(|x| x.as_ref()).collect();
                let kr: Vec<&Output> = k.iter().map(|x| x.as_ref()).collect();
                if preview {
                    ops::boolean::preview(&tr, &kr)
                } else {
                    ops::boolean::eval(p, &tr, &kr)
                }
            }
            Op::PolarArray(p) => ops::transform::eval_polar(p, &*one(p.source)?),
            Op::Mirror(p) => ops::transform::eval_mirror(p, &*one(p.source)?),
            Op::TextOnRail(p) => ops::text::eval_on_rail(p, &*one(p.rail)?),
            Op::Text3d(p) => ops::text::eval_3d(p),
            Op::Signet(p) => ops::signet::eval(p, &*one(p.rail)?),
            Op::ChannelCutter(p) => ops::cutters::eval_channel(p, &*one(p.gems)?),
            Op::ScallopCutter(p) => ops::cutters::eval_scallop(p, &*one(p.gem)?),
            Op::Azure(p) => ops::cutters::eval_azure(p, &*one(p.shank)?),
            Op::PearlCup(p) => ops::extras::eval_pearl_cup(p, &*one(p.gem)?),
            Op::Bail(p) => ops::extras::eval_bail(p),
            Op::SplitShank(p) => ops::extras::eval_split(p, &*one(p.rail)?),
            Op::Rope(p) => ops::extras::eval_rope(p, &*one(p.rail)?),
            Op::Deform(p) => ops::extras::eval_deform(p, &*one(p.source)?),
            Op::GemsOnSurface(p) => ops::extras::eval_on_surface(p, &*one(p.target)?),
            Op::Import(p) => ops::extras::eval_import(p),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    #[serde(default = "yes")]
    pub visible: bool,
    pub op: Op,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectMeta {
    pub title: String,
    pub customer: String,
    pub designer: String,
    pub job_number: String,
    pub status: String,
    pub cad_minutes: u32,
    pub notes: String,
}

pub const FORMAT: &str = "openjewellerybhy";
pub const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub meta: ProjectMeta,
    pub alloy: Alloy,
    pub nodes: Vec<Node>,
}

impl Default for Document {
    fn default() -> Self {
        Document { format: FORMAT.into(), version: VERSION, meta: Default::default(), alloy: Alloy::Gold18kYellow, nodes: vec![] }
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_id(&self) -> NodeId {
        NodeId(self.nodes.iter().map(|n| n.id.0 + 1).max().unwrap_or(1))
    }

    pub fn add(&mut self, name: impl Into<String>, op: Op) -> NodeId {
        let id = self.next_id();
        self.nodes.push(Node { id, name: name.into(), visible: true, op });
        id
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Nodes that (transitively) depend on `id`.
    pub fn dependents(&self, id: NodeId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = Vec::new();
        let mut frontier = vec![id];
        while let Some(x) = frontier.pop() {
            for n in &self.nodes {
                if n.op.inputs().contains(&x) && !out.contains(&n.id) {
                    out.push(n.id);
                    frontier.push(n.id);
                }
            }
        }
        out
    }

    /// Removes a node and everything built on top of it.
    pub fn remove_with_dependents(&mut self, id: NodeId) -> Vec<NodeId> {
        let mut gone = self.dependents(id);
        gone.push(id);
        self.nodes.retain(|n| !gone.contains(&n.id));
        gone
    }

    /// Duplicates a node (with its parameters) right after the original.
    pub fn duplicate(&mut self, id: NodeId) -> Option<NodeId> {
        let n = self.get(id)?.clone();
        let new_id = self.next_id();
        let pos = self.nodes.iter().position(|x| x.id == id)? + 1;
        self.nodes.insert(pos, Node { id: new_id, name: format!("{} copy", n.name), visible: true, op: n.op });
        Some(new_id)
    }

    /// Ids consumed by booleans / arrays (not drawn on their own).
    pub fn consumed(&self) -> HashSet<NodeId> {
        self.nodes.iter().flat_map(|n| n.op.consumes()).collect()
    }

    /// Nodes in dependency order. Errors on cycles / dangling references.
    pub fn topo_order(&self) -> Result<Vec<Vec<NodeId>>, String> {
        let ids: HashSet<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        let mut level: HashMap<NodeId, usize> = HashMap::new();
        let mut remaining: Vec<&Node> = self.nodes.iter().collect();
        while !remaining.is_empty() {
            let before = remaining.len();
            remaining.retain(|n| {
                let ins = n.op.inputs();
                if ins.iter().any(|i| !ids.contains(i)) {
                    // dangling: evaluate at level 0, it will report the error
                    level.insert(n.id, 0);
                    return false;
                }
                if ins.iter().all(|i| level.contains_key(i)) {
                    let l = ins.iter().map(|i| level[i] + 1).max().unwrap_or(0);
                    level.insert(n.id, l);
                    false
                } else {
                    true
                }
            });
            if remaining.len() == before {
                return Err(format!("dependency cycle involving {}", remaining[0].id));
            }
        }
        let depth = level.values().copied().max().unwrap_or(0);
        let mut levels = vec![Vec::new(); depth + 1];
        for n in &self.nodes {
            levels[level[&n.id]].push(n.id);
        }
        Ok(levels)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("document serialises")
    }

    pub fn from_json(s: &str) -> Result<Document, String> {
        let d: Document = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if d.format != FORMAT {
            return Err(format!("not an {FORMAT} file"));
        }
        if d.version > VERSION {
            return Err(format!("file version {} is newer than supported {}", d.version, VERSION));
        }
        Ok(d)
    }
}

pub struct EvalResult {
    pub outputs: HashMap<NodeId, Result<Arc<Output>, String>>,
    pub consumed: HashSet<NodeId>,
    pub reevaluated: usize,
    pub millis: f64,
    pub error: Option<String>,
}

impl EvalResult {
    /// Outputs that make up the final model (visible, not absorbed, valid),
    /// in document order.
    pub fn final_outputs<'a>(&'a self, doc: &'a Document) -> impl Iterator<Item = (&'a Node, &'a Arc<Output>)> + 'a {
        doc.nodes.iter().filter_map(move |n| {
            if !n.visible || self.consumed.contains(&n.id) {
                return None;
            }
            match self.outputs.get(&n.id) {
                Some(Ok(o)) => Some((n, o)),
                _ => None,
            }
        })
    }

    pub fn errors<'a>(&'a self, doc: &'a Document) -> Vec<(&'a Node, &'a str)> {
        doc.nodes
            .iter()
            .filter_map(|n| match self.outputs.get(&n.id) {
                Some(Err(e)) => Some((n, e.as_str())),
                _ => None,
            })
            .collect()
    }
}

/// Incremental, parallel evaluator.
#[derive(Default)]
pub struct Evaluator {
    cache: HashMap<NodeId, (u64, Result<Arc<Output>, String>)>,
    /// Interactive mode: booleans are replaced by a cheap overlay of their
    /// inputs (used while a parameter is being dragged).
    pub preview: bool,
}

struct Fnv(u64);
impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

impl Evaluator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }

    pub fn evaluate(&mut self, doc: &Document) -> EvalResult {
        let t0 = Instant::now();
        let levels = match doc.topo_order() {
            Ok(l) => l,
            Err(e) => {
                return EvalResult {
                    outputs: HashMap::new(),
                    consumed: HashSet::new(),
                    reevaluated: 0,
                    millis: 0.0,
                    error: Some(e),
                }
            }
        };
        let mut hashes: HashMap<NodeId, u64> = HashMap::new();
        let mut results: HashMap<NodeId, Result<Arc<Output>, String>> = HashMap::new();
        let mut reevaluated = 0;
        for level in &levels {
            // compute hashes (cheap, sequential)
            let jobs: Vec<(NodeId, u64, &Op)> = level
                .iter()
                .map(|id| {
                    let n = doc.get(*id).unwrap();
                    let mut h = Fnv(0xcbf29ce484222325);
                    h.write(serde_json::to_string(&n.op).unwrap_or_default().as_bytes());
                    for i in n.op.inputs() {
                        h.write_u64(hashes.get(&i).copied().unwrap_or(0));
                    }
                    if self.preview && matches!(n.op, Op::Boolean(_)) {
                        h.write_u8(1);
                    }
                    (*id, h.finish(), &n.op)
                })
                .collect();
            let todo: Vec<&(NodeId, u64, &Op)> =
                jobs.iter().filter(|(id, h, _)| self.cache.get(id).map(|c| c.0) != Some(*h)).collect();
            reevaluated += todo.len();
            let preview = self.preview;
            let fresh: Vec<(NodeId, u64, Result<Arc<Output>, String>)> = todo
                .par_iter()
                .map(|(id, h, op)| {
                    let get = |i: NodeId| -> Result<Arc<Output>, String> {
                        match results.get(&i) {
                            Some(Ok(o)) => Ok(o.clone()),
                            Some(Err(_)) => Err(format!("input {i} has an error")),
                            None => Err(format!("missing input {i}")),
                        }
                    };
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| op.eval(&get, preview)))
                        .unwrap_or_else(|_| Err("builder panicked".into()))
                        .map(Arc::new);
                    (*id, *h, r)
                })
                .collect();
            for (id, h, r) in fresh {
                self.cache.insert(id, (h, r));
            }
            for (id, h, _) in &jobs {
                hashes.insert(*id, *h);
                results.insert(*id, self.cache[id].1.clone());
            }
        }
        // forget deleted nodes
        self.cache.retain(|id, _| results.contains_key(id));
        EvalResult {
            outputs: results,
            consumed: doc.consumed(),
            reevaluated,
            millis: t0.elapsed().as_secs_f64() * 1000.0,
            error: None,
        }
    }
}
