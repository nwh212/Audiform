// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Second-order filters for EQ and filtering. Coefficients follow the
//! well-known published "audio EQ cookbook" formulas (see ATTRIBUTION.md).
//! Each filter runs in transposed direct form II, in f64 for low noise at
//! low frequencies.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

/// Normalized coefficients (a0 = 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coeffs {
    /// Passes everything unchanged.
    pub const FLAT: Coeffs = Coeffs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// `freq` in Hz, `q` the filter's Q (or shelf slope 1 for shelves),
    /// `gain_db` for Peak and the shelves.
    pub fn new(shape: Shape, rate: u32, freq: f64, q: f64, gain_db: f64) -> Coeffs {
        let rate = f64::from(rate.max(1));
        // Stay safely below Nyquist so the maths holds.
        let f = freq.clamp(1.0, rate * 0.49);
        let q = q.max(0.01);
        let w = 2.0 * PI * f / rate;
        let (sw, cw) = w.sin_cos();
        let alpha = sw / (2.0 * q);
        let a = 10f64.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::LowPass => (
                (1.0 - cw) / 2.0,
                1.0 - cw,
                (1.0 - cw) / 2.0,
                1.0 + alpha,
                -2.0 * cw,
                1.0 - alpha,
            ),
            Shape::HighPass => (
                (1.0 + cw) / 2.0,
                -(1.0 + cw),
                (1.0 + cw) / 2.0,
                1.0 + alpha,
                -2.0 * cw,
                1.0 - alpha,
            ),
            Shape::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            Shape::Notch => (1.0, -2.0 * cw, 1.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
            Shape::Peak => (
                1.0 + alpha * a,
                -2.0 * cw,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cw,
                1.0 - alpha / a,
            ),
            Shape::LowShelf | Shape::HighShelf => {
                // Shelf slope S = 1: alpha from the slope form.
                let alpha = sw / 2.0 * ((a + 1.0 / a) * (1.0 / q.min(1.0) - 1.0) + 2.0).sqrt();
                let k = 2.0 * a.sqrt() * alpha;
                if shape == Shape::LowShelf {
                    (
                        a * ((a + 1.0) - (a - 1.0) * cw + k),
                        2.0 * a * ((a - 1.0) - (a + 1.0) * cw),
                        a * ((a + 1.0) - (a - 1.0) * cw - k),
                        (a + 1.0) + (a - 1.0) * cw + k,
                        -2.0 * ((a - 1.0) + (a + 1.0) * cw),
                        (a + 1.0) + (a - 1.0) * cw - k,
                    )
                } else {
                    (
                        a * ((a + 1.0) + (a - 1.0) * cw + k),
                        -2.0 * a * ((a - 1.0) + (a + 1.0) * cw),
                        a * ((a + 1.0) + (a - 1.0) * cw - k),
                        (a + 1.0) - (a - 1.0) * cw + k,
                        2.0 * ((a - 1.0) - (a + 1.0) * cw),
                        (a + 1.0) - (a - 1.0) * cw - k,
                    )
                }
            }
        };
        Coeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// From an analog section n2 s^2 + n1 s + n0 over d2 s^2 + d1 s + d0,
    /// with s already scaled so `rate` maps through the bilinear
    /// transform (s = 2 * rate * (1 - z^-1) / (1 + z^-1)).
    pub fn bilinear(num: [f64; 3], den: [f64; 3], rate: u32) -> Coeffs {
        let c = 2.0 * f64::from(rate.max(1));
        let z = |[n2, n1, n0]: [f64; 3]| {
            (
                n2 * c * c + n1 * c + n0,
                2.0 * (n0 - n2 * c * c),
                n2 * c * c - n1 * c + n0,
            )
        };
        let (b0, b1, b2) = z(num);
        let (a0, a1, a2) = z(den);
        Coeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// The same filter with its output scaled by `g`.
    pub fn scaled(self, g: f64) -> Coeffs {
        Coeffs {
            b0: self.b0 * g,
            b1: self.b1 * g,
            b2: self.b2 * g,
            ..self
        }
    }

    /// Gain in dB at `freq`, for tests and drawing the curve.
    pub fn response_db(&self, rate: u32, freq: f64) -> f64 {
        let w = 2.0 * PI * freq / f64::from(rate);
        // H(e^jw) = (b0 + b1 z^-1 + b2 z^-2) / (1 + a1 z^-1 + a2 z^-2)
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        let mag = ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt();
        20.0 * mag.max(1e-12).log10()
    }
}

/// One filter for one channel.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    c: Coeffs,
    z1: f64,
    z2: f64,
}

