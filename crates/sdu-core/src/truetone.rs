//! True Tone: which white the screen should show for a given ambient light.
//!
//! Measured on macOS driving a Studio Display XDR (docs/truetone.md). macOS does this itself, the
//! display is not involved: it reads the colour of the ambient light from the display's two sensors
//! and moves the screen's white part of the way towards it.
//!
//! What the measurements show:
//! - The ambient light is the light of both sensors added up, so the brighter one dominates.
//! - The white moves half of the way from D65 towards the ambient light's chromaticity, in the
//!   CIE 1976 u'v' diagram, but no further than a fixed distance: under warm lamps of 2900 to 3600 K
//!   it always ends at the same white, about 5150 K. Over 13 steady states (lamps from 2900 to 5400 K
//!   and greenish daylight) this is within 0.003 in u'v' of what macOS chose.
//! - The colour matters, not only the colour temperature: under greenish light the white turns
//!   greenish too, where a rule on colour temperature alone would turn it pink.
//! - The amount does not depend on how bright the room is, nor on the screen brightness or HDR.
//! - In the dark the sensors report no usable colour; the white stays where it was.
//! - A change takes about five seconds.
//! Light cooler than D65 was not measured; the rule is assumed to hold there too.

/// A chromaticity: CIE 1931 x, y.
pub type Xy = (f64, f64);

/// The white of the display with True Tone doing nothing: D65.
pub const NEUTRAL: Xy = (0.3127, 0.3290);
/// How far the white moves towards the ambient light, in u'v'.
pub const FOLLOW: f64 = 0.5;
/// The furthest the white moves from D65, in u'v'.
pub const MAX_SHIFT: f64 = 0.0191;
/// Seconds for the white to reach a new target.
pub const FADE_SECONDS: f64 = 5.0;
/// Below this the sensors' colour reading is noise, so the white is left where it is.
pub const MIN_LUX: f64 = 3.0;

/// CIE 1976 u'v' of a chromaticity; distances in it are roughly how different colours look.
pub fn uv(xy: Xy) -> (f64, f64) {
    let d = -2.0 * xy.0 + 12.0 * xy.1 + 3.0;
    (4.0 * xy.0 / d, 9.0 * xy.1 / d)
}

pub fn xy_of_uv(uv: (f64, f64)) -> Xy {
    let d = 6.0 * uv.0 - 16.0 * uv.1 + 12.0;
    (9.0 * uv.0 / d, 4.0 * uv.1 / d)
}

