//! The display's ambient light sensor.
//!
//! The sensor sits on USB interface 8. It is a standard HID sensor, but Windows' HID driver fails to
//! start on it (Code 10, it cannot parse the 726 byte report descriptor), so the interface has to be
//! bound to WinUSB once (Zadig) and is read here directly.
//!
//! There are two sensors, HID report ids 1 and 2; id 1 is the one in the top bezel facing the viewer.
//! Both send the same 18 byte input report:
//!
//!   [id] [state] [illuminance: u32 LE, millilux] [colour temperature: u32 LE, kelvin] [x: u32] [y: u32]
//!
//! macOS' automatic brightness follows whichever of the two reads brighter (see docs/), so that is
//! what `lux()` returns.
//!
//! A sensor sends a report only when the light changes by more than its own threshold (about 15 %),
//! so the current values are fetched once with GET_REPORT and after that the reports are simply
//! awaited. Nothing is polled.

use crate::ffi::*;
use crate::util::{log, open_device, registry_strings, registry_subkeys};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const HARDWARE_ID: &str = "VID_05AC&PID_1116&MI_08";
const INTERFACE_NUMBER: u16 = 8;
const SENSORS: [u8; 2] = [1, 2];
const INPUT_REPORT_LEN: usize = 18;
const FEATURE_REPORT_LEN: usize = 41;
const REPORTING_STATE: usize = 1; // feature report byte: 2 = all events
const POWER_STATE: usize = 6; // feature report byte: 2 = D0, full power
/// The last four bytes of the feature report: how far the light's chromaticity (x, y; unit 1e-8) has
/// to move before the sensor reports again. Out of the box this is 1.0, which is never; macOS lowers
/// it to this value (about 0.005) while True Tone is on, and so do we.
const CHROMATICITY_SENSITIVITY: usize = FEATURE_REPORT_LEN - 4;
const CHROMATICITY_CHANGE: u32 = 0x0008_0F0B;
const PIPE_TYPE_INTERRUPT: i32 = 3;
const PIPE_TRANSFER_TIMEOUT: u32 = 3;
const HID_GET_REPORT: u8 = 1;
const HID_SET_REPORT: u8 = 9;
const REPORT_INPUT: u16 = 1;
const REPORT_FEATURE: u16 = 3;
const RETRY: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Shared {
    /// Per sensor: whether it has delivered a reading, and the reading.
    valid: [AtomicBool; 2],
    millilux: [AtomicU32; 2],
    /// Chromaticity of the light, CIE 1931 x and y, in units of 1e-8.
    x: [AtomicU32; 2],
    y: [AtomicU32; 2],
    stop: AtomicBool,
}

