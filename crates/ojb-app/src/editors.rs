//! "Dynamic Command window": parameter editors for every builder.

use eframe::egui::{self, ComboBox, DragValue, Ui};
use ojb_core::document::{NodeId, Op};
use ojb_core::gem::{Cut, GemMaterial, GemSpec};
use ojb_core::ops::boolean::BoolKind;
use ojb_core::ops::gems::{Placement, Spacing};
use ojb_core::ops::head::GalleryRail;
use ojb_core::ops::prongs::ProngLayout;
use ojb_core::ops::shank::Station;
use ojb_core::ops::transform::Axis;
use ojb_core::outline::Shape;
use ojb_core::profile::Profile;
use ojb_core::sizes::Region;
use ojb_core::ops::extras::{AutoFill, DeformKind, SplitMode};
use ojb_core::ops::text::{RingSurface, TextMode};
use ojb_core::ops::Role;
use ojb_core::text::Font;

/// A node that may be referenced, with what it provides.
pub struct Candidate {
    pub id: NodeId,
    pub label: String,
    pub has_rail: bool,
    pub has_gems: bool,
    pub has_shank: bool,
    pub has_solids: bool,
}

fn num(ui: &mut Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, speed: f64, suffix: &str) -> bool {
    ui.label(label);
    let r = ui.add(DragValue::new(v).range(range).speed(speed).suffix(suffix).max_decimals(3));
    ui.end_row();
    r.changed()
}

fn slider(ui: &mut Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str) -> bool {
    ui.label(label);
    let r = ui.add(egui::Slider::new(v, range).suffix(suffix).max_decimals(3));
    ui.end_row();
    r.changed()
}

fn count(ui: &mut Ui, label: &str, v: &mut usize, range: std::ops::RangeInclusive<usize>) -> bool {
    ui.label(label);
    let r = ui.add(DragValue::new(v).range(range));
    ui.end_row();
    r.changed()
}

fn check(ui: &mut Ui, label: &str, v: &mut bool) -> bool {
    ui.label(label);
    let r = ui.checkbox(v, "");
    ui.end_row();
    r.changed()
}

fn combo<T: PartialEq + Clone>(ui: &mut Ui, label: &str, id: &str, v: &mut T, items: &[(T, &str)]) -> bool {
    ui.label(label);
    let cur = items.iter().find(|(x, _)| x == v).map(|x| x.1).unwrap_or("?");
    let mut changed = false;
    ComboBox::from_id_salt(id).selected_text(cur).show_ui(ui, |ui| {
        for (x, name) in items {
            if ui.selectable_label(x == v, *name).clicked() && x != v {
                *v = x.clone();
                changed = true;
            }
        }
    });
    ui.end_row();
    changed
}

fn node_pick(ui: &mut Ui, label: &str, id: &str, v: &mut NodeId, cands: &[&Candidate]) -> bool {
    let items: Vec<(NodeId, &str)> = cands.iter().map(|c| (c.id, c.label.as_str())).collect();
    if items.is_empty() {
        ui.label(label);
        ui.colored_label(egui::Color32::from_rgb(200, 60, 40), "no suitable input");
        ui.end_row();
        return false;
    }
    combo(ui, label, id, v, &items)
}

fn multi_pick(ui: &mut Ui, label: &str, v: &mut Vec<NodeId>, cands: &[&Candidate]) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.vertical(|ui| {
        for c in cands {
            let mut on = v.contains(&c.id);
            if ui.checkbox(&mut on, &c.label).changed() {
                changed = true;
                if on {
                    v.push(c.id);
                } else {
                    v.retain(|x| *x != c.id);
                }
            }
        }
    });
    ui.end_row();
    changed
}

pub fn shapes() -> Vec<(Shape, &'static str)> {
    Shape::BUILTIN.iter().map(|s| (s.clone(), s.name())).collect()
}
pub fn profiles() -> Vec<(Profile, &'static str)> {
    Profile::ALL.iter().map(|p| (*p, p.name())).collect()
}
fn materials() -> Vec<(GemMaterial, &'static str)> {
    GemMaterial::ALL.iter().map(|m| (*m, m.name())).collect()
}
fn cuts() -> Vec<(Cut, &'static str)> {
    Cut::ALL.iter().map(|c| (*c, c.name())).collect()
}
pub fn regions() -> Vec<(Region, &'static str)> {
    Region::ALL.iter().map(|r| (*r, r.name())).collect()
}

