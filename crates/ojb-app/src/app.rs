//! Main window: ribbon, Dynamic Commands (history), viewport, properties,
//! tech report and project (job bag) information.

use crate::editors::{self, Candidate};
use crate::scene::{self, Done, Job};
use crate::styles::StyleStore;
use crate::viewport::{Camera, DrawOptions, Gpu};
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText, Sense};
use ojb_core::document::{Category, Document, NodeId, Op};
use ojb_core::io::{self, ExportFormat};
use ojb_core::materials::Alloy;
use ojb_core::ops;
use ojb_core::templates::{self, TemplateOptions};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Properties,
    Report,
    Project,
}

pub struct OjbApp {
    doc: Document,
    path: Option<PathBuf>,
    saved: Document,
    committed: Document,
    undo: Vec<Document>,
    redo: Vec<Document>,
    selected: Option<NodeId>,
    hovered: Option<NodeId>,
    renaming: Option<(NodeId, String)>,

    jobs: Sender<Job>,
    done: Receiver<Done>,
    version: u64,
    sent: Option<(Document, bool)>,
    last: Option<Done>,
    last_full: Option<Done>,

    gpu: Option<Gpu>,
    camera: Camera,
    fit_pending: bool,
    show_grid: bool,
    show_cutters: bool,
    show_curves: bool,

    tab: Tab,
    status: String,
    styles: StyleStore,
    style_name: String,
    new_opts: TemplateOptions,
    show_new: bool,
    show_about: bool,
    drag_handle: Option<u32>,
    studio: Option<crate::studio::Studio>,
    studio_spp: u32,
    ctx: Option<egui::Context>,
}

