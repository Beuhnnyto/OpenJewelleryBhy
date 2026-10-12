//! StyleSheets: named parameter presets per builder type, stored in the
//! user's configuration directory (`~/.config/openjewellerybhy/styles.json`
//! on Linux, `%APPDATA%\OpenJewelleryBhy\styles.json` on Windows).

use ojb_core::document::Op;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Parameters that reference other nodes are never part of a style.
const REFS: [&str; 9] = ["rail", "gem", "gems", "shank", "source", "targets", "tools", "placement", "gem_index"];

pub fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("OpenJewelleryBhy"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("openjewellerybhy"))
    }
}

#[derive(Default)]
pub struct StyleStore {
    styles: BTreeMap<String, BTreeMap<String, Value>>,
}

impl StyleStore {
    fn path() -> Option<PathBuf> {
        config_dir().map(|d| d.join("styles.json"))
    }

    pub fn load() -> StyleStore {
        let styles = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        StyleStore { styles }
    }

    fn persist(&self) -> Result<(), String> {
        let p = Self::path().ok_or("no configuration directory")?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(p, serde_json::to_string_pretty(&self.styles).unwrap_or_default()).map_err(|e| e.to_string())
    }

    pub fn names(&self, kind: &str) -> Vec<String> {
        self.styles.get(kind).map(|m| m.keys().cloned().collect()).unwrap_or_default()
    }

    pub fn save(&mut self, kind: &str, name: &str, op: &Op) -> Result<(), String> {
        let v = serde_json::to_value(op).map_err(|e| e.to_string())?;
        let mut params = v.get("params").and_then(|p| p.as_object()).cloned().unwrap_or_default();
        params.retain(|k, _| !REFS.contains(&k.as_str()));
        self.styles.entry(kind.to_string()).or_default().insert(name.to_string(), Value::Object(params));
        self.persist()
    }

    /// Merges the style's parameters into `op`, keeping its references.
    pub fn apply(&self, kind: &str, name: &str, op: &mut Op) -> Result<(), String> {
        let style = self.styles.get(kind).and_then(|m| m.get(name)).and_then(|v| v.as_object()).ok_or("unknown style")?;
        let mut v = serde_json::to_value(&*op).map_err(|e| e.to_string())?;
        let params = v.get_mut("params").and_then(|p| p.as_object_mut()).ok_or("bad op")?;
        merge(params, style);
        *op = serde_json::from_value(v).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn merge(dst: &mut Map<String, Value>, src: &Map<String, Value>) {
    for (k, v) in src {
        if !REFS.contains(&k.as_str()) {
            dst.insert(k.clone(), v.clone());
        }
    }
}
