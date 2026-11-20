//! Which way round Windows draws the Studio Display's picture.

use crate::ffi::*;
use crate::util::wide;
use std::ptr::null_mut;

/// DEVMODEW, as bytes: only a few of its fields are needed.
const DEVMODE_SIZE: usize = 220;
const SIZE: usize = 68; // dmSize: u16
const FIELDS: usize = 72; // dmFields: u32
const ORIENTATION: usize = 84; // dmDisplayOrientation: u32
const WIDTH: usize = 172; // dmPelsWidth: u32
const HEIGHT: usize = 176; // dmPelsHeight: u32
const DM_DISPLAYORIENTATION: u32 = 0x80;
const DM_PELSWIDTH: u32 = 0x8_0000;
const DM_PELSHEIGHT: u32 = 0x10_0000;
const ENUM_CURRENT_SETTINGS: u32 = u32::MAX;
const CDS_UPDATEREGISTRY: u32 = 1;

fn read(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The quarter turns Windows currently applies to the display's picture (0 - 3, Windows' DMDO values).
pub fn current() -> Option<u32> {
    mode().map(|(_, mode)| read(&mode, ORIENTATION))
}

fn mode() -> Option<(Vec<u16>, [u8; DEVMODE_SIZE])> {
    let device = wide(&crate::hdr::gdi_device()?);
    let mut mode = [0u8; DEVMODE_SIZE];
    mode[SIZE..SIZE + 2].copy_from_slice(&(DEVMODE_SIZE as u16).to_le_bytes());
    (unsafe { EnumDisplaySettingsExW(device.as_ptr(), ENUM_CURRENT_SETTINGS, mode.as_mut_ptr(), 0) } != 0).then_some((device, mode))
}

/// Makes Windows draw the picture turned by `quarters` quarter turns (Windows' DMDO values: 0 is
/// landscape, 1 portrait, 2 landscape flipped, 3 portrait flipped). Width and height of the mode are
/// swapped when the picture goes from lying to standing or back.
pub fn set(quarters: u32) -> Result<(), String> {
    let (device, mut mode) = mode().ok_or("the display's current mode cannot be read")?;
    let before = read(&mode, ORIENTATION);
    if before == quarters {
        return Ok(());
    }
    if (before + quarters) % 2 == 1 {
        let (width, height) = (read(&mode, WIDTH), read(&mode, HEIGHT));
        mode[WIDTH..WIDTH + 4].copy_from_slice(&height.to_le_bytes());
        mode[HEIGHT..HEIGHT + 4].copy_from_slice(&width.to_le_bytes());
    }
    mode[ORIENTATION..ORIENTATION + 4].copy_from_slice(&quarters.to_le_bytes());
    mode[FIELDS..FIELDS + 4].copy_from_slice(&(DM_DISPLAYORIENTATION | DM_PELSWIDTH | DM_PELSHEIGHT).to_le_bytes());
    match unsafe { ChangeDisplaySettingsExW(device.as_ptr(), mode.as_ptr(), null_mut(), CDS_UPDATEREGISTRY, null_mut()) } {
        0 => Ok(()),
        status => Err(format!("Windows refused to turn the picture (status {status})")),
    }
}

/// The DMDO value that makes the picture upright when the screen is turned by `degrees` as the
/// orientation sensor counts them (270 is a quarter turn clockwise, seen from the front).
/// Windows' "portrait" (1) draws the picture's top towards the screen's left edge, which is the edge
/// that is up after a clockwise quarter turn.
pub fn for_degrees(degrees: u32) -> Option<u32> {
    match degrees {
        0..=20 | 340..=360 => Some(0),
        250..=290 => Some(1),
        160..=200 => Some(2),
        70..=110 => Some(3),
        _ => None, // in between: wait until the screen has arrived
    }
}