fn grid(ui: &mut Ui, id: &str, f: impl FnOnce(&mut Ui) -> bool) -> bool {
    egui::Grid::new(id).num_columns(2).spacing([10.0, 6.0]).striped(true).show(ui, f).inner
}

pub fn gem_spec(ui: &mut Ui, id: &str, g: &mut GemSpec, sized: bool) -> bool {
    let mut c = false;
    c |= grid(ui, id, |ui| {
        let mut c = false;
        let before = g.shape.clone();
        if combo(ui, "Shape", &format!("{id}shape"), &mut g.shape, &shapes()) {
            // keep proportions sensible when switching shape
            g.length = g.width * g.shape.default_ratio();
            let fresh = GemSpec::new(g.shape.clone(), g.width, g.length);
            if before.prefers_step_cut() != g.shape.prefers_step_cut() {
                g.cut = fresh.cut;
                g.depth_pct = fresh.depth_pct;
                g.table_pct = fresh.table_pct;
                g.crown_pct = fresh.crown_pct;
            }
            c = true;
        }
        c |= combo(ui, "Cut", &format!("{id}cut"), &mut g.cut, &cuts());
        c |= combo(ui, "Material", &format!("{id}mat"), &mut g.material, &materials());
        if sized {
            c |= num(ui, "Width W (X)", &mut g.width, 0.5..=40.0, 0.01, " mm");
            if g.cut != Cut::Pearl {
                c |= num(ui, "Length L (Y)", &mut g.length, 0.5..=40.0, 0.01, " mm");
            }
        }
        if !matches!(g.cut, Cut::Pearl) {
            c |= slider(ui, "Depth", &mut g.depth_pct, 20.0..=110.0, " %");
            if g.cut != Cut::Cabochon {
                c |= slider(ui, "Table", &mut g.table_pct, 30.0..=85.0, " %");
                c |= slider(ui, "Crown", &mut g.crown_pct, 3.0..=30.0, " %");
            }
            c |= slider(ui, "Girdle", &mut g.girdle_pct, 0.5..=8.0, " %");
        }
        c
    });
    let formula = g.carat_formula().map(|f| format!(" · formula {f:.3} ct")).unwrap_or_default();
    ui.label(egui::RichText::new(format!("≈ {:.3} ct (model){formula}", g.carat_from_volume())).strong());
    c
}

