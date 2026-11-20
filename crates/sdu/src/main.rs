//! Studio Display Utilities: brightness control for Apple Studio Displays on Windows.
//!
//! - Automatic brightness from the display's ambient light sensor, on the curve macOS uses.
//! - The keyboard's brightness keys: they change Windows' own brightness value and show the system's
//!   brightness bar. This program follows that value and keeps it in step with the real brightness,
//!   so the bar is always right.
//! - Works in SDR (the display's backlight) and in HDR (Windows' SDR content brightness).
//! - True Tone: the screen's white follows the colour of the ambient light (see `white`).
//!
//! - `sdu hdr` makes Windows offer HDR on an AMD card (see `amd` and `edid`).
//!
//! One hidden window on one thread does everything; the only other thread waits for sensor reports.
#![windows_subsystem = "windows"]

mod config;
mod icon;
mod white;

use config::Config;
use sdu_core::fade::Fade;
use sdu_core::whitepoint::Method;
use sdu_core::{auto, edid, scale, truetone};
use sdu_windows::backlight::{self, Backlight};
use sdu_windows::ffi::*;
use sdu_windows::hdr::{self, Hdr};
use sdu_windows::sensor::{self, Sensor};
use sdu_windows::util::{log, wide};
use sdu_windows::orientation::Orientation;
use sdu_windows::presets::{self, Presets};
use sdu_windows::{amd, autostart, elevate, rotation, system, whitepoint};
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::time::Instant;
use white::White;

const FRAME_MS: u32 = 16;
/// Steps of a white point fade.
const WHITE_FRAME_MS: u32 = 33;
const SETTLE_MS: u32 = 1500;

const MSG_SENSOR: u32 = WM_APP + 1;
const MSG_TRAY: u32 = WM_APP + 2;
const MSG_HDR_DONE: u32 = WM_APP + 3;
const MSG_ORIENTATION: u32 = WM_APP + 4;
/// How long the screen has to rest in a new position before the picture follows.
const ROTATE_AFTER_MS: u32 = 700;
/// Seconds before HDR is switched on again after an attempt, so a display that cannot do it is not
/// asked over and over.
const HDR_RETRY_SECONDS: u64 = 60;
const TIMER_FADE: usize = 1;
const TIMER_STATE: usize = 2;
const TIMER_RECONNECT: usize = 4;
const TIMER_WHITE: usize = 5;
const TIMER_PRESETS: usize = 6;
const TIMER_ROTATE: usize = 7;
const MENU_AUTO: usize = 1;
const MENU_RESET: usize = 2;
const MENU_EXIT: usize = 3;
const MENU_TRUETONE: usize = 4;
const MENU_AUTOSTART: usize = 5;
const MENU_HDR: usize = 6;
const MENU_ROTATE: usize = 7;
const MENU_SENSORS: usize = 8;
/// Followed by one id for each of the display's reference modes.
const MENU_PRESET: usize = 200;
/// Followed by one id for each of `Method::ALL`.
const MENU_METHOD: usize = 100;

/// A line of the tray menu.
enum Item {
    /// Id (0: shows information only), flags, text.
    Line(usize, u32, String),
    /// A submenu: its title and its lines.
    Sub(String, Vec<(usize, u32, String)>),
}

struct App {
    hwnd: HWND,
    icon: HANDLE,
    sensor: Sensor,
    orientation: Orientation,
    backlight: Option<Backlight>,
    hdr: Hdr,
    /// Windows outputs HDR: brightness is the SDR content brightness, not the backlight.
    hdr_active: bool,
    config: Config,
    /// Brightness in nits as last read or written; only meaningful while `known`.
    current: f64,
    target: f64,
    known: bool,
    fading: bool,
    /// The fade in progress, in slider positions (see `scale`), and when it began.
    fade: Fade,
    fade_start: Instant,
    white: White,
    /// The ways of changing the white this system offers.
    methods: Vec<Method>,
    /// The display's reference modes by name, as read when the display was last found, and the number
    /// of the one in use.
    presets: Vec<String>,
    preset: Option<u32>,
    preset_tries: u32,
    /// HDR is being switched on in the background; and when that was last started.
    hdr_busy: bool,
    hdr_tried: Option<Instant>,
    /// For testing True Tone without changing the room's light: the environment variable
    /// SDU_AMBIENT_XY ("0.44,0.41") stands in for the chromaticity the sensors read.
    pretend_ambient: Option<truetone::Xy>,
    ticks: u32,
    /// Windows' brightness value (0 - 100) as last seen or written.
    percent: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the application state. Returns `None` before start-up has finished, or when a message
/// arrives while the state is already in use (a nested message loop); such a message is skipped.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut app| app.as_mut().map(f)))
}

