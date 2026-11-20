//! Automatic brightness: how bright the screen should be for a given ambient light.
//!
//! Measured on macOS with its default settings by changing the room light and logging what macOS did
//! (docs/macos-auto-brightness-default-curve.csv): a floor in the dark, then a straight line in lux,
//! where lux is the brighter of the display's two light sensors. The steady states were
//! 0 lux -> 41 nits, 24 -> 67, 70 -> 119, and about 990 lux and above -> 600 nits (the maximum).
//! Nothing was measured between 70 and 990 lux; the line is assumed to hold there.

pub const DARK_NITS: f64 = 41.29;
pub const NITS_PER_LUX: f64 = 1.08;
pub const MAX_NITS: f64 = 600.0;

/// Seconds a brightness change takes, as macOS does it (docs/macos-auto-brightness-log.csv): at a
/// steady pace along the brightness slider, about the same time whatever the distance.
pub const FADE_MANUAL: f64 = 0.3;
pub const FADE_BRIGHTER: f64 = 2.0;
pub const FADE_DARKER: f64 = 5.0;

/// Brightness in nits for this ambient light, before any adjustment by the user.
pub fn brightness(lux: f64) -> f64 {
    (DARK_NITS + NITS_PER_LUX * lux.max(0.0)).min(MAX_NITS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_measured_points() {
        assert!((brightness(0.0) - 41.29).abs() < 0.01);
        assert!((brightness(24.0) - 66.59).abs() < 1.5);
        assert!((brightness(70.0) - 119.39).abs() < 3.0);
        assert_eq!(brightness(990.0), 600.0);
        assert_eq!(brightness(-5.0), brightness(0.0));
    }
}
