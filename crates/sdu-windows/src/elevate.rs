//! Running a part of the program with administrator rights.
//!
//! The program itself runs as the user. The few things that need more (installing the WinUSB binding
//! for the sensors, the AMD driver's connection emulation) are done by starting the program a second
//! time with a command, elevated: Windows asks the user for consent (UAC), the second instance does
//! that one thing and ends.

use crate::ffi::*;
use crate::util::wide;
use std::ptr::{null, null_mut};

const TOKEN_QUERY: u32 = 8;
const TOKEN_ELEVATION: u32 = 20;
const SEE_MASK_NOCLOSEPROCESS: u32 = 0x40;
const INFINITE: u32 = u32::MAX;
const ERROR_CANCELLED: u32 = 1223;

/// Whether this process has administrator rights.
pub fn is_elevated() -> bool {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let (mut elevated, mut length) = (0u32, 0u32);
        let done = GetTokenInformation(token, TOKEN_ELEVATION, &mut elevated as *mut u32 as *mut _, 4, &mut length);
        CloseHandle(token);
        done != 0 && elevated != 0
    }
}

/// Starts this program again with `arguments`, elevated, and waits for it to end. Returns its exit
/// code. Windows shows its consent prompt first; declining it is an error here.
pub fn run(arguments: &str) -> Result<u32, String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let (verb, file, parameters) = (wide("runas"), wide(&program.display().to_string()), wide(arguments));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.lpDirectory = null();
    info.nShow = 0; // hidden: it has nothing to show
    unsafe {
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return Err(match GetLastError() {
                ERROR_CANCELLED => "administrator rights were not granted".to_string(),
                error => format!("could not start with administrator rights, error {error}"),
            });
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}
