//! Studio Display Utilities: everything that is plain calculation.
//!
//! Nothing in this crate touches a device or an operating system, so it builds and is tested
//! anywhere, and a port to another system only has to supply the device access.
//!
//! - `scale`: the macOS brightness slider, position to nits
//! - `auto`: automatic brightness, ambient light to nits
//! - `truetone`: True Tone, ambient colour to the screen's white point
//! - `whitepoint`: the interface of whatever changes the screen's white, and the choice between them
//! - `fade`: gradual changes
//! - `icon`: the program's icon
//! - `edid`: building an EDID from the display's own description

pub mod auto;
pub mod edid;
pub mod fade;
pub mod icon;
pub mod scale;
pub mod truetone;
pub mod whitepoint;
