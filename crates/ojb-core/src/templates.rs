//! Ready-made parametric designs (MatrixGold-style "collections").

use crate::document::{Document, NodeId, Op};
use crate::gem::{Cut, GemSpec};
use crate::materials::Alloy;
use crate::ops::boolean::{BoolKind, BooleanParams};
use crate::ops::cutters::{GemCutterParams, ScoopParams};
use crate::ops::gems::{GemParams, GemsOnRailParams, Placement, Spacing};
use crate::ops::halo::HaloParams;
use crate::ops::head::HeadParams;
use crate::ops::prongs::ProngsParams;
use crate::ops::rail::RingRailParams;
use crate::ops::shank::{ShankParams, Station};
use crate::outline::Shape;
use crate::profile::Profile;
use crate::sizes::FingerSize;

#[derive(Clone, Debug)]
pub struct TemplateOptions {
    pub size: FingerSize,
    pub alloy: Alloy,
    pub shape: Shape,
    /// Centre gem width (mm); length follows the shape's default ratio.
    pub gem_width: f64,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        TemplateOptions { size: FingerSize::us(7.0), alloy: Alloy::Gold18kYellow, shape: Shape::Round, gem_width: 6.5 }
    }
}

pub const NAMES: [(&str, &str); 11] = [
    ("solitaire", "Four/six-prong solitaire with gallery rails, seat cutter and scoop"),
    ("halo", "Solitaire with a halo of melee, shared prongs and halo frame"),
    ("pave", "Pavé band: tapered melee on both shoulders with shared prongs"),
    ("eternity", "Full eternity band"),
    ("bezel", "Bezel-set solitaire"),
    ("three-stone", "Three-stone ring with heads on each stone"),
    ("signet", "Signet ring with engraved monogram, border and inside engraving"),
    ("pendant", "Pear halo pendant with bail and jump ring"),
    ("split-shank", "Split-shank solitaire"),
    ("rope", "Three-strand rope band"),
    ("pearl-stud", "Pearl stud earring (cup, peg and post)"),
];

pub fn build(name: &str, o: &TemplateOptions) -> Option<Document> {
    let d = match name {
        "solitaire" => solitaire(o, false),
        "halo" => solitaire(o, true),
        "pave" => pave(o, false),
        "eternity" => pave(o, true),
        "bezel" => bezel(o),
        "three-stone" => three_stone(o),
        "signet" => signet(o),
        "pendant" => pendant(o),
        "split-shank" => split_shank(o),
        "rope" => rope(o),
        "pearl-stud" => pearl_stud(o),
        _ => return None,
    };
    Some(d)
}

fn base(o: &TemplateOptions, title: &str, stations: Vec<Station>) -> (Document, NodeId, NodeId) {
    let mut d = Document::new();
    d.alloy = o.alloy;
    d.meta.title = title.into();
    d.meta.status = "Design".into();
    let rail = d.add("Ring rail", Op::RingRail(RingRailParams { size: o.size }));
    let shank = d.add("Shank", Op::Shank(ShankParams { rail, stations, ..Default::default() }));
    (d, rail, shank)
}

fn centre_spec(o: &TemplateOptions) -> GemSpec {
    GemSpec::new(o.shape.clone(), o.gem_width, o.gem_width * o.shape.default_ratio())
}

