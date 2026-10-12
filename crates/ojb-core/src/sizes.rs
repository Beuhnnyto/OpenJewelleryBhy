//! Finger size systems. The canonical value is the inner diameter in mm.
//!
//! Sources: US/CA (ANSI, 0.032" per size, size 0 = 11.63 mm), UK/AU
//! (BS 6820: A = 37.50 mm circumference, +1.25 mm per letter), ISO 8653
//! (circumference in mm, also used in France), Germany (inner diameter in
//! mm), Switzerland/Italy/Spain (circumference − 40), Japan/China
//! (approximation calibrated on JP 1 ≈ 41.7 mm, JP 9 ≈ 49.3 mm).

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Us,
    Uk,
    Iso,
    Fr,
    De,
    Ch,
    Jp,
    DiameterMm,
    DiameterIn,
    CircumferenceMm,
    CircumferenceIn,
}

impl Region {
    pub const ALL: [Region; 11] = [
        Region::Us,
        Region::Uk,
        Region::Iso,
        Region::Fr,
        Region::De,
        Region::Ch,
        Region::Jp,
        Region::DiameterMm,
        Region::DiameterIn,
        Region::CircumferenceMm,
        Region::CircumferenceIn,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Region::Us => "US / Canada",
            Region::Uk => "UK / Australia",
            Region::Iso => "EU (ISO 8653)",
            Region::Fr => "France",
            Region::De => "Germany",
            Region::Ch => "Switzerland / Italy / Spain",
            Region::Jp => "Japan / China (approx.)",
            Region::DiameterMm => "Diameter (mm)",
            Region::DiameterIn => "Diameter (in)",
            Region::CircumferenceMm => "Circumference (mm)",
            Region::CircumferenceIn => "Circumference (in)",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Region::Us => "us",
            Region::Uk => "uk",
            Region::Iso => "iso",
            Region::Fr => "fr",
            Region::De => "de",
            Region::Ch => "ch",
            Region::Jp => "jp",
            Region::DiameterMm => "dmm",
            Region::DiameterIn => "din",
            Region::CircumferenceMm => "cmm",
            Region::CircumferenceIn => "cin",
        }
    }

    pub fn from_code(s: &str) -> Option<Region> {
        let s = s.to_ascii_lowercase();
        let alias = match s.as_str() {
            "ca" => "us",
            "au" => "uk",
            "eu" => "iso",
            "it" | "es" => "ch",
            "cn" | "hk" => "jp",
            other => other,
        };
        Region::ALL.into_iter().find(|r| r.code() == alias)
    }

    /// Inner diameter (mm) for a numeric size value. For the UK, the value is
    /// the letter index (A = 1, A½ = 1.5, … Z = 26, Z+1 = 27…).
    pub fn to_diameter(self, v: f64) -> f64 {
        match self {
            Region::Us => 11.63 + 0.8128 * v,
            Region::Uk => (37.5 + (v - 1.0) * 1.25) / PI,
            Region::Iso | Region::Fr | Region::CircumferenceMm => v / PI,
            Region::De | Region::DiameterMm => v,
            Region::Ch => (v + 40.0) / PI,
            Region::Jp => (40.75 + 0.95 * v) / PI,
            Region::DiameterIn => v * 25.4,
            Region::CircumferenceIn => v * 25.4 / PI,
        }
    }

    pub fn from_diameter(self, d: f64) -> f64 {
        match self {
            Region::Us => (d - 11.63) / 0.8128,
            Region::Uk => (d * PI - 37.5) / 1.25 + 1.0,
            Region::Iso | Region::Fr | Region::CircumferenceMm => d * PI,
            Region::De | Region::DiameterMm => d,
            Region::Ch => d * PI - 40.0,
            Region::Jp => (d * PI - 40.75) / 0.95,
            Region::DiameterIn => d / 25.4,
            Region::CircumferenceIn => d * PI / 25.4,
        }
    }

    /// Human readable size, snapped to the system's usual step.
    pub fn format(self, v: f64) -> String {
        match self {
            Region::Us => format!("{}", (v * 4.0).round() / 4.0),
            Region::Uk => uk_letter(v),
            Region::Iso | Region::Fr | Region::Ch | Region::Jp => format!("{:.0}", v.round()),
            Region::De | Region::DiameterMm | Region::CircumferenceMm => format!("{:.2}", v),
            Region::DiameterIn | Region::CircumferenceIn => format!("{:.3}", v),
        }
    }

    /// Parses a size; accepts UK letters like "N", "N½", "N1/2", "Z+2".
    pub fn parse(self, s: &str) -> Option<f64> {
        let s = s.trim();
        if self == Region::Uk {
            return parse_uk(s);
        }
        let s = s.replace(',', ".").replace('½', ".5").replace(" 1/2", ".5").replace("1/4", ".25").replace("3/4", ".75");
        s.parse().ok()
    }
}

