//! Render Studio: progressive photorealistic rendering in a background
//! thread, shown live and savable as PNG.

use eframe::egui;
use ojb_core::render::Item;
use ojb_core::trace::{self, Camera, Tracer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct Shared {
    pub rgba: Vec<u8>,
    pub passes: u32,
    pub done: bool,
}

pub struct Studio {
    pub w: usize,
    pub h: usize,
    pub spp: u32,
    cancel: Arc<AtomicBool>,
    pub shared: Arc<Mutex<Shared>>,
    tex: Option<egui::TextureHandle>,
    seen: u32,
    started: std::time::Instant,
}

impl Studio {
    pub fn start(ctx: egui::Context, items: Vec<Item>, cam: Camera, w: usize, h: usize, spp: u32) -> Studio {
        let cancel = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(Shared { rgba: vec![0; w * h * 4], passes: 0, done: false }));
        let (c2, s2) = (cancel.clone(), shared.clone());
        std::thread::Builder::new()
            .name("ojb-render".into())
            .spawn(move || {
                let tracer = Tracer::new(&items);
                let mut acc = vec![0f32; w * h * 3];
                let mut img = vec![0u8; w * h * 4];
                for pass in 0..spp {
                    if c2.load(Ordering::Relaxed) {
                        break;
                    }
                    tracer.render_pass(&cam, w, h, &mut acc, pass);
                    // refresh the preview often at first, then every 4 passes
                    if pass < 8 || pass % 4 == 3 || pass + 1 == spp {
                        trace::tonemap(&acc, pass + 1, &mut img);
                        let mut s = s2.lock().unwrap();
                        s.rgba.copy_from_slice(&img);
                        s.passes = pass + 1;
                        drop(s);
                        ctx.request_repaint();
                    }
                }
                s2.lock().unwrap().done = true;
                ctx.request_repaint();
            })
            .expect("spawn renderer");
        Studio { w, h, spp, cancel, shared, tex: None, seen: u32::MAX, started: std::time::Instant::now() }
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Shows the image; returns false when the window was closed.
    pub fn ui(&mut self, ctx: &egui::Context, status: &mut String) -> bool {
        let mut open = true;
        let (passes, done) = {
            let s = self.shared.lock().unwrap();
            (s.passes, s.done)
        };
        if passes != self.seen {
            let s = self.shared.lock().unwrap();
            let img = egui::ColorImage::from_rgba_unmultiplied([self.w, self.h], &s.rgba);
            match &mut self.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.tex = Some(ctx.load_texture("render-studio", img, egui::TextureOptions::LINEAR)),
            }
            self.seen = passes;
        }
        egui::Window::new("Render Studio").open(&mut open).default_size([self.w as f32 * 0.7 + 20.0, self.h as f32 * 0.7 + 70.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let state = if done { "finished" } else { "rendering…" };
                ui.label(format!("{passes}/{} samples · {state} · {:.1} s", self.spp, self.started.elapsed().as_secs_f32()));
                if !done && ui.button("Stop").clicked() {
                    self.stop();
                }
                if ui.button("Save PNG…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("render.png").save_file() {
                        let s = self.shared.lock().unwrap();
                        *status = match ojb_core::render::write_png(&p, self.w, self.h, &s.rgba) {
                            Ok(()) => format!("Render saved to {}", p.display()),
                            Err(e) => format!("Save failed: {e}"),
                        };
                    }
                }
            });
            if let Some(t) = &self.tex {
                let avail = ui.available_size();
                let scale = (avail.x / self.w as f32).min(avail.y / self.h as f32).max(0.1);
                ui.image((t.id(), egui::vec2(self.w as f32 * scale, self.h as f32 * scale)));
            }
        });
        if !open {
            self.stop();
        }
        open
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The application icon (shared with the installer and packages).
pub fn icon() -> egui::IconData {
    egui::IconData { rgba: ojb_core::render::app_icon(64), width: 64, height: 64 }
}
