//! The display's orientation sensor (USB interface 9), which tells which way up the screen is.
//!
//! A standard HID sensor (usage page 0x20, usage 0x8A "device orientation") that Windows' HID driver
//! fails to start on, like the light sensor, so the interface has to be bound to WinUSB once.
//! Its input report, id 1, is five bytes: three tilt angles of nine bits each, in degrees (0 - 360).
//! Only the second one moves when the screen is turned on its stand: 0 upright, and 270 after turning
//! it a quarter clockwise (as seen from the front). The sensor reports when the angle changes.
//! Turned a quarter the other way, which Apple's stand does not do, it keeps reporting 0: the sensor
//! only tells the two positions the stand has.

use crate::ffi::*;
use crate::util::log;
use crate::winusb::{Interface, Read, REPORT_INPUT};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const HARDWARE_ID: &str = "VID_05AC&PID_1116&MI_09";
const INTERFACE_NUMBER: u16 = 9;
const REPORT_ID: u8 = 1;
const REPORT_LEN: usize = 5;
const UNKNOWN: u32 = u32::MAX;
const RETRY: Duration = Duration::from_secs(30);

/// The three tilt angles of a report, in degrees.
fn angles(report: &[u8]) -> Option<[u32; 3]> {
    if report.len() < REPORT_LEN || report[0] != REPORT_ID {
        return None;
    }
    let bits = u32::from_le_bytes([report[1], report[2], report[3], report[4]]);
    Some([bits & 0x1FF, bits >> 9 & 0x1FF, bits >> 18 & 0x1FF])
}

#[derive(Default)]
struct Shared {
    degrees: AtomicU32,
    stop: AtomicBool,
}

pub struct Orientation {
    shared: Arc<Shared>,
    wake: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Orientation {
    /// Starts the reader thread. It posts `message` to `hwnd` whenever the angle changes.
    pub fn start(hwnd: HWND, message: u32) -> Orientation {
        let shared = Arc::new(Shared { degrees: AtomicU32::new(UNKNOWN), stop: AtomicBool::new(false) });
        let (wake, wait) = channel::<()>();
        let window = hwnd as isize;
        let thread_shared = shared.clone();
        let thread = std::thread::spawn(move || {
            let seen = std::cell::Cell::new([UNKNOWN; 3]);
            let publish = |report: &[u8]| {
                if let Some(all) = angles(report) {
                    if seen.replace(all) != all {
                        log(&format!("orientation sensor: angles {all:?}"));
                    }
                    let turn = all[1];
                    if thread_shared.degrees.swap(turn, Ordering::Relaxed) != turn {
                        unsafe { PostMessageW(window as HWND, message, 0, 0) };
                    }
                }
            };
            while !thread_shared.stop.load(Ordering::Relaxed) {
                match Interface::open(HARDWARE_ID, INTERFACE_NUMBER, 1000) {
                    Ok(interface) => {
                        log("orientation sensor: connected");
                        let mut current = [0u8; REPORT_LEN];
                        current[0] = REPORT_ID;
                        if interface.report(true, REPORT_INPUT, REPORT_ID, &mut current) {
                            publish(&current);
                        }
                        let mut buffer = vec![0u8; interface.packet_size()];
                        let mut errors = 0;
                        while !thread_shared.stop.load(Ordering::Relaxed) {
                            match interface.read(&mut buffer) {
                                Read::Report(length) => {
                                    errors = 0;
                                    publish(&buffer[..length]);
                                }
                                Read::Quiet => errors = 0,
                                Read::Failed(error) => {
                                    errors += 1;
                                    if errors >= 3 {
                                        log(&format!("orientation sensor: read failed, error {error}"));
                                        break;
                                    }
                                    std::thread::sleep(Duration::from_millis(200));
                                }
                            }
                        }
                    }
                    Err(reason) => log(&format!("orientation sensor: not available ({reason})")),
                }
                thread_shared.degrees.store(UNKNOWN, Ordering::Relaxed);
                if let Err(RecvTimeoutError::Disconnected) = wait.recv_timeout(RETRY) {
                    break;
                }
                while wait.try_recv().is_ok() {}
            }
        });
        Orientation { shared, wake, thread: Some(thread) }
    }

    /// How far the screen is turned, in degrees as the sensor counts them; `None` without a reading.
    pub fn degrees(&self) -> Option<u32> {
        Some(self.shared.degrees.load(Ordering::Relaxed)).filter(|&degrees| degrees != UNKNOWN)
    }

    /// Asks the reader to look for the sensor again right away if it is not connected.
    pub fn retry(&self) {
        let _ = self.wake.send(());
    }
}

impl Drop for Orientation {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        let _ = self.wake.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ")
}

/// Prints everything the interface offers: its report descriptor, what the reports read right now,
/// and what it sends by itself during `seconds`. Fails while the tray program holds the sensor.
pub fn probe(seconds: f64) -> Result<(), String> {
    let interface = Interface::open(HARDWARE_ID, INTERFACE_NUMBER, 500)?;
    match interface.descriptor() {
        Some(descriptor) => println!("report descriptor, {} bytes:\n{}", descriptor.len(), hex(&descriptor)),
        None => println!("report descriptor: could not be read"),
    }
    println!("listening for {seconds} s");
    let start = Instant::now();
    let mut buffer = vec![0u8; interface.packet_size()];
    while start.elapsed() < Duration::from_secs_f64(seconds) {
        match interface.read(&mut buffer) {
            Read::Report(length) if length > 0 => println!("{:7.2} s  {}  angles {:?}", start.elapsed().as_secs_f64(), hex(&buffer[..length]), angles(&buffer[..length])),
            Read::Report(_) | Read::Quiet => {}
            Read::Failed(error) => return Err(format!("read failed, error {error}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::angles;

    #[test]
    fn reads_the_angles_of_recorded_reports() {
        assert_eq!(angles(&[1, 0, 0, 0, 0]), Some([0, 0, 0]));
        assert_eq!(angles(&[1, 0x00, 0x1c, 0x02, 0x00]), Some([0, 270, 0])); // turned a quarter clockwise
        assert_eq!(angles(&[2, 0, 0, 0, 0]), None);
        assert_eq!(angles(&[1, 0]), None);
    }
}
