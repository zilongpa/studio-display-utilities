//! The gamma ramp: the per channel lookup table every graphics card has.
//!
//! Works with any graphics card and does not show in screenshots. The table is applied to the signal
//! on its way to the display. In SDR that signal is gamma encoded and a straight line scales a
//! channel; in HDR it is PQ encoded, so the table has to be worked out through the PQ curve
//! (`signal_table`), else it dims far too much and unevenly, and its three channels are BT.2020's
//! (`signal_gains`). Measured with the display's light sensor, the result then agrees with the colour
//! effect on white and on greys.
//! Windows itself puts the ramp back to a straight line whenever the SDR content brightness changes
//! (which is how brightness is set in HDR), and it does so without a trace: reading the ramp back
//! still returns what was written. Writing it again after every step of a brightness fade flickers,
//! so the caller lets the white go for the duration and brings it back afterwards (see
//! `undone_by_hdr_brightness` in the parent module).
//! Full screen games and other colour tools may replace the ramp at any time, and Windows refuses
//! ramps that stray too far from a straight line.

use crate::ffi::*;
use sdu_core::truetone::{self, Xy};
use sdu_core::whitepoint::{signal_table, WhitePoint};
use crate::util::wide;
use std::ptr::{null, null_mut};

/// Runs `work` with a device context of the Studio Display's screen, so that other screens keep
/// their own ramp; of the whole desktop if the display's screen cannot be told apart.
fn with_screen<R>(work: impl FnOnce(HANDLE) -> R) -> R {
    unsafe {
        if let Some(device) = crate::hdr::gdi_device() {
            let dc = CreateDCW(null(), wide(&device).as_ptr(), null(), null());
            if !dc.is_null() {
                let result = work(dc);
                DeleteDC(dc);
                return result;
            }
        }
        let dc = GetDC(null_mut());
        let result = work(dc);
        ReleaseDC(null_mut(), dc);
        result
    }
}

pub struct GammaRamp {
    applied: Option<(i32, i32, bool)>,
    /// The ramp as last written, to notice when something else replaces it.
    written: [u16; 3 * 256],
}

impl GammaRamp {
    pub fn new() -> GammaRamp {
        GammaRamp { applied: None, written: [0; 3 * 256] }
    }

    /// `gains` scale the light of red, green and blue.
    fn apply(&mut self, gains: [f64; 3], hdr: bool) -> Result<(), String> {
        let mut ramp = [0u16; 3 * 256];
        for (channel, &gain) in gains.iter().enumerate() {
            for (level, signal) in signal_table(gain, hdr).iter().enumerate() {
                ramp[channel * 256 + level] = (signal * 65535.0).round() as u16;
            }
        }
        let done = with_screen(|dc| unsafe { SetDeviceGammaRamp(dc, ramp.as_ptr()) });
        if done == 0 {
            return Err("the gamma ramp was refused".into());
        }
        self.written = ramp;
        Ok(())
    }
}

impl Default for GammaRamp {
    fn default() -> GammaRamp {
        GammaRamp::new()
    }
}

impl WhitePoint for GammaRamp {
    fn name(&self) -> &'static str {
        "gamma ramp"
    }

    fn set(&mut self, white: Xy, hdr: bool) -> Result<(), String> {
        let key = ((white.0 * 20000.0).round() as i32, (white.1 * 20000.0).round() as i32, hdr);
        if self.applied == Some(key) {
            return Ok(());
        }
        self.apply(truetone::signal_gains(white, hdr), hdr)?;
        self.applied = Some(key);
        Ok(())
    }

    fn reset(&mut self) -> Result<(), String> {
        if self.applied.take().is_some() {
            self.apply([1.0; 3], false)?;
        }
        Ok(())
    }

    fn keep(&mut self) {
        if self.applied.is_none() {
            return;
        }
        // Reading the ramp back does not show whether it is still in effect: in HDR Windows drops it
        // (on a change of brightness, and at other moments) while still returning what was written.
        // So it is simply written again. One entry of no consequence alternates by one step, so that
        // the write is never taken for a repetition and skipped.
        let mut ramp = self.written;
        ramp[1] ^= 1;
        self.written = ramp;
        with_screen(|dc| unsafe { SetDeviceGammaRamp(dc, ramp.as_ptr()) });
    }
}

impl Drop for GammaRamp {
    fn drop(&mut self) {
        let _ = self.reset();
    }
}
