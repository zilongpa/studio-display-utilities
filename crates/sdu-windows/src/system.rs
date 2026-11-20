//! Windows' own display brightness value (0 - 100 %).
//!
//! On a desktop PC Windows still keeps this value: the keyboard's brightness keys change it and show
//! the system's brightness flyout, it just is not sent to an external monitor. This program treats it
//! as the brightness the user sees and asks for: changes of it are applied to the display, and when
//! the program changes the brightness itself (ambient light, F14 / F15) it writes the value back, so
//! the system's bar always shows the real level.
//!
//! The value is the power setting "Display brightness" (VIDEONORMALLEVEL) of the active power plan.

use crate::ffi::*;
use std::ptr::null_mut;

const SUB_VIDEO: GUID = GUID { data1: 0x7516b95f, data2: 0xf776, data3: 0x4464, data4: [0x8c, 0x53, 0x06, 0x16, 0x7f, 0x40, 0xcc, 0x99] };
const VIDEO_NORMAL_LEVEL: GUID = GUID { data1: 0xaded5e82, data2: 0xb909, data3: 0x4619, data4: [0x99, 0x49, 0xf5, 0xd7, 0x1d, 0xac, 0x0b, 0xcb] };

/// Runs `f` with the active power plan.
fn with_scheme<R>(f: impl FnOnce(*const GUID) -> R) -> Option<R> {
    let mut scheme: *mut GUID = null_mut();
    if unsafe { PowerGetActiveScheme(null_mut(), &mut scheme) } != 0 || scheme.is_null() {
        return None;
    }
    let result = f(scheme);
    unsafe { LocalFree(scheme as *mut _) };
    Some(result)
}

pub fn read() -> Option<u32> {
    with_scheme(|scheme| {
        let mut value = 0u32;
        (unsafe { PowerReadACValueIndex(null_mut(), scheme, &SUB_VIDEO, &VIDEO_NORMAL_LEVEL, &mut value) } == 0).then_some(value.min(100))
    })
    .flatten()
}

pub fn write(percent: u32) -> bool {
    with_scheme(|scheme| unsafe {
        PowerWriteACValueIndex(null_mut(), scheme, &SUB_VIDEO, &VIDEO_NORMAL_LEVEL, percent) == 0
            && PowerWriteDCValueIndex(null_mut(), scheme, &SUB_VIDEO, &VIDEO_NORMAL_LEVEL, percent) == 0
            && PowerSetActiveScheme(null_mut(), scheme) == 0 // makes the new value take effect
    })
    .unwrap_or(false)
}

/// Asks Windows to send WM_POWERBROADCAST / PBT_POWERSETTINGCHANGE to `hwnd` when the value changes.
pub fn subscribe(hwnd: HWND) -> bool {
    !unsafe { RegisterPowerSettingNotification(hwnd, &VIDEO_NORMAL_LEVEL, 0) }.is_null()
}

/// The new value, if this PBT_POWERSETTINGCHANGE message is about the display brightness.
///
/// # Safety
/// `lparam` must be the LPARAM of a PBT_POWERSETTINGCHANGE message.
pub unsafe fn changed(lparam: isize) -> Option<u32> {
    // POWERBROADCAST_SETTING { GUID PowerSetting; DWORD DataLength; UCHAR Data[] }
    let setting = lparam as *const GUID;
    if setting.is_null() || *setting != VIDEO_NORMAL_LEVEL {
        return None;
    }
    let length = *(lparam as *const u8).add(16).cast::<u32>();
    (length >= 4).then(|| (*(lparam as *const u8).add(20).cast::<u32>()).min(100))
}
