//! File formats: native .ojb (JSON with full history), STL (binary), OBJ, 3MF.

use crate::document::{Document, EvalResult, Node};
use crate::mesh::Mesh;
use crate::ops::Role;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;
use glam::DVec3;

pub fn save(doc: &Document, path: &Path) -> Result<(), String> {
    let tmp = path.with_extension("ojb.tmp");
    fs::write(&tmp, doc.to_json()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn load(path: &Path) -> Result<Document, String> {
    let s = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Document::from_json(&s)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Stl,
    Obj,
    ThreeMf,
}

impl ExportFormat {
    pub fn from_path(p: &Path) -> Option<ExportFormat> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "stl" => Some(ExportFormat::Stl),
            "obj" => Some(ExportFormat::Obj),
            "3mf" => Some(ExportFormat::ThreeMf),
            _ => None,
        }
    }
}

/// Named meshes of the final model: metal per node, optionally gems.
pub fn collect(doc: &Document, r: &EvalResult, include_gems: bool) -> Vec<(String, Mesh)> {
    let mut out = Vec::new();
    for (n, o) in r.final_outputs(doc) {
        let metal = Mesh::merged(o.solids.iter().filter(|s| s.role == Role::Metal).map(|s| s.mesh.as_ref()));
        if !metal.is_empty() {
            out.push((safe(n), metal));
        }
        if include_gems && !o.gems.is_empty() {
            let gems = Mesh::merged(o.gems.iter().map(|g| g.world_mesh()).collect::<Vec<_>>().iter());
            out.push((format!("{}_gems", safe(n)), gems));
        }
    }
    out
}

fn safe(n: &Node) -> String {
    let s: String = n.name.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    format!("{}_{}", s, n.id.0)
}

pub fn export(parts: &[(String, Mesh)], path: &Path, fmt: ExportFormat) -> Result<(), String> {
    let f = fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut w = BufWriter::new(f);
    match fmt {
        ExportFormat::Stl => write_stl(parts, &mut w),
        ExportFormat::Obj => write_obj(parts, &mut w),
        ExportFormat::ThreeMf => write_3mf(parts, w),
    }
    .map_err(|e| e.to_string())
}

fn write_stl(parts: &[(String, Mesh)], w: &mut impl Write) -> std::io::Result<()> {
    let mut header = [0u8; 80];
    let tag = b"OpenJewelleryBhy binary STL (mm)";
    header[..tag.len()].copy_from_slice(tag);
    w.write_all(&header)?;
    let n: usize = parts.iter().map(|(_, m)| m.triangles.len()).sum();
    w.write_all(&(n as u32).to_le_bytes())?;
    for (_, m) in parts {
        for i in 0..m.triangles.len() {
            let [a, b, c] = m.tri(i);
            let nrm = (b - a).cross(c - a).normalize_or_zero();
            for v in [nrm, a, b, c] {
                for x in [v.x, v.y, v.z] {
                    w.write_all(&(x as f32).to_le_bytes())?;
                }
            }
            w.write_all(&[0, 0])?;
        }
    }
    Ok(())
}

fn write_obj(parts: &[(String, Mesh)], w: &mut impl Write) -> std::io::Result<()> {
    writeln!(w, "# OpenJewelleryBhy export (units: mm)")?;
    let mut base = 1usize;
    for (name, m) in parts {
        writeln!(w, "o {name}")?;
        for p in &m.positions {
            writeln!(w, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z)?;
        }
        for t in &m.triangles {
            writeln!(w, "f {} {} {}", t[0] as usize + base, t[1] as usize + base, t[2] as usize + base)?;
        }
        base += m.positions.len();
    }
    Ok(())
}

fn write_3mf(parts: &[(String, Mesh)], w: impl Write + std::io::Seek) -> std::io::Result<()> {
    use zip::write::SimpleFileOptions;
    let mut z = zip::ZipWriter::new(w);
    let opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("[Content_Types].xml", opt)?;
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>"#)?;
    z.start_file("_rels/.rels", opt)?;
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>"#)?;
    z.start_file("3D/3dmodel.model", opt)?;
    let mut s = String::with_capacity(1 << 20);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n<metadata name=\"Application\">OpenJewelleryBhy</metadata>\n<resources>\n");
    use std::fmt::Write as _;
    for (i, (name, m)) in parts.iter().enumerate() {
        let esc = name.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
        let _ = writeln!(s, "<object id=\"{}\" type=\"model\" name=\"{}\"><mesh><vertices>", i + 1, esc);
        for p in &m.positions {
            let _ = writeln!(s, "<vertex x=\"{:.5}\" y=\"{:.5}\" z=\"{:.5}\"/>", p.x, p.y, p.z);
        }
        s.push_str("</vertices><triangles>\n");
        for t in &m.triangles {
            let _ = writeln!(s, "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", t[0], t[1], t[2]);
        }
        s.push_str("</triangles></mesh></object>\n");
    }
    s.push_str("</resources>\n<build>\n");
    for i in 0..parts.len() {
        let _ = writeln!(s, "<item objectid=\"{}\"/>", i + 1);
    }
    s.push_str("</build>\n</model>\n");
    z.write_all(s.as_bytes())?;
    z.finish()?;
    Ok(())
}

