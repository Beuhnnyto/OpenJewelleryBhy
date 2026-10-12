//! Casting alloys with reference densities (g/cm³). Values are typical
//! published figures; actual densities vary by refinery and should be
//! overridden for production quotes.

use serde::{Deserialize, Serialize};

pub const GRAMS_PER_DWT: f64 = 1.555_173_84;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alloy {
    Gold24k,
    Gold22kYellow,
    Gold18kYellow,
    Gold18kWhite,
    Gold18kRose,
    Gold14kYellow,
    Gold14kWhite,
    Gold14kRose,
    Gold10kYellow,
    Gold10kWhite,
    Platinum950,
    Palladium950,
    Silver925,
    Titanium,
    Wax,
}

impl Alloy {
    pub const ALL: [Alloy; 15] = [
        Alloy::Gold24k,
        Alloy::Gold22kYellow,
        Alloy::Gold18kYellow,
        Alloy::Gold18kWhite,
        Alloy::Gold18kRose,
        Alloy::Gold14kYellow,
        Alloy::Gold14kWhite,
        Alloy::Gold14kRose,
        Alloy::Gold10kYellow,
        Alloy::Gold10kWhite,
        Alloy::Platinum950,
        Alloy::Palladium950,
        Alloy::Silver925,
        Alloy::Titanium,
        Alloy::Wax,
    ];

    pub fn density(self) -> f64 {
        match self {
            Alloy::Gold24k => 19.32,
            Alloy::Gold22kYellow => 17.80,
            Alloy::Gold18kYellow => 15.58,
            Alloy::Gold18kWhite => 14.64,
            Alloy::Gold18kRose => 15.18,
            Alloy::Gold14kYellow => 13.07,
            Alloy::Gold14kWhite => 12.61,
            Alloy::Gold14kRose => 13.00,
            Alloy::Gold10kYellow => 11.57,
            Alloy::Gold10kWhite => 11.07,
            Alloy::Platinum950 => 20.70,
            Alloy::Palladium950 => 12.00,
            Alloy::Silver925 => 10.36,
            Alloy::Titanium => 4.51,
            Alloy::Wax => 0.95,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Alloy::Gold24k => "24K gold",
            Alloy::Gold22kYellow => "22K yellow",
            Alloy::Gold18kYellow => "18K yellow",
            Alloy::Gold18kWhite => "18K white",
            Alloy::Gold18kRose => "18K rose",
            Alloy::Gold14kYellow => "14K yellow",
            Alloy::Gold14kWhite => "14K white",
            Alloy::Gold14kRose => "14K rose",
            Alloy::Gold10kYellow => "10K yellow",
            Alloy::Gold10kWhite => "10K white",
            Alloy::Platinum950 => "Platinum 950",
            Alloy::Palladium950 => "Palladium 950",
            Alloy::Silver925 => "Sterling silver 925",
            Alloy::Titanium => "Titanium",
            Alloy::Wax => "Casting wax",
        }
    }

    /// Display colour (linear RGB) for the viewport.
    pub fn color(self) -> [f32; 3] {
        match self {
            Alloy::Gold24k | Alloy::Gold22kYellow => [1.0, 0.70, 0.25],
            Alloy::Gold18kYellow | Alloy::Gold14kYellow | Alloy::Gold10kYellow => [1.0, 0.76, 0.34],
            Alloy::Gold18kRose | Alloy::Gold14kRose => [0.96, 0.58, 0.45],
            Alloy::Gold18kWhite | Alloy::Gold14kWhite | Alloy::Gold10kWhite => [0.88, 0.87, 0.82],
            Alloy::Platinum950 | Alloy::Palladium950 => [0.82, 0.83, 0.85],
            Alloy::Silver925 => [0.95, 0.95, 0.95],
            Alloy::Titanium => [0.6, 0.6, 0.62],
            Alloy::Wax => [0.2, 0.55, 0.35],
        }
    }

    /// grams for a volume in mm³
    pub fn grams(self, volume_mm3: f64) -> f64 {
        volume_mm3 / 1000.0 * self.density()
    }
}
