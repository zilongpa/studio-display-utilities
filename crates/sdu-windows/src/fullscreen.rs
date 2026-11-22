//! Whether a program fills the Studio Display's whole screen.
//!
//! Such a program's picture can go to the display past the desktop's compositor, with the mouse
//! pointer as a layer of the graphics card's own. In HDR that layer is not brought down to the SDR
//! brightness while a gamma ramp other than a straight line is set: the pointer then shines at the
//! display's peak. So True Tone can step aside for as long as a program has the whole screen.
//!
//! What goes wrong is settled at the moment the program takes the screen: taking the ramp away
//! afterwards does not mend the pointer until the program has left the screen and come back. So a
//! program once seen with the whole screen counts for as long as its window lives and is either still
//! that size or minimised, also while the user is in another window: the white must not be back by
//! the time they return.

use crate::ffi::*;
use crate::util::wide;

/// MONITORINFOEXW
#[repr(C)]
struct MonitorInfo {
    size: u32,
    monitor: RECT,
    work: RECT,
    flags: u32,
    device: [u16; 32],
}

const MONITOR_DEFAULTTONEAREST: u32 = 2;

/// Remembers the program that has the whole screen.
#[derive(Default)]
pub struct Watch {
    /// The window last seen in front with the whole screen (0: none).
    window: isize,
}

impl Watch {
    /// Whether a program has the whole screen, or has had it and may take it again at any moment.
    pub fn active(&mut self) -> bool {
        unsafe {
            let front = GetForegroundWindow();
            if covers(front) {
                self.window = front as isize;
                return true;
            }
            let window = self.window as HWND;
            if self.window != 0 && IsWindow(window) != 0 && (IsIconic(window) != 0 || (IsWindowVisible(window) != 0 && covers(window))) {
                return true;
            }
            self.window = 0;
            false
        }
    }
}

/// Whether the window covers all of the Studio Display's screen (of whichever screen it is on, if
/// the display's cannot be told apart). The desktop itself does not count.
fn covers(window: HWND) -> bool {
    unsafe {
        if window.is_null() || window == GetShellWindow() || window == GetDesktopWindow() {
            return false;
        }
        let mut class = [0u16; 64];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
        let class = String::from_utf16_lossy(&class[..length]);
        if class == "WorkerW" || class == "Progman" {
            return false; // the desktop's icons and wallpaper
        }
        let mut bounds = RECT::default();
        let mut info = MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32, monitor: RECT::default(), work: RECT::default(), flags: 0, device: [0; 32] };
        if GetWindowRect(window, &mut bounds) == 0 || GetMonitorInfoW(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST), &mut info as *mut MonitorInfo as *mut _) == 0 {
            return false;
        }
        let screen = info.monitor;
        if bounds.left > screen.left || bounds.top > screen.top || bounds.right < screen.right || bounds.bottom < screen.bottom {
            return false;
        }
        match crate::hdr::gdi_device() {
            Some(device) => {
                let device = wide(&device);
                let name = &device[..device.len() - 1];
                info.device.starts_with(name) && info.device.get(name.len()) == Some(&0)
            }
            None => true,
        }
    }
}
