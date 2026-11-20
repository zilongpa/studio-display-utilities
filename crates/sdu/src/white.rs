//! True Tone as the tray program runs it: the white the screen shows right now, where it is heading,
//! and the way of changing it the user chose.
//!
//! Which white is wanted comes from `sdu_core::truetone`; how it gets onto the screen from one of the
//! `sdu_windows::whitepoint` methods. This only moves the one towards the other, gradually.

use sdu_core::fade::Fade;
use sdu_core::truetone::{self, Xy, NEUTRAL};
use sdu_core::whitepoint::{Method, WhitePoint};
use sdu_windows::util::log;
use sdu_windows::whitepoint;
use std::time::Instant;

/// A new target closer than this (in u'v') to the present one is not worth a fade: the sensors'
/// readings wander a little, and a difference this small cannot be seen.
const DEAD_BAND: f64 = 0.0006;
/// Seconds for the white to come back after a brightness change in HDR took it away.
const RETURN_SECONDS: f64 = 1.5;

pub struct White {
    method: Method,
    /// Opened when the white first has to differ from the display's own, closed when it is back there.
    backend: Option<Box<dyn WhitePoint>>,
    /// The white on the screen now, and the one it is heading for.
    white: Xy,
    target: Xy,
    /// The fade in progress: where it started, its progress from 0 to 1, and when it began.
    fade: Option<(Xy, Fade, Instant)>,
    /// Why the white could not be changed, if it could not; shown in the menu.
    problem: Option<String>,
    /// The brightness is moving in HDR and the method cannot hold the white meanwhile: the screen is
    /// at its own white and stays there until `resume`.
    paused: bool,
}

impl White {
    pub fn new(method: Method) -> White {
        White { method, backend: None, white: NEUTRAL, target: NEUTRAL, fade: None, problem: None, paused: false }
    }

    pub fn white(&self) -> Xy {
        self.white
    }

    pub fn method(&self) -> Method {
        self.method
    }

    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// Heads for a new white. Returns whether a fade is now running, that is, whether `frame` wants
    /// to be called regularly.
    pub fn aim(&mut self, white: Xy) -> bool {
        if truetone::distance(white, self.target) >= DEAD_BAND || (white == NEUTRAL && self.target != NEUTRAL) {
            self.target = white;
            if !self.paused {
                self.fade = Some((self.white, Fade::new(0.0, 1.0, truetone::FADE_SECONDS), Instant::now()));
            }
        }
        self.fade.is_some()
    }

    /// The brightness is about to move in HDR. If that undoes the method's work, the white is given
    /// up now (the system is about to take it away anyway) until `resume`. This holds whether or not
    /// the white has been put on the screen yet: a white fading in while the brightness moves would be
    /// wiped just the same, and nothing could tell.
    pub fn pause(&mut self, hdr: bool) {
        if hdr && !self.paused && whitepoint::undone_by_hdr_brightness(self.method) {
            self.paused = true;
            self.fade = None;
            self.backend = None;
            self.white = NEUTRAL;
        }
    }

    /// The brightness was changed in HDR by something else (the slider in Settings): if that wiped
    /// the white, it comes back as after a pause.
    pub fn disturbed(&mut self, hdr: bool) -> bool {
        self.pause(hdr);
        self.resume()
    }

    /// The brightness has stopped moving: the white comes back, gradually. Returns whether a fade is
    /// now running.
    pub fn resume(&mut self) -> bool {
        if self.paused {
            self.paused = false;
            if self.target != NEUTRAL {
                self.fade = Some((NEUTRAL, Fade::new(0.0, 1.0, RETURN_SECONDS), Instant::now()));
            }
        }
        self.fade.is_some()
    }

    /// One step of the fade. Returns whether it is still running.
    pub fn frame(&mut self, hdr: bool) -> bool {
        let Some((from, fade, start)) = self.fade else { return false };
        let elapsed = start.elapsed().as_secs_f64();
        self.white = if fade.finished(elapsed) { self.target } else { truetone::mix(from, self.target, fade.at(elapsed)) };
        self.apply(hdr);
        if fade.finished(elapsed) {
            self.fade = None;
        }
        self.fade.is_some()
    }

    /// Puts the present white on the screen again: after the display changed between SDR and HDR.
    pub fn apply(&mut self, hdr: bool) {
        if self.paused {
            return;
        }
        if self.white == NEUTRAL {
            // Back at the display's own white: let go of the method, which undoes what it did.
            if self.backend.take().is_some() {
                log("white point: back to the display's own");
            }
            return;
        }
        if self.problem.is_some() {
            return;
        }
        if self.backend.is_none() {
            match whitepoint::create(self.method) {
                Ok(backend) => {
                    log(&format!("white point: using {}", backend.name()));
                    self.backend = Some(backend);
                }
                Err(error) => return self.fail(error),
            }
        }
        if let Some(Err(error)) = self.backend.as_mut().map(|backend| backend.set(self.white, hdr)) {
            self.fail(error);
        }
    }

    fn fail(&mut self, error: String) {
        log(&format!("white point: {error}"));
        self.backend = None;
        self.problem = Some(error);
    }

    /// Called every second: lets the method put its work back if something else undid it.
    pub fn keep(&mut self) {
        if self.paused {
            return;
        }
        if let Some(backend) = self.backend.as_mut() {
            backend.keep();
        }
    }

    /// The display was detected again (plugged in, woke up, changed mode): whatever was set may be
    /// gone or replaced, so the method starts afresh.
    pub fn reconnect(&mut self, hdr: bool) {
        self.backend = None;
        self.problem = None; // a refusal while the display was changing mode says nothing about now
        whitepoint::settle_driver();
        self.apply(hdr);
    }

    /// Switches to another way of changing the white, keeping the white.
    pub fn choose(&mut self, method: Method, hdr: bool) {
        self.backend = None;
        self.problem = None;
        self.method = method;
        self.apply(hdr);
    }
}
