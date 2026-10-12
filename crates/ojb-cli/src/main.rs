//! `ojb` — headless OpenJewelleryBhy: create designs from templates, rebuild
//! history, export for printing/casting and produce tech reports.

use clap::{Parser, Subcommand};
use ojb_core::gem::{Cut, GemMaterial, GemSpec};
use ojb_core::io::{self, ExportFormat};
use ojb_core::materials::Alloy;
use ojb_core::outline::Shape;
use ojb_core::sizes::{FingerSize, Region};
use ojb_core::templates::{self, TemplateOptions};
use ojb_core::{report, Evaluator};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "ojb", version, about = "OpenJewelleryBhy — open parametric jewellery CAD (command line)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List the built-in design templates.
    Templates,
    /// Create a new design (.ojb) from a template.
    New {
        template: String,
        #[arg(short, long)]
        out: PathBuf,
        /// Finger size value (e.g. 7, N½, 54).
        #[arg(long, default_value = "7")]
        size: String,
        /// Size system: us, uk, iso/eu, fr, de, ch/it/es, jp/cn, dmm, cmm…
        #[arg(long, default_value = "us")]
        region: String,
        #[arg(long, default_value = "18k-yellow")]
        alloy: String,
        #[arg(long, default_value = "round")]
        shape: String,
        /// Centre gem width in mm.
        #[arg(long, default_value_t = 6.5)]
        gem: f64,
    },
    /// Evaluate a design and export the final model (STL, OBJ or 3MF by extension).
    Build {
        file: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        /// Also export gem models (as separate objects).
        #[arg(long)]
        gems: bool,
    },
    /// Technical report: metal weight for every alloy, gems, carats, sizes.
    Report {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Convert a finger size to every system.
    Size {
        value: String,
        #[arg(long, default_value = "us")]
        from: String,
    },
    /// Gem calculator: carat weight from dimensions.
    Gem {
        #[arg(long, default_value = "round")]
        shape: String,
        #[arg(long)]
        width: f64,
        #[arg(long)]
        length: Option<f64>,
        /// Total depth as % of the smaller dimension.
        #[arg(long)]
        depth: Option<f64>,
        #[arg(long, default_value = "diamond")]
        material: String,
        /// diamond | gem | cabochon | pearl
        #[arg(long)]
        cut: Option<String>,
    },
    /// Write the application icon (.ico for Windows, .png for Linux).
    #[command(hide = true)]
    Icon {
        #[arg(long)]
        ico: Option<PathBuf>,
        #[arg(long)]
        png: Option<PathBuf>,
    },
    /// Measure cold and incremental rebuild times of a design or template.
    Bench { target: String },
    /// Render a PNG preview (software renderer, no GPU needed).
    Render {
        file: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value_t = 800)]
        px: usize,
        #[arg(long, default_value_t = 35.0)]
        yaw: f64,
        #[arg(long, default_value_t = 25.0)]
        pitch: f64,
        /// Photorealistic path tracing instead of the quick preview.
        #[arg(long)]
        pathtrace: bool,
        /// Samples per pixel for --pathtrace.
        #[arg(long, default_value_t = 64)]
        spp: u32,
    },
}

fn parse_alloy(s: &str) -> Result<Alloy, String> {
    let k: String = s.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    Alloy::ALL
        .into_iter()
        .find(|a| {
            let n: String = a.name().to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            let dbg = format!("{a:?}").to_ascii_lowercase().replace("gold", "");
            n == k || dbg == k || n.starts_with(&k)
        })
        .ok_or_else(|| {
            format!("unknown alloy '{s}'. Known: {}", Alloy::ALL.iter().map(|a| a.name()).collect::<Vec<_>>().join(", "))
        })
}

fn parse_shape(s: &str) -> Result<Shape, String> {
    Shape::from_name(s).ok_or_else(|| {
        format!("unknown shape '{s}'. Known: {}", Shape::BUILTIN.iter().map(|x| x.name()).collect::<Vec<_>>().join(", "))
    })
}

