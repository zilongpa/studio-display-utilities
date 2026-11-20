//! Studio Display Utilities on Windows: the display's USB interfaces, Windows' own brightness and HDR
//! controls, and the AMD driver.
#![cfg(windows)]

pub mod amd;
pub mod autostart;
pub mod backlight;
pub mod elevate;
pub mod ffi;
pub mod hdr;
pub mod orientation;
pub mod presets;
pub mod rotation;
pub mod sensor;
pub mod system;
pub mod util;
pub mod whitepoint;
pub mod winusb;