impl Biquad {
    pub fn new(c: Coeffs) -> Self {
        Self {
            c,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let x = f64::from(x);
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2;
        self.z2 = self.c.b2 * x - self.c.a2 * y;
        // Flush values too small to hear, so they never slow the CPU.
        if self.z1.abs() < 1e-30 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-30 {
            self.z2 = 0.0;
        }
        y as f32
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// New coefficients, keeping the filter's memory (for filters that
    /// move while they run).
    pub fn set(&mut self, c: Coeffs) {
        self.c = c;
    }
}

/// The same filter on both channels of a stereo signal.
#[derive(Debug, Clone, Copy)]
pub struct StereoBiquad {
    pub l: Biquad,
    pub r: Biquad,
}

impl StereoBiquad {
    pub fn new(c: Coeffs) -> Self {
        Self {
            l: Biquad::new(c),
            r: Biquad::new(c),
        }
    }

    /// Filters stereo interleaved samples in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for f in buf.as_chunks_mut::<2>().0 {
            f[0] = self.l.run(f[0]);
            f[1] = self.r.run(f[1]);
        }
    }

    pub fn reset(&mut self) {
        self.l.reset();
        self.r.reset();
    }
}

/// Butterworth Q values for a cascade of `n` second-order sections, so
/// 2, 4 or 8 poles give a flat passband (12, 24 or 48 dB per octave).
pub fn butterworth_qs(sections: usize) -> Vec<f64> {
    let n = (sections * 2) as f64;
    (0..sections)
        .map(|k| 1.0 / (2.0 * (PI * (2.0 * k as f64 + 1.0) / (2.0 * n)).sin()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_gain(c: Coeffs, rate: u32, freq: f64) -> f64 {
        let mut b = Biquad::new(c);
        let mut peak = 0f32;
        let n = rate as usize;
        for i in 0..n {
            let x = (2.0 * PI * freq * i as f64 / f64::from(rate)).sin() as f32;
            let y = b.run(x);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * f64::from(peak).log10()
    }

    #[test]
    fn shapes_do_what_they_say() {
        let r = 48_000;
        let hp = Coeffs::new(Shape::HighPass, r, 100.0, 0.707, 0.0);
        assert!(tone_gain(hp, r, 30.0) < -18.0);
        assert!(tone_gain(hp, r, 1000.0).abs() < 0.2);
        let lp = Coeffs::new(Shape::LowPass, r, 3000.0, 0.707, 0.0);
        assert!(tone_gain(lp, r, 12_000.0) < -20.0);
        let pk = Coeffs::new(Shape::Peak, r, 1000.0, 1.0, 6.0);
        assert!((tone_gain(pk, r, 1000.0) - 6.0).abs() < 0.2);
        assert!((pk.response_db(r, 1000.0) - 6.0).abs() < 0.01);
        let notch = Coeffs::new(Shape::Notch, r, 60.0, 10.0, 0.0);
        assert!(tone_gain(notch, r, 60.0) < -30.0);
        assert!(tone_gain(notch, r, 500.0).abs() < 0.2);
        let ls = Coeffs::new(Shape::LowShelf, r, 200.0, 1.0, -6.0);
        assert!((ls.response_db(r, 20.0) + 6.0).abs() < 0.3);
        assert!(ls.response_db(r, 5000.0).abs() < 0.3);
        let hs = Coeffs::new(Shape::HighShelf, r, 4000.0, 1.0, 4.0);
        assert!((hs.response_db(r, 18_000.0) - 4.0).abs() < 0.4);
    }

    #[test]
    fn butterworth_cascade_is_minus_three_at_the_corner() {
        let r = 48_000;
        let total: f64 = butterworth_qs(2)
            .iter()
            .map(|q| Coeffs::new(Shape::HighPass, r, 200.0, *q, 0.0).response_db(r, 200.0))
            .sum();
        assert!((total + 3.0).abs() < 0.1, "{total}");
    }
}
