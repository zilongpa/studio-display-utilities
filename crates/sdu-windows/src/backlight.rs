//! The display's backlight, over USB HID.
//!
//! Interface 7 of the Studio Display XDR (USB 05AC:1116) carries a standard HID "Monitor Control"
//! collection (usage page 0x80, usage 1) that Windows' own HID driver handles, so no extra driver is
//! needed. Brightness is feature report 1, seven bytes:
//!
//!   [0x01] [brightness: u32 LE, in 0.01 nit] [u16, left at 0]
//!
//! The report descriptor declares 400 - 60000 (4 - 600 nits).

use crate::ffi::*;
use crate::util::{open_device, registry_subkeys};

pub const MIN_NITS: f64 = 4.0;
pub const MAX_NITS: f64 = 600.0;

const HARDWARE_ID_PREFIX: &str = "VID_05AC&PID_1116&MI_07";
const HID_INTERFACE_GUID: &str = "{4d1e55b2-f16f-11cf-88cb-001111000030}";
const USAGE_PAGE_MONITOR: u16 = 0x80;
const USAGE_MONITOR_CONTROL: u16 = 0x01;
const REPORT_ID: u8 = 1;
const REPORT_LEN: usize = 7;

pub struct Backlight {
    handle: HANDLE,
}

impl Backlight {
    /// Finds the display's Monitor Control collection among the HID devices and opens it.
    pub fn open() -> Option<Backlight> {
        const ENUM: &str = r"SYSTEM\CurrentControlSet\Enum\HID";
        for device in registry_subkeys(ENUM) {
            if !device.to_ascii_uppercase().starts_with(HARDWARE_ID_PREFIX) {
                continue;
            }
            for instance in registry_subkeys(&format!(r"{ENUM}\{device}")) {
                let path = format!(r"\\?\hid#{}#{}#{}", device.to_lowercase(), instance.to_lowercase(), HID_INTERFACE_GUID);
                let Some(handle) = open_device(&path, 0) else { continue };
                if is_monitor_control(handle) {
                    return Some(Backlight { handle });
                }
                unsafe { CloseHandle(handle) };
            }
        }
        None
    }

    pub fn get(&self) -> Option<f64> {
        let mut report = [0u8; REPORT_LEN];
        report[0] = REPORT_ID;
        if unsafe { HidD_GetFeature(self.handle, report.as_mut_ptr(), REPORT_LEN as u32) } == 0 {
            return None;
        }
        Some(u32::from_le_bytes([report[1], report[2], report[3], report[4]]) as f64 / 100.0)
    }

    pub fn set(&self, nits: f64) -> bool {
        self.set_over(nits, 0)
    }

    /// Sets the brightness with the report's second field filled in: HID usage "Duration" (page
    /// 0x0F, usage 0x50), 0 - 20000 milliseconds. Whether the display fades by itself over that time
    /// is what `sdu backlight` is there to find out.
    pub fn set_over(&self, nits: f64, milliseconds: u16) -> bool {
        let value = (nits.clamp(MIN_NITS, MAX_NITS) * 100.0).round() as u32;
        let mut report = [0u8; REPORT_LEN];
        report[0] = REPORT_ID;
        report[1..5].copy_from_slice(&value.to_le_bytes());
        report[5..7].copy_from_slice(&milliseconds.min(20000).to_le_bytes());
        unsafe { HidD_SetFeature(self.handle, report.as_ptr(), REPORT_LEN as u32) != 0 }
    }
}

impl Drop for Backlight {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

fn is_monitor_control(handle: HANDLE) -> bool {
    let mut preparsed = 0isize;
    if unsafe { HidD_GetPreparsedData(handle, &mut preparsed) } == 0 {
        return false;
    }
    let mut caps = [0u16; 32];
    let status = unsafe { HidP_GetCaps(preparsed, &mut caps) };
    unsafe { HidD_FreePreparsedData(preparsed) };
    status == HIDP_STATUS_SUCCESS && caps[0] == USAGE_MONITOR_CONTROL && caps[1] == USAGE_PAGE_MONITOR && caps[4] as usize >= REPORT_LEN
}
