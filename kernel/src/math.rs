//! float math missing from core

// the sin_cos polynomials and the pi / 2 and ln 2 splits come from fdlibm
//
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.

use core::f64::consts::{FRAC_2_PI, LOG2_E};

/// `value.floor() as i32`
pub fn floor(value: f32) -> i32 {
    // casts truncate toward zero so step back when that moved up
    let whole = value as i32;
    whole.saturating_sub((whole as f32 > value) as i32)
}

/// `value.ceil() as i32`
pub fn ceil(value: f32) -> i32 {
    // casts truncate toward zero so step forward when that moved down
    let whole = value as i32;
    whole.saturating_add(((whole as f32) < value) as i32)
}

/// `value.round() as i32`, rounding halves away from zero
pub fn round(value: f64) -> i32 {
    let whole = value as i32;
    let fraction = value - whole as f64;
    whole
        .saturating_add((fraction >= 0.5) as i32)
        .saturating_sub((fraction <= -0.5) as i32)
}

/// `value.sqrt()` for finite values at least zero
pub fn sqrt(value: f64) -> f64 {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    {
        use core::arch::x86_64::{_mm_cvtsd_f64, _mm_set_sd, _mm_sqrt_pd};
        // safety: sse2 is enabled for this build
        unsafe { _mm_cvtsd_f64(_mm_sqrt_pd(_mm_set_sd(value))) }
    }
    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    {
        use core::arch::aarch64::{vdup_n_f64, vget_lane_f64, vsqrt_f64};
        // safety: neon is enabled for this build
        unsafe { vget_lane_f64::<0>(vsqrt_f64(vdup_n_f64(value))) }
    }
    #[cfg(not(any(
        all(target_arch = "x86_64", target_feature = "sse2"),
        all(target_arch = "aarch64", target_feature = "neon")
    )))]
    {
        // division free newton steps toward the inverse square root from a guess with the exponent halved
        let mut inverse = f64::from_bits(0x5fe6_eb50_c7b5_37a9 - (value.to_bits() >> 1));
        for _ in 0..4 {
            inverse *= 1.5 - 0.5 * value * inverse * inverse;
        }
        value * inverse
    }
}

/// `value.exp()` for values at most zero
pub fn exp(value: f64) -> f64 {
    if value < -708.0 {
        return 0.0;
    }
    // value is k thirty seconds of ln 2 plus a remainder r within a sixty fourth
    let shifted = value * (32.0 * LOG2_E) + SHIFT;
    let k = shifted.to_bits();
    let whole = shifted - SHIFT;
    let r = value - whole * (LN2_HI / 32.0) - whole * (LN2_LO / 32.0);
    // e^r minus one to degree 5 in two independent halves
    let r2 = r * r;
    let tail = r + r2 * (0.5 + r * (1.0 / 6.0)) + r2 * r2 * (1.0 / 24.0 + r * (1.0 / 120.0));
    // 2^(k / 32) from the entry for its fraction with the whole part added to the exponent bits
    let scale = f64::from_bits(EXP2_FRACTIONS[(k % 32) as usize].to_bits().wrapping_add((k >> 5) << 52));
    scale + scale * tail
}

/// `value.sin_cos()` for values at least zero
pub fn sin_cos(value: f64) -> (f64, f64) {
    // reduce by quarter turns split in three parts so large angles stay accurate
    let shifted = value * FRAC_2_PI + SHIFT;
    let quarters = shifted.to_bits();
    let whole = shifted - SHIFT;
    let x = value - whole * 1.570_796_326_734_125_6;
    let x = x - whole * 6.077_100_506_303_966e-11;
    let x = x - whole * 2.022_266_248_711_166_5e-21;
    // fdlibm minimax polynomials within an eighth turn
    let z = x * x;
    let w = z * z;
    let r = 8.333_333_333_322_49e-3
        + z * (-1.984_126_982_985_795e-4 + z * 2.755_731_370_707_006_8e-6)
        + z * w * (-2.505_076_025_340_686_3e-8 + z * 1.589_690_995_211_55e-10);
    let sin = x + z * x * (-1.666_666_666_666_663_2e-1 + z * r);
    let r = z * (4.166_666_666_666_66e-2 + z * (-1.388_888_888_887_411e-3 + z * 2.480_158_728_947_673e-5))
        + w * w * (-2.755_731_435_139_066_3e-7 + z * (2.087_572_321_298_175e-9 + z * -1.135_964_755_778_819_5e-11));
    let half = 0.5 * z;
    let one = 1.0 - half;
    let cos = one + (((1.0 - one) - half) + z * r);
    match quarters & 3 {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    }
}

// adding 1.5 * 2^52 rounds to an integer held in the low mantissa bits
const SHIFT: f64 = 6_755_399_441_055_744.0;

// ln 2 split so whole multiples of the high part stay exact
const LN2_HI: f64 = 6.931_471_803_691_238e-1;
const LN2_LO: f64 = 1.908_214_929_270_587_7e-10;

// 2^(i / 32) by taylor series of e^(i ln 2 / 32)
const EXP2_FRACTIONS: [f64; 32] = {
    let mut table = [0.0; 32];
    let mut i = 0;
    while i < 32 {
        let r = i as f64 * (core::f64::consts::LN_2 / 32.0);
        let mut sum = 1.0;
        let mut n = 25;
        while n > 0 {
            sum = 1.0 + r * sum / n as f64;
            n -= 1;
        }
        table[i] = sum;
        i += 1;
    }
    table
};
