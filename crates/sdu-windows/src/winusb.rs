//! A HID interface of the display that is bound to WinUSB instead of Windows' HID driver.
//!
//! Windows' HID driver fails to start on the display's sensor interfaces (Code 10), so they are given
//! to WinUSB once (Zadig) and spoken to here with plain USB transfers: HID class requests on the
//! control endpoint, and the interface's interrupt endpoint for the reports the device sends by itself.

use crate::ffi::*;
use crate::util::{open_device, registry_strings, registry_subkeys};
use std::ptr::null_mut;

const PIPE_TYPE_INTERRUPT: i32 = 3;
const PIPE_TRANSFER_TIMEOUT: u32 = 3;
const HID_GET_REPORT: u8 = 1;
const HID_SET_REPORT: u8 = 9;
const GET_DESCRIPTOR: u8 = 6;
const DESCRIPTOR_HID_REPORT: u16 = 0x22;
pub const REPORT_INPUT: u16 = 1;
pub const REPORT_FEATURE: u16 = 3;

pub struct Interface {
    file: HANDLE,
    usb: HANDLE,
    number: u16,
    pipe: u8,
    packet_size: usize,
}

/// What a read of the interrupt endpoint came to.
pub enum Read {
    Report(usize),
    /// Nothing arrived within the timeout: the device had nothing to say.
    Quiet,
    Failed(u32),
}

impl Interface {
    /// Opens interface `number` of the device with this hardware id (like `VID_05AC&PID_1116&MI_08`).
    /// A read of its interrupt endpoint gives up after `timeout_ms`.
    pub fn open(hardware_id: &str, number: u16, timeout_ms: u32) -> Result<Interface, String> {
        let path = find_path(hardware_id).ok_or(format!("interface {number} is not bound to WinUSB or the display is not connected"))?;
        let file = open_device(&path, FILE_FLAG_OVERLAPPED).ok_or_else(|| format!("cannot open it, error {}", unsafe { GetLastError() }))?;
        let mut usb = null_mut();
        if unsafe { WinUsb_Initialize(file, &mut usb) } == 0 {
            let error = unsafe { GetLastError() };
            unsafe { CloseHandle(file) };
            return Err(format!("WinUsb_Initialize failed, error {error}"));
        }
        let mut interface = Interface { file, usb, number, pipe: 0, packet_size: 0 };
        let mut info = WINUSB_PIPE_INFORMATION::default();
        let mut index = 0u8;
        while unsafe { WinUsb_QueryPipe(usb, 0, index, &mut info) } != 0 {
            if info.pipe_type == PIPE_TYPE_INTERRUPT && info.pipe_id & 0x80 != 0 && info.maximum_packet_size > 0 {
                interface.pipe = info.pipe_id;
                interface.packet_size = info.maximum_packet_size as usize;
                break;
            }
            index += 1;
        }
        if interface.pipe == 0 {
            return Err("no interrupt IN endpoint".into());
        }
        unsafe { WinUsb_SetPipePolicy(usb, interface.pipe, PIPE_TRANSFER_TIMEOUT, 4, &timeout_ms as *const u32 as *const _) };
        Ok(interface)
    }

    /// The size a buffer for `read` must have: the endpoint's packet size.
    pub fn packet_size(&self) -> usize {
        self.packet_size
    }

    fn transfer(&self, request_type: u8, request: u8, value: u16, buffer: &mut [u8]) -> Option<usize> {
        let setup = request_type as u64 | (request as u64) << 8 | (value as u64) << 16 | (self.number as u64) << 32 | (buffer.len() as u64) << 48;
        let mut transferred = 0u32;
        let done = unsafe { WinUsb_ControlTransfer(self.usb, setup, buffer.as_mut_ptr(), buffer.len() as u32, &mut transferred, null_mut()) };
        (done != 0).then_some(transferred as usize)
    }

    /// HID GET_REPORT (`get`) or SET_REPORT for report `id` of the given type. Returns whether it
    /// went through and carried data.
    pub fn report(&self, get: bool, report_type: u16, id: u8, buffer: &mut [u8]) -> bool {
        let (request_type, request) = if get { (0xA1, HID_GET_REPORT) } else { (0x21, HID_SET_REPORT) };
        self.transfer(request_type, request, (report_type << 8) | id as u16, buffer).is_some_and(|length| length > 0)
    }

    /// Like `report` for reading, but tells how many bytes came.
    pub fn get_report(&self, report_type: u16, id: u8, buffer: &mut [u8]) -> Option<usize> {
        self.transfer(0xA1, HID_GET_REPORT, (report_type << 8) | id as u16, buffer).filter(|&length| length > 0)
    }

    /// The interface's HID report descriptor.
    pub fn descriptor(&self) -> Option<Vec<u8>> {
        let mut buffer = vec![0u8; 4096];
        let length = self.transfer(0x81, GET_DESCRIPTOR, DESCRIPTOR_HID_REPORT << 8, &mut buffer)?;
        buffer.truncate(length);
        Some(buffer)
    }

    /// Waits for a report on the interrupt endpoint. `buffer` must be `packet_size` long.
    pub fn read(&self, buffer: &mut [u8]) -> Read {
        let mut transferred = 0u32;
        if unsafe { WinUsb_ReadPipe(self.usb, self.pipe, buffer.as_mut_ptr(), buffer.len() as u32, &mut transferred, null_mut()) } != 0 {
            return Read::Report(transferred as usize);
        }
        match unsafe { GetLastError() } {
            ERROR_SEM_TIMEOUT => Read::Quiet,
            error => Read::Failed(error),
        }
    }
}

impl Drop for Interface {
    fn drop(&mut self) {
        unsafe {
            WinUsb_Free(self.usb);
            CloseHandle(self.file);
        }
    }
}

/// WinUSB registers the interface under a GUID chosen by its INF, so the GUID is read from the device
/// node in the registry and the device interface path is put together from it.
fn find_path(hardware_id: &str) -> Option<String> {
    let device = format!(r"SYSTEM\CurrentControlSet\Enum\USB\{hardware_id}");
    for instance in registry_subkeys(&device) {
        let parameters = format!(r"{device}\{instance}\Device Parameters");
        let mut guids = registry_strings(&parameters, "DeviceInterfaceGUIDs");
        guids.extend(registry_strings(&parameters, "DeviceInterfaceGUID"));
        for guid in guids {
            let path = format!(r"\\?\usb#{}#{}#{}", hardware_id.to_lowercase(), instance.to_lowercase(), guid.to_lowercase());
            // Only a present device can be opened; stale instances are skipped this way.
            if let Some(handle) = open_device(&path, FILE_FLAG_OVERLAPPED) {
                unsafe { CloseHandle(handle) };
                return Some(path);
            }
        }
    }
    None
}
