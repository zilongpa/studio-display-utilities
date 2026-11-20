//! The tray icon: `sdu_core::icon` turned into a Windows icon.

use sdu_windows::ffi::*;
use std::ptr::{null, null_mut};

/// Large enough for the notification area at any scaling; Windows scales it down.
const SIZE: usize = 48;

pub fn create() -> HANDLE {
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: SIZE as i32,
        biHeight: -(SIZE as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    unsafe {
        let mut bits: *mut core::ffi::c_void = null_mut();
        let color = CreateDIBSection(null_mut(), &header, 0, &mut bits, null_mut(), 0);
        if color.is_null() || bits.is_null() {
            return null_mut();
        }
        std::slice::from_raw_parts_mut(bits as *mut u32, SIZE * SIZE).copy_from_slice(&sdu_core::icon::render(SIZE));
        let mask = CreateBitmap(SIZE as i32, SIZE as i32, 1, 1, null());
        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(mask);
        DeleteObject(color);
        icon
    }
}

/// Writes the icon as a 32 bit BMP file, for looking at it or making other formats from it.
pub fn save(path: &str, size: usize) -> Result<(), String> {
    let pixels = sdu_core::icon::render(size);
    let mut file = Vec::with_capacity(54 + pixels.len() * 4);
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&((54 + pixels.len() * 4) as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&54u32.to_le_bytes());
    file.extend_from_slice(&40u32.to_le_bytes());
    file.extend_from_slice(&(size as i32).to_le_bytes());
    file.extend_from_slice(&(-(size as i32)).to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&32u16.to_le_bytes());
    file.extend_from_slice(&[0; 24]);
    for pixel in pixels {
        file.extend_from_slice(&pixel.to_le_bytes());
    }
    std::fs::write(path, file).map_err(|error| error.to_string())
}