/// Edits one op. Returns true when something changed.
pub fn edit(ui: &mut Ui, op: &mut Op, cands: &[Candidate]) -> bool {
    let rails: Vec<&Candidate> = cands.iter().filter(|c| c.has_rail).collect();
    let gems: Vec<&Candidate> = cands.iter().filter(|c| c.has_gems).collect();
    let shanks: Vec<&Candidate> = cands.iter().filter(|c| c.has_shank).collect();
    let solids: Vec<&Candidate> = cands.iter().filter(|c| c.has_solids).collect();
    let all: Vec<&Candidate> = cands.iter().collect();
    match op {
        Op::RingRail(p) => {
            let mut c = grid(ui, "rail", |ui| {
                let mut c = false;
                let old = p.size;
                if combo(ui, "Size system", "region", &mut p.size.region, &regions()) {
                    // keep the same finger, express it in the new system
                    p.size = old.convert(p.size.region);
                    c = true;
                }
                c |= num(ui, "Size", &mut p.size.value, -50.0..=200.0, 0.05, "");
                ui.label("Inner diameter");
                ui.label(format!("{:.2} mm  ({})", p.size.diameter(), p.size.display()));
                ui.end_row();
                c
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Quick:");
                for v in [4.0, 5.0, 6.0, 6.5, 7.0, 7.5, 8.0, 9.0, 10.0] {
                    if ui.small_button(format!("US {v}")).clicked() {
                        p.size = ojb_core::sizes::FingerSize::us(v).convert(p.size.region);
                        c = true;
                    }
                }
            });
            c
        }
        Op::Shank(p) => {
            let mut c = grid(ui, "shank", |ui| {
                let mut c = node_pick(ui, "Ring rail", "shank_rail", &mut p.rail, &rails);
                c |= check(ui, "Symmetric (mirror L/R)", &mut p.symmetric);
                c |= count(ui, "Rail samples", &mut p.samples, 32..=1024);
                c |= count(ui, "Profile points", &mut p.profile_points, 12..=96);
                c
            });
            ui.separator();
            ui.label(egui::RichText::new("Profile stations (Profile Placer)").strong());
            ui.small("Position: 0 = top (12 o'clock), 0.25 = side, 0.5 = bottom");
            let mut remove = None;
            for (i, s) in p.stations.iter_mut().enumerate() {
                egui::CollapsingHeader::new(format!("Station {} @ {:.2}", i + 1, s.pos)).id_salt(("st", i)).default_open(i < 3).show(ui, |ui| {
                    c |= grid(ui, &format!("st{i}"), |ui| {
                        let mut c = slider(ui, "Position", &mut s.pos, 0.0..=1.0, "");
                        c |= num(ui, "Width", &mut s.width, 0.3..=30.0, 0.01, " mm");
                        c |= num(ui, "Thickness", &mut s.thickness, 0.3..=15.0, 0.01, " mm");
                        c |= combo(ui, "Profile", &format!("stp{i}"), &mut s.profile, &profiles());
                        c
                    });
                    if ui.small_button("Remove station").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                if p.stations.len() > 1 {
                    p.stations.remove(i);
                    c = true;
                }
            }
            if ui.button("➕ Add station").clicked() {
                let last = p.stations.last().cloned().unwrap_or(Station { pos: 0.0, width: 2.0, thickness: 1.6, profile: Profile::Comfort });
                p.stations.push(Station { pos: (last.pos + 0.1).min(1.0), ..last });
                c = true;
            }
            c
        }
        Op::Gem(p) => {
            let mut c = gem_spec(ui, "gem", &mut p.spec, true);
            ui.separator();
            let on_rail = matches!(p.placement, Placement::OnRail { .. });
            let mut want = on_rail;
            ui.horizontal(|ui| {
                ui.radio_value(&mut want, false, "Free");
                ui.radio_value(&mut want, true, "On ring rail");
            });
            if want != on_rail {
                p.placement = if want {
                    Placement::OnRail { rail: rails.last().map(|r| r.id).unwrap_or_default(), t: 0.0, height: 2.0, spin: 0.0, tilt: 0.0 }
                } else {
                    Placement::Free { position: [0.0; 3], rotation: [0.0; 3] }
                };
                c = true;
            }
            c |= grid(ui, "placement", |ui| {
                let mut c = false;
                match &mut p.placement {
                    Placement::Free { position, rotation } => {
                        for (k, n) in ["X", "Y", "Z"].iter().enumerate() {
                            c |= num(ui, &format!("Position {n}"), &mut position[k], -100.0..=100.0, 0.02, " mm");
                        }
                        for (k, n) in ["X", "Y", "Z"].iter().enumerate() {
                            c |= num(ui, &format!("Rotation {n}"), &mut rotation[k], -360.0..=360.0, 0.5, "°");
                        }
                    }
                    Placement::OnRail { rail, t, height, spin, tilt } => {
                        c |= node_pick(ui, "Rail / shank", "gem_rail", rail, &rails);
                        c |= slider(ui, "Position on rail", t, 0.0..=1.0, "");
                        c |= num(ui, "Girdle height", height, -10.0..=20.0, 0.01, " mm");
                        c |= slider(ui, "Rotation", spin, -180.0..=180.0, "°");
                        c |= slider(ui, "Tilt", tilt, -60.0..=60.0, "°");
                    }
                }
                c
            });
            c
        }
        Op::GemsOnRail(p) => {
            let mut c = grid(ui, "gor", |ui| {
                let mut c = node_pick(ui, "Rail / shank", "gor_rail", &mut p.rail, &rails);
                c |= check(ui, "Full circle (eternity)", &mut p.full_circle);
                if !p.full_circle {
                    c |= slider(ui, "Start", &mut p.start, 0.0..=1.0, "");
                    c |= slider(ui, "End", &mut p.end, 0.0..=1.0, "");
                    c |= check(ui, "Mirror other side", &mut p.mirror);
                }
                c |= num(ui, "Start size", &mut p.size_start, 0.5..=10.0, 0.01, " mm");
                c |= num(ui, "End size (taper)", &mut p.size_end, 0.5..=10.0, 0.01, " mm");
                c |= num(ui, "Spacing", &mut p.spacing, 0.0..=5.0, 0.005, " mm");
                c |= combo(ui, "Spacing method", "gor_m", &mut p.method, &[(Spacing::Minimum, "Minimum"), (Spacing::Fixed, "Fixed")]);
                c |= num(ui, "Height vs surface", &mut p.height, -5.0..=5.0, 0.01, " mm");
                c |= count(ui, "Rows", &mut p.rows, 1..=7);
                if p.rows > 1 {
                    c |= num(ui, "Row spacing", &mut p.row_spacing, 0.0..=3.0, 0.01, " mm");
                }
                c
            });
            ui.separator();
            c |= gem_spec(ui, "gorspec", &mut p.spec, false);
            c
        }
        Op::Head(p) => {
            let mut c = grid(ui, "head", |ui| {
                let mut c = node_pick(ui, "Gem", "head_gem", &mut p.gem, &gems);
                c |= count(ui, "Gem index", &mut p.gem_index, 0..=999);
                c |= count(ui, "Prongs", &mut p.prongs, 2..=12);
                c |= slider(ui, "Layout rotation", &mut p.rotation, -180.0..=180.0, "°");
                c |= num(ui, "Tip diameter", &mut p.tip_diameter, 0.3..=4.0, 0.01, " mm");
                c |= num(ui, "Base diameter", &mut p.base_diameter, 0.3..=4.0, 0.01, " mm");
                c |= num(ui, "Height above girdle", &mut p.above_girdle, 0.0..=6.0, 0.01, " mm");
                c |= num(ui, "Depth below girdle", &mut p.depth, 0.5..=20.0, 0.01, " mm");
                c |= slider(ui, "Base footprint", &mut p.base_scale, 0.0..=1.5, "");
                c |= num(ui, "Prong bend", &mut p.bend, -3.0..=3.0, 0.01, " mm");
                c |= slider(ui, "Nudge", &mut p.nudge, 0.0..=1.0, "");
                c |= slider(ui, "Dome", &mut p.dome, 0.0..=1.5, "");
                c
            });
            ui.separator();
            ui.label(egui::RichText::new("Gallery rails").strong());
            let mut remove = None;
            for (i, r) in p.rails.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("#{}", i + 1));
                    c |= ui.add(egui::Slider::new(&mut r.at, 0.0..=1.0).text("height")).changed();
                    c |= ui.add(DragValue::new(&mut r.wire).range(0.2..=3.0).speed(0.01).suffix(" mm")).changed();
                    if ui.small_button("✖").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                p.rails.remove(i);
                c = true;
            }
            if ui.button("➕ Add gallery rail").clicked() {
                p.rails.push(GalleryRail { at: 0.4, wire: 0.8 });
                c = true;
            }
            c
        }
        Op::Bezel(p) => grid(ui, "bezel", |ui| {
            let mut c = node_pick(ui, "Gem", "bezel_gem", &mut p.gem, &gems);
            c |= count(ui, "Gem index", &mut p.gem_index, 0..=999);
            c |= num(ui, "Girdle offset", &mut p.girdle_offset, -0.5..=1.0, 0.005, " mm");
            c |= num(ui, "Top thickness", &mut p.top_thickness, 0.1..=3.0, 0.01, " mm");
            c |= num(ui, "Bottom thickness", &mut p.bottom_thickness, 0.1..=3.0, 0.01, " mm");
            c |= num(ui, "Top above girdle", &mut p.above_girdle, 0.0..=5.0, 0.01, " mm");
            c |= num(ui, "Height", &mut p.height, 0.5..=20.0, 0.01, " mm");
            c |= num(ui, "Seat", &mut p.seat, 0.0..=2.0, 0.01, " mm");
            c |= num(ui, "Dome", &mut p.dome, 0.0..=1.0, 0.01, " mm");
            c
        }),
        Op::Halo(p) => grid(ui, "halo", |ui| {
            let mut c = node_pick(ui, "Centre gem", "halo_gem", &mut p.gem, &gems);
            c |= count(ui, "Gem index", &mut p.gem_index, 0..=999);
            c |= num(ui, "Melee size", &mut p.stone, 0.5..=5.0, 0.01, " mm");
            c |= combo(ui, "Material", "halo_mat", &mut p.material, &materials());
            c |= num(ui, "Spacing from centre", &mut p.spacing_from_center, -0.5..=5.0, 0.01, " mm");
            c |= num(ui, "Gem spacing", &mut p.gem_spacing, 0.0..=2.0, 0.005, " mm");
            c |= num(ui, "Height offset", &mut p.z_offset, -10.0..=5.0, 0.01, " mm");
            c |= check(ui, "Metal frame", &mut p.metal);
            if p.metal {
                c |= num(ui, "Frame height", &mut p.metal_height, 0.2..=5.0, 0.01, " mm");
                c |= num(ui, "Frame wall", &mut p.wall, 0.0..=2.0, 0.01, " mm");
            }
            c |= check(ui, "Shared prongs", &mut p.prongs);
            if p.prongs {
                c |= num(ui, "Prong diameter", &mut p.prong_diameter, 0.2..=2.0, 0.01, " mm");
            }
            c
        }),
        Op::Prongs(p) => grid(ui, "prongs", |ui| {
            let mut c = node_pick(ui, "Gems", "pr_gems", &mut p.gems, &gems);
            c |= combo(
                ui,
                "Layout",
                "pr_lay",
                &mut p.layout,
                &[(ProngLayout::Shared, "Shared between gems"), (ProngLayout::TwoPerGem, "Two per gem"), (ProngLayout::FourPerGem, "Four per gem")],
            );
            c |= slider(ui, "Diameter (% of gem)", &mut p.diameter_pct, 5.0..=60.0, " %");
            c |= num(ui, "Minimum diameter", &mut p.min_diameter, 0.2..=2.0, 0.01, " mm");
            c |= slider(ui, "Tip above girdle", &mut p.above_girdle_pct, 0.0..=80.0, " %");
            c |= num(ui, "Depth below girdle", &mut p.below_girdle, 0.0..=5.0, 0.01, " mm");
            c |= slider(ui, "Nudge", &mut p.nudge, 0.0..=1.0, "");
            c |= slider(ui, "Dome", &mut p.dome, 0.0..=1.5, "");
            c
        }),
        Op::GemCutter(p) => grid(ui, "gc", |ui| {
            let mut c = node_pick(ui, "Gems", "gc_gems", &mut p.gems, &gems);
            c |= num(ui, "Top length", &mut p.top_length, 0.0..=10.0, 0.01, " mm");
            c |= num(ui, "Clearance", &mut p.clearance, 0.0..=0.5, 0.005, " mm");
            c |= slider(ui, "Seat depth", &mut p.seat_depth, 0.05..=0.95, "");
            c |= slider(ui, "Hole", &mut p.hole, 0.05..=0.95, "");
            c |= num(ui, "Bottom length", &mut p.bottom_length, 0.0..=15.0, 0.01, " mm");
            c
        }),
        Op::Scoop(p) => grid(ui, "scoop", |ui| {
            let mut c = node_pick(ui, "Shank", "sc_shank", &mut p.shank, &shanks);
            c |= slider(ui, "Start", &mut p.start, 0.0..=1.0, "");
            c |= slider(ui, "End", &mut p.end, 0.0..=1.0, "");
            c |= num(ui, "Wall", &mut p.wall, 0.2..=3.0, 0.01, " mm");
            c |= slider(ui, "Depth", &mut p.depth, 0.0..=1.0, "");
            c
        }),
        Op::Boolean(p) => grid(ui, "bool", |ui| {
            let mut c = combo(
                ui,
                "Operation",
                "bool_k",
                &mut p.kind,
                &[(BoolKind::Difference, "Difference"), (BoolKind::Union, "Union"), (BoolKind::Intersection, "Intersection")],
            );
            c |= multi_pick(ui, "Targets (metal)", &mut p.targets, &solids);
            c |= multi_pick(ui, "Tools", &mut p.tools, &solids);
            c
        }),
        Op::PolarArray(p) => grid(ui, "polar", |ui| {
            let mut c = node_pick(ui, "Source", "pa_src", &mut p.source, &all);
            c |= count(ui, "Copies", &mut p.count, 1..=512);
            c |= combo(ui, "Axis", "pa_axis", &mut p.axis, &[(Axis::X, "X"), (Axis::Y, "Y (finger)"), (Axis::Z, "Z")]);
            c |= num(ui, "Total angle", &mut p.angle, -360.0..=360.0, 0.5, "°");
            c |= num(ui, "Start angle", &mut p.start_angle, -360.0..=360.0, 0.5, "°");
            c
        }),
        Op::Mirror(p) => grid(ui, "mirror", |ui| {
            let mut c = node_pick(ui, "Source", "mi_src", &mut p.source, &all);
            c |= check(ui, "Across YZ (x → -x)", &mut p.x);
            c |= check(ui, "Across XZ (y → -y)", &mut p.y);
            c |= check(ui, "Across XY (z → -z)", &mut p.z);
            c
        }),
        Op::TextOnRail(p) => grid(ui, "tor", |ui| {
            let mut c = node_pick(ui, "Rail / shank", "tor_rail", &mut p.rail, &rails);
            c |= text(ui, "Text", &mut p.text);
            c |= combo(ui, "Font", "tor_font", &mut p.font, &fonts());
            c |= combo(ui, "Surface", "tor_surf", &mut p.surface, &[(RingSurface::Inside, "Inside"), (RingSurface::Outside, "Outside")]);
            c |= combo(ui, "Mode", "tor_mode", &mut p.mode, &[(TextMode::Engrave, "Engrave (cutter)"), (TextMode::Emboss, "Emboss (metal)")]);
            c |= num(ui, "Size (cap height)", &mut p.size, 0.3..=20.0, 0.01, " mm");
            c |= num(ui, "Depth", &mut p.depth, 0.05..=3.0, 0.01, " mm");
            c |= num(ui, "Letter spacing", &mut p.spacing, -1.0..=3.0, 0.01, " mm");
            c |= slider(ui, "Position on rail", &mut p.t, 0.0..=1.0, "");
            c |= num(ui, "Offset across band", &mut p.offset, -10.0..=10.0, 0.01, " mm");
            c |= check(ui, "Flip reading direction", &mut p.flip);
            c
        }),
        Op::Text3d(p) => grid(ui, "t3d", |ui| {
            let mut c = text(ui, "Text", &mut p.text);
            c |= combo(ui, "Font", "t3d_font", &mut p.font, &fonts());
            c |= combo(ui, "Mode", "t3d_mode", &mut p.mode, &[(TextMode::Emboss, "Metal"), (TextMode::Engrave, "Cutter")]);
            c |= num(ui, "Size (cap height)", &mut p.size, 0.3..=50.0, 0.01, " mm");
            c |= num(ui, "Depth", &mut p.depth, 0.05..=20.0, 0.01, " mm");
            c |= num(ui, "Letter spacing", &mut p.spacing, -2.0..=5.0, 0.01, " mm");
            c |= xyz(ui, "Position", &mut p.position, 0.02, " mm");
            c |= xyz(ui, "Rotation", &mut p.rotation, 0.5, "°");
            c
        }),
        Op::Signet(p) => grid(ui, "signet", |ui| {
            let mut c = node_pick(ui, "Ring rail", "sg_rail", &mut p.rail, &rails);
            c |= combo(ui, "Top shape", "sg_shape", &mut p.shape, &shapes());
            c |= num(ui, "Top width (X)", &mut p.top_width, 2.0..=30.0, 0.02, " mm");
            c |= num(ui, "Top length (Y)", &mut p.top_length, 2.0..=30.0, 0.02, " mm");
            c |= num(ui, "Top height", &mut p.height, 0.8..=10.0, 0.01, " mm");
            c |= num(ui, "Dome", &mut p.dome, 0.0..=2.0, 0.01, " mm");
            c |= num(ui, "Shank width", &mut p.shank_width, 1.0..=10.0, 0.01, " mm");
            c |= num(ui, "Shank thickness", &mut p.shank_thickness, 0.8..=5.0, 0.01, " mm");
            c |= num(ui, "Border inset (0 = none)", &mut p.border, 0.0..=3.0, 0.01, " mm");
            c |= text(ui, "Monogram", &mut p.text);
            c |= combo(ui, "Font", "sg_font", &mut p.font, &fonts());
            c |= combo(ui, "Text mode", "sg_mode", &mut p.text_mode, &[(TextMode::Engrave, "Engrave"), (TextMode::Emboss, "Emboss")]);
            c |= num(ui, "Text size", &mut p.text_size, 0.5..=20.0, 0.02, " mm");
            c |= num(ui, "Text depth", &mut p.text_depth, 0.05..=2.0, 0.01, " mm");
            c
        }),
        Op::ChannelCutter(p) => grid(ui, "chan", |ui| {
            let mut c = node_pick(ui, "Gem rows", "ch_gems", &mut p.gems, &gems);
            c |= slider(ui, "Width (% of girdle)", &mut p.width_pct, 50.0..=110.0, " %");
            c |= num(ui, "Height above table", &mut p.top, 0.0..=5.0, 0.01, " mm");
            c |= slider(ui, "Depth (% of pavilion)", &mut p.depth_pct, 0.0..=100.0, " %");
            c |= num(ui, "End extension", &mut p.end_extension, -2.0..=5.0, 0.01, " mm");
            c
        }),
        Op::ScallopCutter(p) => grid(ui, "scal", |ui| {
            let mut c = node_pick(ui, "Gem", "sc_gem", &mut p.gem, &gems);
            c |= count(ui, "Gem index", &mut p.gem_index, 0..=999);
            c |= count(ui, "Scallops", &mut p.count, 1..=64);
            c |= num(ui, "Radius", &mut p.radius, 0.1..=5.0, 0.01, " mm");
            c |= num(ui, "Height above girdle", &mut p.height, -3.0..=6.0, 0.01, " mm");
            c |= slider(ui, "Rotation", &mut p.rotation, -180.0..=180.0, "°");
            c
        }),
        Op::Azure(p) => grid(ui, "azure", |ui| {
            let mut c = node_pick(ui, "Shank", "az_shank", &mut p.shank, &shanks);
            c |= slider(ui, "Start", &mut p.start, 0.0..=0.5, "");
            c |= slider(ui, "End", &mut p.end, 0.0..=0.5, "");
            c |= num(ui, "Cell size", &mut p.size, 0.3..=4.0, 0.01, " mm");
            c |= num(ui, "Wall", &mut p.wall, 0.1..=2.0, 0.01, " mm");
            c |= num(ui, "Skin", &mut p.skin, 0.1..=2.0, 0.01, " mm");
            c
        }),
        Op::PearlCup(p) => grid(ui, "pcup", |ui| {
            let mut c = node_pick(ui, "Pearl", "pc_gem", &mut p.gem, &gems);
            c |= count(ui, "Gem index", &mut p.gem_index, 0..=999);
            c |= slider(ui, "Cup height", &mut p.cup_height, 5.0..=60.0, " %");
            c |= num(ui, "Thickness", &mut p.thickness, 0.2..=2.0, 0.01, " mm");
            c |= slider(ui, "Peg height", &mut p.peg_height, 0.0..=90.0, " %");
            c |= num(ui, "Peg diameter", &mut p.peg_diameter, 0.3..=3.0, 0.01, " mm");
            c |= check(ui, "Earring post", &mut p.post);
            if p.post {
                c |= num(ui, "Post length", &mut p.post_length, 2.0..=20.0, 0.1, " mm");
                c |= num(ui, "Post diameter", &mut p.post_diameter, 0.5..=1.5, 0.01, " mm");
            }
            c
        }),
        Op::Bail(p) => grid(ui, "bail", |ui| {
            let mut c = xyz(ui, "Position", &mut p.position, 0.02, " mm");
            c |= xyz(ui, "Rotation", &mut p.rotation, 0.5, "°");
            c |= num(ui, "Inner diameter", &mut p.inner_diameter, 1.0..=12.0, 0.01, " mm");
            c |= num(ui, "Width", &mut p.width, 0.5..=10.0, 0.01, " mm");
            c |= num(ui, "Thickness", &mut p.thickness, 0.3..=4.0, 0.01, " mm");
            c |= check(ui, "Jump ring", &mut p.jump_ring);
            if p.jump_ring {
                c |= num(ui, "Jump ring inner Ø", &mut p.jump_ring_inner, 1.0..=10.0, 0.01, " mm");
                c |= num(ui, "Jump ring wire", &mut p.jump_ring_wire, 0.3..=2.0, 0.01, " mm");
            }
            c
        }),
        Op::SplitShank(p) => grid(ui, "split", |ui| {
            let mut c = node_pick(ui, "Ring rail", "sp_rail", &mut p.rail, &rails);
            c |= combo(ui, "Style", "sp_mode", &mut p.mode, &[(SplitMode::Split, "Split shank"), (SplitMode::Bypass, "Bypass")]);
            c |= num(ui, "Strand width", &mut p.width, 0.5..=8.0, 0.01, " mm");
            c |= num(ui, "Strand thickness", &mut p.thickness, 0.5..=8.0, 0.01, " mm");
            c |= num(ui, "Bottom width", &mut p.bottom_width, 0.5..=10.0, 0.01, " mm");
            c |= num(ui, "Bottom thickness", &mut p.bottom_thickness, 0.5..=8.0, 0.01, " mm");
            c |= num(ui, "Gap at top", &mut p.gap, 0.0..=10.0, 0.01, " mm");
            c |= slider(ui, "Split span", &mut p.span, 0.02..=0.45, "");
            c |= combo(ui, "Profile", "sp_prof", &mut p.profile, &profiles());
            c
        }),
        Op::Rope(p) => grid(ui, "rope", |ui| {
            let mut c = node_pick(ui, "Rail / shank", "rp_rail", &mut p.rail, &rails);
            c |= count(ui, "Strands", &mut p.strands, 1..=12);
            c |= num(ui, "Wire diameter", &mut p.wire, 0.1..=4.0, 0.01, " mm");
            c |= num(ui, "Twist pitch", &mut p.pitch, 0.3..=30.0, 0.05, " mm");
            c |= num(ui, "Height", &mut p.height, -5.0..=10.0, 0.01, " mm");
            c |= check(ui, "Full circle", &mut p.full_circle);
            if !p.full_circle {
                c |= slider(ui, "Start", &mut p.start, 0.0..=1.0, "");
                c |= slider(ui, "End", &mut p.end, 0.0..=1.0, "");
            }
            c
        }),
        Op::Deform(p) => grid(ui, "deform", |ui| {
            let mut c = node_pick(ui, "Source", "df_src", &mut p.source, &all);
            c |= combo(ui, "Deformation", "df_kind", &mut p.kind, &[(DeformKind::Twist, "Twist"), (DeformKind::Taper, "Taper")]);
            c |= combo(ui, "Axis", "df_axis", &mut p.axis, &[(Axis::X, "X"), (Axis::Y, "Y (finger)"), (Axis::Z, "Z")]);
            match p.kind {
                DeformKind::Twist => c |= num(ui, "Total angle", &mut p.amount, -1080.0..=1080.0, 1.0, "°"),
                DeformKind::Taper => c |= num(ui, "End scale", &mut p.amount, 0.05..=5.0, 0.01, "×"),
            }
            c
        }),
        Op::GemsOnSurface(p) => {
            let mut c = grid(ui, "gos", |ui| {
                let mut c = node_pick(ui, "Target surface", "gos_t", &mut p.target, &solids);
                c |= num(ui, "Gem size", &mut p.spec.width, 0.5..=10.0, 0.01, " mm");
                p.spec.length = p.spec.width;
                c |= num(ui, "Height vs surface", &mut p.height, -5.0..=5.0, 0.01, " mm");
                c |= num(ui, "Minimum spacing", &mut p.spacing, 0.0..=3.0, 0.005, " mm");
                ui.label("Placed points");
                ui.label(format!("{}", p.points.len()));
                ui.end_row();
                c
            });
            ui.small("Click on the surface in the viewport to place a gem (this node must be selected).");
            ui.horizontal(|ui| {
                if ui.button("Remove last").clicked() {
                    c |= p.points.pop().is_some();
                }
                if ui.button("Clear").clicked() && !p.points.is_empty() {
                    p.points.clear();
                    c = true;
                }
            });
            let mut fill = p.fill.is_some();
            if ui.checkbox(&mut fill, "Auto fill a region (Poisson disk)").changed() {
                p.fill = fill.then(|| AutoFill { center: p.points.last().copied().unwrap_or([0.0, 0.0, 10.0]), radius: 4.0, max_angle: 25.0 });
                c = true;
            }
            if let Some(f) = &mut p.fill {
                c |= grid(ui, "gos_fill", |ui| {
                    let mut c = xyz(ui, "Region centre", &mut f.center, 0.05, " mm");
                    c |= num(ui, "Region radius", &mut f.radius, 0.5..=40.0, 0.05, " mm");
                    c |= slider(ui, "Max surface angle", &mut f.max_angle, 0.0..=90.0, "°");
                    c
                });
                ui.small("Tip: the last clicked point becomes the region centre when you enable fill.");
            }
            ui.separator();
            c |= gem_spec(ui, "gosspec", &mut p.spec, false);
            c
        }
        Op::Import(p) => grid(ui, "imp", |ui| {
            ui.label("Source");
            ui.label(&p.name);
            ui.end_row();
            let mut c = combo(ui, "Role", "imp_role", &mut p.role, &[(Role::Metal, "Metal"), (Role::Cutter, "Cutter")]);
            c |= num(ui, "Scale", &mut p.scale, 0.001..=1000.0, 0.01, "×");
            c |= xyz(ui, "Position", &mut p.position, 0.02, " mm");
            c |= xyz(ui, "Rotation", &mut p.rotation, 0.5, "°");
            c
        }),
    }
}

fn text(ui: &mut Ui, label: &str, v: &mut String) -> bool {
    ui.label(label);
    let r = ui.text_edit_singleline(v);
    ui.end_row();
    r.changed()
}

fn xyz(ui: &mut Ui, label: &str, v: &mut [f64; 3], speed: f64, suffix: &str) -> bool {
    ui.label(label);
    let mut c = false;
    ui.horizontal(|ui| {
        for x in v.iter_mut() {
            c |= ui.add(DragValue::new(x).speed(speed).suffix(suffix).max_decimals(3)).changed();
        }
    });
    ui.end_row();
    c
}

fn fonts() -> Vec<(Font, &'static str)> {
    Font::ALL.iter().map(|f| (*f, f.name())).collect()
}
