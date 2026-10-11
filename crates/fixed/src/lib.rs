//! Deterministic fixed-point numbers for the Clamber simulation.
//!
//! The simulation must produce byte-identical traces on every platform
//! (`TESTING.md`), so positions, speeds and physics constants never touch
//! floating point. A [`Fixed`] holds a value in 1/2^32 steps of a pixel (or
//! pixel per frame, and so on), which writes any binary fraction down to
//! 2^-32 exactly and covers ±2^31 pixels.
//!
//! Floating point is only accepted at the edges, when reading
//! hand-written content ([`Fixed::from_f64`]), and is rounded the same way
//! everywhere.

use std::fmt;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

/// Fractional bits.
pub const FRAC_BITS: u32 = 32;

const ONE: i64 = 1 << FRAC_BITS;

/// A signed fixed-point number with 32 integer and 32 fractional bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(i64);

impl Fixed {
    pub const ZERO: Fixed = Fixed(0);
    pub const ONE: Fixed = Fixed(ONE);
    pub const MIN: Fixed = Fixed(i64::MIN);
    pub const MAX: Fixed = Fixed(i64::MAX);

    /// The raw value in 1/2^32 steps.
    pub const fn from_bits(bits: i64) -> Fixed {
        Fixed(bits)
    }

    pub const fn to_bits(self) -> i64 {
        self.0
    }

    pub const fn from_int(n: i32) -> Fixed {
        Fixed((n as i64) << FRAC_BITS)
    }

    /// `numerator / 2^shift`, exactly. `from_ratio_pow2(3, 4)` is 0.1875.
    pub const fn from_ratio_pow2(numerator: i32, shift: u32) -> Fixed {
        assert!(shift <= FRAC_BITS);
        Fixed((numerator as i64) << (FRAC_BITS - shift))
    }

    /// The nearest `Fixed` to `x`, with ties rounded away from zero.
    /// Exact for binary fractions down to 2^-32. Returns `None` for NaN,
    /// infinities and values out of range.
    pub fn from_f64(x: f64) -> Option<Fixed> {
        // Scaling by a power of two is exact, so this rounds only once.
        let scaled = (x * ONE as f64).round();
        // i64::MAX isn't representable as f64; 2^63 is the first value past it.
        const LIMIT: f64 = 9_223_372_036_854_775_808.0;
        (scaled.is_finite() && (-LIMIT..LIMIT).contains(&scaled)).then_some(Fixed(scaled as i64))
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn abs(self) -> Fixed {
        Fixed(self.0.abs())
    }

    /// The product, rounded to the nearest step with ties away from zero,
    /// or `None` if it's out of range.
    pub fn checked_mul(self, rhs: Fixed) -> Option<Fixed> {
        let wide = i128::from(self.0) * i128::from(rhs.0);
        let half = 1i128 << (FRAC_BITS - 1);
        let rounded = if wide < 0 {
            -((-wide + half) >> FRAC_BITS)
        } else {
            (wide + half) >> FRAC_BITS
        };
        i64::try_from(rounded).ok().map(Fixed)
    }
}

impl Add for Fixed {
    type Output = Fixed;

    fn add(self, rhs: Fixed) -> Fixed {
        Fixed(self.0 + rhs.0)
    }
}

impl AddAssign for Fixed {
    fn add_assign(&mut self, rhs: Fixed) {
        self.0 += rhs.0;
    }
}

impl Sub for Fixed {
    type Output = Fixed;

    fn sub(self, rhs: Fixed) -> Fixed {
        Fixed(self.0 - rhs.0)
    }
}

impl SubAssign for Fixed {
    fn sub_assign(&mut self, rhs: Fixed) {
        self.0 -= rhs.0;
    }
}

impl Neg for Fixed {
    type Output = Fixed;

    fn neg(self) -> Fixed {
        Fixed(-self.0)
    }
}

/// Panics on overflow; use [`Fixed::checked_mul`] for untrusted values.
impl Mul for Fixed {
    type Output = Fixed;

    fn mul(self, rhs: Fixed) -> Fixed {
        self.checked_mul(rhs)
            .expect("Fixed multiplication overflowed")
    }
}

/// Exact decimal notation with no trailing zeros. Every value has one,
/// since every binary fraction is a finite decimal.
impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        let int = magnitude >> FRAC_BITS;
        let mut frac = u128::from(magnitude & (ONE as u64 - 1));
        if frac == 0 {
            return write!(f, "{sign}{int}");
        }
        write!(f, "{sign}{int}.")?;
        while frac != 0 {
            frac *= 10;
            write!(f, "{}", frac >> FRAC_BITS)?;
            frac &= u128::from(ONE as u64 - 1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: f64) -> Fixed {
        Fixed::from_f64(x).unwrap()
    }

    #[test]
    fn binary_fractions_are_exact() {
        assert_eq!(f(0.1875), Fixed::from_ratio_pow2(3, 4));
        assert_eq!(f(-2.5), -Fixed::from_ratio_pow2(5, 1));
        assert_eq!(f(7.0), Fixed::from_int(7));
        assert_eq!(f(2f64.powi(-32)), Fixed::from_bits(1));
    }

    #[test]
    fn rounds_to_nearest_step() {
        assert_eq!(f(2f64.powi(-33)), Fixed::from_bits(1));
        assert_eq!(f(-(2f64.powi(-33))), Fixed::from_bits(-1));
        assert_eq!(f(2f64.powi(-34)), Fixed::ZERO);
        // 0.1 isn't a binary fraction; it rounds the same way everywhere.
        assert_eq!(f(0.1).to_bits(), 429_496_730);
    }

    #[test]
    fn rejects_unrepresentable_floats() {
        for x in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            2f64.powi(31),
            -1e12,
        ] {
            assert_eq!(Fixed::from_f64(x), None, "{x}");
        }
        assert!(Fixed::from_f64(-(2f64.powi(31))).is_some());
    }

    #[test]
    fn multiplies_with_rounding() {
        assert_eq!(f(1.5) * f(0.25), f(0.375));
        assert_eq!(f(-3.0) * f(0.5), f(-1.5));
        // 1/2^32 * 0.5 is a tie; it rounds away from zero.
        assert_eq!(Fixed::from_bits(1) * f(0.5), Fixed::from_bits(1));
        assert_eq!(Fixed::from_bits(-1) * f(0.5), Fixed::from_bits(-1));
        assert_eq!(
            Fixed::from_int(1 << 30).checked_mul(Fixed::from_int(4)),
            None
        );
    }

    #[test]
    fn displays_exact_decimal() {
        for (x, s) in [
            (0.0, "0"),
            (6.0, "6"),
            (-0.0625, "-0.0625"),
            (2.75, "2.75"),
            (2f64.powi(-32), "0.00000000023283064365386962890625"),
        ] {
            assert_eq!(f(x).to_string(), s);
        }
    }
}