fn uk_letter(v: f64) -> String {
    let half = (v * 2.0).round() / 2.0;
    let idx = half.floor() as i64;
    let frac = if half.fract() > 0.0 { "½" } else { "" };
    if idx < 1 {
        return format!("<A ({half})");
    }
    if idx <= 26 {
        format!("{}{}", (b'A' + (idx - 1) as u8) as char, frac)
    } else {
        format!("Z+{}{}", idx - 26, frac)
    }
}

fn parse_uk(s: &str) -> Option<f64> {
    let up = s.to_ascii_uppercase();
    let mut chars = up.chars();
    let c = chars.next()?;
    if !c.is_ascii_uppercase() {
        return s.parse().ok();
    }
    let mut v = (c as u8 - b'A' + 1) as f64;
    let rest: String = chars.collect::<String>().trim().to_string();
    let rest = rest.as_str();
    if let Some(plus) = rest.strip_prefix('+') {
        let (n, half) = match plus.strip_suffix('½') {
            Some(n) => (n, 0.5),
            None => (plus, 0.0),
        };
        v += n.trim().parse::<f64>().ok()? + half;
    } else if rest == "½" || rest == "1/2" || rest == ".5" {
        v += 0.5;
    } else if !rest.is_empty() {
        return None;
    }
    Some(v)
}

/// A finger size with region, stored as given by the user.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FingerSize {
    pub region: Region,
    pub value: f64,
}

impl FingerSize {
    pub fn us(v: f64) -> Self {
        FingerSize { region: Region::Us, value: v }
    }
    pub fn diameter(&self) -> f64 {
        self.region.to_diameter(self.value)
    }
    pub fn display(&self) -> String {
        format!("{} {}", self.region.code().to_uppercase(), self.region.format(self.value))
    }
    /// Re-express the same diameter in another region.
    pub fn convert(&self, to: Region) -> FingerSize {
        FingerSize { region: to, value: to.from_diameter(self.diameter()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us7() {
        assert!((Region::Us.to_diameter(7.0) - 17.3196).abs() < 1e-3);
    }

    #[test]
    fn roundtrip_all() {
        for r in Region::ALL {
            let d = 17.5;
            assert!((r.to_diameter(r.from_diameter(d)) - d).abs() < 1e-9, "{:?}", r);
        }
    }

    #[test]
    fn uk_letters() {
        assert_eq!(Region::Uk.parse("N"), Some(14.0));
        assert_eq!(Region::Uk.parse("N½"), Some(14.5));
        assert_eq!(Region::Uk.parse("Z+2"), Some(28.0));
        assert_eq!(Region::Uk.format(14.5), "N½");
        // UK N ≈ US 6¾
        let us = Region::Us.from_diameter(Region::Uk.to_diameter(14.0));
        assert!((us - 6.75).abs() < 0.3, "{us}");
    }

    #[test]
    fn jp9_is_us5() {
        let us = Region::Us.from_diameter(Region::Jp.to_diameter(9.0));
        assert!((us - 5.0).abs() < 0.25, "{us}");
    }
}
