//! The brightness scale of macOS for this display: slider position -> nits.
//!
//! Measured on a Mac driving the Studio Display XDR (macOS 27.0.1) with `tools/measure-mac.swift`:
//! the slider was set to 65 evenly spaced positions through the same API the brightness keys use, and
//! the backlight was read back from the display in nits each time. `NITS[i]` is the brightness at
//! slider position `i / 64`. Positions in between are interpolated linearly.

const NITS: [f64; 65] = [
    4.0, 7.58, 9.85, 12.03, 14.24, 16.52, 18.89, 21.36, 23.95, 26.67, 29.52, 32.52, 35.66, 38.96, 42.42, 46.04,
    49.85, 53.83, 58.0, 62.36, 66.93, 71.7, 76.68, 81.89, 87.33, 93.0, 98.92, 105.09, 111.52, 118.22, 125.19, 132.44,
    140.0, 147.85, 156.01, 164.49, 173.31, 182.46, 191.96, 201.82, 212.05, 222.67, 233.67, 245.07, 256.89, 269.14, 281.82, 294.95,
    308.54, 322.61, 337.16, 352.22, 367.78, 383.88, 400.52, 417.71, 435.47, 453.82, 472.77, 492.34, 512.54, 533.39, 554.9, 577.1,
    600.0,
];

const LAST: usize = NITS.len() - 1;

/// Brightness in nits at a slider position (0 - 1).
pub fn nits_at(slider: f64) -> f64 {
    let position = slider.clamp(0.0, 1.0) * LAST as f64;
    let index = (position.floor() as usize).min(LAST - 1);
    NITS[index] + (NITS[index + 1] - NITS[index]) * (position - index as f64)
}

/// Slider position (0 - 1) that gives this brightness; the inverse of `nits_at`.
pub fn slider_of(nits: f64) -> f64 {
    let nits = nits.clamp(NITS[0], NITS[LAST]);
    let index = NITS.partition_point(|&value| value <= nits).clamp(1, LAST) - 1;
    (index as f64 + (nits - NITS[index]) / (NITS[index + 1] - NITS[index])) / LAST as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_measurement() {
        assert_eq!(nits_at(0.0), 4.0);
        assert_eq!(nits_at(0.5), 140.0);
        assert_eq!(nits_at(1.0), 600.0);
        assert!((nits_at(0.5625) - 173.31).abs() < 1e-9);
    }

    #[test]
    fn inverse_round_trips() {
        for step in 0..=1000 {
            let slider = step as f64 / 1000.0;
            assert!((slider_of(nits_at(slider)) - slider).abs() < 1e-9, "at {slider}");
        }
        assert_eq!(slider_of(1.0), 0.0);
        assert_eq!(slider_of(2000.0), 1.0);
    }
}
