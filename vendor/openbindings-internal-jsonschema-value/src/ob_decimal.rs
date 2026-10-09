//! OpenBindings qualification patch: compact exact decimal operations.
//! Inputs are already lexically validated JSON numbers. No exponent expansion.
use num_bigint::BigUint;
use num_traits::Zero;
use std::cmp::Ordering;
// A decimal exponent needs comparison and addition/subtraction of a byte count,
// not multiplication. Keep its signed digits so even a long exponent is handled
// in linear time; binary BigInt parsing would add avoidable superlinear work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Power {
    negative: bool,
    digits: Box<str>,
}
impl Power {
    fn parse(text: &str) -> Self {
        let negative = text.starts_with('-');
        let digits = text.trim_start_matches(['-', '+']).trim_start_matches('0');
        Self {
            negative: negative && !digits.is_empty(),
            digits: if digits.is_empty() {
                "0".into()
            } else {
                digits.into()
            },
        }
    }
    fn small(&self) -> Option<i64> {
        let magnitude = self.digits.parse::<u64>().ok()? as i128;
        i64::try_from(if self.negative { -magnitude } else { magnitude }).ok()
    }
    fn offset(&self, amount: usize, negative: bool) -> Self {
        if amount == 0 {
            return self.clone();
        }
        if let Ok(magnitude) = self.digits.parse::<u64>() {
            let value = if self.negative {
                -(magnitude as i128)
            } else {
                magnitude as i128
            };
            let delta = if negative {
                -(amount as i128)
            } else {
                amount as i128
            };
            return Self::parse(&(value + delta).to_string());
        }
        // The existing magnitude exceeds u64::MAX and therefore any byte count.
        // Opposite signs cannot cross zero here. Work from the least-significant
        // digit, carrying/borrowing only the bounded machine-sized offset.
        let mut digits = self.digits.as_bytes().to_vec();
        let mut rest = amount;
        if self.negative == negative {
            for digit in digits.iter_mut().rev() {
                if rest == 0 {
                    break;
                }
                let sum = (*digit - b'0') as usize + rest % 10;
                *digit = b'0' + (sum % 10) as u8;
                rest = rest / 10 + sum / 10;
            }
            if rest > 0 {
                let mut prefix = rest.to_string().into_bytes();
                prefix.extend(digits);
                digits = prefix;
            }
        } else {
            for digit in digits.iter_mut().rev() {
                if rest == 0 {
                    break;
                }
                let sub = rest % 10;
                rest /= 10;
                let original = (*digit - b'0') as usize;
                if original >= sub {
                    *digit = b'0' + (original - sub) as u8;
                } else {
                    *digit = b'0' + (original + 10 - sub) as u8;
                    rest += 1;
                }
            }
            debug_assert_eq!(rest, 0);
        }
        let digits = String::from_utf8(digits).expect("decimal digits");
        Self {
            negative: self.negative,
            digits: digits.trim_start_matches('0').into(),
        }
    }
}
impl Ord for Power {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.negative != other.negative {
            return self.negative.cmp(&other.negative).reverse();
        }
        let magnitude = self
            .digits
            .len()
            .cmp(&other.digits.len())
            .then_with(|| self.digits.cmp(&other.digits));
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}
impl PartialOrd for Power {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Decimal {
    negative: bool,
    digits: Box<str>,
    power: Power,
}
impl Decimal {
    pub fn parse(text: &str) -> Self {
        let negative = text.starts_with('-');
        let text = text.strip_prefix('-').unwrap_or(text);
        let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
        let exponent = exponent.strip_prefix('+').unwrap_or(exponent);
        let fraction = mantissa.split_once('.').map_or(0, |(_, s)| s.len());
        let digits: String = mantissa.chars().filter(|&c| c != '.').collect();
        let digits = digits.trim_start_matches('0');
        if digits.is_empty() {
            return Self {
                negative: false,
                digits: "0".into(),
                power: Power::parse("0"),
            };
        }
        let trimmed = digits.trim_end_matches('0');
        let zeros = digits.len() - trimmed.len();
        Self {
            negative,
            digits: trimmed.into(),
            power: Power::parse(exponent)
                .offset(zeros, false)
                .offset(fraction, true),
        }
    }
    pub fn is_zero(&self) -> bool {
        self.digits.as_ref() == "0"
    }
    pub fn is_integer(&self) -> bool {
        !self.power.negative
    }
    pub fn nonnegative_integer_u64_saturating(&self) -> Option<u64> {
        if self.negative || !self.is_integer() {
            return None;
        }
        if self.is_zero() {
            return Some(0);
        }
        if self.power > Power::parse("19") || self.digits.len() > 20 {
            return Some(u64::MAX);
        }
        let power = self.power.small().unwrap() as u32;
        Some(
            self.digits
                .parse::<u64>()
                .ok()
                .and_then(|n| 10u64.checked_pow(power).and_then(|p| n.checked_mul(p)))
                .unwrap_or(u64::MAX),
        )
    }
    pub fn compare(&self, other: &Self) -> Ordering {
        if self.negative != other.negative {
            return self.negative.cmp(&other.negative).reverse();
        }
        let magnitude = if self.is_zero() || other.is_zero() {
            other.is_zero().cmp(&self.is_zero())
        } else {
            let left = self.power.offset(self.digits.len(), false);
            let right = other.power.offset(other.digits.len(), false);
            left.cmp(&right).then_with(|| {
                let n = self.digits.len().max(other.digits.len());
                self.digits
                    .bytes()
                    .chain(std::iter::repeat(b'0'))
                    .take(n)
                    .cmp(other.digits.bytes().chain(std::iter::repeat(b'0')).take(n))
            })
        };
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
    /// Exact division within a finite arithmetic admission. Comparison, equality,
    /// integer classification and counts do not inherit this arithmetic limit.
    pub fn multiple_of(&self, divisor: &Self) -> Option<bool> {
        assert!(!divisor.is_zero(), "positive schema multipleOf was checked");
        if self.is_zero() {
            return Some(true);
        }
        // Both coefficients have no trailing zero: negative shifts cannot divide.
        if self.power < divisor.power {
            return Some(false);
        }
        if self.digits.len() > 4096 || divisor.digits.len() > 4096 {
            return None;
        }
        let left = self.power.small()?;
        let right = divisor.power.small()?;
        // Normalization can move a 4,096-character coefficient into its exponent.
        // This covers the Go ±10,000/token-4,096 floor with room for that shift.
        if left.unsigned_abs() > 20_000 || right.unsigned_abs() > 20_000 {
            return None;
        }
        let shift = (left - right) as u64;
        let numerator: BigUint = self.digits.parse().unwrap();
        let denominator: BigUint = divisor.digits.parse().unwrap();
        let scale = BigUint::from(10u8).modpow(&BigUint::from(shift), &denominator);
        Some((numerator * scale % denominator).is_zero())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_decimal_exponent_offsets_match_independent_bigint_arithmetic() {
        use num_bigint::BigInt;
        let mut seed = 0x4b62_0e81_5173_a02du64;
        for length in [1, 18, 19, 20, 21, 32, 64, 256] {
            for _ in 0..200 {
                let mut token = String::new();
                for _ in 0..length {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    token.push((b'0' + (seed % 10) as u8) as char);
                }
                for sign in ["", "-"] {
                    let exponent = format!("{sign}{token}");
                    let original = exponent.parse::<BigInt>().unwrap();
                    for amount in [1, 9, 10, 999, 10000, usize::MAX] {
                        for negative in [true, false] {
                            let got = Power::parse(&exponent).offset(amount, negative);
                            let expected = if negative {
                                &original - BigInt::from(amount)
                            } else {
                                &original + BigInt::from(amount)
                            };
                            assert_eq!(
                                got,
                                Power::parse(&expected.to_string()),
                                "{exponent}, {amount}, {negative}"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn huge_exponents_stay_compact_and_arithmetic_refusal_is_selective() {
        let exponent = "9".repeat(100_000);
        let a = Decimal::parse(&format!("10e{exponent}"));
        let b = Decimal::parse(&format!("1e1{}", "0".repeat(100_000)));
        assert_eq!(a, b);
        assert!(a.is_integer());
        assert_eq!(a.nonnegative_integer_u64_saturating(), Some(u64::MAX));
        assert_eq!(a.compare(&Decimal::parse("0")), Ordering::Greater);
        assert_eq!(a.multiple_of(&Decimal::parse("3")), None);
        assert_eq!(Decimal::parse("0").multiple_of(&a), Some(true));
    }
    #[test]
    fn comparison() {
        for (a, b, want) in [
            ("1e2000000", "1e2000001", Ordering::Less),
            ("-1e-2000000", "-1e-2000001", Ordering::Less),
            (
                "10e9223372036854775806",
                "1e9223372036854775807",
                Ordering::Equal,
            ),
            ("0e99999999999999999999999999", "-0.00", Ordering::Equal),
            ("1.000000000000000000000000000001", "1", Ordering::Greater),
        ] {
            assert_eq!(Decimal::parse(a).compare(&Decimal::parse(b)), want);
        }
    }
    #[test]
    fn divisibility() {
        for (a, b, want) in [
            ("0", "3", true),
            ("0.000000009", "0.000000003", true),
            ("0.9", "0.03", true),
            ("0.03", "0.9", false),
            ("1e10000", "2", true),
            ("1e10000", "3", false),
            ("12.4", "0.2", true),
            ("12.4", "0.3", false),
        ] {
            assert_eq!(
                Decimal::parse(a).multiple_of(&Decimal::parse(b)),
                Some(want)
            );
        }
    }
}
