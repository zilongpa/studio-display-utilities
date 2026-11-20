//! Builds a classic EDID from what the display itself reports.
//!
//! The Studio Display XDR describes itself only with DisplayID 2.0. The AMD driver understands that,
//! but hands Windows no monitor description, so Windows sees a "default monitor" without HDR. Giving
//! the driver an EDID to use instead fixes that. The EDID is not a canned file: everything in it is
//! taken from the display's own DisplayID data:
//!
//! - identity, size, primaries and white point, gamma, bit depth
//! - the HDR metadata and colorimetry (the CTA data blocks, copied byte for byte)
//! - the variable refresh range
//! - the timings: the detailed one the display lists (5120 x 2880 @ 120), and the ones it lists by
//!   formula (resolution + refresh rate), worked out with VESA CVT reduced blanking v2
//!
//! Layout, four blocks (the most the driver's override call takes):
//!
//!   0  base EDID 1.4, two detailed timings
//!   1  CTA-861 extension: HDR / colorimetry data blocks, more detailed timings
//!   2  DisplayID 1.2 extension: Type I detailed timings (a classic detailed timing cannot describe
//!   3  DisplayID 1.2 extension    anything wider than 4095 pixels or faster than 655 MHz)

const BLOCK: usize = 128;
/// PNP manufacturer id "APP". DisplayID 2.0 carries Apple's IEEE OUI instead, which has no PNP form.
const MANUFACTURER_APP: [u8; 2] = [0x06, 0x10];

/// Refresh rates offered besides the ones the display lists, where its variable refresh range allows.
const COMMON_RATES: [u32; 3] = [60, 50, 48];

const TAG_PRODUCT: u8 = 0x20;
const TAG_PARAMETERS: u8 = 0x21;
const TAG_TIMING_DETAILED: u8 = 0x22; // Type VII
const TAG_INTERFACE: u8 = 0x26;
const TAG_TIMING_FORMULA: u8 = 0x2A; // Type IX
const TAG_ADAPTIVE_SYNC: u8 = 0x2B;
const TAG_CTA: u8 = 0x81;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub clock_khz: u32,
    pub h_active: u32,
    pub h_blank: u32,
    pub h_front: u32,
    pub h_sync: u32,
    pub h_positive: bool,
    pub v_active: u32,
    pub v_blank: u32,
    pub v_front: u32,
    pub v_sync: u32,
    pub v_positive: bool,
    /// The display's own detailed timing: listed first and marked as preferred.
    pub native: bool,
}

impl Timing {
    fn h_total(&self) -> u32 {
        self.h_active + self.h_blank
    }

    fn v_total(&self) -> u32 {
        self.v_active + self.v_blank
    }

    pub fn refresh(&self) -> f64 {
        self.clock_khz as f64 * 1000.0 / (self.h_total() as f64 * self.v_total() as f64)
    }

    fn h_khz(&self) -> f64 {
        self.clock_khz as f64 / self.h_total() as f64
    }

    /// VESA CVT 1.2 reduced blanking v2: fixed 80 pixel horizontal blanking, vertical blanking of at
    /// least 460 microseconds.
    pub fn cvt_rb2(h_active: u32, v_active: u32, refresh: u32) -> Timing {
        const H_BLANK: u32 = 80;
        const H_FRONT: u32 = 8;
        const H_SYNC: u32 = 32;
        const V_SYNC: u32 = 8;
        const V_BACK: u32 = 6;
        const MIN_V_BLANK_US: f64 = 460.0;
        let line_us = (1_000_000.0 / refresh as f64 - MIN_V_BLANK_US) / v_active as f64;
        let v_blank = ((MIN_V_BLANK_US / line_us).floor() as u32 + 1).max(1 + V_SYNC + V_BACK);
        let clock_khz = (refresh as u64 * (v_active + v_blank) as u64 * (h_active + H_BLANK) as u64 / 1000) as u32;
        Timing {
            clock_khz,
            h_active,
            h_blank: H_BLANK,
            h_front: H_FRONT,
            h_sync: H_SYNC,
            h_positive: true,
            v_active,
            v_blank,
            v_front: v_blank - V_SYNC - V_BACK,
            v_sync: V_SYNC,
            v_positive: false,
            native: false,
        }
    }

