// Embeds the payload archive given by OJB_PAYLOAD_ZIP (built by
// scripts/package.sh). Without it the installer is built empty (dev builds).
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=OJB_PAYLOAD_ZIP");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("payload.rs");
    let code = match std::env::var("OJB_PAYLOAD_ZIP") {
        Ok(p) if std::path::Path::new(&p).exists() => {
            println!("cargo:rerun-if-changed={p}");
            format!("pub static PAYLOAD: &[u8] = include_bytes!({p:?});\n")
        }
        _ => "pub static PAYLOAD: &[u8] = &[];\n".to_string(),
    };
    std::fs::write(out, code).unwrap();
}
