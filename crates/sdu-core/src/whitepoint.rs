//! Ways of changing the white the screen shows.
//!
//! True Tone decides which white is wanted (see `truetone`); a `WhitePoint` makes it happen. There are
//! several ways to do that on a given system, each with its own drawbacks, so the choice is left to
//! the user: the platform code offers what it has, and `Method` is what gets stored in the settings.

use crate::truetone::Xy;

/// One way of changing the screen's white.
pub trait WhitePoint {
    /// Short name for logs.
    fn name(&self) -> &'static str;

    /// Moves the white to this chromaticity (CIE 1931 x, y), or as near as the method gets. `hdr`
    /// says whether the display is in HDR right now, which changes how some methods have to do it.
    fn set(&mut self, white: Xy, hdr: bool) -> Result<(), String>;

    /// Undoes everything, back to the display's own white.
    fn reset(&mut self) -> Result<(), String>;

    /// Called every second or so: puts the white back if something else has undone it.
    fn keep(&mut self) {}
}

/// Which way the user wants. `Automatic` lets the program pick: the gamma ramp, which shows the
/// exact white on any graphics card, in SDR and HDR, without tinting screenshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Automatic,
    /// The graphics driver's colour temperature control. Applied at the output, so screenshots are
    /// not tinted, and it behaves the same in SDR and HDR. Only where the driver has one (AMD), and
    /// it only knows colour temperatures: a greenish or pinkish white becomes the nearest plain one.
    Driver,
    /// A colour effect applied by the desktop compositor. Works with any graphics card, in SDR and
    /// HDR, but screenshots, recordings and remote sessions show the tint, and it is the single such
    /// effect the system has: other colour tools and Windows' colour filters replace it.
    ColorEffect,
    /// The classic gamma ramp, the lookup table at the graphics card's output. Works with any
    /// graphics card, does not tint screenshots; games and other colour tools may replace it.
    GammaRamp,
}

impl Method {
    pub const ALL: [Method; 4] = [Method::Automatic, Method::Driver, Method::ColorEffect, Method::GammaRamp];

    /// The name used in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Method::Automatic => "auto",
            Method::Driver => "driver",
            Method::ColorEffect => "color-effect",
            Method::GammaRamp => "gamma-ramp",
        }
    }

    pub fn from_key(key: &str) -> Option<Method> {
        Method::ALL.into_iter().find(|method| method.key() == key)
    }

    /// What to call it in a menu, with its main drawback.
    pub fn label(self) -> &'static str {
        match self {
            Method::Automatic => "Automatic (gamma ramp)",
            Method::Driver => "Graphics driver (AMD; colour temperature only)",
            Method::ColorEffect => "Colour effect (exact; tints screenshots)",
            Method::GammaRamp => "Gamma ramp (exact)",
        }
    }
}

/// Per-channel factors for values as they are stored in an SDR frame (gamma encoded), given the
/// factors wanted in linear light. SDR methods multiply encoded values, so they need these.
pub fn encoded_gains(linear: [f64; 3]) -> [f64; 3] {
    linear.map(|gain| gain.clamp(0.0, 1.0).powf(1.0 / 2.2))
}

/// The HDR signal's transfer function (SMPTE ST 2084, "PQ"): signal value 0 - 1 to light, where 1
/// is 10 000 nits, and back.
fn pq_to_light(signal: f64) -> f64 {
    let p = signal.max(0.0).powf(1.0 / 78.84375);
    ((p - 0.8359375).max(0.0) / (18.8515625 - 18.6875 * p)).powf(1.0 / 0.1593017578125)
}

fn light_to_pq(light: f64) -> f64 {
    let y = light.max(0.0).powf(0.1593017578125);
    ((0.8359375 + 18.8515625 * y) / (1.0 + 18.6875 * y)).powf(78.84375)
}

/// A lookup table for one channel that scales its light by `gain`, for a table that is applied to
/// the signal as it goes to the display: 256 entries, signal in, signal out, both 0 - 1.
/// In SDR the signal is gamma encoded, so the table is a straight line. In HDR the signal is PQ
/// encoded: there a straight line would dim bright and dark parts by different amounts (and far too
/// much), so each entry is decoded to light, scaled, and encoded again.
pub fn signal_table(gain: f64, hdr: bool) -> [f64; 256] {
    let gain = gain.clamp(0.0, 1.0);
    let mut table = [0.0; 256];
    for (index, entry) in table.iter_mut().enumerate() {
        let signal = index as f64 / 255.0;
        *entry = if gain >= 1.0 {
            signal
        } else if hdr {
            light_to_pq(gain * pq_to_light(signal))
        } else {
            signal * gain.powf(1.0 / 2.2)
        };
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_keys_round_trip() {
        for method in Method::ALL {
            assert_eq!(Method::from_key(method.key()), Some(method));
        }
        assert_eq!(Method::from_key("nonsense"), None);
    }

    #[test]
    fn pq_round_trips_and_matches_known_points() {
        for signal in [0.0, 0.1, 0.5, 0.75, 1.0] {
            assert!((light_to_pq(pq_to_light(signal)) - signal).abs() < 1e-5);
        }
        // 100 nits is signal 0.508, 1000 nits 0.752 (ST 2084)
        assert!((light_to_pq(0.01) - 0.5081).abs() < 1e-3 && (light_to_pq(0.1) - 0.7518).abs() < 1e-3);
    }

    #[test]
    fn signal_tables_scale_the_light() {
        let sdr = signal_table(0.5, false);
        assert!((sdr[255].powf(2.2) - 0.5).abs() < 1e-9 && sdr[0] == 0.0);
        let hdr = signal_table(0.5, true);
        for index in [64, 128, 200, 255] {
            let (before, after) = (pq_to_light(index as f64 / 255.0), pq_to_light(hdr[index]));
            assert!((after / before - 0.5).abs() < 1e-9, "entry {index}");
        }
        assert_eq!(signal_table(1.0, true)[128], 128.0 / 255.0);
    }

    #[test]
    fn encoded_gains_undo_the_display_gamma() {
        let gains = encoded_gains([1.0, 0.5, 0.25]);
        assert_eq!(gains[0], 1.0);
        assert!((gains[1].powf(2.2) - 0.5).abs() < 1e-12 && (gains[2].powf(2.2) - 0.25).abs() < 1e-12);
    }
}