impl OjbApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        let (jobs, done) = scene::spawn(cc.egui_ctx.clone());
        let gpu = cc.wgpu_render_state.as_ref().map(Gpu::new);
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        cc.egui_ctx.set_global_style(style);
        let (doc, path, status) = match open.as_ref().map(|p| (io::load(p), p)) {
            Some((Ok(d), p)) => (d, Some(p.clone()), format!("Opened {}", p.display())),
            Some((Err(e), _)) => (default_doc(), None, format!("Could not open: {e}")),
            None => (default_doc(), None, "Welcome to OpenJewelleryBhy".into()),
        };
        OjbApp {
            saved: doc.clone(),
            committed: doc.clone(),
            doc,
            path,
            undo: vec![],
            redo: vec![],
            // OJB_SELECT=<node id> preselects a node (handy for scripted screenshots)
            selected: std::env::var("OJB_SELECT").ok().and_then(|v| v.parse().ok()).map(NodeId),
            hovered: None,
            renaming: None,
            jobs,
            done,
            version: 0,
            sent: None,
            last: None,
            last_full: None,
            gpu,
            camera: Camera::default(),
            fit_pending: true,
            show_grid: true,
            show_cutters: false,
            show_curves: false,
            tab: Tab::Properties,
            status,
            styles: StyleStore::load(),
            style_name: String::new(),
            new_opts: TemplateOptions::default(),
            show_new: false,
            show_about: false,
            drag_handle: None,
            studio: None,
            studio_spp: 128,
            ctx: Some(cc.egui_ctx.clone()),
        }
    }

    fn set_doc(&mut self, doc: Document, path: Option<PathBuf>) {
        self.undo.push(self.doc.clone());
        self.doc = doc;
        self.committed = self.doc.clone();
        self.saved = self.doc.clone();
        self.path = path;
        self.selected = None;
        self.fit_pending = true;
    }

    fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "untitled".into());
        let dirty = if self.doc != self.saved { " •" } else { "" };
        format!("OpenJewelleryBhy — {name}{dirty}")
    }

    // ---------------------------------------------------------------- files

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("OpenJewelleryBhy", &["ojb"]).pick_file() {
            match io::load(&p) {
                Ok(d) => {
                    self.status = format!("Opened {}", p.display());
                    self.set_doc(d, Some(p));
                }
                Err(e) => self.status = format!("Open failed: {e}"),
            }
        }
    }

    fn save(&mut self, save_as: bool) {
        let path = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().add_filter("OpenJewelleryBhy", &["ojb"]).set_file_name("design.ojb").save_file(),
        };
        if let Some(p) = path {
            match io::save(&self.doc, &p) {
                Ok(()) => {
                    self.saved = self.doc.clone();
                    self.status = format!("Saved {}", p.display());
                    self.path = Some(p);
                }
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
    }

    fn export(&mut self, fmt: ExportFormat, gems: bool) {
        let Some(done) = self.last_full.as_ref().filter(|d| d.doc == self.doc) else {
            self.status = "Still computing — try again in a moment".into();
            return;
        };
        let ext = match fmt {
            ExportFormat::Stl => "stl",
            ExportFormat::Obj => "obj",
            ExportFormat::ThreeMf => "3mf",
        };
        if let Some(p) = rfd::FileDialog::new().add_filter(ext.to_uppercase(), &[ext]).set_file_name(format!("design.{ext}")).save_file() {
            let parts = io::collect(&done.doc, &done.result, gems);
            match io::export(&parts, &p, fmt) {
                Ok(()) => self.status = format!("Exported {}", p.display()),
                Err(e) => self.status = format!("Export failed: {e}"),
            }
        }
    }

    fn export_report(&mut self, json: bool) {
        let Some(rep) = self.last_full.as_ref().filter(|d| d.doc == self.doc).and_then(|d| d.report.clone()) else {
            self.status = "Report not ready yet".into();
            return;
        };
        let ext = if json { "json" } else { "md" };
        if let Some(p) = rfd::FileDialog::new().set_file_name(format!("tech-report.{ext}")).save_file() {
            let text = if json { serde_json::to_string_pretty(&rep).unwrap_or_default() } else { rep.to_markdown() };
            match std::fs::write(&p, text) {
                Ok(()) => self.status = format!("Report written to {}", p.display()),
                Err(e) => self.status = format!("Write failed: {e}"),
            }
        }
    }

    fn render_png(&mut self) {
        let Some(done) = self.last_full.as_ref().filter(|d| d.doc == self.doc) else { return };
        if let Some(p) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("preview.png").save_file() {
            let img = ojb_core::render::render(&ojb_core::render::items(&done.doc, &done.result), 1200, 1200, ojb_core::render::View::default());
            match ojb_core::render::write_png(&p, 1200, 1200, &img) {
                Ok(()) => self.status = format!("Rendered {}", p.display()),
                Err(e) => self.status = format!("Render failed: {e}"),
            }
        }
    }

    // ------------------------------------------------------------- editing

    fn candidates(&self, before: Option<NodeId>) -> Vec<Candidate> {
        let outputs = self.last.as_ref().map(|d| &d.result.outputs);
        let mut out = Vec::new();
        for n in &self.doc.nodes {
            if Some(n.id) == before {
                break;
            }
            let o = outputs.and_then(|m| m.get(&n.id)).and_then(|r| r.as_ref().ok());
            let (has_rail, has_gems, has_shank, has_solids) = match o {
                Some(o) => (o.rail.is_some(), !o.gems.is_empty(), o.shank.is_some(), !o.solids.is_empty()),
                None => (
                    matches!(n.op, Op::RingRail(_) | Op::Shank(_)),
                    matches!(n.op, Op::Gem(_) | Op::GemsOnRail(_) | Op::Halo(_)),
                    matches!(n.op, Op::Shank(_)),
                    !matches!(n.op, Op::RingRail(_) | Op::Gem(_) | Op::GemsOnRail(_)),
                ),
            };
            out.push(Candidate { id: n.id, label: format!("{} {}", n.id, n.name), has_rail, has_gems, has_shank, has_solids });
        }
        out
    }

    fn add(&mut self, kind: &str) {
        let c = self.candidates(None);
        let sel = self.selected;
        let pick = |f: &dyn Fn(&Candidate) -> bool| -> Option<NodeId> {
            sel.filter(|s| c.iter().any(|x| x.id == *s && f(x))).or_else(|| c.iter().rev().find(|x| f(x)).map(|x| x.id))
        };
        let rail = pick(&|x| x.has_rail);
        let gems = pick(&|x| x.has_gems);
        let shank = pick(&|x| x.has_shank);
        let consumed = self.doc.consumed();
        let need = |v: Option<NodeId>, what: &str| v.ok_or_else(|| format!("Add a {what} first"));
        let res: Result<(String, Op), String> = (|| {
            Ok(match kind {
                "rail" => ("Ring rail".into(), Op::RingRail(Default::default())),
                "shank" => ("Shank".into(), Op::Shank(ops::shank::ShankParams { rail: need(rail, "ring rail")?, ..Default::default() })),
                "gem" => (
                    "Gem".into(),
                    Op::Gem(ops::gems::GemParams {
                        placement: match rail {
                            Some(r) => ops::gems::Placement::OnRail { rail: r, t: 0.0, height: 2.4, spin: 0.0, tilt: 0.0 },
                            None => ops::gems::Placement::Free { position: [0.0; 3], rotation: [0.0; 3] },
                        },
                        ..Default::default()
                    }),
                ),
                "gems_on_rail" => ("Gems on rail".into(), Op::GemsOnRail(ops::gems::GemsOnRailParams { rail: need(rail, "ring rail or shank")?, ..Default::default() })),
                "halo" => ("Halo".into(), Op::Halo(ops::halo::HaloParams { gem: need(gems, "gem")?, ..Default::default() })),
                "head" => ("Head".into(), Op::Head(ops::head::HeadParams { gem: need(gems, "gem")?, ..Default::default() })),
                "bezel" => ("Bezel".into(), Op::Bezel(ops::bezel::BezelParams { gem: need(gems, "gem")?, ..Default::default() })),
                "prongs" => ("Prongs".into(), Op::Prongs(ops::prongs::ProngsParams { gems: need(gems, "gem row")?, ..Default::default() })),
                "gem_cutter" => ("Gem cutter".into(), Op::GemCutter(ops::cutters::GemCutterParams { gems: need(gems, "gem")?, ..Default::default() })),
                "scoop" => ("Scoop".into(), Op::Scoop(ops::cutters::ScoopParams { shank: need(shank, "shank")?, ..Default::default() })),
                "boolean" => {
                    let free: Vec<&ojb_core::Node> = self.doc.nodes.iter().filter(|n| !consumed.contains(&n.id) && n.visible).collect();
                    let tools: Vec<NodeId> = free.iter().filter(|n| n.op.category() == Category::Cutters).map(|n| n.id).collect();
                    let targets: Vec<NodeId> = free
                        .iter()
                        .filter(|n| matches!(n.op.category(), Category::Tools | Category::Settings | Category::Solid | Category::Transform) && !matches!(n.op, Op::RingRail(_))
                            || matches!(n.op, Op::Halo(_)))
                        .map(|n| n.id)
                        .collect();
                    if targets.is_empty() {
                        return Err("Nothing to combine".into());
                    }
                    ("Boolean".into(), Op::Boolean(ops::boolean::BooleanParams { kind: ops::boolean::BoolKind::Difference, targets, tools }))
                }
                "split" => ("Split shank".into(), Op::SplitShank(ops::extras::SplitShankParams { rail: need(rail, "ring rail")?, ..Default::default() })),
                "signet" => ("Signet".into(), Op::Signet(ops::signet::SignetParams { rail: need(rail, "ring rail")?, ..Default::default() })),
                "text_rail" => ("Text on ring".into(), Op::TextOnRail(ops::text::TextOnRailParams { rail: need(rail, "ring rail or shank")?, ..Default::default() })),
                "text3d" => ("Text".into(), Op::Text3d(Default::default())),
                "gems_surface" => {
                    let target = sel
                        .filter(|s| c.iter().any(|x| x.id == *s && x.has_solids))
                        .or_else(|| c.iter().rev().find(|x| x.has_solids).map(|x| x.id));
                    ("Gems on surface".into(), Op::GemsOnSurface(ops::extras::GemsOnSurfaceParams { target: need(target, "metal surface")?, ..Default::default() }))
                }
                "pearl_cup" => ("Pearl cup".into(), Op::PearlCup(ops::extras::PearlCupParams { gem: need(gems, "pearl / gem")?, ..Default::default() })),
                "bail" => ("Bail".into(), Op::Bail(Default::default())),
                "channel" => ("Channel cutter".into(), Op::ChannelCutter(ops::cutters::ChannelCutterParams { gems: need(gems, "gem row")?, ..Default::default() })),
                "scallop" => ("Scallop cutter".into(), Op::ScallopCutter(ops::cutters::ScallopParams { gem: need(gems, "gem")?, ..Default::default() })),
                "azure" => ("Azure cutter".into(), Op::Azure(ops::cutters::AzureParams { shank: need(shank, "shank")?, ..Default::default() })),
                "rope" => ("Rope".into(), Op::Rope(ops::extras::RopeParams { rail: need(rail, "ring rail")?, ..Default::default() })),
                "deform" => ("Twist".into(), Op::Deform(ops::extras::DeformParams { source: need(sel, "selection")?, ..Default::default() })),
                "import" => {
                    let p = rfd::FileDialog::new().add_filter("Mesh", &["stl", "obj"]).pick_file().ok_or("Import cancelled")?;
                    let m = io::import_mesh(&p)?;
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    (
                        name.clone(),
                        Op::Import(ops::extras::ImportParams { name, data: ops::extras::encode_mesh(&m), ..Default::default() }),
                    )
                }
                "polar" => ("Polar array".into(), Op::PolarArray(ops::transform::PolarArrayParams { source: need(sel, "selection")?, ..Default::default() })),
                "mirror" => ("Mirror".into(), Op::Mirror(ops::transform::MirrorParams { source: need(sel, "selection")?, ..Default::default() })),
                _ => return Err(format!("unknown builder {kind}")),
            })
        })();
        match res {
            Ok((name, op)) => {
                let id = self.doc.add(name, op);
                self.selected = Some(id);
                self.tab = Tab::Properties;
                self.status = format!("Added {}", self.doc.get(id).map(|n| n.op.kind_name()).unwrap_or(""));
            }
            Err(e) => self.status = e,
        }
    }

    fn undo(&mut self) {
        if let Some(d) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, d));
            self.committed = self.doc.clone();
        }
    }

    fn redo(&mut self) {
        if let Some(d) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, d));
            self.committed = self.doc.clone();
        }
    }

    fn delete_selected(&mut self) {
        if let Some(id) = self.selected.take() {
            let gone = self.doc.remove_with_dependents(id);
            self.status = format!("Deleted {} node(s)", gone.len());
        }
    }

    // ------------------------------------------------------------ worker IO

    fn pump(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let mut newest = None;
        while let Ok(d) = self.done.try_recv() {
            newest = Some(d);
        }
        if let Some(d) = newest {
            if let (Some(gpu), Some(rs)) = (self.gpu.as_mut(), frame.wgpu_render_state()) {
                gpu.upload(rs, &d.scene);
            }
            if self.fit_pending && !d.scene.bbox.is_empty() {
                self.camera.fit(&d.scene.bbox);
                self.fit_pending = false;
            }
            if !d.preview {
                self.last_full = Some(Done {
                    version: d.version,
                    preview: false,
                    doc: d.doc.clone(),
                    scene: d.scene.clone(),
                    result: d.result.clone(),
                    report: d.report.clone(),
                    millis: d.millis,
                });
            }
            self.last = Some(d);
        }
        // request a new evaluation when the document changed
        let dragging = ctx.input(|i| i.pointer.any_down());
        let want_preview = dragging;
        let stale = match &self.sent {
            Some((doc, prev)) => doc != &self.doc || (*prev && !want_preview),
            None => true,
        };
        if stale {
            self.version += 1;
            let _ = self.jobs.send(Job { doc: self.doc.clone(), preview: want_preview, version: self.version });
            self.sent = Some((self.doc.clone(), want_preview));
        }
        // undo checkpoints once an edit gesture is finished
        if !dragging && self.doc != self.committed {
            self.undo.push(std::mem::replace(&mut self.committed, self.doc.clone()));
            self.redo.clear();
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
    }

    fn busy(&self) -> bool {
        self.last.as_ref().map(|d| d.version) != Some(self.version) || self.last.as_ref().is_some_and(|d| d.preview)
    }

    // ------------------------------------------------------------------ UI

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let sc = |m, k| KeyboardShortcut::new(m, k);
        let (undo, redo, save, open, new, fit, del) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Z)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::Y)) || i.consume_shortcut(&sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&sc(Modifiers::COMMAND, Key::N)),
                i.consume_shortcut(&sc(Modifiers::NONE, Key::F)),
                i.consume_shortcut(&sc(Modifiers::NONE, Key::Delete)),
            )
        });
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if save {
            self.save(false);
        }
        if open {
            self.open_dialog();
        }
        if new {
            self.show_new = true;
        }
        if fit && !ctx.egui_wants_keyboard_input() {
            self.fit_pending = true;
            if let Some(d) = &self.last {
                self.camera.fit(&d.scene.bbox);
                self.fit_pending = false;
            }
        }
        if del && !ctx.egui_wants_keyboard_input() {
            self.delete_selected();
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New from template…").clicked() {
                    self.show_new = true;
                    ui.close();
                }
                if ui.button("New empty").clicked() {
                    self.set_doc(Document::new(), None);
                    ui.close();
                }
                if ui.button("Open…").clicked() {
                    self.open_dialog();
                    ui.close();
                }
                ui.separator();
                if ui.button("Save").clicked() {
                    self.save(false);
                    ui.close();
                }
                if ui.button("Save as…").clicked() {
                    self.save(true);
                    ui.close();
                }
                ui.separator();
                for (label, fmt, gems) in [
                    ("Export STL (metal)…", ExportFormat::Stl, false),
                    ("Export 3MF (metal + gems)…", ExportFormat::ThreeMf, true),
                    ("Export OBJ (metal + gems)…", ExportFormat::Obj, true),
                ] {
                    if ui.button(label).clicked() {
                        self.export(fmt, gems);
                        ui.close();
                    }
                }
                if ui.button("Export tech report (Markdown)…").clicked() {
                    self.export_report(false);
                    ui.close();
                }
                if ui.button("Export tech report (JSON)…").clicked() {
                    self.export_report(true);
                    ui.close();
                }
                if ui.button("Render PNG preview…").clicked() {
                    self.render_png();
                    ui.close();
                }
                ui.separator();
                if ui.button("Import STL / OBJ…").clicked() {
                    self.add("import");
                    ui.close();
                }
                ui.separator();
                if ui.button("Render Studio (photorealistic)…").clicked() {
                    self.start_studio();
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo  Ctrl+Z")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo  Ctrl+Y")).clicked() {
                    self.redo();
                }
                ui.separator();
                if ui.add_enabled(self.selected.is_some(), egui::Button::new("Duplicate")).clicked() {
                    if let Some(id) = self.selected.and_then(|s| self.doc.duplicate(s)) {
                        self.selected = Some(id);
                    }
                    ui.close();
                }
                if ui.add_enabled(self.selected.is_some(), egui::Button::new("Delete (with dependents)  Del")).clicked() {
                    self.delete_selected();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Zoom to fit  F").clicked() {
                    if let Some(d) = &self.last {
                        self.camera.fit(&d.scene.bbox);
                    }
                    ui.close();
                }
                for (name, yaw, pitch) in [("Perspective", 35.0f32, 22.0f32), ("Front", 0.0, 0.0), ("Top", 0.0, 89.9), ("Right", 90.0, 0.0)] {
                    if ui.button(name).clicked() {
                        self.camera.yaw = yaw.to_radians();
                        self.camera.pitch = pitch.to_radians();
                        ui.close();
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.show_grid, "Grid (1 mm)");
                ui.checkbox(&mut self.show_cutters, "Show all cutters");
                ui.checkbox(&mut self.show_curves, "Show rails & curves");
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About").clicked() {
                    self.show_about = true;
                    ui.close();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(self.title()).weak());
            });
        });
    }

    fn ribbon(&mut self, ui: &mut egui::Ui) {
        let groups: [(&str, &[(&str, &str, &str)]); 6] = [
            ("Tools", &[
                ("Ring Rail", "rail", "Finger size circle (gvRingRail)"),
                ("Shank", "shank", "Profiles swept on the rail (Profile Placer + Sweep)"),
                ("Split/Bypass", "split", "Split shank or bypass band"),
                ("Signet", "signet", "Signet / award ring with monogram"),
                ("Text on Ring", "text_rail", "Engraved or embossed text inside/outside the band"),
                ("Text", "text3d", "Free solid text"),
            ]),
            ("Gems", &[
                ("💎 Gem", "gem", "Single gem, free or on the rail"),
                ("Gems on Rail", "gems_on_rail", "Pavé / channel / eternity rows"),
                ("Halo", "halo", "Melee around the selected gem"),
                ("Gems on Surface", "gems_surface", "Click to place gems on any surface, or auto-fill a region"),
            ]),
            ("Settings", &[
                ("Head", "head", "Prong head with gallery rails"),
                ("Bezel", "bezel", "Bezel around the selected gem"),
                ("Prongs", "prongs", "Prong placer for a row of gems"),
                ("Pearl Cup", "pearl_cup", "Cup, peg and post for a pearl"),
                ("Bail", "bail", "Pendant bail with jump ring"),
            ]),
            ("Cutters", &[
                ("Gem Cutter", "gem_cutter", "Seat cutters for every gem of the input"),
                ("Channel", "channel", "Channel cutter along gem rows"),
                ("Scallop", "scallop", "Scallop cutters around a bezel"),
                ("Azure", "azure", "Honeycomb windows through the shank sides"),
                ("Scoop", "scoop", "Weight reduction channel inside the shank"),
            ]),
            ("Solid", &[
                ("Boolean", "boolean", "Combine all free metal with all free cutters"),
                ("Rope", "rope", "Twisted rope along the rail"),
                ("Import…", "import", "Import an STL or OBJ mesh"),
            ]),
            ("Transform", &[
                ("Polar Array", "polar", "Array the selected node around an axis"),
                ("Mirror", "mirror", "Mirror the selected node"),
                ("Twist/Taper", "deform", "Deform the selected node"),
            ]),
        ];
        ui.horizontal_wrapped(|ui| {
            for (title, items) in groups {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).small().weak());
                    ui.horizontal(|ui| {
                        for (label, kind, tip) in items {
                            if ui.button(*label).on_hover_text(*tip).clicked() {
                                self.add(kind);
                            }
                        }
                    });
                });
                ui.separator();
            }
        });
    }

    fn history(&mut self, ui: &mut egui::Ui) {
        ui.heading("Dynamic Commands");
        ui.small("Newest on top · click to edit · hover to locate");
        ui.separator();
        let errors: std::collections::HashMap<NodeId, String> = self
            .last
            .as_ref()
            .map(|d| d.result.errors(&d.doc).into_iter().map(|(n, e)| (n.id, e.to_string())).collect())
            .unwrap_or_default();
        let consumed = self.doc.consumed();
        let mut hovered = None;
        let mut action: Option<(&str, NodeId)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for n in self.doc.nodes.iter_mut().rev() {
                let id = n.id;
                ui.horizontal(|ui| {
                    ui.checkbox(&mut n.visible, "").on_hover_text("Visible");
                    let (icon, col) = match n.op.category() {
                        Category::Tools => ("T", Color32::from_rgb(200, 150, 40)),
                        Category::Gems => ("G", Color32::from_rgb(70, 140, 230)),
                        Category::Settings => ("S", Color32::from_rgb(190, 110, 200)),
                        Category::Cutters => ("C", Color32::from_rgb(220, 80, 60)),
                        Category::Solid => ("B", Color32::from_rgb(90, 170, 90)),
                        Category::Transform => ("X", Color32::from_rgb(120, 120, 120)),
                    };
                    ui.label(RichText::new(icon).monospace().strong().color(col));
                    if let Some((rid, text)) = self.renaming.as_mut().filter(|(r, _)| *r == id) {
                        let _ = rid;
                        let r = ui.text_edit_singleline(text);
                        if r.lost_focus() {
                            n.name = text.clone();
                            self.renaming = None;
                        } else {
                            r.request_focus();
                        }
                        return;
                    }
                    let mut text = RichText::new(&n.name);
                    if consumed.contains(&id) {
                        text = text.weak();
                    }
                    if errors.contains_key(&id) {
                        text = text.color(Color32::from_rgb(210, 60, 40));
                    }
                    let r = ui.selectable_label(self.selected == Some(id), text);
                    let r = r.on_hover_text(match errors.get(&id) {
                        Some(e) => format!("{} {} — error: {e}", n.op.kind_name(), id),
                        None => format!("{} {}{}", n.op.kind_name(), id, if consumed.contains(&id) { " (merged into a later step)" } else { "" }),
                    });
                    if r.hovered() {
                        hovered = Some(id);
                    }
                    if r.clicked() {
                        self.selected = Some(id);
                        self.tab = Tab::Properties;
                    }
                    if r.double_clicked() {
                        self.renaming = Some((id, n.name.clone()));
                    }
                    r.context_menu(|ui| {
                        if ui.button("Rename").clicked() {
                            action = Some(("rename", id));
                            ui.close();
                        }
                        if ui.button("Duplicate").clicked() {
                            action = Some(("dup", id));
                            ui.close();
                        }
                        if ui.button("Delete (with dependents)").clicked() {
                            action = Some(("del", id));
                            ui.close();
                        }
                    });
                });
            }
        });
        self.hovered = hovered;
        match action {
            Some(("rename", id)) => self.renaming = self.doc.get(id).map(|n| (id, n.name.clone())),
            Some(("dup", id)) => self.selected = self.doc.duplicate(id),
            Some(("del", id)) => {
                self.selected = Some(id);
                self.delete_selected();
            }
            _ => {}
        }
    }

    fn properties(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected else {
            ui.label("Select a dynamic command in the list or click an object in the viewport.");
            ui.add_space(8.0);
            ui.label("Tip: start from File > New from template, or add a Ring Rail, then a Shank, a Gem, a Head…");
            return;
        };
        let cands = self.candidates(Some(id));
        let Some(node) = self.doc.get_mut(id) else {
            self.selected = None;
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(node.op.kind_name());
            ui.label(RichText::new(format!("{id}")).weak());
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut node.name);
        });
        if let Some(Some(e)) = self.last.as_ref().map(|d| d.result.outputs.get(&id).and_then(|r| r.as_ref().err().cloned())) {
            ui.colored_label(Color32::from_rgb(210, 60, 40), format!("⚠ {e}"));
        }
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            editors::edit(ui, &mut node.op, &cands);
            ui.add_space(10.0);
            ui.separator();
            ui.label(RichText::new("StyleSheet").strong());
            let kind = node.op.kind_name().to_string();
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.style_name);
                if ui.button("Save style").clicked() && !self.style_name.trim().is_empty() {
                    self.status = match self.styles.save(&kind, self.style_name.trim(), &node.op) {
                        Ok(()) => format!("Style '{}' saved", self.style_name.trim()),
                        Err(e) => format!("Style not saved: {e}"),
                    };
                }
            });
            let names = self.styles.names(&kind);
            if !names.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Apply:");
                    for n in names {
                        if ui.button(&n).clicked() {
                            if let Err(e) = self.styles.apply(&kind, &n, &mut node.op) {
                                self.status = format!("Style error: {e}");
                            }
                        }
                    }
                });
            }
        });
    }

    fn report(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Alloy");
            egui::ComboBox::from_id_salt("alloy").selected_text(self.doc.alloy.name()).show_ui(ui, |ui| {
                for a in Alloy::ALL {
                    ui.selectable_value(&mut self.doc.alloy, a, a.name());
                }
            });
        });
        let Some(rep) = self.last_full.as_ref().and_then(|d| d.report.as_ref()) else {
            ui.spinner();
            return;
        };
        ui.separator();
        ui.label(RichText::new(format!("{:.2} g  ·  {:.2} dwt", rep.metal_grams, rep.metal_dwt)).size(22.0).strong());
        ui.label(format!("{} — volume {:.1} mm³ — surface {:.0} mm²", rep.selected_alloy, rep.metal_volume_mm3, rep.surface_mm2));
        ui.label(format!("Size {:.2} × {:.2} × {:.2} mm", rep.size_mm[0], rep.size_mm[1], rep.size_mm[2]));
        ui.label(format!("{} gems — {:.3} ct total", rep.gem_count, rep.total_carat));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if !rep.gems.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Gem report").strong());
                egui::Grid::new("gemrep").striped(true).show(ui, |ui| {
                    ui.label("Gem");
                    ui.label("Qty");
                    ui.label("ct each");
                    ui.label("formula");
                    ui.end_row();
                    for g in &rep.gems {
                        ui.label(&g.description);
                        ui.label(g.count.to_string());
                        ui.label(format!("{:.3}", g.carat_each_model));
                        ui.label(g.carat_each_formula.map(|f| format!("{f:.3}")).unwrap_or_else(|| "—".into()));
                        ui.end_row();
                    }
                });
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Metal weights").strong());
            egui::Grid::new("metalrep").striped(true).show(ui, |ui| {
                for m in &rep.metals {
                    ui.label(&m.alloy);
                    ui.label(format!("{:.2} g", m.grams));
                    ui.label(format!("{:.2} dwt", m.dwt));
                    ui.end_row();
                }
            });
            if !rep.finger_sizes.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Finger size").strong());
                egui::Grid::new("sizerep").striped(true).show(ui, |ui| {
                    for (r, v) in &rep.finger_sizes {
                        ui.label(r);
                        ui.label(v);
                        ui.end_row();
                    }
                });
            }
        });
    }

    fn project(&mut self, ui: &mut egui::Ui) {
        let m = &mut self.doc.meta;
        egui::Grid::new("proj").num_columns(2).striped(true).show(ui, |ui| {
            for (label, v) in [("Title", &mut m.title), ("Customer", &mut m.customer), ("Designer", &mut m.designer), ("Job number", &mut m.job_number), ("Status", &mut m.status)] {
                ui.label(label);
                ui.text_edit_singleline(v);
                ui.end_row();
            }
            ui.label("CAD labour");
            ui.add(egui::DragValue::new(&mut m.cad_minutes).suffix(" min"));
            ui.end_row();
        });
        ui.label("Notes");
        ui.text_edit_multiline(&mut m.notes);
    }

    fn start_studio(&mut self) {
        let Some(done) = self.last_full.as_ref().filter(|d| d.doc == self.doc) else {
            self.status = "Still computing — try again in a moment".into();
            return;
        };
        let items = ojb_core::render::items(&done.doc, &done.result);
        let c = &self.camera;
        let cam = ojb_core::trace::Camera {
            eye: c.eye().as_dvec3(),
            target: c.target.as_dvec3(),
            up: if c.pitch.abs() > 1.5 { glam::DVec3::Y } else { glam::DVec3::Z },
            fov_deg: c.fov.to_degrees() as f64,
        };
        let (w, h) = (1100, 800);
        if let Some(ctx) = self.ctx.clone() {
            self.studio = Some(crate::studio::Studio::start(ctx, items, cam, w, h, self.studio_spp));
        }
    }

    /// Handles of the selected node, from the last evaluation.
    fn handles(&self) -> Vec<ops::handles::Handle> {
        let (Some(id), Some(last)) = (self.selected, self.last.as_ref()) else { return vec![] };
        let Some(node) = self.doc.get(id) else { return vec![] };
        let Some(Ok(out)) = last.result.outputs.get(&id) else { return vec![] };
        let input = node.op.inputs().first().and_then(|i| last.result.outputs.get(i)).and_then(|r| r.as_ref().ok()).cloned();
        ops::handles::handles(&node.op, out, input.as_deref())
    }

    fn viewport(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let aspect = rect.width() / rect.height();
        let vp = self.camera.view_proj(aspect);
        let to_screen = |p: glam::DVec3| -> Option<egui::Pos2> {
            let c = vp * glam::Vec4::new(p.x as f32, p.y as f32, p.z as f32, 1.0);
            if c.w <= 0.0 {
                return None;
            }
            Some(egui::pos2(rect.left() + (c.x / c.w * 0.5 + 0.5) * rect.width(), rect.top() + (0.5 - c.y / c.w * 0.5) * rect.height()))
        };
        // control handles of the selected node
        let handles = self.handles();
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let hovered_handle = pointer.and_then(|pp| {
            handles
                .iter()
                .filter_map(|h| to_screen(h.pos).map(|s| (h.id, s.distance(pp))))
                .filter(|(_, d)| *d < 9.0)
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|x| x.0)
        });
        if resp.drag_started_by(egui::PointerButton::Primary) {
            self.drag_handle = hovered_handle;
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.drag_handle = None;
        }
        let shift = ui.input(|i| i.modifiers.shift);
        if let Some(hid) = self.drag_handle {
            if let Some(h) = handles.iter().find(|h| h.id == hid) {
                if let (Some(a), Some(b)) = (to_screen(h.pos), to_screen(h.pos + h.axis)) {
                    let axis = b - a; // screen pixels per mm along the handle axis
                    let l2 = axis.length_sq();
                    if l2 > 1e-3 {
                        let d = resp.drag_delta();
                        let mm = (d.x * axis.x + d.y * axis.y) / l2;
                        if mm != 0.0 {
                            if let (Some(id), Some(last)) = (self.selected, self.last.as_ref()) {
                                let out = last.result.outputs.get(&id).and_then(|r| r.as_ref().ok()).cloned();
                                let node = self.doc.get(id).cloned();
                                if let (Some(out), Some(node)) = (out, node) {
                                    let input = node.op.inputs().first().and_then(|i| last.result.outputs.get(i)).and_then(|r| r.as_ref().ok()).cloned();
                                    if let Some(n) = self.doc.get_mut(id) {
                                        ops::handles::apply(&mut n.op, hid, mm as f64, &out, input.as_deref());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else if resp.dragged_by(egui::PointerButton::Middle) || (shift && resp.dragged()) {
            let d = resp.drag_delta();
            self.camera.pan(d.x, d.y, rect.height());
        } else if resp.dragged() {
            let d = resp.drag_delta();
            self.camera.orbit(d.x, d.y);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.zoom(scroll);
            }
        }
        let (Some(gpu), Some(rs)) = (self.gpu.as_mut(), frame.wgpu_render_state()) else {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "No GPU renderer available", egui::FontId::proportional(18.0), Color32::RED);
            return;
        };
        if resp.clicked() && hovered_handle.is_none() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let ndc = [(pos.x - rect.left()) / rect.width() * 2.0 - 1.0, 1.0 - (pos.y - rect.top()) / rect.height() * 2.0];
                let (o, d) = self.camera.ray(ndc, aspect);
                let hit = gpu.pick(o, d);
                // Gems on Surface: clicking places a gem instead of selecting
                let placing = self.selected.and_then(|s| self.doc.get_mut(s)).and_then(|n| match &mut n.op {
                    Op::GemsOnSurface(p) => Some(p),
                    _ => None,
                });
                match (placing, hit) {
                    (Some(p), Some((_, at))) => {
                        p.points.push([at.x as f64, at.y as f64, at.z as f64]);
                        self.status = format!("{} gem point(s)", p.points.len());
                    }
                    (Some(_), None) => {}
                    (None, hit) => {
                        self.selected = hit.map(|h| h.0);
                        if self.selected.is_some() {
                            self.tab = Tab::Properties;
                        }
                    }
                }
            }
        }
        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        let tex = gpu.render(
            rs,
            size,
            &self.camera,
            DrawOptions {
                selected: self.selected,
                hovered: self.hovered,
                show_grid: self.show_grid,
                show_cutters: self.show_cutters,
                show_curves: self.show_curves,
            },
        );
        let painter = ui.painter_at(rect);
        painter.image(tex, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        for h in &handles {
            let Some(p) = to_screen(h.pos) else { continue };
            let active = Some(h.id) == self.drag_handle || Some(h.id) == hovered_handle;
            if let Some(q) = to_screen(h.pos + h.axis * 1.5) {
                let dir = (q - p).normalized() * 16.0;
                painter.line_segment([p - dir, p + dir], egui::Stroke::new(1.5, Color32::from_rgba_unmultiplied(20, 130, 230, 150)));
            }
            painter.circle(p, if active { 7.5 } else { 5.5 }, if active { Color32::from_rgb(255, 170, 30) } else { Color32::WHITE }, egui::Stroke::new(1.5, Color32::from_rgb(20, 110, 210)));
            if active {
                painter.text(p + egui::vec2(10.0, -10.0), egui::Align2::LEFT_BOTTOM, h.label, egui::FontId::proportional(12.0), Color32::from_rgb(20, 80, 160));
            }
        }
        let hint = if matches!(self.selected.and_then(|s| self.doc.get(s)).map(|n| &n.op), Some(Op::GemsOnSurface(_))) {
            "click on a surface to place a gem · drag: orbit · wheel: zoom"
        } else {
            "drag: orbit · shift/middle-drag: pan · wheel: zoom · F: fit · click: select · drag white handles to edit"
        };
        painter.text(rect.left_bottom() + egui::vec2(10.0, -10.0), egui::Align2::LEFT_BOTTOM, hint, egui::FontId::proportional(12.0), Color32::from_gray(110));
        if self.busy() {
            painter.text(rect.right_top() + egui::vec2(-12.0, 10.0), egui::Align2::RIGHT_TOP, "computing…", egui::FontId::proportional(13.0), Color32::from_rgb(30, 120, 200));
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.show_new {
            let mut open = true;
            let mut create: Option<&str> = None;
            egui::Window::new("New design from template").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("newopts").num_columns(2).show(ui, |ui| {
                    ui.label("Size system");
                    let old = self.new_opts.size;
                    egui::ComboBox::from_id_salt("nreg").selected_text(self.new_opts.size.region.name()).show_ui(ui, |ui| {
                        for (r, n) in editors::regions() {
                            if ui.selectable_label(self.new_opts.size.region == r, n).clicked() {
                                self.new_opts.size = old.convert(r);
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Finger size");
                    ui.add(egui::DragValue::new(&mut self.new_opts.size.value).speed(0.05));
                    ui.end_row();
                    ui.label("Alloy");
                    egui::ComboBox::from_id_salt("nalloy").selected_text(self.new_opts.alloy.name()).show_ui(ui, |ui| {
                        for a in Alloy::ALL {
                            ui.selectable_value(&mut self.new_opts.alloy, a, a.name());
                        }
                    });
                    ui.end_row();
                    ui.label("Centre gem shape");
                    egui::ComboBox::from_id_salt("nshape").selected_text(self.new_opts.shape.name()).show_ui(ui, |ui| {
                        for (s, n) in editors::shapes() {
                            let on = self.new_opts.shape == s;
                            if ui.selectable_label(on, n).clicked() {
                                self.new_opts.shape = s;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Centre gem width");
                    ui.add(egui::DragValue::new(&mut self.new_opts.gem_width).range(1.0..=20.0).speed(0.05).suffix(" mm"));
                    ui.end_row();
                });
                ui.separator();
                for (name, desc) in templates::NAMES {
                    if ui.button(RichText::new(name).strong()).on_hover_text(desc).clicked() {
                        create = Some(name);
                    }
                    ui.small(desc);
                }
            });
            if let Some(name) = create {
                if let Some(d) = templates::build(name, &self.new_opts) {
                    self.set_doc(d, None);
                    self.status = format!("New {name}");
                }
                open = false;
            }
            self.show_new = open;
        }
        if let Some(st) = &mut self.studio {
            if !st.ui(ctx, &mut self.status) {
                self.studio = None;
            }
        }
        if self.show_about {
            egui::Window::new("About OpenJewelleryBhy").open(&mut self.show_about).collapsible(false).show(ctx, |ui| {
                ui.heading("OpenJewelleryBhy");
                ui.label(format!("version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Open-source parametric jewellery CAD written in Rust.");
                ui.label("Parametric history, ring rails in 11 size systems, 21 gem shapes, heads, bezels, halos, pavé, cutters, booleans, metal weights.");
                ui.label("Not affiliated with Stuller, Gemvision or McNeel.");
            });
        }
    }
}

fn default_doc() -> Document {
    templates::build("solitaire", &TemplateOptions::default()).unwrap_or_default()
}

impl eframe::App for OjbApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::top("ribbon").show(ui, |ui| self.ribbon(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(d) = &self.last {
                        let tris: usize = d.scene.nodes.iter().filter(|n| n.in_final).map(|n| (n.metal.len() + n.gems.len()) / 3).sum();
                        ui.label(RichText::new(format!(
                            "{} · {:.0} ms · {} rebuilt · {} triangles",
                            if d.preview { "preview" } else { "final" },
                            d.millis,
                            d.result.reevaluated,
                            tris
                        )).weak());
                    }
                    if let Some(r) = self.last_full.as_ref().and_then(|d| d.report.as_ref()) {
                        ui.label(RichText::new(format!("{:.2} g {} · {:.3} ct", r.metal_grams, r.selected_alloy, r.total_carat)).strong());
                    }
                });
            });
        });
        egui::Panel::left("history").resizable(true).default_size(250.0).show(ui, |ui| self.history(ui));
        egui::Panel::right("props").resizable(true).default_size(340.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Properties, "Properties");
                ui.selectable_value(&mut self.tab, Tab::Report, "Tech report");
                ui.selectable_value(&mut self.tab, Tab::Project, "Project");
            });
            ui.separator();
            match self.tab {
                Tab::Properties => self.properties(ui),
                Tab::Report => self.report(ui),
                Tab::Project => self.project(ui),
            }
        });
        egui::CentralPanel::no_frame().show(ui, |ui| self.viewport(ui, frame));
        self.dialogs(&ctx);
        self.pump(&ctx, frame);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
    }
}