impl App {
    fn range(&self) -> (f64, f64) {
        if self.hdr_active {
            (hdr::MIN_NITS, hdr::MAX_NITS)
        } else {
            (backlight::MIN_NITS, backlight::MAX_NITS)
        }
    }

    /// The part of the macOS slider (see `scale`) that the control in charge can reach. With the
    /// backlight that is all of it; in HDR it is the stretch from 80 to 480 nits.
    fn slider_span(&self) -> (f64, f64) {
        let (min, max) = self.range();
        (scale::slider_of(min), scale::slider_of(max))
    }

    /// Windows' brightness value (0 - 100) is treated as the macOS brightness slider, so every
    /// position gives the brightness the same position gives on a Mac. In HDR the 0 - 100 scale is
    /// laid over the reachable part of the slider instead, which keeps the shape and has no dead ends.
    fn percent_of(&self, nits: f64) -> u32 {
        let (low, high) = self.slider_span();
        ((scale::slider_of(nits) - low) / (high - low) * 100.0).round().clamp(0.0, 100.0) as u32
    }

    fn nits_of(&self, percent: u32) -> f64 {
        let (low, high) = self.slider_span();
        scale::nits_at(low + (high - low) * percent.min(100) as f64 / 100.0)
    }

    /// Writes the brightness the display is heading for into Windows' brightness value.
    fn sync_percent(&mut self) {
        if !self.known {
            return;
        }
        let percent = self.percent_of(self.target);
        if percent != self.percent {
            self.percent = percent;
            if !system::write(percent) {
                log("system brightness value could not be written");
            }
        }
    }

    /// Windows' brightness value changed (the keyboard's brightness keys, usually).
    fn system_changed(&mut self, percent: u32) {
        if percent == self.percent {
            return; // our own write coming back
        }
        self.percent = percent;
        if !self.known {
            return;
        }
        self.target = self.nits_of(percent);
        self.remember_adjustment();
        self.start_fade(true);
    }

    /// What the curve asks for right now, before the user's adjustment; `None` if automatic
    /// brightness is off or the sensor has no reading.
    fn curve_now(&self) -> Option<f64> {
        if self.config.auto {
            self.sensor.lux().map(auto::brightness)
        } else {
            None
        }
    }

    /// Takes a brightness that was just read from the control in charge as the starting point.
    fn adopt(&mut self, nits: f64) {
        self.current = nits;
        self.target = nits;
        self.known = true;
        self.retarget(false);
    }

    /// Finds out which control is in charge (HDR or SDR) and what it is set to.
    fn refresh_state(&mut self) {
        let white = self.hdr.refresh();
        let was_hdr = self.hdr_active;
        self.hdr_active = white.is_some();
        if was_hdr != self.hdr_active {
            self.white.apply(self.hdr_active);
        }
        match white {
            Some(nits) => {
                if !was_hdr || !self.known {
                    log(&format!("HDR: SDR content brightness is {nits:.1} nits"));
                    self.adopt(nits);
                } else if !self.fading && (nits - (self.current * 12.5).round() / 12.5).abs() > 0.2 {
                    // Somebody moved the slider in Settings: treat it like the brightness keys.
                    log(&format!("HDR: SDR content brightness changed elsewhere to {nits:.1} nits"));
                    self.current = nits;
                    self.target = nits;
                    self.remember_adjustment();
                }
            }
            None => {
                // The backlight is only read when it takes over; it is not polled.
                if self.backlight.is_none() && (was_hdr || self.ticks % 5 == 0) {
                    self.backlight = Backlight::open();
                    if self.backlight.is_some() {
                        log("backlight: connected");
                        self.known = false;
                    }
                }
                if was_hdr || !self.known {
                    match self.backlight.as_ref().and_then(Backlight::get) {
                        Some(nits) => {
                            log(&format!("SDR: backlight is {nits:.1} nits"));
                            self.adopt(nits);
                        }
                        None => self.known = false,
                    }
                }
            }
        }
        self.ticks = self.ticks.wrapping_add(1);
    }

    /// Stores the user's adjustment: the ratio between the chosen brightness and what the curve asks for.
    fn remember_adjustment(&mut self) {
        if let Some(base) = self.curve_now() {
            self.config.factor = (self.target / base).clamp(config::MIN_FACTOR, config::MAX_FACTOR);
            self.config.save();
        }
    }

    /// Recomputes the target from the ambient light and starts fading towards it.
    fn retarget(&mut self, fast: bool) {
        if !self.known {
            return;
        }
        if let Some(base) = self.curve_now() {
            let (min, max) = self.range();
            self.target = (base * self.config.factor).clamp(min, max);
        }
        self.start_fade(fast);
    }

