//! The program's icon, drawn by formula so that no image file is needed and any size comes out sharp.
//!
//! A rounded square shading from the amber of lamp light to the blue of daylight (the two ends of
//! what True Tone follows), with a white sun for brightness on it. Coloured, so it stands out on a
//! light and on a dark taskbar alike.

/// The colours of the background from the bottom left to the top right corner.
const STOPS: [[f64; 3]; 3] = [[255.0, 177.0, 61.0], [255.0, 107.0, 139.0], [91.0, 124.0, 255.0]];
const SAMPLES: usize = 4;

/// The icon at `size` x `size` pixels, top row first: 0xAARRGGBB with the colour premultiplied by alpha.
pub fn render(size: usize) -> Vec<u32> {
    let mut pixels = Vec::with_capacity(size * size);
    for row in 0..size {
        for column in 0..size {
            // Each pixel is the average of a grid of points inside it, which smooths the edges.
            let (mut red, mut green, mut blue, mut alpha) = (0.0, 0.0, 0.0, 0.0);
            for sample in 0..SAMPLES * SAMPLES {
                let x = (column as f64 + (0.5 + (sample % SAMPLES) as f64) / SAMPLES as f64) / size as f64;
                let y = (row as f64 + (0.5 + (sample / SAMPLES) as f64) / SAMPLES as f64) / size as f64;
                if let Some(colour) = point(x, y) {
                    red += colour[0];
                    green += colour[1];
                    blue += colour[2];
                    alpha += 255.0;
                }
            }
            let part = |sum: f64| (sum / (SAMPLES * SAMPLES) as f64).round() as u32;
            pixels.push(part(alpha) << 24 | part(red) << 16 | part(green) << 8 | part(blue));
        }
    }
    pixels
}

/// The colour at a point of the unit square (x to the right, y down), or nothing outside the shape.
fn point(x: f64, y: f64) -> Option<[f64; 3]> {
    // the rounded square
    const INSET: f64 = 0.04;
    const CORNER: f64 = 0.23;
    let half = 0.5 - INSET;
    let (dx, dy) = ((x - 0.5).abs() - (half - CORNER), (y - 0.5).abs() - (half - CORNER));
    if dx > CORNER || dy > CORNER || (dx > 0.0 && dy > 0.0 && dx.hypot(dy) > CORNER) {
        return None;
    }

    // the sun: a disc and eight rays with round ends
    let (sx, sy) = (x - 0.5, y - 0.5);
    let radius = sx.hypot(sy);
    let mut sun = radius < 0.155;
    if !sun {
        let step = std::f64::consts::FRAC_PI_4;
        let nearest = (sy.atan2(sx) / step).round() * step;
        // the point as seen along the nearest ray: how far out, and how far to the side
        let (along, across) = (sx * nearest.cos() + sy * nearest.sin(), -sx * nearest.sin() + sy * nearest.cos());
        let (start, end, width) = (0.235, 0.335, 0.034);
        let beyond = (start - along).max(along - end).max(0.0);
        sun = beyond.hypot(across) < width;
    }
    if sun {
        return Some([255.0; 3]);
    }

    // the background: from the bottom left to the top right corner
    let t = ((x + (1.0 - y)) / 2.0).clamp(0.0, 1.0) * (STOPS.len() - 1) as f64;
    let index = (t as usize).min(STOPS.len() - 2);
    let (from, to, part) = (STOPS[index], STOPS[index + 1], t - index as f64);
    Some([0, 1, 2].map(|channel| from[channel] + (to[channel] - from[channel]) * part))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_clear_corners_a_white_centre_and_a_coloured_body() {
        let size = 32;
        let icon = render(size);
        assert_eq!(icon.len(), size * size);
        assert_eq!(icon[0], 0, "the corner is outside the rounded square");
        assert_eq!(icon[size / 2 * size + size / 2], 0xFFFF_FFFF, "the sun's disc is white");
        let body = icon[(size - 5) * size + 4]; // bottom left: amber
        assert!(body >> 24 == 0xFF && (body >> 16) & 0xFF > 200 && body & 0xFF < 120, "{body:08X}");
        let body = icon[4 * size + size - 5]; // top right: blue
        assert!(body >> 24 == 0xFF && body & 0xFF > 200 && (body >> 16) & 0xFF < 150, "{body:08X}");
    }
}