pub struct Sensor {
    shared: Arc<Shared>,
    wake: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Sensor {
    /// Starts the reader thread. It posts `message` to `hwnd` whenever a reading arrives or the
    /// sensor is lost.
    pub fn start(hwnd: HWND, message: u32) -> Sensor {
        let shared = Arc::new(Shared::default());
        let (wake, wait) = channel::<()>();
        let window = hwnd as isize;
        let thread_shared = shared.clone();
        let thread = std::thread::spawn(move || {
            let notify = || unsafe {
                PostMessageW(window as HWND, message, 0, 0);
            };
            while !thread_shared.stop.load(Ordering::Relaxed) {
                match Device::open() {
                    Ok(device) => {
                        log("sensor: connected");
                        device.run(&thread_shared, &notify);
                        log("sensor: disconnected");
                    }
                    Err(reason) => log(&format!("sensor: not available ({reason})")),
                }
                for valid in &thread_shared.valid {
                    valid.store(false, Ordering::Relaxed);
                }
                notify();
                // Wait for a reason to try again: a device change, resume from sleep, or simply time.
                if let Err(RecvTimeoutError::Disconnected) = wait.recv_timeout(RETRY) {
                    break;
                }
                while wait.try_recv().is_ok() {}
            }
        });
        Sensor { shared, wake, thread: Some(thread) }
    }

    /// Ambient light in lux: the brighter of the two sensors, once at least one has delivered a reading.
    pub fn lux(&self) -> Option<f64> {
        (0..SENSORS.len())
            .filter(|&index| self.shared.valid[index].load(Ordering::Relaxed))
            .map(|index| self.shared.millilux[index].load(Ordering::Relaxed))
            .max()
            .map(|millilux| millilux as f64 / 1000.0)
    }

    /// What each sensor that has delivered a reading sees: lux and the light's chromaticity (CIE 1931
    /// x, y). In the dark the chromaticity is noise.
    pub fn colours(&self) -> Vec<(f64, (f64, f64))> {
        (0..SENSORS.len())
            .filter(|&index| self.shared.valid[index].load(Ordering::Relaxed))
            .map(|index| {
                let value = |cell: &[AtomicU32; 2], unit: f64| cell[index].load(Ordering::Relaxed) as f64 / unit;
                (value(&self.shared.millilux, 1000.0), (value(&self.shared.x, 1e8), value(&self.shared.y, 1e8)))
            })
            .collect()
    }

    /// Asks the reader to look for the sensor again right away if it is not connected.
    pub fn retry(&self) {
        let _ = self.wake.send(());
    }
}

/// One sensor's reading, as asked for directly (see `sample`).
pub struct Reading {
    pub sensor: u8,
    pub lux: f64,
    pub kelvin: u32,
    /// CIE 1931 chromaticity of the light.
    pub x: f64,
    pub y: f64,
}

/// Asks both sensors for their current reading every `every` for `duration`, and hands each round to
/// `each`. For measurements and diagnostics: unlike the reader thread this does not wait for the
/// sensors to report a change, it asks. Fails if the tray program (or anything else) holds the sensor.
pub fn sample(duration: Duration, every: Duration, mut each: impl FnMut(&[Reading])) -> Result<(), String> {
    let device = Device::open()?;
    let start = std::time::Instant::now();
    loop {
        let mut readings = Vec::new();
        for sensor in SENSORS {
            let mut report = [0u8; INPUT_REPORT_LEN];
            report[0] = sensor;
            if device.control(true, REPORT_INPUT, sensor, &mut report) {
                let field = |at: usize| u32::from_le_bytes([report[at], report[at + 1], report[at + 2], report[at + 3]]);
                readings.push(Reading { sensor, lux: field(2) as f64 / 1000.0, kelvin: field(6), x: field(10) as f64 / 1e8, y: field(14) as f64 / 1e8 });
            }
        }
        each(&readings);
        if start.elapsed() >= duration {
            return Ok(());
        }
        std::thread::sleep(every);
    }
}

impl Drop for Sensor {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        let _ = self.wake.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Device {
    file: HANDLE,
    usb: HANDLE,
    pipe: u8,
    packet_size: usize,
}

impl Device {
    fn open() -> Result<Device, String> {
        let path = find_path().ok_or("interface 8 is not bound to WinUSB or the display is not connected")?;
        let file = open_device(&path, FILE_FLAG_OVERLAPPED).ok_or_else(|| format!("cannot open it, error {}", unsafe { GetLastError() }))?;
        let mut usb = null_mut();
        if unsafe { WinUsb_Initialize(file, &mut usb) } == 0 {
            let error = unsafe { GetLastError() };
            unsafe { CloseHandle(file) };
            return Err(format!("WinUsb_Initialize failed, error {error}"));
        }
        let mut device = Device { file, usb, pipe: 0, packet_size: 0 };

        let mut info = WINUSB_PIPE_INFORMATION::default();
        let mut index = 0u8;
        while unsafe { WinUsb_QueryPipe(usb, 0, index, &mut info) } != 0 {
            if info.pipe_type == PIPE_TYPE_INTERRUPT && info.pipe_id & 0x80 != 0 && info.maximum_packet_size > 0 {
                device.pipe = info.pipe_id;
                device.packet_size = info.maximum_packet_size as usize;
                break;
            }
            index += 1;
        }
        if device.pipe == 0 {
            return Err("no interrupt IN endpoint".into());
        }
        // A read gives up after a second so the thread can notice a stop request; that is a local
        // wait, the display is not asked for anything.
        let timeout: u32 = 1000;
        unsafe { WinUsb_SetPipePolicy(usb, device.pipe, PIPE_TRANSFER_TIMEOUT, 4, &timeout as *const u32 as *const _) };
        device.enable();
        Ok(device)
    }