    /// Starts (or redirects) the fade from the current brightness to the target.
    fn start_fade(&mut self, manual: bool) {
        if (self.target - self.current).abs() < 0.02 {
            self.sync_percent();
            return;
        }
        let seconds = if manual {
            auto::FADE_MANUAL
        } else if self.target > self.current {
            auto::FADE_BRIGHTER
        } else {
            auto::FADE_DARKER
        };
        self.fade = Fade::new(scale::slider_of(self.current), scale::slider_of(self.target), seconds);
        self.fade_start = Instant::now();
        self.white.pause(self.hdr_active);
        if !self.fading {
            self.fading = true;
            unsafe { SetTimer(self.hwnd, TIMER_FADE, FRAME_MS, null()) };
        }
    }

    fn stop_fade(&mut self) {
        if self.fading {
            self.fading = false;
            unsafe { KillTimer(self.hwnd, TIMER_FADE) };
            self.sync_percent();
            self.update_tip();
            if self.white.resume() {
                unsafe { SetTimer(self.hwnd, TIMER_WHITE, WHITE_FRAME_MS, null()) };
            }
        }
    }

    fn fade_frame(&mut self) {
        let elapsed = self.fade_start.elapsed().as_secs_f64();
        let finished = self.fade.finished(elapsed);
        let next = if finished { self.target } else { scale::nits_at(self.fade.at(elapsed)) };
        if !self.write(next) {
            self.stop_fade();
            return;
        }
        self.current = next;
        if finished {
            self.stop_fade();
        }
    }

    /// The white True Tone wants right now; `None` to leave it where it is.
    fn white_wanted(&self) -> Option<truetone::Xy> {
        if !self.config.truetone {
            return Some(truetone::NEUTRAL);
        }
        if let Some(ambient) = self.pretend_ambient {
            return Some(truetone::target(ambient));
        }
        let readings = self.sensor.colours();
        if readings.is_empty() {
            // No sensor, which also means no display: nothing to adapt.
            return Some(truetone::NEUTRAL);
        }
        match truetone::combine(&readings) {
            Some((lux, ambient)) if lux >= truetone::MIN_LUX => Some(truetone::target(ambient)),
            // Too dark for the sensors to tell the colour of the light: macOS holds the white too.
            _ => None,
        }
    }

    /// Recomputes the white from the ambient light and starts moving towards it.
    fn retarget_white(&mut self) {
        if let Some(white) = self.white_wanted() {
            if self.white.aim(white) {
                unsafe { SetTimer(self.hwnd, TIMER_WHITE, WHITE_FRAME_MS, null()) };
            }
        }
    }

    fn white_frame(&mut self) {
        if !self.white.frame(self.hdr_active) {
            unsafe { KillTimer(self.hwnd, TIMER_WHITE) };
        }
    }

    fn write(&mut self, nits: f64) -> bool {
        if self.hdr_active {
            let done = self.hdr.set_sdr_white(nits);
            if !done {
                log("HDR: SDR content brightness could not be set");
            }
            return done;
        }
        if self.brightness_fixed() {
            return false;
        }
        match &self.backlight {
            Some(backlight) if backlight.set(nits) => true,
            Some(_) => {
                log("backlight: write failed, disconnected");
                self.backlight = None;
                self.known = false;
                false
            }
            None => false,
        }
    }

    /// The screen has rested in a new position: turn the picture to match.
    fn rotate(&mut self) {
        if !self.config.rotate {
            return;
        }
        let Some(quarters) = self.orientation.degrees().and_then(rotation::for_degrees) else { return };
        if rotation::current() != Some(quarters) {
            log(&format!("screen turned to {:?} degrees: picture to position {quarters}: {:?}", self.orientation.degrees(), rotation::set(quarters)));
        }
    }

    /// Switches HDR on if the user wants it kept on and it is off: on an AMD card the driver forgets
    /// the EDID that makes Windows offer HDR whenever the display is detected afresh. Takes several
    /// seconds and makes the screen flicker, so it runs on its own thread.
    fn keep_hdr(&mut self) {
        if !self.config.hdr || self.hdr_busy || self.hdr_tried.is_some_and(|at| at.elapsed().as_secs() < HDR_RETRY_SECONDS) {
            return;
        }
        if self.hdr.hdr_offered_and_on().map_or(true, |(offered, on)| offered && on) {
            return; // on already, or no display to ask
        }
        log("HDR is off but wanted: switching it on");
        self.hdr_busy = true;
        self.hdr_tried = Some(Instant::now());
        let window = self.hwnd as isize;
        std::thread::spawn(move || {
            if let Err(error) = apply_hdr(None) {
                log(&format!("HDR could not be switched on: {error}"));
            }
            unsafe { PostMessageW(window as HWND, MSG_HDR_DONE, 0, 0) };
        });
    }