/// Reads an STL (binary or ASCII) or OBJ file as a welded mesh.
pub fn import_mesh(path: &Path) -> Result<Mesh, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let m = match ext.as_str() {
        "stl" => parse_stl(&bytes)?,
        "obj" => parse_obj(&String::from_utf8_lossy(&bytes))?,
        _ => return Err("only .stl and .obj can be imported".into()),
    };
    if m.triangles.is_empty() {
        return Err("no triangles found".into());
    }
    Ok(m)
}

fn weld(tris: impl Iterator<Item = [DVec3; 3]>) -> Mesh {
    let mut m = Mesh::new();
    let mut map: std::collections::HashMap<[i64; 3], u32> = std::collections::HashMap::new();
    for t in tris {
        let mut idx = [0u32; 3];
        for (k, p) in t.iter().enumerate() {
            let key = [(p.x * 1e5).round() as i64, (p.y * 1e5).round() as i64, (p.z * 1e5).round() as i64];
            idx[k] = *map.entry(key).or_insert_with(|| {
                m.positions.push(*p);
                (m.positions.len() - 1) as u32
            });
        }
        if idx[0] != idx[1] && idx[1] != idx[2] && idx[0] != idx[2] {
            m.triangles.push(idx);
        }
    }
    m
}

fn parse_stl(b: &[u8]) -> Result<Mesh, String> {
    let binary = b.len() >= 84 && {
        let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
        84 + n * 50 == b.len()
    };
    if binary {
        let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
        let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as f64;
        Ok(weld((0..n).map(|i| {
            let o = 84 + i * 50 + 12;
            [0, 1, 2].map(|k| DVec3::new(f(o + k * 12), f(o + k * 12 + 4), f(o + k * 12 + 8)))
        })))
    } else {
        let s = String::from_utf8_lossy(b);
        let mut verts = Vec::new();
        for line in s.lines() {
            let mut it = line.split_whitespace();
            if it.next() == Some("vertex") {
                let v: Vec<f64> = it.filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    verts.push(DVec3::new(v[0], v[1], v[2]));
                }
            }
        }
        if verts.len() < 3 {
            return Err("not a valid STL file".into());
        }
        Ok(weld(verts.chunks_exact(3).map(|c| [c[0], c[1], c[2]])))
    }
}

fn parse_obj(s: &str) -> Result<Mesh, String> {
    let mut v = Vec::new();
    let mut tris = Vec::new();
    for line in s.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let c: Vec<f64> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if c.len() == 3 {
                    v.push(DVec3::new(c[0], c[1], c[2]));
                }
            }
            Some("f") => {
                let idx: Vec<usize> = it
                    .filter_map(|x| x.split('/').next()?.parse::<i64>().ok())
                    .map(|i| if i < 0 { (v.len() as i64 + i) as usize } else { (i - 1) as usize })
                    .collect();
                for k in 1..idx.len().saturating_sub(1) {
                    let t = [idx[0], idx[k], idx[k + 1]];
                    if t.iter().all(|i| *i < v.len()) {
                        tris.push([v[t[0]], v[t[1]], v[t[2]]]);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(weld(tris.into_iter()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stl_size() {
        let m = Mesh::boxed(DVec3::ZERO, DVec3::ONE);
        let dir = std::env::temp_dir().join("ojb_test_export");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("cube.stl");
        export(&[("cube".into(), m.clone())], &p, ExportFormat::Stl).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().len() as usize, 84 + 50 * m.triangles.len());
        let p3 = dir.join("cube.3mf");
        export(&[("cube".into(), m)], &p3, ExportFormat::ThreeMf).unwrap();
        assert!(std::fs::metadata(&p3).unwrap().len() > 200);
        // import round trips
        let back = import_mesh(&p).unwrap();
        assert!((back.volume() - 1.0).abs() < 1e-6);
        let po = dir.join("cube.obj");
        export(&[("cube".into(), Mesh::boxed(DVec3::ZERO, DVec3::ONE))], &po, ExportFormat::Obj).unwrap();
        assert!((import_mesh(&po).unwrap().volume() - 1.0).abs() < 1e-6);
    }
}