/// How different two whites look: their distance in u'v'.
pub fn distance(a: Xy, b: Xy) -> f64 {
    let (a, b) = (uv(a), uv(b));
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The white part of the way from `from` to `to`, on a straight line in u'v'.
pub fn mix(from: Xy, to: Xy, part: f64) -> Xy {
    let (a, b) = (uv(from), uv(to));
    xy_of_uv((a.0 + (b.0 - a.0) * part, a.1 + (b.1 - a.1) * part))
}

/// The ambient light of several sensors as one: each given as (lux, chromaticity). Returns the total
/// lux and the chromaticity of the lights added up; `None` if there is no light or no valid reading.
pub fn combine(readings: &[(f64, Xy)]) -> Option<(f64, Xy)> {
    let (mut x_sum, mut y_sum, mut z_sum) = (0.0, 0.0, 0.0);
    for &(lux, (x, y)) in readings {
        if lux > 0.0 && y > 0.0 {
            x_sum += lux * x / y;
            y_sum += lux;
            z_sum += lux * (1.0 - x - y) / y;
        }
    }
    let total = x_sum + y_sum + z_sum;
    (y_sum > 0.0 && total > 0.0).then(|| (y_sum, (x_sum / total, y_sum / total)))
}

/// The white the screen should show under ambient light of this chromaticity.
pub fn target(ambient: Xy) -> Xy {
    if !(ambient.0 > 0.0 && ambient.1 > 0.0) {
        return NEUTRAL;
    }
    let shift = FOLLOW * distance(NEUTRAL, ambient);
    if shift <= 0.0 {
        return NEUTRAL;
    }
    mix(NEUTRAL, ambient, FOLLOW * (MAX_SHIFT / shift).min(1.0))
}

/// The colour temperature a white is closest to (McCamy's approximation), for showing to people.
pub fn kelvin_of(xy: Xy) -> f64 {
    let n = (xy.0 - 0.3320) / (0.1858 - xy.1);
    437.0 * n * n * n + 3601.0 * n * n + 6861.0 * n + 5517.0
}

/// The whites a plain colour temperature control passes through between 6500 and 5150 K, as measured
/// (the AMD driver's control follows this within a few percent): (kelvin, x, y), coolest first.
const TEMPERATURE_WHITES: [(f64, f64, f64); 5] = [
    (6504.0, 0.3127, 0.3290),
    (5946.0, 0.3227, 0.3396),
    (5516.0, 0.3321, 0.3454),
    (5236.0, 0.3390, 0.3501),
    (5148.0, 0.3413, 0.3512),
];

/// The white of a colour temperature, for controls that only know colour temperatures: interpolated
/// between the measured whites on the mired scale (1 000 000 / kelvin), 5150 to 6500 K.
pub fn white_of_kelvin(kelvin: f64) -> Xy {
    let whites = &TEMPERATURE_WHITES;
    let mired = |kelvin: f64| 1_000_000.0 / kelvin;
    let m = mired(kelvin.clamp(whites[4].0, whites[0].0));
    for pair in whites.windows(2) {
        let (cool, warm) = (pair[0], pair[1]);
        let (a, b) = (mired(cool.0), mired(warm.0));
        if m <= b {
            let t = (m - a) / (b - a);
            return (cool.1 + (warm.1 - cool.1) * t, cool.2 + (warm.2 - cool.2) * t);
        }
    }
    (whites[4].1, whites[4].2)
}

/// The colour temperature, among those `white_of_kelvin` knows, whose white looks closest to `white`.
/// A control with only a colour temperature cannot show a greenish or pinkish white; this is the
/// nearest it can get.
pub fn nearest_kelvin(white: Xy) -> f64 {
    let (coolest, warmest) = (TEMPERATURE_WHITES[0].0, TEMPERATURE_WHITES[4].0);
    let mut best = (f64::MAX, coolest);
    let mut kelvin = warmest;
    while kelvin <= coolest {
        let apart = distance(white_of_kelvin(kelvin), white);
        if apart < best.0 {
            best = (apart, kelvin);
        }
        kelvin += 10.0;
    }
    best.1
}

type Matrix = [[f64; 3]; 3];

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

fn apply(m: &Matrix, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn xyz_of(xy: Xy) -> [f64; 3] {
    [xy.0 / xy.1, 1.0, (1.0 - xy.0 - xy.1) / xy.1]
}

const BRADFORD: Matrix = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
const BRADFORD_INVERSE: Matrix = [[0.9869929, -0.1470543, 0.1599627], [0.4323053, 0.5183603, 0.0492912], [-0.0085287, 0.0400428, 0.9684867]];
/// Linear sRGB (D65) to XYZ and back.
const RGB_TO_XYZ: Matrix = [[0.4124564, 0.3575761, 0.1804375], [0.2126729, 0.7151522, 0.0721750], [0.0193339, 0.1191920, 0.9503041]];
const XYZ_TO_RGB: Matrix = [[3.2404542, -1.5371385, -0.4985314], [-0.9692660, 1.8760108, 0.0415560], [0.0556434, -0.2040259, 1.0572252]];

/// The transform, on linear RGB with sRGB primaries, that turns the display's D65 white into `white`:
/// a Bradford chromatic adaptation, scaled so that no channel has to exceed full.
/// `result[i][j]` is how much input channel `j` contributes to output channel `i`.
pub fn adaptation(white: Xy) -> Matrix {
    let from = apply(&BRADFORD, xyz_of(NEUTRAL));
    let to = apply(&BRADFORD, xyz_of(white));
    let mut scale = [[0.0; 3]; 3];
    for i in 0..3 {
        scale[i][i] = to[i] / from[i];
    }
    let xyz = multiply(&BRADFORD_INVERSE, &multiply(&scale, &BRADFORD));
    let mut rgb = multiply(&XYZ_TO_RGB, &multiply(&xyz, &RGB_TO_XYZ));
    let peak = rgb.iter().map(|row| row.iter().sum::<f64>()).fold(0.0, f64::max);
    for row in rgb.iter_mut() {
        for cell in row.iter_mut() {
            *cell /= peak;
        }
    }
    rgb
}

/// What that transform does to white, per channel, in linear light: red, green, blue between 0 and 1.
pub fn white_gains(white: Xy) -> [f64; 3] {
    adaptation(white).map(|row| row.iter().sum())
}

/// XYZ to linear RGB with the primaries of the HDR signal (ITU-R BT.2020, D65).
const XYZ_TO_REC2020: Matrix = [[1.7166512, -0.3556708, -0.2533663], [-0.6666844, 1.6164812, 0.0157685], [0.0176399, -0.0427706, 0.9421031]];

/// The factors by which to scale the light of the red, green and blue of the signal going to the
/// display, so that its white becomes `white`. The channels of an SDR signal are sRGB's, those of an
/// HDR signal BT.2020's, which are more saturated and so need less scaling for the same white.
pub fn signal_gains(white: Xy, hdr: bool) -> [f64; 3] {
    if !hdr {
        return white_gains(white);
    }
    let rgb = apply(&XYZ_TO_REC2020, xyz_of(white));
    let neutral = apply(&XYZ_TO_REC2020, xyz_of(NEUTRAL));
    let gains = [0, 1, 2].map(|channel| rgb[channel] / neutral[channel]);
    let peak = gains.iter().cloned().fold(0.0, f64::max);
    gains.map(|gain| gain / peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Xy, b: Xy, within: f64) -> bool {
        distance(a, b) < within
    }

    #[test]
    fn matches_what_macos_chose() {
        // ambient light (both sensors: lux, x, y) -> the white macOS settled on (docs/truetone.md)
        let cases: [(&[(f64, Xy)], Xy); 6] = [
            (&[(7840.0, (0.4433, 0.4108)), (63.0, (0.4413, 0.4110))], (0.3413, 0.3512)), // 2940 K lamp
            (&[(7940.0, (0.4120, 0.3943)), (64.0, (0.4089, 0.3920))], (0.3413, 0.3512)), // 3600 K
            (&[(7893.0, (0.3821, 0.3769)), (64.0, (0.3776, 0.3724))], (0.3390, 0.3501)), // 4200 K
            (&[(7926.0, (0.3569, 0.3617)), (63.0, (0.3485, 0.3528))], (0.3321, 0.3454)), // 4800 K
            (&[(8022.0, (0.3337, 0.3475)), (64.0, (0.3213, 0.3383))], (0.3227, 0.3396)), // 5350 K
            (&[(163.0, (0.3221, 0.3552)), (37.0, (0.3914, 0.3791))], (0.3239, 0.3463)),  // greenish daylight
        ];
        for (readings, chosen) in cases {
            let (_, ambient) = combine(readings).unwrap();
            let white = target(ambient);
            assert!(close(white, chosen, 0.003), "{readings:?}: {white:?} is {:.4} from {chosen:?}", distance(white, chosen));
        }
    }

    #[test]
    fn stays_within_its_range() {
        assert_eq!(target(NEUTRAL), NEUTRAL);
        assert_eq!(target((0.0, 0.0)), NEUTRAL);
        assert_eq!(target((f64::NAN, 0.3)), NEUTRAL);
        for ambient in [(0.45, 0.41), (0.25, 0.25), (0.30, 0.45), (0.5, 0.3)] {
            assert!(distance(NEUTRAL, target(ambient)) <= MAX_SHIFT + 1e-9);
        }
        assert!(combine(&[(0.0, (0.3, 0.3)), (5.0, (0.0, 0.0))]).is_none());
        let (lux, xy) = combine(&[(100.0, (0.3, 0.32)), (0.0, (0.5, 0.4))]).unwrap();
        assert!(lux == 100.0 && close(xy, (0.3, 0.32), 1e-9));
    }

    #[test]
    fn mixing_runs_from_one_white_to_the_other() {
        let (a, b) = (NEUTRAL, (0.3413, 0.3512));
        assert!(close(mix(a, b, 0.0), a, 1e-12) && close(mix(a, b, 1.0), b, 1e-12));
        assert!((distance(a, mix(a, b, 0.25)) - 0.25 * distance(a, b)).abs() < 1e-9);
    }

    #[test]
    fn colour_temperatures_map_to_whites_and_back() {
        assert_eq!(white_of_kelvin(6504.0), (0.3127, 0.3290));
        assert_eq!(white_of_kelvin(8000.0), (0.3127, 0.3290));
        assert_eq!(white_of_kelvin(4000.0), (0.3413, 0.3512));
        for kelvin in [5200.0, 5600.0, 6000.0, 6400.0] {
            assert!((nearest_kelvin(white_of_kelvin(kelvin)) - kelvin).abs() <= 10.0);
            assert!((kelvin_of(white_of_kelvin(kelvin)) - kelvin).abs() < 150.0);
        }
        // a greenish white is not as warm as its x alone suggests
        assert!(nearest_kelvin((0.3239, 0.3463)) > 5600.0);
    }

    #[test]
    fn signal_gains_are_gentler_for_the_wider_hdr_primaries() {
        assert!(signal_gains(NEUTRAL, true).iter().all(|g| (g - 1.0).abs() < 1e-9));
        let (sdr, hdr) = (signal_gains((0.3413, 0.3512), false), signal_gains((0.3413, 0.3512), true));
        assert!((hdr[0] - 1.0).abs() < 1e-9 && hdr[1] > sdr[1] && hdr[2] > sdr[2] && hdr[2] < 1.0, "{sdr:?} {hdr:?}");
    }

    #[test]
    fn adaptation_is_neutral_at_d65_and_gives_the_requested_white() {
        let neutral = white_gains(NEUTRAL);
        assert!(neutral.iter().all(|g| (g - 1.0).abs() < 1e-3), "{neutral:?}");
        for wanted in [(0.3413, 0.3512), (0.3239, 0.3463)] {
            let gains = white_gains(wanted);
            assert!(gains.iter().all(|&g| g > 0.5 && g <= 1.0 + 1e-9), "{gains:?}");
            let white = apply(&RGB_TO_XYZ, gains);
            let sum: f64 = white.iter().sum();
            assert!((white[0] / sum - wanted.0).abs() < 2e-4 && (white[1] / sum - wanted.1).abs() < 2e-4);
        }
        // what macOS itself applied for (0.3239, 0.3463): white keeps its green, loses 16 % of its blue
        let gains = white_gains((0.3238581, 0.3463373));
        assert!((gains[1] - 0.991).abs() < 0.01 && (gains[2] - 0.840).abs() < 0.01, "{gains:?}");
    }
}