    /// Reads the display's reference modes and which one is in use.
    ///
    /// Right after the display appears or changes its mode it answers these requests unreliably (names
    /// come out shifted or the list ends early), so the list is read twice and only believed when both
    /// readings agree; otherwise it is tried again a little later.
    fn read_presets(&mut self) {
        unsafe { KillTimer(self.hwnd, TIMER_PRESETS) };
        self.presets.clear();
        self.preset = None;
        let Some(display) = Presets::open() else { return };
        let (first, second, active) = (display.names(), display.names(), display.active());
        if !first.is_empty() && first == second && active.is_some_and(|index| (index as usize) < first.len()) {
            self.presets = first;
            self.preset = active;
            self.preset_tries = 0;
            log(&format!("reference mode: {} ({} in all)", self.preset_name().unwrap_or("?"), self.presets.len()));
        } else if self.preset_tries < 5 {
            self.preset_tries += 1;
            unsafe { SetTimer(self.hwnd, TIMER_PRESETS, 2000, null()) };
        } else {
            log("reference modes could not be read reliably");
            self.preset_tries = 0;
        }
    }

    fn preset_name(&self) -> Option<&str> {
        self.presets.get(self.preset? as usize).map(String::as_str)
    }

    /// The reference mode in use has a fixed brightness (all but the "Apple ..." ones do, as on macOS).
    fn brightness_fixed(&self) -> bool {
        self.preset_name().is_some_and(|name| !presets::adjustable(name))
    }

    fn switch_preset(&mut self, index: u32) {
        let Some(name) = self.presets.get(index as usize).cloned() else { return };
        if self.hdr_active && !presets::works_in_hdr(&name) {
            return;
        }
        let done = Presets::open().is_some_and(|display| display.switch(index));
        log(&format!("reference mode: switching to {name}: {done}"));
        if done {
            self.preset = Some(index);
            // The display may drop off USB and come back; start over once it has settled.
            unsafe { SetTimer(self.hwnd, TIMER_RECONNECT, SETTLE_MS, null()) };
        }
    }

    /// Something about the hardware changed (device plugged, resume from sleep, display mode):
    /// start over with fresh handles once things have settled.
    fn reconnect(&mut self) {
        log("reconnecting");
        self.backlight = None;
        self.known = false;
        self.ticks = 0;
        self.sensor.retry();
        self.orientation.retry();
        self.read_presets();
        self.refresh_state();
        self.white.reconnect(self.hdr_active);
        self.keep_hdr();
    }

