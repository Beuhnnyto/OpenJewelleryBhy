//! Boolean (gvBoolean): union / difference / intersection of node outputs.
//! Metal solids of the targets form A, every solid of the tools forms B.
//! Gems of the inputs pass through unchanged.

use super::{OpResult, Output, Role, Solid};
use crate::csg::{boolean, BoolOp};
use crate::document::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoolKind {
    Union,
    Difference,
    Intersection,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BooleanParams {
    pub kind: BoolKind,
    pub targets: Vec<NodeId>,
    pub tools: Vec<NodeId>,
}

impl Default for BooleanParams {
    fn default() -> Self {
        BooleanParams { kind: BoolKind::Difference, targets: vec![], tools: vec![] }
    }
}

pub fn eval(p: &BooleanParams, targets: &[&Output], tools: &[&Output]) -> OpResult {
    let a: Vec<&crate::mesh::Mesh> = targets
        .iter()
        .flat_map(|o| o.solids.iter())
        .filter(|s| s.role == Role::Metal)
        .map(|s| s.mesh.as_ref())
        .collect();
    let b: Vec<&crate::mesh::Mesh> = tools.iter().flat_map(|o| o.solids.iter()).map(|s| s.mesh.as_ref()).collect();
    if a.is_empty() {
        return Err("targets contain no metal".into());
    }
    let op = match p.kind {
        BoolKind::Union => BoolOp::Union,
        BoolKind::Difference => BoolOp::Difference,
        BoolKind::Intersection => BoolOp::Intersection,
    };
    let mesh = boolean(&a, &b, op);
    let mut out = Output::default();
    for o in targets.iter().chain(tools.iter()) {
        let mut pass = (*o).clone();
        pass.solids.clear();
        out.extend(&pass);
    }
    out.solids.push(Solid::metal(mesh));
    Ok(out)
}

/// Cheap stand-in used while dragging: metal of the targets plus the tools
/// shown as cutters, no CSG.
pub fn preview(targets: &[&Output], tools: &[&Output]) -> OpResult {
    let mut out = Output::default();
    for o in targets {
        out.extend(o);
    }
    out.solids.retain(|s| s.role == Role::Metal);
    for o in tools {
        let mut t = (*o).clone();
        for s in &mut t.solids {
            s.role = Role::Cutter;
        }
        out.extend(&t);
    }
    Ok(out)
}