    /// The sensors normally are already reporting; if one is not, switch it to full power / all events.
    /// They are also told to report when the colour of the light changes, not only its amount.
    fn enable(&self) {
        for sensor in SENSORS {
            let mut report = [0u8; FEATURE_REPORT_LEN];
            report[0] = sensor;
            if !self.control(true, REPORT_FEATURE, sensor, &mut report) {
                continue;
            }
            let at = CHROMATICITY_SENSITIVITY;
            let sensitivity = u32::from_le_bytes([report[at], report[at + 1], report[at + 2], report[at + 3]]);
            let idle = report[REPORTING_STATE] != 2 || report[POWER_STATE] != 2;
            if idle || sensitivity > CHROMATICITY_CHANGE {
                report[REPORTING_STATE] = 2;
                report[POWER_STATE] = 2;
                report[at..at + 4].copy_from_slice(&CHROMATICITY_CHANGE.min(sensitivity).to_le_bytes());
                let done = self.control(false, REPORT_FEATURE, sensor, &mut report);
                log(&format!("sensor {sensor}: idle {idle}, colour change threshold {sensitivity} -> {}: {done}", CHROMATICITY_CHANGE.min(sensitivity)));
            }
        }
    }

    /// HID class GET_REPORT / SET_REPORT for one of the sensors on the control endpoint.
    fn control(&self, get: bool, report_type: u16, sensor: u8, buffer: &mut [u8]) -> bool {
        let request_type: u8 = if get { 0xA1 } else { 0x21 };
        let request = if get { HID_GET_REPORT } else { HID_SET_REPORT };
        let value = (report_type << 8) | sensor as u16;
        let setup = request_type as u64
            | (request as u64) << 8
            | (value as u64) << 16
            | (INTERFACE_NUMBER as u64) << 32
            | (buffer.len() as u64) << 48;
        let mut transferred = 0u32;
        unsafe { WinUsb_ControlTransfer(self.usb, setup, buffer.as_mut_ptr(), buffer.len() as u32, &mut transferred, null_mut()) != 0 && transferred > 0 }
    }

    fn run(&self, shared: &Shared, notify: &dyn Fn()) {
        let publish = |report: &[u8]| {
            let Some(index) = report.first().and_then(|id| SENSORS.iter().position(|sensor| sensor == id)) else { return };
            if report.len() >= INPUT_REPORT_LEN {
                let field = |at: usize| u32::from_le_bytes([report[at], report[at + 1], report[at + 2], report[at + 3]]);
                shared.millilux[index].store(field(2), Ordering::Relaxed);
                shared.x[index].store(field(10), Ordering::Relaxed);
                shared.y[index].store(field(14), Ordering::Relaxed);
                shared.valid[index].store(true, Ordering::Relaxed);
                notify();
            }
        };

        for sensor in SENSORS {
            let mut current = [0u8; INPUT_REPORT_LEN];
            current[0] = sensor;
            if self.control(true, REPORT_INPUT, sensor, &mut current) {
                publish(&current);
            }
        }

        // The read size must equal the endpoint's packet size.
        let mut buffer = vec![0u8; self.packet_size];
        let mut errors = 0;
        while !shared.stop.load(Ordering::Relaxed) {
            let mut transferred = 0u32;
            if unsafe { WinUsb_ReadPipe(self.usb, self.pipe, buffer.as_mut_ptr(), buffer.len() as u32, &mut transferred, null_mut()) } != 0 {
                errors = 0;
                publish(&buffer[..transferred as usize]);
                continue;
            }
            let error = unsafe { GetLastError() };
            if error == ERROR_SEM_TIMEOUT {
                errors = 0; // nothing changed enough for the sensor to report
                continue;
            }
            errors += 1;
            if errors >= 3 {
                log(&format!("sensor: read failed, error {error}"));
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            WinUsb_Free(self.usb);
            CloseHandle(self.file);
        }
    }
}

/// WinUSB registers the interface under a GUID chosen by its INF, so the GUID is read from the device
/// node in the registry and the device interface path is put together from it.
fn find_path() -> Option<String> {
    let device = format!(r"SYSTEM\CurrentControlSet\Enum\USB\{HARDWARE_ID}");
    for instance in registry_subkeys(&device) {
        let parameters = format!(r"{device}\{instance}\Device Parameters");
        let mut guids = registry_strings(&parameters, "DeviceInterfaceGUIDs");
        guids.extend(registry_strings(&parameters, "DeviceInterfaceGUID"));
        for guid in guids {
            let path = format!(r"\\?\usb#{}#{}#{}", HARDWARE_ID.to_lowercase(), instance.to_lowercase(), guid.to_lowercase());
            // Only a present device can be opened; stale instances are skipped this way.
            if let Some(handle) = open_device(&path, FILE_FLAG_OVERLAPPED) {
                unsafe { CloseHandle(handle) };
                return Some(path);
            }
        }
    }
    None
}
