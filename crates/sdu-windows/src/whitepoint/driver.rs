//! The AMD driver's colour temperature for the display.
//!
//! The driver applies it at the output, after Windows has composed the picture, so it looks the same
//! in SDR and HDR and does not show in screenshots. The driver accepts 4000 to 10000 K in steps of
//! 100 K, so a fade moves in small steps rather than continuously, and it knows nothing but colour
//! temperatures: a white that is greenish or pinkish is shown as the plain white nearest to it.
//! Photographed with a locked white balance next to the colour effect (whose result is known), its
//! whites between 5000 and 6500 K are within a few percent of what `truetone::white_of_kelvin` says.
//!
//! The setting belongs to the driver and outlives this program, so a file marks that it is changed,
//! and `reset` (or, after a crash, the next start) puts it back to the driver's default, which is the
//! display's own white. Not to "what it was before": the driver remembers the last value per display
//! and brings it back when the display is plugged in again, so what is read then may be our own value.

use crate::amd::{self, Temperature};
use crate::util::{data_dir, log};
use sdu_core::truetone::{self, Xy};
use sdu_core::whitepoint::WhitePoint;
use std::fs;
use std::path::PathBuf;

pub struct Driver {
    display: amd::Display,
    range: Temperature,
    /// Where the driver took the colour temperature from before this program touched it.
    source: i32,
    applied: Option<i32>,
}

fn marker() -> PathBuf {
    data_dir().join("whitepoint-driver.txt")
}

/// Exists once this program has ever changed the driver's colour temperature on this machine.
fn used() -> PathBuf {
    data_dir().join("whitepoint-driver-used.txt")
}

/// Puts the driver's colour temperature back to its default if this program has used it before and
/// is not using it now. Needed because the driver keeps a value per display and brings it back when
/// that display is detected again (plugged in, or its description changed): a value this program
/// set long ago can reappear with nobody asking for it. Call at start-up and after a display change.
pub fn settle() {
    if !used().exists() {
        return;
    }
    let Ok(display) = amd::Display::find() else { return };
    if let Ok(range) = display.temperature() {
        if range.current != range.default {
            let done = display.set_temperature(range.default);
            log(&format!("white point (driver): the driver brought back {} K, set to {} K again: {done:?}", range.current, range.default));
        }
    }
}

impl Driver {
    pub fn new() -> Result<Driver, String> {
        let display = amd::Display::find()?;
        let range = display.temperature()?;
        let source = display.temperature_source()?;
        let mut driver = Driver { display, range, source, applied: None };
        // A marker left behind means the program ended without putting the setting back.
        if let Some(source) = read_marker() {
            log(&format!("white point (driver): {} K was left behind by an earlier run, back to {} K", range.current, range.default));
            driver.source = source;
            driver.restore()?;
        }
        Ok(driver)
    }

    fn restore(&mut self) -> Result<(), String> {
        self.display.set_temperature(self.range.default)?;
        if self.display.temperature_source() != Ok(self.source) {
            self.display.set_temperature_source(self.source)?;
        }
        let _ = fs::remove_file(marker());
        self.applied = None;
        Ok(())
    }
}

/// The colour temperature source noted in the marker file.
fn read_marker() -> Option<i32> {
    fs::read_to_string(marker()).ok()?.trim().parse().ok()
}

impl WhitePoint for Driver {
    fn name(&self) -> &'static str {
        "driver"
    }

    fn set(&mut self, white: Xy, _hdr: bool) -> Result<(), String> {
        let Temperature { min, max, step, .. } = self.range;
        let kelvin = truetone::nearest_kelvin(white);
        let kelvin = ((kelvin / step as f64).round() as i32 * step).clamp(min, max);
        if self.applied == Some(kelvin) {
            return Ok(());
        }
        if self.applied.is_none() {
            let _ = fs::write(used(), "1\n");
            fs::write(marker(), format!("{}\n", self.source)).map_err(|error| format!("cannot note the driver's setting: {error}"))?;
            if self.source != amd::TEMPERATURE_FROM_USER {
                self.display.set_temperature_source(amd::TEMPERATURE_FROM_USER)?;
            }
        }
        self.display.set_temperature(kelvin)?;
        self.applied = Some(kelvin);
        Ok(())
    }

    fn reset(&mut self) -> Result<(), String> {
        if self.applied.is_none() {
            return Ok(());
        }
        self.restore()
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        if let Err(error) = self.reset() {
            log(&format!("white point (driver): could not put the setting back: {error}"));
        }
    }
}