    fn tray(&self, action: u32) {
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = MSG_TRAY;
        data.hIcon = self.icon;
        let tip = if self.known { format!("Studio Display XDR \u{2013} {:.0} nits", self.target) } else { "Studio Display XDR \u{2013} not connected".to_string() };
        for (slot, unit) in data.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
            *slot = unit;
        }
        unsafe { Shell_NotifyIconW(action, &data) };
    }

    fn update_tip(&self) {
        self.tray(NIM_MODIFY);
    }

    fn menu_items(&self) -> Vec<Item> {
        let status = if self.known && self.brightness_fixed() && !self.hdr_active {
            "Brightness fixed by the reference mode".to_string()
        } else if self.known {
            format!("{:.0} nits ({})", self.target, if self.hdr_active { "HDR, SDR content brightness" } else { "backlight" })
        } else {
            "Display not connected".to_string()
        };
        let ambient = match (self.sensor.lux(), truetone::combine(&self.sensor.colours())) {
            (Some(lux), Some((total, colour))) if total >= truetone::MIN_LUX => format!("Ambient light: {lux:.0} lux, {:.0} K", truetone::kelvin_of(colour)),
            (Some(lux), _) => format!("Ambient light: {lux:.0} lux"),
            (None, _) => "Ambient light sensor not available".to_string(),
        };
        let white = match self.white.problem() {
            Some(problem) => format!("White point: {problem}"),
            None => {
                let (x, y) = self.white.white();
                format!("White point: {:.0} K (x {x:.4}, y {y:.4})", truetone::kelvin_of((x, y)))
            }
        };
        let adjustment = (self.config.factor - 1.0) * 100.0;
        let checked = |on: bool| if on { MF_CHECKED } else { MF_STRING };
        let methods = self
            .methods
            .iter()
            .map(|&method| {
                let id = MENU_METHOD + Method::ALL.iter().position(|&known| known == method).unwrap_or(0);
                (id, checked(method == self.white.method()), method.label().to_string())
            })
            .collect();
        let modes: Vec<(usize, u32, String)> = self
            .presets
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let flags = if self.preset == Some(index as u32) {
                    MF_CHECKED
                } else if self.hdr_active && !presets::works_in_hdr(name) {
                    MF_GRAYED // the display shows nothing in these while Windows outputs HDR
                } else {
                    MF_STRING
                };
                (MENU_PRESET + index, flags, name.clone())
            })
            .collect();
        let line = |id, flags, text: &str| Item::Line(id, flags, text.to_string());
        let mut items = vec![line(0, MF_GRAYED, &status), line(0, MF_GRAYED, &ambient)];
        if self.config.truetone {
            items.push(line(0, MF_GRAYED, &white));
        }
        items.extend([
            line(0, MF_SEPARATOR, ""),
            line(MENU_AUTO, checked(self.config.auto), "Automatic brightness"),
            line(MENU_RESET, if adjustment.abs() < 0.5 { MF_GRAYED } else { MF_STRING }, &format!("Reset adjustment ({adjustment:+.0} %)")),
            line(0, MF_SEPARATOR, ""),
            line(MENU_TRUETONE, checked(self.config.truetone), "True Tone"),
            Item::Sub("True Tone method".to_string(), methods),
            line(0, MF_SEPARATOR, ""),
        ]);
        if !modes.is_empty() {
            items.push(Item::Sub("Reference mode".to_string(), modes));
        }
        items.extend([
            line(MENU_HDR, checked(self.config.hdr), "Keep HDR on"),
        ]);
        if self.orientation.degrees().is_some() {
            items.push(line(MENU_ROTATE, checked(self.config.rotate), "Turn the picture with the screen"));
        }
        if self.known && (self.sensor.lux().is_none() || self.orientation.degrees().is_none()) {
            // The display is there but a sensor is not: its interface has no WinUSB binding yet.
            items.push(line(MENU_SENSORS, MF_STRING, "Install sensor drivers (administrator)..."));
        }
        items.extend([
            line(MENU_AUTOSTART, checked(autostart::enabled()), "Start at sign-in"),
            line(0, MF_SEPARATOR, ""),
            line(MENU_EXIT, MF_STRING, "Exit"),
        ]);
        items
    }

    fn menu_command(&mut self, command: usize) {
        match command {
            MENU_AUTO => {
                self.config.auto = !self.config.auto;
                self.config.save();
                self.retarget(true);
            }
            MENU_RESET => {
                self.config.factor = 1.0;
                self.config.save();
                self.retarget(true);
            }
            MENU_TRUETONE => {
                self.config.truetone = !self.config.truetone;
                self.config.save();
                // A method that failed gets another chance when True Tone is switched on again.
                if self.config.truetone && self.white.problem().is_some() {
                    self.white.choose(self.config.whitepoint, self.hdr_active);
                }
                self.retarget_white();
            }
            MENU_SENSORS => {
                // Waits for the user's consent and the installation, so not on this thread.
                let window = self.hwnd as isize;
                std::thread::spawn(move || {
                    match elevate::run("admin sensors") {
                        Ok(code) => log(&format!("sensor driver installation ended with code {code}")),
                        Err(error) => log(&format!("sensor driver installation: {error}")),
                    }
                    unsafe { PostMessageW(window as HWND, MSG_HDR_DONE, 0, 0) }; // look for the devices again
                });
            }
            MENU_ROTATE => {
                self.config.rotate = !self.config.rotate;
                self.config.save();
                self.rotate();
            }
            MENU_HDR => {
                self.config.hdr = !self.config.hdr;
                self.config.save();
                self.hdr_tried = None;
                self.keep_hdr();
            }
            MENU_AUTOSTART => {
                let on = !autostart::enabled();
                log(&format!("start at sign-in {on}: {}", autostart::set(on)));
            }
            MENU_EXIT => unsafe {
                DestroyWindow(self.hwnd);
            },
            _ if command >= MENU_PRESET => self.switch_preset((command - MENU_PRESET) as u32),
            _ => {
                if let Some(&method) = command.checked_sub(MENU_METHOD).and_then(|index| Method::ALL.get(index)) {
                    self.config.whitepoint = method;
                    self.config.save();
                    self.white.choose(method, self.hdr_active);
                }
            }
        }
    }
}

