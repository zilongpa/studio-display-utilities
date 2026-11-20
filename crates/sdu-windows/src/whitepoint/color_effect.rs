//! The desktop compositor's full screen colour effect (the Magnification API).
//!
//! Works with any graphics card. The compositor multiplies every pixel with a matrix: in SDR the
//! values as stored (gamma encoded), in HDR linear light; measured, the same matrix has about half the
//! effect in HDR. So the matrix is built differently for the two.
//!
//! The effect covers all screens, shows in screenshots and recordings, and there is only one per
//! system: Windows' colour filters and other tools that use it replace it. It ends with this process.

use crate::ffi::*;
use sdu_core::truetone::{self, Xy};
use sdu_core::whitepoint::{encoded_gains, WhitePoint};

pub struct ColorEffect {
    applied: Option<(i32, i32, bool)>,
}

impl ColorEffect {
    pub fn new() -> Result<ColorEffect, String> {
        if unsafe { MagInitialize() } == 0 {
            return Err(format!("the colour effect is not available (error {})", unsafe { GetLastError() }));
        }
        Ok(ColorEffect { applied: None })
    }

    fn apply(&self, matrix: &[[f32; 5]; 5]) -> Result<(), String> {
        if unsafe { MagSetFullscreenColorEffect(matrix.as_ptr() as *const f32) } == 0 {
            return Err(format!("the colour effect was refused (error {})", unsafe { GetLastError() }));
        }
        Ok(())
    }
}

fn identity() -> [[f32; 5]; 5] {
    let mut matrix = [[0.0; 5]; 5];
    for (index, row) in matrix.iter_mut().enumerate() {
        row[index] = 1.0;
    }
    matrix
}

impl WhitePoint for ColorEffect {
    fn name(&self) -> &'static str {
        "colour effect"
    }

    fn set(&mut self, white: Xy, hdr: bool) -> Result<(), String> {
        let key = ((white.0 * 20000.0).round() as i32, (white.1 * 20000.0).round() as i32, hdr);
        if self.applied == Some(key) {
            return Ok(());
        }
        let mut matrix = identity();
        if hdr {
            // Linear light: the full adaptation. A colour is a row vector here, so the matrix is transposed.
            let adaptation = truetone::adaptation(white);
            for input in 0..3 {
                for output in 0..3 {
                    matrix[input][output] = adaptation[output][input] as f32;
                }
            }
        } else {
            // Encoded values cannot be mixed linearly; scaling each channel gets white exactly right.
            let gains = encoded_gains(truetone::white_gains(white));
            for channel in 0..3 {
                matrix[channel][channel] = gains[channel] as f32;
            }
        }
        self.apply(&matrix)?;
        self.applied = Some(key);
        Ok(())
    }

    fn reset(&mut self) -> Result<(), String> {
        if self.applied.take().is_some() {
            self.apply(&identity())?;
        }
        Ok(())
    }
}

impl Drop for ColorEffect {
    fn drop(&mut self) {
        let _ = self.reset();
        unsafe { MagUninitialize() };
    }
}
