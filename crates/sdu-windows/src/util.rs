//! Small helpers: wide strings, registry reads, the data folder and the log file.

use crate::ffi::*;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::ptr::{null, null_mut};

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// `%APPDATA%\sdu`, created on first use. Holds the settings and the log.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join("sdu");
    let _ = fs::create_dir_all(&dir);
    dir
}

const LOG_LIMIT: u64 = 256 * 1024;

pub fn log(message: &str) {
    let path = data_dir().join("log.txt");
    if fs::metadata(&path).map(|m| m.len() > LOG_LIMIT).unwrap_or(false) {
        let _ = fs::rename(&path, data_dir().join("log.old.txt"));
    }
    let mut now = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut now) };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        // one write per line, so lines from different threads do not run into each other
        let line = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} {}\n",
            now.year, now.month, now.day, now.hour, now.minute, now.second, now.milliseconds, message
        );
        let _ = file.write_all(line.as_bytes());
    }
}

struct Key(HANDLE);

impl Key {
    fn open(path: &str) -> Option<Key> {
        let mut key = null_mut();
        let status = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wide(path).as_ptr(), 0, KEY_READ, &mut key) };
        (status == 0).then_some(Key(key))
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

/// Names of the sub keys of a key under HKEY_LOCAL_MACHINE (empty if the key does not exist).
pub fn registry_subkeys(path: &str) -> Vec<String> {
    let mut names = Vec::new();
    let Some(key) = Key::open(path) else { return names };
    for index in 0.. {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        let status = unsafe { RegEnumKeyExW(key.0, index, name.as_mut_ptr(), &mut len, null_mut(), null_mut(), null_mut(), null_mut()) };
        if status != 0 {
            break;
        }
        names.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    names
}

/// Strings of a REG_SZ or REG_MULTI_SZ value under HKEY_LOCAL_MACHINE.
pub fn registry_strings(path: &str, value: &str) -> Vec<String> {
    let Some(key) = Key::open(path) else { return Vec::new() };
    let mut kind = 0u32;
    let mut data = [0u16; 1024];
    let mut len = (data.len() * 2) as u32;
    let status = unsafe { RegQueryValueExW(key.0, wide(value).as_ptr(), null_mut(), &mut kind, data.as_mut_ptr() as *mut u8, &mut len) };
    if status != 0 || (kind != REG_SZ && kind != REG_MULTI_SZ) {
        return Vec::new();
    }
    data[..len as usize / 2]
        .split(|&c| c == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Opens a device interface path for reading and writing; `None` if it is not there.
pub fn open_device(path: &str, flags: u32) -> Option<HANDLE> {
    let handle = unsafe {
        CreateFileW(wide(path).as_ptr(), GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ_WRITE, null(), OPEN_EXISTING, flags, null_mut())
    };
    (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then_some(handle)
}
