//! Exact decimal numbers for trace values and tolerances.
//!
//! Traces write decimals in plain notation with as many places as needed,
//! and comparisons are numeric. A candidate that is off by exactly the
//! tolerance must pass, and binary floating point gets that wrong for
//! values like `1.1` and `1.35` at `tol 0.25`. So values are held as
//! fixed-point integers and every comparison is exact.

use std::fmt;
use std::ops::{Neg, Sub};

/// Fractional digits kept. Enough to write any binary fraction down to
/// 2^-24 exactly.
pub const MAX_PLACES: usize = 24;

/// Integer digits allowed. Keeps the difference of any two values inside
/// `i128`.
pub const MAX_INT_DIGITS: usize = 12;

const ONE: i128 = 10i128.pow(MAX_PLACES as u32);

/// An exact decimal number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Decimal(i128);

impl Decimal {
    pub const ZERO: Decimal = Decimal(0);

    /// A whole number, such as a frame count.
    pub fn from_count(n: usize) -> Decimal {
        Decimal(i128::try_from(n).expect("count fits in i128") * ONE)
    }

    /// The binary fixed-point number `bits / 2^frac_bits`, rounded to the
    /// nearest [`MAX_PLACES`]-place decimal (ties away from zero). Exact
    /// whenever `frac_bits` is at most `MAX_PLACES`.
    pub fn from_binary(bits: i64, frac_bits: u32) -> Decimal {
        // The fraction times ONE (under 2^80) must fit in a u128.
        assert!(frac_bits <= 48, "at most 48 fractional bits");
        let magnitude = bits.unsigned_abs();
        let int = i128::from(magnitude >> frac_bits);
        let frac = u128::from(magnitude & ((1u64 << frac_bits) - 1));
        let scaled = if frac_bits == 0 {
            0
        } else {
            let half = 1u128 << (frac_bits - 1);
            (frac * ONE.unsigned_abs() + half) >> frac_bits
        };
        let value = int * ONE + scaled as i128;
        Decimal(if bits < 0 { -value } else { value })
    }

    pub fn abs(self) -> Decimal {
        Decimal(self.0.abs())
    }

    /// Whether `self` is a whole multiple of 2^-`bits`.
    pub fn is_multiple_of_binary(self, bits: u32) -> bool {
        self.0 * (1i128 << bits) % ONE == 0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Whether `self` is within `±tol` of `other`, inclusive.
    pub fn within(self, other: Decimal, tol: Decimal) -> bool {
        (self - other).abs() <= tol
    }

    /// Parses plain decimal notation: an optional `-`, digits, and an
    /// optional `.` followed by digits. No exponents, no `+`, no bare `.5`.
    pub fn parse(s: &str) -> Result<Decimal, String> {
        let (negative, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let (int, frac) = match body.split_once('.') {
            Some((int, frac)) => (int, Some(frac)),
            None => (body, None),
        };
        let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !digits(int) || frac.is_some_and(|f| !digits(f)) {
            return Err(format!("`{s}` is not a plain decimal number"));
        }

        let int = int.trim_start_matches('0');
        let frac = frac.unwrap_or("").trim_end_matches('0');
        if int.len() > MAX_INT_DIGITS {
            return Err(format!(
                "`{s}` has more than {MAX_INT_DIGITS} integer digits"
            ));
        }
        if frac.len() > MAX_PLACES {
            return Err(format!("`{s}` has more than {MAX_PLACES} decimal places"));
        }

        let mut units: i128 = 0;
        for b in int.bytes() {
            units = units * 10 + i128::from(b - b'0');
        }
        let frac = frac.as_bytes();
        for place in 0..MAX_PLACES {
            units = units * 10 + frac.get(place).map_or(0, |b| i128::from(b - b'0'));
        }
        Ok(Decimal(if negative { -units } else { units }))
    }
}

impl Sub for Decimal {
    type Output = Decimal;

    fn sub(self, rhs: Decimal) -> Decimal {
        Decimal(self.0 - rhs.0)
    }
}

impl Neg for Decimal {
    type Output = Decimal;

    fn neg(self) -> Decimal {
        Decimal(-self.0)
    }
}

/// Plain notation with no trailing zeros, the shortest form that parses
/// back to the same value.
impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        let one = ONE.unsigned_abs();
        let (int, frac) = (magnitude / one, magnitude % one);
        if frac == 0 {
            write!(f, "{sign}{int}")
        } else {
            let frac = format!("{frac:0MAX_PLACES$}");
            write!(f, "{sign}{int}.{}", frac.trim_end_matches('0'))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn parses_plain_notation() {
        assert_eq!(d("0"), Decimal::ZERO);
        assert_eq!(d("-0"), Decimal::ZERO);
        assert_eq!(d("2.250"), d("2.25"));
        assert_eq!(d("007"), Decimal::from_count(7));
        assert!(d("-5") < d("-4.75"));
        assert!(d("-0.0625").is_negative());
    }

    #[test]
    fn rejects_other_notation() {
        for bad in [
            "", "-", ".5", "5.", "+1", "1e3", "1E-2", "0x10", "1,5", " 1", "nan", "inf", "--1",
        ] {
            assert!(Decimal::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn enforces_limits() {
        assert!(Decimal::parse("999999999999").is_ok());
        assert!(Decimal::parse("1000000000000").is_err());
        assert!(Decimal::parse("0.000000000000000000000001").is_ok());
        assert!(Decimal::parse("0.0000000000000000000000001").is_err());
        // Trailing zeros don't count against the limit.
        assert!(Decimal::parse("0.5000000000000000000000000000").is_ok());
    }

    #[test]
    fn displays_shortest_form() {
        for s in [
            "0",
            "7",
            "-7",
            "2.25",
            "-0.0625",
            "84.5",
            "0.000000000000000000000001",
        ] {
            assert_eq!(d(s).to_string(), s);
        }
        assert_eq!(d("1.500").to_string(), "1.5");
        assert_eq!(d("-0").to_string(), "0");
    }

    #[test]
    fn converts_binary_fixed_point() {
        assert_eq!(Decimal::from_binary(0, 32), Decimal::ZERO);
        assert_eq!(Decimal::from_binary(7, 0), d("7"));
        assert_eq!(Decimal::from_binary(9 << 30, 32), d("2.25"));
        assert_eq!(Decimal::from_binary(-(1 << 28), 32), d("-0.0625"));
        assert_eq!(
            Decimal::from_binary(-(84 << 32) - (1 << 31), 32),
            d("-84.5")
        );
        // 2^-24 is exact; 2^-32 rounds to 24 places.
        assert_eq!(Decimal::from_binary(1, 24), d("0.000000059604644775390625"));
        assert_eq!(Decimal::from_binary(1, 32), d("0.000000000232830643653870"));
        assert_eq!(
            Decimal::from_binary(-1, 32),
            d("-0.00000000023283064365387")
        );
    }

    #[test]
    fn tolerance_boundary_is_exact() {
        // 1.35 - 1.1 is 0.25000000000000022 in f64.
        assert!(d("1.35").within(d("1.1"), d("0.25")));
        assert!(d("1.1").within(d("1.35"), d("0.25")));
        assert!(!d("1.3500001").within(d("1.1"), d("0.25")));
        assert!(d("3").within(d("3"), Decimal::ZERO));
    }
}
