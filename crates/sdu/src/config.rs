//! Settings, kept in `%APPDATA%\sdu\config.txt` as `key=value` lines.

use sdu_core::whitepoint::Method;
use sdu_windows::util::data_dir;
use std::fs;

#[derive(Clone, Copy, PartialEq)]
pub struct Config {
    /// Follow the ambient light sensor.
    pub auto: bool,
    /// The user's adjustment of automatic brightness: the curve's result is multiplied by this.
    /// The brightness keys change it, so automatic brightness keeps working around the chosen level.
    pub factor: f64,
    /// Follow the colour of the ambient light with the screen's white.
    pub truetone: bool,
    /// How the white is changed.
    pub whitepoint: Method,
    /// Keep HDR switched on: on an AMD card that means giving the driver the EDID again whenever it
    /// has forgotten it (after a restart, or when the display was plugged in again).
    pub hdr: bool,
    /// Turn the picture when the screen is turned on its stand.
    pub rotate: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config { auto: true, factor: 1.0, truetone: false, whitepoint: Method::Automatic, hdr: false, rotate: true }
    }
}

pub const MIN_FACTOR: f64 = 0.05;
pub const MAX_FACTOR: f64 = 8.0;

impl Config {
    pub fn load() -> Config {
        let mut config = Config::default();
        let Ok(text) = fs::read_to_string(data_dir().join("config.txt")) else { return config };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            match (key.trim(), value.trim()) {
                ("auto", value) => config.auto = value != "0",
                ("factor", value) => {
                    if let Ok(factor) = value.parse::<f64>() {
                        if factor.is_finite() {
                            config.factor = factor.clamp(MIN_FACTOR, MAX_FACTOR);
                        }
                    }
                }
                ("truetone", value) => config.truetone = value != "0",
                ("rotate", value) => config.rotate = value != "0",
                ("hdr", value) => config.hdr = value != "0",
                ("whitepoint", value) => config.whitepoint = Method::from_key(value).unwrap_or(Method::Automatic),
                _ => {}
            }
        }
        config
    }

    pub fn save(&self) {
        let text = format!("auto={}\nfactor={:.4}\ntruetone={}\nwhitepoint={}\nhdr={}\nrotate={}\n", self.auto as u8, self.factor, self.truetone as u8, self.whitepoint.key(), self.hdr as u8, self.rotate as u8);
        let _ = fs::write(data_dir().join("config.txt"), text);
    }
}
