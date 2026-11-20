//! A value moving from one level to another over a fixed time, at a steady pace.
//!
//! What "steady" means is up to the caller, who chooses the scale the value is expressed in:
//! brightness fades along the brightness slider, colour temperature along the mired scale.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub from: f64,
    pub to: f64,
    pub seconds: f64,
}

impl Fade {
    pub fn new(from: f64, to: f64, seconds: f64) -> Fade {
        Fade { from, to, seconds }
    }

    /// The value `elapsed` seconds after the start.
    pub fn at(&self, elapsed: f64) -> f64 {
        if self.finished(elapsed) {
            self.to
        } else {
            self.from + (self.to - self.from) * (elapsed / self.seconds).max(0.0)
        }
    }

    pub fn finished(&self, elapsed: f64) -> bool {
        self.seconds <= 0.0 || elapsed >= self.seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_steadily_and_stops_exactly() {
        let fade = Fade::new(10.0, 20.0, 4.0);
        assert_eq!(fade.at(0.0), 10.0);
        assert_eq!(fade.at(1.0), 12.5);
        assert_eq!(fade.at(4.0), 20.0);
        assert_eq!(fade.at(9.0), 20.0);
        assert!(!fade.finished(3.9) && fade.finished(4.0));
        assert_eq!(Fade::new(1.0, 2.0, 0.0).at(0.0), 2.0);
    }
}