fn parse_size(value: &str, region: &str) -> Result<FingerSize, String> {
    let r = Region::from_code(region).ok_or_else(|| format!("unknown size system '{region}'"))?;
    let v = r.parse(value).ok_or_else(|| format!("cannot read size '{value}' for {}", r.name()))?;
    Ok(FingerSize { region: r, value: v })
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.cmd {
        Cmd::Templates => {
            for (n, d) in templates::NAMES {
                println!("{n:<12} {d}");
            }
        }
        Cmd::New { template, out, size, region, alloy, shape, gem } => {
            let o = TemplateOptions {
                size: parse_size(&size, &region)?,
                alloy: parse_alloy(&alloy)?,
                shape: parse_shape(&shape)?,
                gem_width: gem,
            };
            let doc = templates::build(&template, &o).ok_or_else(|| format!("unknown template '{template}'"))?;
            io::save(&doc, &out)?;
            println!("created {} ({} nodes)", out.display(), doc.nodes.len());
        }
        Cmd::Build { file, out, gems } => {
            let doc = io::load(&file)?;
            let fmt = ExportFormat::from_path(&out).ok_or("output must end in .stl, .obj or .3mf")?;
            let t = Instant::now();
            let r = Evaluator::new().evaluate(&doc);
            if let Some(e) = &r.error {
                return Err(e.clone());
            }
            for (n, e) in r.errors(&doc) {
                eprintln!("warning: {} ({}): {e}", n.name, n.id);
            }
            let parts = io::collect(&doc, &r, gems);
            io::export(&parts, &out, fmt)?;
            let tris: usize = parts.iter().map(|(_, m)| m.triangles.len()).sum();
            println!("exported {} objects, {} triangles to {} in {:.0} ms", parts.len(), tris, out.display(), t.elapsed().as_secs_f64() * 1e3);
        }
        Cmd::Report { file, json } => {
            let doc = io::load(&file)?;
            let r = Evaluator::new().evaluate(&doc);
            if let Some(e) = &r.error {
                return Err(e.clone());
            }
            let rep = report::build(&doc, &r);
            if json {
                println!("{}", serde_json::to_string_pretty(&rep).map_err(|e| e.to_string())?);
            } else {
                print!("{}", rep.to_markdown());
            }
        }
        Cmd::Size { value, from } => {
            let s = parse_size(&value, &from)?;
            println!("inner diameter: {:.2} mm, circumference: {:.2} mm", s.diameter(), s.diameter() * std::f64::consts::PI);
            for r in Region::ALL {
                println!("{:<30} {}", r.name(), r.format(s.convert(r).value));
            }
        }
        Cmd::Gem { shape, width, length, depth, material, cut } => {
            let shape = parse_shape(&shape)?;
            let mut g = GemSpec::new(shape.clone(), width, length.unwrap_or(width * shape.default_ratio()));
            if let Some(d) = depth {
                g.depth_pct = d;
            }
            let mk = material.to_ascii_lowercase().replace([' ', '_', '-'], "");
            g.material = GemMaterial::ALL
                .into_iter()
                .find(|m| m.name().to_ascii_lowercase().replace(' ', "") == mk)
                .ok_or_else(|| format!("unknown material '{material}'"))?;
            if let Some(c) = cut {
                g.cut = Cut::ALL
                    .into_iter()
                    .find(|x| format!("{x:?}").eq_ignore_ascii_case(&c))
                    .ok_or_else(|| format!("unknown cut '{c}'"))?;
            }
            let h = g.heights();
            println!("{}", g.label());
            println!("depth {:.2} mm (table {:+.2}, culet {:+.2} from girdle)", g.depth(), h.table, h.culet);
            println!("volume {:.2} mm³", g.volume_mm3());
            println!("carat (model volume × SG {:.2}): {:.3} ct", g.material.sg(), g.carat_from_volume());
            match g.carat_formula() {
                Some(c) => println!("carat (industry formula):        {c:.3} ct"),
                None => println!("carat (industry formula):        n/a for this shape"),
            }
        }
        Cmd::Render { file, out, px, yaw, pitch, pathtrace, spp } => {
            let doc = io::load(&file)?;
            let r = Evaluator::new().evaluate(&doc);
            if let Some(e) = &r.error {
                return Err(e.clone());
            }
            let t = Instant::now();
            let items = ojb_core::render::items(&doc, &r);
            let img = if pathtrace {
                let mut bb = ojb_core::mesh::Aabb::EMPTY;
                for it in &items {
                    bb = bb.union(&it.mesh.bbox());
                }
                let cam = ojb_core::trace::Camera::framing(&bb, yaw, pitch);
                ojb_core::trace::render(&items, &cam, px, px, spp)
            } else {
                ojb_core::render::render(&items, px, px, ojb_core::render::View { yaw, pitch })
            };
            ojb_core::render::write_png(&out, px, px, &img)?;
            println!("rendered {} in {:.0} ms", out.display(), t.elapsed().as_secs_f64() * 1e3);
        }
        Cmd::Icon { ico, png } => {
            if let Some(p) = ico {
                ojb_core::render::write_ico(&p, &[16, 24, 32, 48, 64, 128, 256])?;
            }
            if let Some(p) = png {
                ojb_core::render::write_png(&p, 256, 256, &ojb_core::render::app_icon(256))?;
            }
        }
        Cmd::Bench { target } => {
            let doc = if std::path::Path::new(&target).exists() {
                io::load(std::path::Path::new(&target))?
            } else {
                templates::build(&target, &TemplateOptions::default()).ok_or("not a file nor a template")?
            };
            let mut ev = Evaluator::new();
            let t = Instant::now();
            let r = ev.evaluate(&doc);
            let cold = t.elapsed().as_secs_f64() * 1e3;
            let n = r.reevaluated;
            // change the finger size: whole history must follow
            let mut doc2 = doc.clone();
            if let Some(ojb_core::Op::RingRail(p)) = doc2.nodes.iter_mut().map(|n| &mut n.op).find(|o| matches!(o, ojb_core::Op::RingRail(_))) {
                p.size.value += 0.5;
            }
            let t = Instant::now();
            let r2 = ev.evaluate(&doc2);
            let resize = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let r3 = ev.evaluate(&doc2);
            let noop = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let rep = report::build(&doc2, &r3);
            let rep_ms = t.elapsed().as_secs_f64() * 1e3;
            println!("cold evaluation : {cold:8.1} ms ({n} nodes)");
            println!("resize (+½ size): {resize:8.1} ms ({} nodes re-run)", r2.reevaluated);
            println!("no change       : {noop:8.3} ms ({} nodes re-run)", r3.reevaluated);
            println!("tech report     : {rep_ms:8.1} ms ({:.2} g {}, {} triangles)", rep.metal_grams, rep.selected_alloy, rep.triangles);
            println!("threads         : {}", rayon_threads());
        }
    }
    Ok(())
}

fn rayon_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
