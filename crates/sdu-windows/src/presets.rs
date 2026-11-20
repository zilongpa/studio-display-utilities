//! The display's reference modes (colour presets), over USB HID.
//!
//! Interface 6 carries a vendor collection (usage page 0xFF00, usage 2) with feature reports on usage
//! page 0xFF20. How they work was found by the Studio Brightness++ project:
//!
//! - usage 3 (report 3): the number of the preset in use; reading tells, writing switches.
//! - usage 4 (report 4): a cursor, write only. After writing `i`, report 5 describes preset `i`.
//! - usage 6 (report 5): whether the preset under the cursor exists; 0 ends the list.
//! - usage 8 (report 5): its name, UTF-16.
//!
//! The cursor report must not be read (the display stalls); it is written from a zeroed buffer.
//!
//! The presets whose names begin with "Apple" are the general ones, with adjustable brightness; of
//! these the "Apple XDR Display" ones are the only ones that work while Windows outputs HDR. All
//! others are reference modes with a fixed brightness, as on macOS.

use crate::ffi::*;
use crate::util::{open_device, registry_subkeys};

const HARDWARE_ID_PREFIX: &str = "VID_05AC&PID_1116&MI_06";
const HID_INTERFACE_GUID: &str = "{4d1e55b2-f16f-11cf-88cb-001111000030}";
const COLLECTION_PAGE: u16 = 0xFF00;
const COLLECTION_USAGE: u16 = 0x0002;
const PAGE: u16 = 0xFF20;
const USAGE_ACTIVE: u16 = 3;
const USAGE_CURSOR: u16 = 4;
const USAGE_VALID: u16 = 6;
const USAGE_NAME: u16 = 8;
const REPORT_ACTIVE: u8 = 3;
const REPORT_CURSOR: u8 = 4;
const REPORT_PRESET: u8 = 5;
const HIDP_FEATURE: i32 = 2;
const MOST: u32 = 64;

pub struct Presets {
    handle: HANDLE,
    preparsed: isize,
    report_len: usize,
}

/// Whether brightness can be changed in the preset of this name.
pub fn adjustable(name: &str) -> bool {
    name.starts_with("Apple")
}

/// Whether the preset of this name works while Windows outputs HDR.
pub fn works_in_hdr(name: &str) -> bool {
    name.starts_with("Apple XDR Display")
}

impl Presets {
    pub fn open() -> Option<Presets> {
        const ENUM: &str = r"SYSTEM\CurrentControlSet\Enum\HID";
        for device in registry_subkeys(ENUM) {
            if !device.to_ascii_uppercase().starts_with(HARDWARE_ID_PREFIX) {
                continue;
            }
            for instance in registry_subkeys(&format!(r"{ENUM}\{device}")) {
                let path = format!(r"\\?\hid#{}#{}#{}", device.to_lowercase(), instance.to_lowercase(), HID_INTERFACE_GUID);
                let Some(handle) = open_device(&path, 0) else { continue };
                let mut preparsed = 0isize;
                if unsafe { HidD_GetPreparsedData(handle, &mut preparsed) } != 0 {
                    let mut caps = [0u16; 32];
                    if unsafe { HidP_GetCaps(preparsed, &mut caps) } == HIDP_STATUS_SUCCESS && caps[0] == COLLECTION_USAGE && caps[1] == COLLECTION_PAGE && caps[4] > 0 {
                        return Some(Presets { handle, preparsed, report_len: caps[4] as usize });
                    }
                    unsafe { HidD_FreePreparsedData(preparsed) };
                }
                unsafe { CloseHandle(handle) };
            }
        }
        None
    }

    /// A zeroed feature report with `usage` set to `value`.
    fn report_with(&self, id: u8, usage: u16, value: u32) -> Option<Vec<u8>> {
        let mut report = vec![0u8; self.report_len];
        report[0] = id;
        let status = unsafe { HidP_SetUsageValue(HIDP_FEATURE, PAGE, 0, usage, value, self.preparsed, report.as_mut_ptr(), report.len() as u32) };
        (status == HIDP_STATUS_SUCCESS).then_some(report)
    }

    fn read(&self, id: u8) -> Option<Vec<u8>> {
        let mut report = vec![0u8; self.report_len];
        report[0] = id;
        (unsafe { HidD_GetFeature(self.handle, report.as_mut_ptr(), report.len() as u32) } != 0).then_some(report)
    }

    fn value(&self, report: &[u8], usage: u16) -> Option<u32> {
        let mut value = 0u32;
        let status = unsafe { HidP_GetUsageValue(HIDP_FEATURE, PAGE, 0, usage, &mut value, self.preparsed, report.as_ptr(), report.len() as u32) };
        (status == HIDP_STATUS_SUCCESS).then_some(value)
    }

    /// The names of the presets, in the display's order; the position is the preset's number.
    pub fn names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for index in 0..MOST {
            let Some(cursor) = self.report_with(REPORT_CURSOR, USAGE_CURSOR, index) else { break };
            if unsafe { HidD_SetFeature(self.handle, cursor.as_ptr(), cursor.len() as u32) } == 0 {
                break;
            }
            let Some(preset) = self.read(REPORT_PRESET) else { break };
            if self.value(&preset, USAGE_VALID).unwrap_or(0) == 0 {
                break;
            }
            let mut name = [0u8; 256];
            unsafe { HidP_GetUsageValueArray(HIDP_FEATURE, PAGE, 0, USAGE_NAME, name.as_mut_ptr(), name.len() as u16, self.preparsed, preset.as_ptr(), preset.len() as u32) };
            let units: Vec<u16> = name.chunks(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).take_while(|&unit| unit != 0).collect();
            names.push(String::from_utf16_lossy(&units));
        }
        names
    }

    /// The number of the preset in use.
    pub fn active(&self) -> Option<u32> {
        self.value(&self.read(REPORT_ACTIVE)?, USAGE_ACTIVE)
    }

    /// Switches to a preset. The display may drop off USB for a moment afterwards.
    pub fn switch(&self, index: u32) -> bool {
        match self.report_with(REPORT_ACTIVE, USAGE_ACTIVE, index) {
            Some(report) => unsafe { HidD_SetFeature(self.handle, report.as_ptr(), report.len() as u32) != 0 },
            None => false,
        }
    }
}

impl Drop for Presets {
    fn drop(&mut self) {
        unsafe {
            HidD_FreePreparsedData(self.preparsed);
            CloseHandle(self.handle);
        }
    }
}