    /// Whether an 18 byte EDID detailed timing descriptor can hold this timing.
    fn fits_descriptor(&self) -> bool {
        self.clock_khz <= 655_350
            && self.h_active <= 4095
            && self.v_active <= 4095
            && self.h_blank <= 4095
            && self.v_blank <= 4095
            && self.h_front <= 1023
            && self.h_sync <= 1023
            && self.v_front <= 63
            && self.v_sync <= 63
    }

    fn descriptor(&self, width_mm: u32, height_mm: u32) -> [u8; 18] {
        let clock = ((self.clock_khz + 5) / 10) as u16;
        let mut d = [0u8; 18];
        d[0..2].copy_from_slice(&clock.to_le_bytes());
        d[2] = self.h_active as u8;
        d[3] = self.h_blank as u8;
        d[4] = ((self.h_active >> 8) << 4 | self.h_blank >> 8) as u8;
        d[5] = self.v_active as u8;
        d[6] = self.v_blank as u8;
        d[7] = ((self.v_active >> 8) << 4 | self.v_blank >> 8) as u8;
        d[8] = self.h_front as u8;
        d[9] = self.h_sync as u8;
        d[10] = ((self.v_front & 0xF) << 4 | (self.v_sync & 0xF)) as u8;
        d[11] = ((self.h_front >> 8) << 6 | (self.h_sync >> 8) << 4 | (self.v_front >> 4) << 2 | self.v_sync >> 4) as u8;
        d[12] = width_mm as u8;
        d[13] = height_mm as u8;
        d[14] = ((width_mm >> 8) << 4 | (height_mm >> 8) & 0xF) as u8;
        d[17] = 0x18 | (self.v_positive as u8) << 2 | (self.h_positive as u8) << 1; // digital separate sync
        d
    }

    /// DisplayID Type I detailed timing (20 bytes).
    fn type_one(&self) -> [u8; 20] {
        let aspect = match (self.h_active * 9 == self.v_active * 16, self.h_active * 10 == self.v_active * 16) {
            (true, _) => 4,
            (_, true) => 5,
            _ => 8, // not one of the listed ratios
        };
        let mut t = [0u8; 20];
        t[0..3].copy_from_slice(&((self.clock_khz + 5) / 10 - 1).to_le_bytes()[..3]);
        t[3] = (self.native as u8) << 7 | aspect;
        let words = [
            self.h_active - 1,
            self.h_blank - 1,
            (self.h_front - 1) | (self.h_positive as u32) << 15,
            self.h_sync - 1,
            self.v_active - 1,
            self.v_blank - 1,
            (self.v_front - 1) | (self.v_positive as u32) << 15,
            self.v_sync - 1,
        ];
        for (index, word) in words.iter().enumerate() {
            t[4 + index * 2..6 + index * 2].copy_from_slice(&(*word as u16).to_le_bytes());
        }
        t
    }

    /// DisplayID 2.0 Type VII detailed timing: same layout, pixel clock in kHz.
    fn from_type_seven(t: &[u8]) -> Timing {
        let word = |at: usize| u16::from_le_bytes([t[at], t[at + 1]]) as u32;
        Timing {
            clock_khz: u32::from_le_bytes([t[0], t[1], t[2], 0]) + 1,
            h_active: word(4) + 1,
            h_blank: word(6) + 1,
            h_front: (word(8) & 0x7FFF) + 1,
            h_sync: word(10) + 1,
            h_positive: word(8) & 0x8000 != 0,
            v_active: word(12) + 1,
            v_blank: word(14) + 1,
            v_front: (word(16) & 0x7FFF) + 1,
            v_sync: word(18) + 1,
            v_positive: word(16) & 0x8000 != 0,
            native: true,
        }
    }
}

/// What is needed from the display's DisplayID 2.0 data.
#[derive(Default, Debug)]
pub struct Native {
    pub product_code: u16,
    pub serial: u32,
    pub week: u8,
    pub year: u16,
    pub name: String,
    pub width_mm: u32,
    pub height_mm: u32,
    pub width_cm: u8,
    pub height_cm: u8,
    /// Red, green, blue, white: x and y, 12 bit fractions.
    pub chromaticity: [(u16, u16); 4],
    /// EDID encoding: gamma * 100 - 100.
    pub gamma: u8,
    pub bits_per_colour: u8,
    pub min_refresh: Option<u8>,
    /// CTA-861 data blocks (HDR static metadata, colorimetry), as the display lists them.
    pub cta: Vec<u8>,
    pub timings: Vec<Timing>,
}

