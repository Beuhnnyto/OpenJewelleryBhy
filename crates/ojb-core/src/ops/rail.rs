//! Ring rail (gvRingRail equivalent): the finger circle.

use super::{OpResult, Output, Rail};
use crate::sizes::FingerSize;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RingRailParams {
    pub size: FingerSize,
}

impl Default for RingRailParams {
    fn default() -> Self {
        RingRailParams { size: FingerSize::us(7.0) }
    }
}

pub fn eval(p: &RingRailParams) -> OpResult {
    let d = p.size.diameter();
    if !(5.0..=40.0).contains(&d) {
        return Err(format!("finger diameter {d:.2} mm out of range"));
    }
    let rail = Rail { radius: d / 2.0 };
    Ok(Output { rail: Some(rail), curves: vec![rail.polyline(128)], ..Default::default() })
}