fn solitaire(o: &TemplateOptions, halo: bool) -> Document {
    let stations = vec![
        Station { pos: 0.0, width: 2.0, thickness: 1.7, profile: Profile::Comfort },
        Station { pos: 0.2, width: 2.1, thickness: 1.6, profile: Profile::Comfort },
        Station { pos: 0.5, width: 2.4, thickness: 1.5, profile: Profile::Comfort },
    ];
    let (mut d, _rail, shank) = base(o, if halo { "Halo engagement ring" } else { "Solitaire" }, stations);
    let spec = centre_spec(o);
    let h = spec.heights();
    let lift: f64 = if halo { 3.0 } else { 2.4 };
    let girdle = lift.max(-h.culet * 0.85);
    let gem = d.add(
        "Centre gem",
        Op::Gem(GemParams { spec: spec.clone(), placement: Placement::OnRail { rail: shank, t: 0.0, height: girdle, spin: 0.0, tilt: 0.0 } }),
    );
    let prongs = if matches!(o.shape, Shape::Round | Shape::Cushion) && !halo { 6 } else { 4 };
    let head = d.add(
        "Head",
        Op::Head(HeadParams {
            gem,
            prongs,
            depth: girdle + 0.5,
            tip_diameter: (spec.min_dim() * 0.15).clamp(0.8, 1.3),
            base_diameter: (spec.min_dim() * 0.18).clamp(0.9, 1.5),
            above_girdle: h.table - h.girdle_half + 0.1,
            ..Default::default()
        }),
    );
    let cutter = d.add("Centre seat", Op::GemCutter(GemCutterParams { gems: gem, ..Default::default() }));
    let scoop = d.add("Scoop", Op::Scoop(ScoopParams { shank, ..Default::default() }));
    let mut targets = vec![shank, head];
    let mut tools = vec![cutter, scoop];
    if halo {
        let halo = d.add("Halo", Op::Halo(HaloParams { gem, ..Default::default() }));
        let hc = d.add("Halo seats", Op::GemCutter(GemCutterParams { gems: halo, top_length: 0.8, bottom_length: 1.5, ..Default::default() }));
        targets.push(halo);
        tools.push(hc);
    }
    d.add("Final metal", Op::Boolean(BooleanParams { kind: BoolKind::Difference, targets, tools }));
    d
}

fn pave(o: &TemplateOptions, eternity: bool) -> Document {
    let (w, t) = if eternity { (2.6, 1.9) } else { (2.3, 1.8) };
    let stations = vec![
        Station { pos: 0.0, width: w, thickness: t, profile: Profile::Flat },
        Station { pos: 0.5, width: w + if eternity { 0.0 } else { 0.4 }, thickness: t - if eternity { 0.0 } else { 0.25 }, profile: if eternity { Profile::Flat } else { Profile::Comfort } },
    ];
    let (mut d, _rail, shank) = base(o, if eternity { "Eternity band" } else { "Pavé band" }, stations);
    let size = if eternity { 2.0 } else { 1.6 };
    let mut spec = GemSpec::round(size);
    spec.cut = Cut::Diamond;
    let gems = d.add(
        "Melee",
        Op::GemsOnRail(GemsOnRailParams {
            rail: shank,
            spec,
            start: 0.0,
            end: if eternity { 1.0 } else { 0.21 },
            size_start: size,
            size_end: if eternity { size } else { 1.1 },
            spacing: 0.12,
            method: Spacing::Minimum,
            height: -0.15,
            mirror: !eternity,
            full_circle: eternity,
            ..Default::default()
        }),
    );
    let prongs = d.add("Prongs", Op::Prongs(ProngsParams { gems, above_girdle_pct: 35.0, below_girdle: 0.8, ..Default::default() }));
    let cut = d.add("Seats", Op::GemCutter(GemCutterParams { gems, top_length: 1.0, bottom_length: 2.0, ..Default::default() }));
    d.add(
        "Final metal",
        Op::Boolean(BooleanParams { kind: BoolKind::Difference, targets: vec![shank, prongs], tools: vec![cut] }),
    );
    d
}

fn bezel(o: &TemplateOptions) -> Document {
    let stations = vec![
        Station { pos: 0.0, width: 2.4, thickness: 1.8, profile: Profile::HalfRound },
        Station { pos: 0.5, width: 2.6, thickness: 1.6, profile: Profile::HalfRound },
    ];
    let (mut d, _rail, shank) = base(o, "Bezel solitaire", stations);
    let spec = centre_spec(o);
    let h = spec.heights();
    let lift = -h.culet + 0.4;
    let gem = d.add(
        "Centre gem",
        Op::Gem(GemParams { spec, placement: Placement::OnRail { rail: shank, t: 0.0, height: lift - 0.6, spin: 0.0, tilt: 0.0 } }),
    );
    let bezel = d.add(
        "Bezel",
        Op::Bezel(crate::ops::bezel::BezelParams { gem, height: lift + 0.5, ..Default::default() }),
    );
    let cut = d.add("Seat", Op::GemCutter(GemCutterParams { gems: gem, top_length: 3.0, ..Default::default() }));
    d.add("Final metal", Op::Boolean(BooleanParams { kind: BoolKind::Difference, targets: vec![shank, bezel], tools: vec![cut] }));
    d
}

