//! Starting the program when the user signs in: an entry under the user's own "Run" registry key,
//! which needs no administrator rights and shows up in Task Manager's list of start-up apps.

use crate::ffi::*;
use crate::util::wide;
use std::ptr::null_mut;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "Studio Display Utilities";

fn open(access: u32) -> Option<HANDLE> {
    let mut key = null_mut();
    (unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), 0, access, &mut key) } == 0).then_some(key)
}

/// The command stored for this program, if there is one.
fn stored() -> Option<String> {
    let key = open(KEY_READ)?;
    let mut kind = 0u32;
    let mut data = [0u16; 1024];
    let mut len = (data.len() * 2) as u32;
    let status = unsafe { RegQueryValueExW(key, wide(NAME).as_ptr(), null_mut(), &mut kind, data.as_mut_ptr() as *mut u8, &mut len) };
    unsafe { RegCloseKey(key) };
    (status == 0 && kind == REG_SZ).then(|| String::from_utf16_lossy(&data[..len as usize / 2]).trim_end_matches('\0').to_string())
}

fn command() -> Option<String> {
    Some(format!("\"{}\"", std::env::current_exe().ok()?.display()))
}

/// Whether this very program file is set to start at sign-in.
pub fn enabled() -> bool {
    stored().is_some() && stored() == command()
}

pub fn set(on: bool) -> bool {
    let Some(key) = open(KEY_SET_VALUE) else { return false };
    let status = match (on, command()) {
        (true, Some(command)) => {
            let text = wide(&command);
            unsafe { RegSetValueExW(key, wide(NAME).as_ptr(), 0, REG_SZ, text.as_ptr() as *const u8, (text.len() * 2) as u32) }
        }
        (true, None) => 1,
        (false, _) => match unsafe { RegDeleteValueW(key, wide(NAME).as_ptr()) } {
            2 => 0, // there was nothing to remove
            status => status,
        },
    };
    unsafe { RegCloseKey(key) };
    status == 0
}
