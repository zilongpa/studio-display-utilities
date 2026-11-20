//! Brightness while Windows drives the display in HDR.
//!
//! In HDR mode the display follows the absolute luminance of the signal and its backlight setting has
//! no visible effect. What makes the desktop brighter or darker is the level at which Windows composes
//! SDR content: "SDR content brightness" in Settings, 80 - 480 nits.
//!
//! Reading it is documented (DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL). Writing uses the request
//! Settings itself sends, which is not in the public headers: type 0xFFFFFFEE with
//! { header, ULONG level, BYTE finalValue }. The level is in units of 80/1000 nit.

use crate::ffi::*;
use std::ptr::null_mut;

pub const MIN_NITS: f64 = 80.0;
pub const MAX_NITS: f64 = 480.0;

const QDC_ONLY_ACTIVE_PATHS: u32 = 2;
const PATH_SIZE: usize = 72; // DISPLAYCONFIG_PATH_INFO; targetInfo.adapterId at +20, targetInfo.id at +28
const MODE_SIZE: usize = 64;
const GET_SOURCE_NAME: u32 = 1;
const GET_TARGET_NAME: u32 = 2;
const GET_SDR_WHITE_LEVEL: u32 = 11;
const GET_ADVANCED_COLOR_INFO_2: u32 = 15;
const SET_HDR_STATE: u32 = 16;
const SET_SDR_WHITE_LEVEL: u32 = 0xFFFF_FFEE;
const ACTIVE_COLOR_MODE_HDR: u32 = 2;
const EDID_MANUFACTURER_APPLE: u16 = 0x1006; // "APP" as stored in the EDID (06 10)

#[derive(Default)]
pub struct Hdr {
    /// Adapter LUID and target id of the display, as they appear in a DisplayConfig request header.
    target: Option<[u8; 12]>,
}

impl Hdr {
    /// Looks the display up again. Returns the SDR content brightness in nits if Windows currently
    /// outputs HDR to it, `None` otherwise.
    pub fn refresh(&mut self) -> Option<f64> {
        self.target = find_target();
        let target = self.target?;

        let mut info = packet(&target, GET_ADVANCED_COLOR_INFO_2, 36);
        if unsafe { DisplayConfigGetDeviceInfo(info.as_mut_ptr()) } != 0 || read_u32(&info, 32) != ACTIVE_COLOR_MODE_HDR {
            return None;
        }
        let mut white = packet(&target, GET_SDR_WHITE_LEVEL, 24);
        if unsafe { DisplayConfigGetDeviceInfo(white.as_mut_ptr()) } != 0 {
            return None;
        }
        Some(read_u32(&white, 20) as f64 * 80.0 / 1000.0)
    }

    /// Whether Windows offers HDR for the display, and whether the user has it switched on.
    /// `None` if the display cannot be found or asked.
    pub fn hdr_offered_and_on(&mut self) -> Option<(bool, bool)> {
        self.target = find_target();
        let mut info = packet(&self.target?, GET_ADVANCED_COLOR_INFO_2, 36);
        if unsafe { DisplayConfigGetDeviceInfo(info.as_mut_ptr()) } != 0 {
            return None;
        }
        let flags = read_u32(&info, 20);
        Some((flags >> 4 & 1 != 0, flags >> 5 & 1 != 0))
    }

    /// Switches HDR on, like the switch in Settings.
    pub fn enable_hdr(&self) -> bool {
        self.set_hdr(true)
    }

    pub fn set_hdr(&self, on: bool) -> bool {
        let Some(target) = self.target else { return false };
        let mut request = packet(&target, SET_HDR_STATE, 24);
        request[20] = on as u8;
        unsafe { DisplayConfigSetDeviceInfo(request.as_ptr()) == 0 }
    }

    /// Sets the SDR content brightness; the value is clamped to what Windows accepts.
    pub fn set_sdr_white(&self, nits: f64) -> bool {
        let Some(target) = self.target else { return false };
        let level = (nits.clamp(MIN_NITS, MAX_NITS) * 1000.0 / 80.0).round() as u32;
        let mut request = packet(&target, SET_SDR_WHITE_LEVEL, 28);
        request[20..24].copy_from_slice(&level.to_le_bytes());
        request[24] = 1; // finalValue: store it, as when the slider in Settings is released
        unsafe { DisplayConfigSetDeviceInfo(request.as_ptr()) == 0 }
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
}

/// A DisplayConfig request: DISPLAYCONFIG_DEVICE_INFO_HEADER { type, size, adapterId, id } + payload.
fn packet(target: &[u8; 12], kind: u32, size: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; size];
    bytes[0..4].copy_from_slice(&kind.to_le_bytes());
    bytes[4..8].copy_from_slice(&(size as u32).to_le_bytes());
    bytes[8..20].copy_from_slice(target);
    bytes
}

/// The name GDI knows the Studio Display's screen by (like `\\.\DISPLAY1`), for the functions that
/// work on one screen, such as the gamma ramp.
pub fn gdi_device() -> Option<String> {
    let (source, _) = find_path()?;
    // DISPLAYCONFIG_SOURCE_DEVICE_NAME: header, then viewGdiDeviceName (32 WCHAR)
    let mut name = packet(&source, GET_SOURCE_NAME, 84);
    if unsafe { DisplayConfigGetDeviceInfo(name.as_mut_ptr()) } != 0 {
        return None;
    }
    let units: Vec<u16> = name[20..84].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
    (!units.is_empty()).then(|| String::from_utf16_lossy(&units))
}

fn find_target() -> Option<[u8; 12]> {
    find_path().map(|(_, target)| target)
}

/// Picks the active display path that belongs to the Studio Display: by EDID manufacturer or name,
/// or simply the only active display when there is just one. Returns its source and its target, each
/// as adapter LUID and id.
fn find_path() -> Option<([u8; 12], [u8; 12])> {
    let (mut num_paths, mut num_modes) = (0u32, 0u32);
    if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes) } != 0 || num_paths == 0 {
        return None;
    }
    let mut paths = vec![0u8; num_paths as usize * PATH_SIZE];
    let mut modes = vec![0u8; num_modes as usize * MODE_SIZE];
    let status = unsafe {
        QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, paths.as_mut_ptr(), &mut num_modes, modes.as_mut_ptr(), null_mut())
    };
    if status != 0 || num_paths == 0 {
        return None;
    }

    let target_of = |index: usize| -> [u8; 12] { paths[index * PATH_SIZE + 20..index * PATH_SIZE + 32].try_into().unwrap() };
    let both = |index: usize| -> ([u8; 12], [u8; 12]) { (paths[index * PATH_SIZE..index * PATH_SIZE + 12].try_into().unwrap(), target_of(index)) };
    for index in 0..num_paths as usize {
        let target = target_of(index);
        // DISPLAYCONFIG_TARGET_DEVICE_NAME: edidManufactureId at +28, monitorFriendlyDeviceName (64 WCHAR) at +36
        let mut name = packet(&target, GET_TARGET_NAME, 420);
        if unsafe { DisplayConfigGetDeviceInfo(name.as_mut_ptr()) } != 0 {
            continue;
        }
        let manufacturer = u16::from_le_bytes([name[28], name[29]]);
        let friendly: Vec<u16> = name[36..164].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
        if manufacturer == EDID_MANUFACTURER_APPLE || String::from_utf16_lossy(&friendly).to_lowercase().contains("studio") {
            return Some(both(index));
        }
    }
    (num_paths == 1).then(|| both(0))
}
