//! The ways Windows offers to change the white the screen shows (see `sdu_core::whitepoint`).
//!
//! What was tried and measured is in docs/truetone.md. In short:
//! - `gamma_ramp`: the gamma ramp. Exact, in SDR and HDR, on any graphics card; the default.
//! - `color_effect`: the compositor's full screen colour effect. Exact too, but shows in screenshots.
//! - `driver`: the AMD driver's colour temperature. Cannot show a greenish or pinkish white.
//! Two more exist in Windows but could not be made to work on the test machine and are not offered:
//! an ICC profile with an MHC2 tag, and the Night Light setting.

mod color_effect;
mod driver;
mod gamma_ramp;

pub use color_effect::ColorEffect;
pub use driver::{settle as settle_driver, Driver};
pub use gamma_ramp::GammaRamp;

use sdu_core::whitepoint::{Method, WhitePoint};

/// Opens the way the user chose. `Automatic` is the gamma ramp: exact, in SDR and HDR, on any
/// graphics card, and screenshots stay clean.
pub fn create(method: Method) -> Result<Box<dyn WhitePoint>, String> {
    Ok(match method {
        Method::Automatic | Method::GammaRamp => Box::new(GammaRamp::new()),
        Method::Driver => Box::new(Driver::new()?),
        Method::ColorEffect => Box::new(ColorEffect::new()?),
    })
}

/// Whether changing the brightness in HDR silently undoes what this method set (Windows resets the
/// gamma ramp whenever the SDR content brightness changes). For such a method the white has to be
/// written again after every change of brightness.
pub fn undone_by_hdr_brightness(method: Method) -> bool {
    matches!(method, Method::Automatic | Method::GammaRamp)
}

/// The choices that make sense on this system, for a menu.
pub fn available() -> Vec<Method> {
    let mut methods = vec![Method::Automatic, Method::GammaRamp, Method::ColorEffect];
    if Driver::new().is_ok() {
        methods.push(Method::Driver);
    }
    methods
}