impl Native {
    /// Reads the DisplayID sections found in `data` (one after the other, each with its own header).
    pub fn parse(data: &[u8]) -> Result<Native, String> {
        if data.len() < 5 || data[0] >> 4 != 2 {
            return Err("this is not DisplayID 2.x data".into());
        }
        let mut native = Native { gamma: 120, bits_per_colour: 8, ..Native::default() };
        let mut formula: Vec<(u32, u32, u32)> = Vec::new();
        let mut section = 0;
        let mut sections_left = 1 + data[3] as usize;
        while sections_left > 0 && section + 5 <= data.len() && data[section] >> 4 == 2 {
            let end = section + 4 + data[section + 1] as usize;
            if end >= data.len() {
                break;
            }
            let mut at = section + 4;
            while at + 3 <= end {
                let (tag, len) = (data[at], data[at + 2] as usize);
                if tag == 0 && len == 0 {
                    break; // padding
                }
                let Some(payload) = data.get(at + 3..at + 3 + len).filter(|_| at + 3 + len <= end) else { break };
                native.block(tag, payload, &mut formula);
                at += 3 + len;
            }
            section = end + 1;
            sections_left -= 1;
        }
        if native.timings.is_empty() {
            return Err("the display lists no detailed timing".into());
        }

        // Formula timings: only the display's own shape, and nothing the detailed timings already cover.
        let (native_h, native_v) = (native.timings[0].h_active, native.timings[0].v_active);
        let mut seen: Vec<(u32, u32, u32)> = native.timings.iter().map(|t| (t.h_active, t.v_active, t.refresh().round() as u32)).collect();
        // The display can run at any rate in its variable refresh range, but the driver may hand out
        // only the first part of the description, where just the top rate is listed. So the usual
        // lower rates are offered for every resolution as well (the display's full list has the same).
        if let Some(lowest) = native.min_refresh {
            let sizes: Vec<(u32, u32)> = seen.iter().map(|m| (m.0, m.1)).chain(formula.iter().map(|m| (m.0, m.1))).collect();
            for (h, v) in sizes {
                formula.extend(COMMON_RATES.iter().filter(|&&rate| rate >= lowest as u32).map(|&rate| (h, v, rate)));
            }
        }
        formula.sort_by(|a, b| (b.0 * b.1, b.2).cmp(&(a.0 * a.1, a.2)));
        for mode in formula {
            if mode.0 * native_v == mode.1 * native_h && !seen.contains(&mode) {
                seen.push(mode);
                native.timings.push(Timing::cvt_rb2(mode.0, mode.1, mode.2));
            }
        }
        Ok(native)
    }

    fn block(&mut self, tag: u8, p: &[u8], formula: &mut Vec<(u32, u32, u32)>) {
        let word = |at: usize| u16::from_le_bytes([p[at], p[at + 1]]);
        match tag {
            TAG_PRODUCT if p.len() >= 12 => {
                self.product_code = word(3);
                self.serial = u32::from_le_bytes([p[5], p[6], p[7], p[8]]);
                self.week = p[9];
                self.year = 2000 + p[10] as u16;
                let name = &p[12..(12 + p[11] as usize).min(p.len())];
                self.name = name.iter().filter(|c| c.is_ascii_graphic() || **c == b' ').map(|&c| c as char).collect();
            }
            TAG_PARAMETERS if p.len() >= 29 => {
                self.width_mm = (word(0) as u32 + 5) / 10; // 0.1 mm
                self.height_mm = (word(2) as u32 + 5) / 10;
                self.width_cm = ((word(0) as u32 + 50) / 100) as u8;
                self.height_cm = ((word(2) as u32 + 50) / 100) as u8;
                for (index, pair) in self.chromaticity.iter_mut().enumerate() {
                    let b = &p[9 + index * 3..12 + index * 3];
                    *pair = (b[0] as u16 | (b[1] as u16 & 0x0F) << 8, (b[1] as u16) >> 4 | (b[2] as u16) << 4);
                }
                self.gamma = p[28];
            }
            TAG_INTERFACE if !p.is_empty() => {
                // supported bits per colour for RGB: bit 0 = 6, 1 = 8, 2 = 10, 3 = 12, 4 = 14, 5 = 16
                self.bits_per_colour = [16, 14, 12, 10, 8, 6].into_iter().zip((0..6).rev()).find(|(_, bit)| p[0] >> bit & 1 != 0).map_or(8, |(bits, _)| bits);
            }
            TAG_ADAPTIVE_SYNC if p.len() >= 3 => self.min_refresh = Some(p[2]),
            TAG_CTA => self.cta = p.to_vec(),
            TAG_TIMING_DETAILED => self.timings.extend(p.chunks_exact(20).map(Timing::from_type_seven)),
            TAG_TIMING_FORMULA => formula.extend(p.chunks_exact(6).map(|e| {
                (u16::from_le_bytes([e[1], e[2]]) as u32 + 1, u16::from_le_bytes([e[3], e[4]]) as u32 + 1, e[5] as u32 + 1)
            })),
            _ => {}
        }
    }