fn three_stone(o: &TemplateOptions) -> Document {
    let stations = vec![
        Station { pos: 0.0, width: 2.2, thickness: 1.7, profile: Profile::Comfort },
        Station { pos: 0.5, width: 2.4, thickness: 1.5, profile: Profile::Comfort },
    ];
    let (mut d, _rail, shank) = base(o, "Three-stone", stations);
    let main = centre_spec(o);
    let mut side = main.clone();
    side.width *= 0.7;
    side.length *= 0.7;
    let mut targets = vec![shank];
    let mut tools = vec![];
    let r = o.size.diameter() / 2.0 + 2.0;
    let dt = (main.width / 2.0 + side.width / 2.0 + 0.35) / (std::f64::consts::TAU * r);
    for (name, spec, t) in [("Centre gem", main.clone(), 0.0), ("Side gem R", side.clone(), dt), ("Side gem L", side.clone(), 1.0 - dt)] {
        let h = spec.heights();
        let girdle = (-h.culet * 0.85).max(2.0) - if t == 0.0 { 0.0 } else { 0.5 };
        let gem = d.add(name, Op::Gem(GemParams { spec: spec.clone(), placement: Placement::OnRail { rail: shank, t, height: girdle, spin: 0.0, tilt: 0.0 } }));
        let head = d.add(
            format!("Head {name}"),
            Op::Head(HeadParams {
                gem,
                prongs: 4,
                depth: girdle + 0.5,
                tip_diameter: (spec.min_dim() * 0.15).clamp(0.7, 1.2),
                base_diameter: (spec.min_dim() * 0.17).clamp(0.8, 1.4),
                above_girdle: h.table - h.girdle_half + 0.1,
                rails: vec![crate::ops::head::GalleryRail { at: 0.55, wire: 0.7 }],
                ..Default::default()
            }),
        );
        let cut = d.add(format!("Seat {name}"), Op::GemCutter(GemCutterParams { gems: gem, ..Default::default() }));
        targets.push(head);
        tools.push(cut);
    }
    d.add("Final metal", Op::Boolean(BooleanParams { kind: BoolKind::Difference, targets, tools }));
    d
}

fn signet(o: &TemplateOptions) -> Document {
    use crate::ops::text::{RingSurface, TextMode, TextOnRailParams};
    let mut d = Document::new();
    d.alloy = o.alloy;
    d.meta.title = "Signet ring".into();
    let rail = d.add("Ring rail", Op::RingRail(RingRailParams { size: o.size }));
    let signet = d.add("Signet", Op::Signet(crate::ops::signet::SignetParams { rail, ..Default::default() }));
    let inside = d.add(
        "Inside engraving",
        Op::TextOnRail(TextOnRailParams { rail, text: "Forever".into(), surface: RingSurface::Inside, mode: TextMode::Engrave, ..Default::default() }),
    );
    d.add("Final metal", Op::Boolean(BooleanParams { kind: BoolKind::Difference, targets: vec![signet], tools: vec![inside] }));
    d
}