/// Shows the tray menu. The menu runs its own message loop, so the application state is not held
/// while it is open.
unsafe fn show_menu(hwnd: HWND) {
    let Some(items) = with_app(|app| app.menu_items()) else { return };
    let menu = CreatePopupMenu();
    for item in &items {
        match item {
            Item::Line(id, flags, text) => {
                AppendMenuW(menu, *flags, *id, wide(text).as_ptr());
            }
            Item::Sub(title, lines) => {
                // A submenu belongs to the menu it is put into and is destroyed with it.
                let sub = CreatePopupMenu();
                for (id, flags, text) in lines {
                    AppendMenuW(sub, *flags, *id, wide(text).as_ptr());
                }
                AppendMenuW(menu, MF_POPUP, sub as usize, wide(title).as_ptr());
            }
        }
    }
    let mut cursor = POINT::default();
    GetCursorPos(&mut cursor);
    SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, cursor.x, cursor.y, 0, hwnd, null());
    DestroyMenu(menu);
    if command > 0 {
        with_app(|app| app.menu_command(command as usize));
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
    match message {
        WM_TIMER => {
            match wparam {
                TIMER_FADE => {
                    with_app(App::fade_frame);
                }
                TIMER_STATE => {
                    with_app(|app| {
                        app.refresh_state();
                        app.white.keep();
                    });
                }
                TIMER_WHITE => {
                    with_app(App::white_frame);
                }
                TIMER_PRESETS => {
                    with_app(App::read_presets);
                }
                TIMER_ROTATE => {
                    KillTimer(hwnd, TIMER_ROTATE);
                    with_app(App::rotate);
                }
                TIMER_RECONNECT => {
                    KillTimer(hwnd, TIMER_RECONNECT);
                    with_app(App::reconnect);
                }
                _ => {}
            }
            0
        }
        MSG_SENSOR => {
            with_app(|app| {
                app.retarget(false);
                app.retarget_white();
            });
            0
        }
        MSG_ORIENTATION => {
            SetTimer(hwnd, TIMER_ROTATE, ROTATE_AFTER_MS, null());
            0
        }
        MSG_HDR_DONE => {
            with_app(|app| app.hdr_busy = false);
            SetTimer(hwnd, TIMER_RECONNECT, SETTLE_MS, null());
            0
        }
        MSG_TRAY => {
            if lparam as u32 == WM_RBUTTONUP || lparam as u32 == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_DEVICECHANGE if wparam == DBT_DEVNODES_CHANGED => {
            SetTimer(hwnd, TIMER_RECONNECT, SETTLE_MS, null());
            1
        }
        WM_POWERBROADCAST if wparam == PBT_POWERSETTINGCHANGE => {
            if let Some(percent) = system::changed(lparam) {
                with_app(|app| app.system_changed(percent));
            }
            1
        }
        WM_POWERBROADCAST if wparam == PBT_APMRESUMEAUTOMATIC => {
            SetTimer(hwnd, TIMER_RECONNECT, SETTLE_MS, null());
            1
        }
        WM_DISPLAYCHANGE => {
            with_app(App::refresh_state);
            0
        }
        WM_DESTROY => {
            with_app(|app| app.tray(NIM_DELETE));
            PostQuitMessage(0);
            0
        }
        _ => {
            // Explorer restarted: the tray icon has to be added again.
            if message == TASKBAR_CREATED.with(|id| id.get()) {
                with_app(|app| app.tray(NIM_ADD));
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
    }
}

thread_local! {
    static TASKBAR_CREATED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Prints to the console the command was started from (if any) and writes to the log.
fn say(text: &str) {
    println!("{text}");
    log(text);
}

/// `sdu hdr`: makes Windows offer HDR for the display on an AMD card and switches it on.
///
/// The AMD driver is given an EDID built from the display's own description. Nothing is done if
/// Windows already offers HDR. With `folder`, nothing is changed: the display's description and the
/// EDID that would be used are written there instead, for inspection.
fn apply_hdr(folder: Option<&str>) -> Result<(), String> {
    let mut hdr = Hdr::default();
    let offered = |hdr: &mut Hdr| hdr.hdr_offered_and_on().is_some_and(|(offered, _)| offered);

    if folder.is_none() && offered(&mut hdr) {
        say("Windows already offers HDR for the display; the EDID is left alone.");
    } else {
        let display = amd::Display::find()?;
        say(&format!("Found {}.", display.description));
        let mut description = display.description_data()?;
        if description.starts_with(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]) {
            // The driver already uses an EDID; the connector still tells what the display itself says.
            description = display.real_description().ok_or("the driver already uses an EDID for this display and does not give the display's own description; replug the display or restart")?;
        }
        let native = edid::Native::parse(&description)?;
        let edid = native.edid();
        say(&format!("Read {} bytes of DisplayID from the display: {}, {} timings; built an EDID of {} bytes.", description.len(), native.name, native.timings.len(), edid.len()));
        if let Some(folder) = folder {
            let folder = std::path::Path::new(folder);
            std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
            std::fs::write(folder.join("displayid-from-display.bin"), &description).map_err(|e| e.to_string())?;
            std::fs::write(folder.join("edid-generated.bin"), &edid).map_err(|e| e.to_string())?;
            say(&format!("Written to {}; nothing was changed.", folder.display()));
            return Ok(());
        }
        display.override_edid(&edid)?;
        say("The driver accepted the EDID; waiting for Windows to detect the display again.");
        let started = Instant::now();
        while !offered(&mut hdr) {
            if started.elapsed().as_secs() > 30 {
                return Err("Windows still offers no HDR after 30 seconds".into());
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        say("Windows now offers HDR.");
    }

    if hdr.hdr_offered_and_on() == Some((true, false)) {
        let done = hdr.enable_hdr();
        std::thread::sleep(std::time::Duration::from_secs(3));
        say(&format!("HDR switched on: {}.", done && hdr.hdr_offered_and_on() == Some((true, true))));
    } else {
        say("HDR is on.");
    }
    Ok(())
}

/// The script that binds the sensor interfaces to WinUSB, carried inside the program.
const SENSOR_DRIVER_SCRIPT: &str = include_str!("../../../tools/install-sensor-driver.ps1");

/// `sdu admin <task>`: the things that need administrator rights. Started without them, the program
/// starts itself again elevated (Windows asks for consent) and waits for that to finish.
///
/// - `sensors`: binds the light and the orientation sensor to WinUSB (what Zadig does by hand)
/// - `sensors-remove`: undoes that (the tray program must not be running: it holds the sensors open)
fn admin(task: &str) -> Result<(), String> {
    if !elevate::is_elevated() {
        let lines_before = std::fs::read_to_string(sdu_windows::util::data_dir().join("log.txt")).map(|text| text.lines().count()).unwrap_or(0);
        let code = elevate::run(&format!("admin {task}"))?;
        // the elevated instance has no console: show what it wrote to the log
        if let Ok(text) = std::fs::read_to_string(sdu_windows::util::data_dir().join("log.txt")) {
            for line in text.lines().skip(lines_before) {
                println!("{line}");
            }
        }
        return if code == 0 { Ok(()) } else { Err(format!("the elevated part ended with code {code}")) };
    }
    match task {
        "sensors" | "sensors-remove" => {
            let script = std::env::temp_dir().join("sdu-install-sensor-driver.ps1");
            std::fs::write(&script, SENSOR_DRIVER_SCRIPT).map_err(|error| error.to_string())?;
            let output = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&script)
                .arg(if task == "sensors" { "-Orientation" } else { "-Remove" })
                .output()
                .map_err(|error| error.to_string())?;
            let _ = std::fs::remove_file(&script);
            for line in String::from_utf8_lossy(&output.stdout).lines().chain(String::from_utf8_lossy(&output.stderr).lines()) {
                if !line.trim().is_empty() {
                    say(line.trim_end());
                }
            }
            output.status.success().then_some(()).ok_or("the script failed".into())
        }
        _ => Err("admin sensors | sensors-remove".into()),
    }
}

/// "0.3239,0.3463" as a chromaticity.
fn parse_xy(text: &str) -> Option<truetone::Xy> {
    let (x, y) = text.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// `sdu white <method> <kelvin | x,y> [seconds]`: shows a white point with one of the methods for a
/// while (five seconds unless told otherwise), then puts everything back. For trying the methods out.
fn show_white(method: &str, white: Option<&String>, seconds: Option<&String>) -> Result<(), String> {
    let keys: Vec<&str> = Method::ALL.iter().map(|method| method.key()).collect();
    let method = Method::from_key(method).ok_or(format!("the methods are: {}", keys.join(", ")))?;
    let white = white
        .and_then(|text| parse_xy(text).or_else(|| text.parse::<f64>().ok().map(truetone::white_of_kelvin)))
        .ok_or("which white: a colour temperature in kelvin, or x,y?")?;
    let seconds = seconds.and_then(|text| text.parse::<f64>().ok()).unwrap_or(5.0);
    let hdr = Hdr::default().refresh().is_some();
    let mut backend = whitepoint::create(method)?;
    backend.set(white, hdr)?;
    say(&format!("Showing x {:.4}, y {:.4} with the {} method ({}) for {seconds:.0} seconds.", white.0, white.1, backend.name(), if hdr { "HDR" } else { "SDR" }));
    std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
    backend.reset()
}

fn main() {
    // Commands: they do their job and exit, the tray program is not started.
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some(command) = arguments.first() {
        unsafe { AttachConsole(u32::MAX) }; // the parent's console, if there is one
        let result = match (command.as_str(), arguments.get(1)) {
            ("hdr", None) => apply_hdr(None),
            ("admin", Some(task)) => admin(task),
            ("hdr", Some(word)) if word == "off" => {
                let mut hdr = Hdr::default();
                hdr.hdr_offered_and_on();
                hdr.set_hdr(false).then_some(()).ok_or("HDR could not be switched off".into())
            }
            // `backlight <nits> [milliseconds]`: sets the backlight, with the report's "Duration" field.
            ("backlight", Some(nits)) => match (Backlight::open(), nits.parse::<f64>()) {
                (Some(backlight), Ok(nits)) => {
                    let milliseconds = arguments.get(2).and_then(|text| text.parse::<u16>().ok()).unwrap_or(0);
                    let done = backlight.set_over(nits, milliseconds);
                    println!("set {nits} nits over {milliseconds} ms: {done}; reads back {:?}", backlight.get());
                    Ok(())
                }
                _ => Err("backlight <nits> [milliseconds]".into()),
            },
            // `sensor [seconds]`: prints what the two light sensors read, four times a second.
            ("sensor", seconds) => {
                let seconds = seconds.and_then(|text| text.parse::<f64>().ok()).unwrap_or(0.0);
                sensor::sample(std::time::Duration::from_secs_f64(seconds), std::time::Duration::from_millis(250), |readings| {
                    let line: Vec<String> = readings.iter().map(|r| format!("{} {:.0} {} {:.4} {:.4}", r.sensor, r.lux, r.kelvin, r.x, r.y)).collect();
                    println!("{}", line.join("  "));
                })
            }
            ("edid", Some(folder)) => apply_hdr(Some(folder)),
            // `icon <file.bmp> [size]`: writes the program's icon to a file.
            ("icon", Some(file)) => icon::save(file, arguments.get(2).and_then(|size| size.parse().ok()).unwrap_or(256)),
            // `presets`: lists the display's reference modes; `presets <number>` switches to one.
            ("presets", number) => match Presets::open() {
                Some(display) => {
                    if let Some(number) = number.and_then(|text| text.parse::<u32>().ok()) {
                        println!("switched: {}", display.switch(number));
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                    let active = Presets::open().and_then(|display| display.active());
                    for (index, name) in display.names().iter().enumerate() {
                        println!("{} {index:2} {name}", if active == Some(index as u32) { '*' } else { ' ' });
                    }
                    Ok(())
                }
                None => Err("the display's reference modes cannot be reached".into()),
            },
            // `orientation [seconds]`: prints what the orientation sensor offers and sends.
            ("orientation", seconds) => sdu_windows::orientation::probe(seconds.and_then(|text| text.parse().ok()).unwrap_or(5.0)),
            ("white", Some(method)) => show_white(method, arguments.get(2), arguments.get(3)),
            _ => Err("usage: sdu [hdr | edid <folder> | sensor [seconds] | presets [number] | orientation [seconds] | admin sensors | admin sensors-remove | white <method> <kelvin | x,y> [seconds]]".into()),
        };
        if let Err(error) = &result {
            say(&format!("Failed: {error}"));
        }
        std::process::exit(result.is_err() as i32);
    }

    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // One instance only: two would fight over the sensor and the keys.
        let mutex = CreateMutexW(null(), 0, wide("Local\\studio-display-utilities").as_ptr());
        if mutex.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        let instance = GetModuleHandleW(null());
        let class_name = wide("StudioDisplayUtilities");
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&class);
        // A hidden top-level window (not message-only): only those are told about device changes.
        let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, class_name.as_ptr(), wide("Studio Display Utilities").as_ptr(), WS_POPUP, 0, 0, 0, 0, null_mut(), null_mut(), instance, null_mut());
        if hwnd.is_null() {
            log(&format!("could not create the window, error {}", GetLastError()));
            return;
        }
        log("started");
        TASKBAR_CREATED.with(|id| id.set(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr())));

        let config = Config::load();
        let mut app = App {
            hwnd,
            icon: icon::create(),
            sensor: Sensor::start(hwnd, MSG_SENSOR),
            orientation: Orientation::start(hwnd, MSG_ORIENTATION),
            backlight: None,
            hdr: Hdr::default(),
            hdr_active: false,
            config,
            current: 0.0,
            target: 0.0,
            known: false,
            fading: false,
            fade: Fade::new(0.0, 0.0, 0.0),
            fade_start: Instant::now(),
            white: White::new(config.whitepoint),
            methods: whitepoint::available(),
            presets: Vec::new(),
            preset: None,
            preset_tries: 0,
            hdr_busy: false,
            hdr_tried: None,
            pretend_ambient: std::env::var("SDU_AMBIENT_XY").ok().and_then(|text| parse_xy(&text)),
            ticks: 0,
            percent: system::read().unwrap_or(50),
        };
        app.read_presets();
        app.refresh_state();
        app.keep_hdr();
        whitepoint::settle_driver();
        app.retarget_white();
        app.tray(NIM_ADD);
        APP.with(|cell| *cell.borrow_mut() = Some(app));

        if !system::subscribe(hwnd) {
            log(&format!("system brightness changes could not be subscribed to, error {}", GetLastError()));
        }
        SetTimer(hwnd, TIMER_STATE, 1000, null());

        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        // Dropping the state stops the sensor thread and closes the device handles.
        APP.with(|cell| cell.borrow_mut().take());
        log("stopped");
        CloseHandle(mutex);
    }
}
