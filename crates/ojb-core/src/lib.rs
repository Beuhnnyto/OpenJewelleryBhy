//! OpenJewelleryBhy core: a parametric jewellery CAD kernel.
//!
//! Conventions (same as the MatrixGold training guide): millimetres, +Z up,
//! the ring rail lies in the XZ plane centred on the origin, the finger runs
//! along Y, and a gem's girdle is centred on its local origin with the table
//! towards +Z (length along local Y, width along local X).

pub mod csg;
pub mod document;
pub mod gem;
pub mod io;
pub mod materials;
pub mod mesh;
pub mod ops;
pub mod outline;
pub mod profile;
pub mod render;
pub mod report;
pub mod sizes;
pub mod templates;
pub mod text;
pub mod trace;

pub use document::{Document, EvalResult, Evaluator, Node, NodeId, Op};
pub use mesh::Mesh;

pub use glam::{DAffine3, DMat3, DQuat, DVec2, DVec3};