    /// The monitor name for the EDID, which has room for 13 characters.
    fn short_name(&self) -> String {
        let mut name = self.name.clone();
        if name.len() > 13 {
            name = name.replace(" Display", "");
        }
        name.truncate(13);
        name
    }

    /// The EDID: 128 byte blocks, at most four.
    pub fn edid(&self) -> Vec<u8> {
        // Where each timing goes: classic descriptors where they fit, DisplayID timings for the rest.
        let (small, large): (Vec<Timing>, Vec<Timing>) = self.timings.iter().partition(|t| !t.native && t.fits_descriptor());
        let cta_room = (BLOCK - 5 - self.cta.len()) / 18;
        let base_timings = &small[..small.len().min(2)];
        let cta_timings = &small[base_timings.len()..small.len().min(2 + cta_room)];
        let mut display_id: Vec<Timing> = large;
        display_id.extend_from_slice(&small[base_timings.len() + cta_timings.len()..]);
        display_id.truncate(10);

        let mut edid = self.base_block(base_timings);
        edid.extend(self.cta_block(cta_timings));
        for chunk in display_id.chunks(5) {
            edid.extend(display_id_block(chunk));
        }
        edid[126] = (edid.len() / BLOCK - 1) as u8;
        edid[127] = checksum(&edid[..127]);
        edid
    }

