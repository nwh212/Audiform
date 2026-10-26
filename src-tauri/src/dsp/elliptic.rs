// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Elliptic (Cauer) filter design: the steepest slope for an order,
//! with a little ripple in the passband and a set depth in the
//! stopband. Built from the textbook method: Jacobi elliptic functions
//! computed by Landen's transformation, as in S. J. Orfanidis' public
//! lecture notes on elliptic filter design (see ATTRIBUTION.md). Even
//! orders only (2, 4, 6, 8), as second-order sections.

use std::f64::consts::PI;

use realfft::num_complex::Complex64;

use super::biquad::Coeffs;

/// Landen steps: plenty for double precision at any useful modulus.
const STEPS: usize = 8;

/// The descending Landen moduli of `k`.
fn landen(mut k: f64) -> Vec<f64> {
    (0..STEPS)
        .map(|_| {
            let kp = (1.0 - k * k).max(0.0).sqrt();
            k = (k / (1.0 + kp)).powi(2);
            k
        })
        .collect()
}

/// cd(uK, k) for complex u (K the complete integral of `k`).
fn cde(u: Complex64, k: f64) -> Complex64 {
    let mut w = (u * PI / 2.0).cos();
    for v in landen(k).iter().rev() {
        w = w * (1.0 + v) / (w * w * *v + 1.0);
    }
    w
}

/// sn(uK, k) for real u.
fn sne(u: f64, k: f64) -> f64 {
    let mut w = (u * PI / 2.0).sin();
    for v in landen(k).iter().rev() {
        w = (1.0 + v) * w / (1.0 + v * w * w);
    }
    w
}

/// The inverse of sn for a purely imaginary argument j*y: returns the
/// real a with sn(j*a*K, k) = j*y.
fn asne_imag(y: f64, k: f64) -> f64 {
    let v = landen(k);
    let mut w = y;
    for (n, vn) in v.iter().enumerate() {
        let prev = if n == 0 { k } else { v[n - 1] };
        // w is imaginary, so -w^2 is +y^2.
        w = w / (1.0 + (1.0 + w * w * prev * prev).sqrt()) * 2.0 / (1.0 + vn);
    }
    2.0 * w.asinh() / PI
}

/// Analog low-pass prototype, passband edge at 1 rad/s: for each
/// section, (zero frequency, pole frequency, pole Q), plus the passband
/// ripple gain to apply.
fn prototype(order: usize, ripple_db: f64, stop_db: f64) -> (Vec<(f64, f64, f64)>, f64) {
    let ep = (10f64.powf(ripple_db / 10.0) - 1.0).sqrt();
    let es = (10f64.powf(stop_db / 10.0) - 1.0).sqrt();
    let k1 = ep / es;
    let k1p = (1.0 - k1 * k1).sqrt();
    let pairs = order / 2;
    let us: Vec<f64> = (1..=pairs)
        .map(|i| (2 * i - 1) as f64 / order as f64)
        .collect();
    // The degree equation: the selectivity k this order reaches.
    let kp = k1p.powi(order as i32) * us.iter().map(|u| sne(*u, k1p)).product::<f64>().powi(4);
    let k = (1.0 - kp * kp).max(0.0).sqrt();
    let v0 = asne_imag(1.0 / ep, k1) / order as f64;
    let sections = us
        .iter()
        .map(|&u| {
            let zero = 1.0 / (k * cde(Complex64::new(u, 0.0), k).re);
            let p = Complex64::new(0.0, 1.0) * cde(Complex64::new(u, -v0), k);
            let w0 = p.norm();
            (zero, w0, w0 / (-2.0 * p.re))
        })
        .collect();
    // Even orders start at the bottom of the ripple.
    (sections, 1.0 / (1.0 + ep * ep).sqrt())
}

/// An elliptic low-pass or high-pass at `freq` Hz (the passband edge).
pub fn elliptic(
    high_pass: bool,
    order: usize,
    ripple_db: f64,
    stop_db: f64,
    freq: f64,
    rate: u32,
) -> Vec<Coeffs> {
    let order = (order.clamp(2, 8) / 2) * 2;
    let r = f64::from(rate.max(1));
    let f = freq.clamp(1.0, r * 0.49);
    // Prewarp so the edge lands where asked after the bilinear transform.
    let wc = 2.0 * r * (PI * f / r).tan();
    let (sections, ripple_gain) = prototype(
        order,
        ripple_db.clamp(0.01, 3.0),
        stop_db.clamp(20.0, 120.0),
    );
    sections
        .into_iter()
        .enumerate()
        .map(|(i, (wz, w0, q))| {
            let (num, den) = if high_pass {
                // s -> wc / s, times s^2.
                ([wz * wz, 0.0, wc * wc], [w0 * w0, w0 * wc / q, wc * wc])
            } else {
                // s -> s / wc, times wc^2.
                (
                    [1.0, 0.0, wz * wz * wc * wc],
                    [1.0, w0 * wc / q, w0 * w0 * wc * wc],
                )
            };
            let c = Coeffs::bilinear(num, den, rate);
            // Each section passes 0 dB in the passband (at DC, or at
            // Nyquist for a high-pass); the first carries the ripple offset.
            let at = if high_pass { r / 2.0 * 0.999 } else { 0.0 };
            let g = 10f64.powf(-c.response_db(rate, at) / 20.0);
            c.scaled(if i == 0 { g * ripple_gain } else { g })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(c: &[Coeffs], rate: u32, f: f64) -> f64 {
        c.iter().map(|s| s.response_db(rate, f)).sum()
    }

    #[test]
    fn elliptic_ripple_edge_and_stopband() {
        let rate = 48_000;
        for order in [2, 4, 6, 8] {
            let lp = elliptic(false, order, 0.5, 60.0, 1000.0, rate);
            assert_eq!(lp.len(), order / 2);
            // The passband stays within the ripple.
            for f in [20.0, 200.0, 500.0, 900.0] {
                let v = db(&lp, rate, f);
                assert!(
                    (-0.55..=0.05).contains(&v),
                    "order {order}: {v} dB at {f} Hz"
                );
            }
            // The edge is at the ripple depth.
            assert!(
                (db(&lp, rate, 1000.0) + 0.5).abs() < 0.1,
                "order {order} edge {}",
                db(&lp, rate, 1000.0)
            );
            // Far above, at least the stopband depth (a 2nd order only
            // gets there past the top of the audio band).
            if order >= 4 {
                let v = db(&lp, rate, 15_000.0);
                assert!(v < -59.0, "order {order} stop {v}");
            }
            // Never above 0 dB anywhere.
            for i in 0..400 {
                let f = 20.0 * 1000f64.powf(f64::from(i) / 400.0);
                assert!(db(&lp, rate, f) < 0.05, "order {order}: peak at {f}");
            }
        }
        // Higher orders are steeper: depth reached closer to the edge.
        let at = |order| {
            db(
                &elliptic(false, order, 0.5, 60.0, 1000.0, rate),
                rate,
                1300.0,
            )
        };
        assert!(at(8) < at(4));
        assert!(at(8) < -55.0, "8th order at 1.3 kHz: {}", at(8));
        // High-pass mirrors it.
        let hp = elliptic(true, 6, 0.5, 60.0, 1000.0, rate);
        assert!((db(&hp, rate, 1000.0) + 0.5).abs() < 0.1);
        assert!(db(&hp, rate, 5000.0) > -0.6);
        assert!(db(&hp, rate, 300.0) < -59.0);
    }
}