fn pendant(o: &TemplateOptions) -> Document {
    let mut d = Document::new();
    d.alloy = o.alloy;
    d.meta.title = "Halo pendant".into();
    let shape = if o.shape == Shape::Round { Shape::Pear } else { o.shape.clone() };
    let spec = GemSpec::new(shape.clone(), o.gem_width.min(8.0), o.gem_width.min(8.0) * shape.default_ratio());
    let h = spec.heights();
    let gem = d.add("Centre gem", Op::Gem(GemParams { spec: spec.clone(), placement: Placement::Free { position: [0.0; 3], rotation: [0.0; 3] } }));
    let head = d.add(
        "Head",
        Op::Head(HeadParams {
            gem,
            prongs: if shape == Shape::Pear { 3 } else { 4 },
            depth: -h.culet + 0.6,
            base_scale: 0.75,
            above_girdle: h.table - h.girdle_half + 0.1,
            tip_diameter: 0.9,
            base_diameter: 0.9,
            rails: vec![crate::ops::head::GalleryRail { at: 0.15, wire: 0.8 }],
            ..Default::default()
        }),
    );
    let halo = d.add("Halo", Op::Halo(HaloParams { gem, stone: 1.2, z_offset: -0.3, ..Default::default() }));
    let top = spec.length / 2.0 + 0.25 + 1.2 + 0.4;
    let bail = d.add(
        "Bail",
        Op::Bail(crate::ops::extras::BailParams { position: [0.0, top, -0.6], rotation: [-90.0, 0.0, 0.0], ..Default::default() }),
    );
    let cut = d.add("Centre seat", Op::GemCutter(GemCutterParams { gems: gem, bottom_length: 2.0, ..Default::default() }));
    let hc = d.add("Halo seats", Op::GemCutter(GemCutterParams { gems: halo, top_length: 0.8, bottom_length: 1.5, ..Default::default() }));
    d.add("Final metal", Op::Boolean(BooleanParams { kind: BoolKind::Difference, targets: vec![head, halo, bail], tools: vec![cut, hc] }));
    d
}

fn split_shank(o: &TemplateOptions) -> Document {
    let mut d = Document::new();
    d.alloy = o.alloy;
    d.meta.title = "Split-shank solitaire".into();
    let rail = d.add("Ring rail", Op::RingRail(RingRailParams { size: o.size }));
    let shank = d.add("Split shank", Op::SplitShank(crate::ops::extras::SplitShankParams { rail, ..Default::default() }));
    let spec = centre_spec(o);
    let h = spec.heights();
    let girdle = (-h.culet * 0.85).max(2.4);
    let gem = d.add("Centre gem", Op::Gem(GemParams { spec: spec.clone(), placement: Placement::OnRail { rail: shank, t: 0.0, height: girdle, spin: 0.0, tilt: 0.0 } }));
    let head = d.add(
        "Head",
        Op::Head(HeadParams { gem, prongs: 4, depth: girdle + 0.5, above_girdle: h.table - h.girdle_half + 0.1, ..Default::default() }),
    );
    let cut = d.add("Centre seat", Op::GemCutter(GemCutterParams { gems: gem, ..Default::default() }));
    d.add("Final metal", Op::Boolean(BooleanParams { kind: BoolKind::Difference, targets: vec![shank, head], tools: vec![cut] }));
    d
}

fn rope(o: &TemplateOptions) -> Document {
    let mut d = Document::new();
    d.alloy = o.alloy;
    d.meta.title = "Rope band".into();
    let rail = d.add("Ring rail", Op::RingRail(RingRailParams { size: o.size }));
    d.add("Rope", Op::Rope(crate::ops::extras::RopeParams { rail, ..Default::default() }));
    d
}

fn pearl_stud(o: &TemplateOptions) -> Document {
    let mut d = Document::new();
    d.alloy = o.alloy;
    d.meta.title = "Pearl stud".into();
    let mut spec = GemSpec::round(o.gem_width.clamp(5.0, 10.0) + 1.5);
    spec.cut = Cut::Pearl;
    spec.material = crate::gem::GemMaterial::Pearl;
    let gem = d.add("Pearl", Op::Gem(GemParams { spec, placement: Placement::Free { position: [0.0; 3], rotation: [0.0; 3] } }));
    d.add("Pearl cup", Op::PearlCup(crate::ops::extras::PearlCupParams { gem, ..Default::default() }));
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Evaluator;

    #[test]
    fn every_template_evaluates() {
        for (name, _) in NAMES {
            let d = build(name, &TemplateOptions::default()).unwrap();
            let mut ev = Evaluator::new();
            let r = ev.evaluate(&d);
            let errs = r.errors(&d);
            assert!(errs.is_empty(), "{name}: {:?}", errs);
            let solids: usize = r.final_outputs(&d).map(|(_, o)| o.solids.len()).sum();
            assert!(solids > 0, "{name} produced no metal");
            // incremental: nothing to redo
            let r2 = ev.evaluate(&d);
            assert_eq!(r2.reevaluated, 0);
        }
    }
}