    fn base_block(&self, timings: &[Timing]) -> Vec<u8> {
        let mut b = vec![0u8; BLOCK];
        b[0..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        b[8..10].copy_from_slice(&MANUFACTURER_APP);
        b[10..12].copy_from_slice(&self.product_code.to_le_bytes());
        b[12..16].copy_from_slice(&self.serial.to_le_bytes());
        b[16] = self.week;
        b[17] = (self.year.saturating_sub(1990)) as u8;
        b[18] = 1;
        b[19] = 4;
        let depth = match self.bits_per_colour {
            6 => 1,
            8 => 2,
            10 => 3,
            12 => 4,
            14 => 5,
            16 => 6,
            _ => 0,
        };
        b[20] = 0x80 | depth << 4 | 0x05; // digital, DisplayPort
        b[21] = self.width_cm;
        b[22] = self.height_cm;
        b[23] = self.gamma;
        b[24] = 0x03; // RGB 4:4:4, continuous frequency
        // chromaticity: ten bit values, the two low bits of each packed into the first two bytes
        let ten: Vec<u16> = self.chromaticity.iter().flat_map(|&(x, y)| [(x + 2) >> 2, (y + 2) >> 2]).map(|v| v.min(1023)).collect();
        b[25] = ((ten[0] & 3) << 6 | (ten[1] & 3) << 4 | (ten[2] & 3) << 2 | (ten[3] & 3)) as u8;
        b[26] = ((ten[4] & 3) << 6 | (ten[5] & 3) << 4 | (ten[6] & 3) << 2 | (ten[7] & 3)) as u8;
        for (index, value) in ten.iter().enumerate() {
            b[27 + index] = (value >> 2) as u8;
        }
        b[38..54].copy_from_slice(&[1; 16]); // no standard timings

        let mut at = 54;
        for timing in timings {
            b[at..at + 18].copy_from_slice(&timing.descriptor(self.width_mm, self.height_mm));
            at += 18;
        }
        b[at..at + 18].copy_from_slice(&self.range_limits());
        at += 18;
        b[at + 3] = 0xFC; // monitor name
        let name = self.short_name();
        let text = &mut b[at + 5..at + 18];
        text.fill(0x20);
        text[..name.len()].copy_from_slice(name.as_bytes());
        if name.len() < 13 {
            text[name.len()] = 0x0A;
        }
        at += 18;
        while at < 126 {
            b[at + 3] = 0x10; // unused descriptor
            at += 18;
        }
        b
    }

    /// Display range limits: the variable refresh range and the largest pixel clock.
    fn range_limits(&self) -> [u8; 18] {
        let refresh_max = self.timings.iter().map(|t| t.refresh().round() as u32).max().unwrap_or(60);
        let refresh_min = self.min_refresh.map_or_else(|| self.timings.iter().map(|t| t.refresh().round() as u32).min().unwrap_or(60), u32::from);
        let h_max = self.timings.iter().map(|t| t.h_khz().ceil() as u32).max().unwrap_or(0);
        let h_min = self.timings.iter().map(|t| t.h_khz().floor() as u32).min().unwrap_or(0);
        let clock = self.timings.iter().map(|t| t.clock_khz).max().unwrap_or(0).div_ceil(10_000);
        let mut d = [0u8; 18];
        d[3] = 0xFD;
        // rates above 255 are stored minus 255, flagged in byte 4
        d[4] = ((h_max > 255) as u8) << 3 | ((h_min > 255) as u8) << 2;
        d[5] = refresh_min as u8;
        d[6] = refresh_max as u8;
        d[7] = (if h_min > 255 { h_min - 255 } else { h_min }) as u8;
        d[8] = (if h_max > 255 { h_max - 255 } else { h_max }) as u8;
        d[9] = clock.min(255) as u8;
        d[10] = 0x01; // range limits only
        d[11] = 0x0A;
        d[12..18].fill(0x20);
        d
    }

    fn cta_block(&self, timings: &[Timing]) -> Vec<u8> {
        let mut b = vec![0u8; BLOCK];
        b[0] = 0x02;
        b[1] = 0x03;
        b[2] = (4 + self.cta.len()) as u8; // where the detailed timings start
        b[4..4 + self.cta.len()].copy_from_slice(&self.cta);
        let mut at = 4 + self.cta.len();
        for timing in timings {
            b[at..at + 18].copy_from_slice(&timing.descriptor(self.width_mm, self.height_mm));
            at += 18;
        }
        b[127] = checksum(&b[..127]);
        b
    }
}

/// An EDID extension block holding one DisplayID 1.2 section with Type I detailed timings.
fn display_id_block(timings: &[Timing]) -> Vec<u8> {
    let mut b = vec![0u8; BLOCK];
    b[0] = 0x70; // DisplayID extension
    b[1] = 0x12; // version 1.2
    b[2] = 121; // bytes in the section
    b[5] = 0x03; // Type I detailed timings
    b[6] = 0x01;
    b[7] = (timings.len() * 20) as u8;
    for (index, timing) in timings.iter().enumerate() {
        b[8 + index * 20..28 + index * 20].copy_from_slice(&timing.type_one());
    }
    b[126] = checksum(&b[1..126]); // the section's own checksum
    b[127] = checksum(&b[..127]);
    b
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)).wrapping_neg()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two DisplayID 2.0 sections of a Studio Display XDR as the AMD driver returns them, with the
    /// serial number and the container id zeroed.
    const SECTION_0: [u8; 256] = [
        0x20, 0xfb, 0x04, 0x01, 0x20, 0x00, 0x1e, 0x00, 0x10, 0xfa, 0x42, 0xae, 0x00, 0x00, 0x00, 0x00,
        0x09, 0x1a, 0x12, 0x53, 0x74, 0x75, 0x64, 0x69, 0x6f, 0x20, 0x44, 0x69, 0x73, 0x70, 0x6c, 0x61,
        0x79, 0x20, 0x58, 0x44, 0x52, 0x21, 0x01, 0x1d, 0x40, 0x17, 0x14, 0x0d, 0x00, 0x14, 0x40, 0x0b,
        0x00, 0xe1, 0xfa, 0x51, 0x3d, 0xa4, 0xb0, 0x66, 0x62, 0x0f, 0x01, 0x45, 0x54, 0xb0, 0x60, 0xd0,
        0x67, 0x00, 0x80, 0x14, 0x78, 0x28, 0x00, 0x16, 0x82, 0x10, 0x00, 0x00, 0xff, 0x09, 0x3f, 0x0b,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x41, 0x50, 0x50, 0x43, 0xae, 0x00, 0x00, 0x00, 0x00, 0x2b, 0x00,
        0x06, 0x04, 0x00, 0x2f, 0x77, 0x00, 0x00, 0x26, 0x00, 0x09, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x81, 0x00, 0x0b, 0xe3, 0x05, 0x80, 0x00, 0xe6, 0x06, 0x05, 0x01, 0xab, 0x73,
        0x00, 0x29, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x7e, 0x00, 0x05, 0x3a, 0x02, 0x92, 0x81, 0x00, 0x7e, 0x00, 0x07, 0x00,
        0x10, 0xfa, 0x04, 0x01, 0x00, 0x00, 0x7e, 0x00, 0x06, 0x00, 0x10, 0xfa, 0x05, 0x01, 0x02, 0x7e,
        0x00, 0x07, 0x00, 0x10, 0xfa, 0x07, 0x01, 0x05, 0x01, 0x22, 0x02, 0x14, 0x89, 0x0a, 0x1d, 0x04,
        0xff, 0x13, 0x4f, 0x00, 0x07, 0x80, 0x1f, 0x00, 0x3f, 0x0b, 0xa8, 0x00, 0x9a, 0x00, 0x07, 0x00,
        0x2a, 0x00, 0x1e, 0x03, 0xff, 0x09, 0x3f, 0x0b, 0x77, 0x03, 0xff, 0x0e, 0x6f, 0x08, 0x77, 0x03,
        0xff, 0x09, 0x9f, 0x05, 0x77, 0x03, 0x7f, 0x07, 0x37, 0x04, 0x77, 0x03, 0x7f, 0x07, 0x6f, 0x08,
        0x77, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0e,
    ];
    const SECTION_1: [u8; 128] = [
        0x20, 0x7b, 0x00, 0x00, 0x2a, 0x00, 0x78, 0x03, 0xff, 0x09, 0x3f, 0x0b, 0x3b, 0x12, 0xff, 0x13,
        0x3f, 0x0b, 0x3b, 0x12, 0xff, 0x0e, 0x6f, 0x08, 0x3b, 0x12, 0xff, 0x09, 0x9f, 0x05, 0x3b, 0x12,
        0x7f, 0x07, 0x37, 0x04, 0x3b, 0x02, 0xff, 0x13, 0x3f, 0x0b, 0x31, 0x02, 0xff, 0x0e, 0x6f, 0x08,
        0x31, 0x02, 0xff, 0x09, 0x9f, 0x05, 0x31, 0x02, 0x7f, 0x07, 0x37, 0x04, 0x31, 0x12, 0xff, 0x13,
        0x3f, 0x0b, 0x2f, 0x12, 0xff, 0x0e, 0x6f, 0x08, 0x2f, 0x12, 0xff, 0x09, 0x9f, 0x05, 0x2f, 0x12,
        0x7f, 0x07, 0x37, 0x04, 0x2f, 0x02, 0xff, 0x13, 0x3f, 0x0b, 0x77, 0x02, 0xff, 0x13, 0x3f, 0x0b,
        0x77, 0x02, 0xff, 0x09, 0x3f, 0x0b, 0x77, 0x02, 0xff, 0x0e, 0x6f, 0x08, 0x77, 0x02, 0xff, 0x09,
        0x9f, 0x05, 0x77, 0x02, 0x7f, 0x07, 0x37, 0x04, 0x77, 0x02, 0x7f, 0x07, 0x6f, 0x08, 0x77, 0x01,
    ];

    fn native() -> Native {
        Native::parse(&[SECTION_0.as_slice(), SECTION_1.as_slice()].concat()).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn reads_the_display() {
        let n = native();
        assert_eq!(n.name, "Studio Display XDR");
        assert_eq!(n.product_code, 0xAE42);
        assert_eq!((n.week, n.year), (9, 2026));
        assert_eq!((n.width_mm, n.height_mm), (595, 335));
        assert_eq!(n.bits_per_colour, 12);
        assert_eq!(n.min_refresh, Some(47));
        assert_eq!(hex(&n.cta), "e3 05 80 00 e6 06 05 01 ab 73 00");
        let first = n.timings[0];
        assert!(first.native);
        assert_eq!((first.h_active, first.v_active, first.clock_khz), (5120, 2880, 1_903_242));
        assert_eq!(first.refresh().round(), 120.0);
        let modes: Vec<(u32, u32, u32)> = n.timings.iter().map(|t| (t.h_active, t.v_active, t.refresh().round() as u32)).collect();
        assert!(modes.contains(&(5120, 2880, 60)) && modes.contains(&(3840, 2160, 120)) && modes.contains(&(1920, 1080, 48)));
        // the half-width tile modes are not offered, and nothing is listed twice
        assert!(!modes.iter().any(|m| m.0 == 2560 && m.1 == 2880));
        let mut unique = modes.clone();
        unique.dedup();
        assert_eq!(unique.len(), modes.len());
    }

    #[test]
    fn cvt_reduced_blanking_matches_known_timings() {
        // the display's own 120 Hz timing has exactly the blanking the formula gives (its pixel clock
        // is a touch higher: 120.04 Hz)
        let own = native().timings[0];
        assert_eq!(Timing { native: true, clock_khz: own.clock_khz, ..Timing::cvt_rb2(5120, 2880, 120) }, own);
        let t = Timing::cvt_rb2(5120, 2880, 60);
        assert_eq!((t.clock_khz, t.v_blank, t.v_front), (924_144, 82, 68));
        let t = Timing::cvt_rb2(3840, 2160, 60);
        assert_eq!((t.clock_khz, t.v_blank), (522_614, 62));
    }

    #[test]
    fn first_section_alone_is_enough() {
        // the driver sometimes returns only the first 256 bytes
        let alone = Native::parse(&SECTION_0).unwrap();
        assert_eq!(alone.edid(), native().edid());
    }

    #[test]
    fn builds_a_valid_edid() {
        let edid = native().edid();
        assert_eq!(edid.len(), 512);
        for block in edid.chunks(BLOCK) {
            assert_eq!(block.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)), 0, "block checksum");
        }
        assert_eq!(hex(&edid[0..12]), "00 ff ff ff ff ff ff 00 06 10 42 ae");
        assert_eq!(hex(&edid[16..25]), "09 24 01 04 c5 3c 21 78 03");
        // P3 primaries, D65 white (high eight bits of each)
        assert_eq!(hex(&edid[27..35]), "ae 52 43 b0 26 0f 50 54");
        // first detailed timing: 3840 x 2160 @ 60, CVT-RB2
        assert_eq!(hex(&edid[54..66]), "25 cc 00 50 f0 70 3e 80 08 20 08 0c");
        // range limits: 47 - 120 Hz, up to 1910 MHz
        assert_eq!(hex(&edid[90..101]), "00 00 00 fd 08 2f 78 35 70 bf 01");
        assert_eq!(&edid[113..123], b"Studio XDR");
        assert_eq!(edid[126], 3);
        // CTA extension carries the display's HDR metadata and colorimetry unchanged
        assert_eq!(hex(&edid[128..143]), "02 03 0f 00 e3 05 80 00 e6 06 05 01 ab 73 00");
        // DisplayID extension: the native 5120 x 2880 @ 120 first and preferred, then 60 Hz
        assert_eq!(hex(&edid[256..264]), "70 12 79 00 00 03 01 64");
        assert_eq!(hex(&edid[264..284]), "73 e7 02 84 ff 13 4f 00 07 80 1f 00 3f 0b a8 00 9a 00 07 00");
        assert_eq!(hex(&edid[284..304]), "fd 68 01 04 ff 13 4f 00 07 80 1f 00 3f 0b 51 00 43 00 07 00");
        assert_eq!(edid[384], 0x70);
    }
}
