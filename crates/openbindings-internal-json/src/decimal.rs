//! Compact exact decimal operations for OpenBindings §5.
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

/// An exact decimal: a sign and a coefficient without trailing zeros, scaled
/// by a power of ten. Each value has one representation, so equality and
/// hashing are numeric: inline while the coefficient has at most 19 digits and
/// the exponent fits `i32`, which holds every ordinary number without
/// allocating, and as digit text otherwise.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Decimal(Repr);
#[derive(Clone, PartialEq, Eq, Hash)]
enum Repr {
    /// `coefficient` × 10^`exponent`. Zero is 0 × 10^0 and never negative.
    Inline {
        negative: bool,
        coefficient: u64,
        exponent: i32,
    },
    Wide(Wide),
}
/// The digits a coefficient has at most to be held inline: 10^19 - 1 fits
/// `u64`.
const INLINE_DIGITS: usize = 19;
const ZERO: Decimal = Decimal(Repr::Inline {
    negative: false,
    coefficient: 0,
    exponent: 0,
});
impl std::fmt::Debug for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decimal").finish_non_exhaustive()
    }
}

impl Decimal {
    pub fn parse(text: &str) -> Self {
        if let Some(inline) = Self::parse_inline(text) {
            return inline;
        }
        let wide = Wide::parse(text);
        if wide.is_zero() {
            ZERO
        } else {
            Self(Repr::Wide(wide))
        }
    }
    // One pass over a JSON number token, without allocating; `None` when the
    // value is not held inline (a longer coefficient or an exponent beyond
    // `i32`) or the text is not a number token, which the digit text then reads.
    fn parse_inline(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let negative = bytes.first() == Some(&b'-');
        let mut at = usize::from(negative);
        let (mut coefficient, mut digits) = (0u64, 0usize);
        // Zeros after the last nonzero digit, which are trailing until another
        // nonzero digit follows them.
        let mut zeros = 0usize;
        let (mut fraction, mut in_fraction) = (0usize, false);
        while let Some(&byte) = bytes.get(at) {
            match byte {
                b'0' if digits == 0 => {}
                b'0' => zeros += 1,
                b'1'..=b'9' => {
                    let added = zeros + 1;
                    if digits + added > INLINE_DIGITS {
                        return None;
                    }
                    coefficient = coefficient * 10u64.pow(added as u32) + u64::from(byte - b'0');
                    digits += added;
                    zeros = 0;
                }
                b'.' if !in_fraction => in_fraction = true,
                b'e' | b'E' => break,
                _ => return None,
            }
            fraction += usize::from(in_fraction && byte != b'.');
            at += 1;
        }
        if digits == 0 {
            return Some(ZERO);
        }
        let mut written = 0i128;
        if at < bytes.len() {
            at += 1;
            let exponent_negative = bytes.get(at) == Some(&b'-');
            at += usize::from(matches!(bytes.get(at), Some(b'-' | b'+')));
            let mut significant = 0;
            for &byte in &bytes[at..] {
                if !byte.is_ascii_digit() {
                    return None;
                }
                if significant == 0 && byte == b'0' {
                    continue;
                }
                // More than 38 significant digits put the exponent far
                // beyond `i32`, whatever the coefficient's shift.
                significant += 1;
                if significant > 38 {
                    return None;
                }
                written = written * 10 + i128::from(byte - b'0');
            }
            if exponent_negative {
                written = -written;
            }
        }
        let exponent = i32::try_from(written + zeros as i128 - fraction as i128).ok()?;
        Some(Self(Repr::Inline {
            negative,
            coefficient,
            exponent,
        }))
    }
    // The digit-text form of this value, which every operation with a wide
    // operand reads.
    fn wide(&self) -> std::borrow::Cow<'_, Wide> {
        match &self.0 {
            Repr::Wide(wide) => std::borrow::Cow::Borrowed(wide),
            &Repr::Inline {
                negative,
                coefficient,
                exponent,
            } => std::borrow::Cow::Owned(Wide {
                negative,
                digits: coefficient.to_string().into(),
                power: Power::parse(&exponent.to_string()),
            }),
        }
    }
    pub fn is_zero(&self) -> bool {
        match &self.0 {
            Repr::Inline { coefficient, .. } => *coefficient == 0,
            Repr::Wide(wide) => wide.is_zero(),
        }
    }
    pub fn is_integer(&self) -> bool {
        match &self.0 {
            Repr::Inline { exponent, .. } => *exponent >= 0,
            Repr::Wide(wide) => !wide.power.negative,
        }
    }
    pub fn nonnegative_integer_u64_saturating(&self) -> Option<u64> {
        match self.0 {
            Repr::Inline {
                negative,
                coefficient,
                exponent,
            } => {
                if negative || exponent < 0 {
                    return None;
                }
                Some(
                    10u64
                        .checked_pow(exponent as u32)
                        .and_then(|scale| coefficient.checked_mul(scale))
                        .unwrap_or(u64::MAX),
                )
            }
            Repr::Wide(ref wide) => wide.nonnegative_integer_u64_saturating(),
        }
    }
    pub fn compare(&self, other: &Self) -> Ordering {
        let (
            &Repr::Inline {
                negative,
                coefficient,
                exponent,
            },
            &Repr::Inline {
                negative: other_negative,
                coefficient: other_coefficient,
                exponent: other_exponent,
            },
        ) = (&self.0, &other.0)
        else {
            return self.wide().compare(&other.wide());
        };
        if negative != other_negative {
            return negative.cmp(&other_negative).reverse();
        }
        let magnitude = if coefficient == 0 || other_coefficient == 0 {
            (other_coefficient == 0).cmp(&(coefficient == 0))
        } else {
            // The leading digit's place, then the digits aligned to it.
            let (digits, other_digits) = (coefficient.ilog10() + 1, other_coefficient.ilog10() + 1);
            (i64::from(exponent) + i64::from(digits))
                .cmp(&(i64::from(other_exponent) + i64::from(other_digits)))
                .then_with(|| {
                    let width = digits.max(other_digits);
                    (coefficient * 10u64.pow(width - digits))
                        .cmp(&(other_coefficient * 10u64.pow(width - other_digits)))
                })
        };
        if negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
    /// Exact division within a finite arithmetic admission. Comparison, equality,
    /// integer classification and counts do not inherit this arithmetic limit.
    pub fn multiple_of(&self, divisor: &Self) -> Option<bool> {
        assert!(!divisor.is_zero(), "positive schema multipleOf was checked");
        let (
            &Repr::Inline {
                coefficient,
                exponent,
                ..
            },
            &Repr::Inline {
                coefficient: modulus,
                exponent: divisor_exponent,
                ..
            },
        ) = (&self.0, &divisor.0)
        else {
            return self.wide().multiple_of(&divisor.wide());
        };
        if coefficient == 0 {
            return Some(true);
        }
        // Both coefficients have no trailing zero: negative shifts cannot divide.
        if exponent < divisor_exponent {
            return Some(false);
        }
        // The admission the digit text applies, which no inline coefficient's
        // length reaches.
        if exponent.unsigned_abs() > 20_000 || divisor_exponent.unsigned_abs() > 20_000 {
            return None;
        }
        // coefficient × 10^shift mod modulus, in u128: every product of two
        // residues of a u64 modulus fits.
        let modulus = u128::from(modulus);
        let (mut scale, mut base, mut shift) = (
            1 % modulus,
            10 % modulus,
            (exponent - divisor_exponent) as u32,
        );
        while shift > 0 {
            if shift & 1 == 1 {
                scale = scale * base % modulus;
            }
            base = base * base % modulus;
            shift >>= 1;
        }
        Some(u128::from(coefficient) % modulus * scale % modulus == 0)
    }
}

/// A decimal whose coefficient or exponent is not held inline, as digit text.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Wide {
    negative: bool,
    digits: Box<str>,
    power: Power,
}
impl Wide {
    fn parse(text: &str) -> Self {
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
    fn is_zero(&self) -> bool {
        self.digits.as_ref() == "0"
    }
    fn is_integer(&self) -> bool {
        !self.power.negative
    }
    fn nonnegative_integer_u64_saturating(&self) -> Option<u64> {
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
    fn compare(&self, other: &Self) -> Ordering {
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
    fn multiple_of(&self, divisor: &Self) -> Option<bool> {
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
    // JSON number tokens around every edge of the inline form: coefficient
    // lengths and values near 19 digits and u64::MAX, leading and trailing
    // zeros on both sides of the point, long and zero-padded exponents, the
    // i32 exponent range and the multipleOf admission at ±20,000.
    fn tokens(count: usize) -> Vec<String> {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move |bound: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % bound
        };
        let mut tokens: Vec<String> = concat!(
            "0 -0 0.0 -0.000e7 0e-99999999999999999999999 1 -1 10 100e-2 0.1e1 ",
            "9999999999999999999 10000000000000000000 1000000000000000000.0 ",
            "18446744073709551615 18446744073709551616 1844674407370955161.5 ",
            "-9223372036854775808 9223372036854775807 1e2147483647 1e2147483648 ",
            "1e-2147483648 1e-2147483649 10e2147483646 0.1e-2147483647 1e20000 1e20001 ",
            "1e-20000 1e-20001 5e19999 7e-19999 1e000000000000000000000000000000000000005 ",
            "1e+0 1E-0 2.50 0.000000003 1e99999999999999999999999999999999999999 ",
            "1e999999999999999999999999999999999999999"
        )
        .split(' ')
        .map(str::to_owned)
        .collect();
        while tokens.len() < count {
            let mut token = String::new();
            if next(3) == 0 {
                token.push('-');
            }
            let lead = next(4) as usize;
            let digits = [1, 2, 5, 18, 19, 20, 21, 30][next(8) as usize];
            if next(4) == 0 {
                token.push('0');
            } else {
                token.push((b'1' + next(9) as u8) as char);
                for _ in 1..digits {
                    token.push((b'0' + next(10) as u8) as char);
                }
            }
            if next(2) == 0 {
                token.push('.');
                token.extend(std::iter::repeat_n('0', lead));
                for _ in 0..[1, 3, 19, 20][next(4) as usize] {
                    token.push((b'0' + next(10) as u8) as char);
                }
                token.extend(std::iter::repeat_n('0', next(3) as usize));
            }
            if next(2) == 0 {
                token.push(['e', 'E'][next(2) as usize]);
                token.push_str(["", "+", "-"][next(3) as usize]);
                token.extend(std::iter::repeat_n('0', next(3) as usize));
                let magnitude = match next(5) {
                    0 => next(30),
                    1 => 19_990 + next(20),
                    2 => 2_147_483_600 + next(100),
                    3 => next(1 << 40),
                    _ => next(400),
                };
                token.push_str(&magnitude.to_string());
            }
            tokens.push(token);
        }
        tokens
    }
    #[test]
    fn inline_values_decide_exactly_as_their_digit_text() {
        let tokens = tokens(600);
        let inline = tokens
            .iter()
            .filter(|t| Decimal::parse_inline(t).is_some())
            .count();
        assert!(
            inline > 240 && inline < 560,
            "both forms are exercised: {inline}"
        );
        let parsed: Vec<_> = tokens.iter().map(|t| Decimal::parse(t)).collect();
        let texts: Vec<_> = tokens.iter().map(|t| Wide::parse(t)).collect();
        for ((token, value), text) in tokens.iter().zip(&parsed).zip(&texts) {
            // The inline form, when used, spells the same normalized value.
            assert!(value.wide().as_ref() == text, "{token}");
            assert_eq!(value.is_zero(), text.is_zero(), "{token}");
            assert_eq!(value.is_integer(), text.is_integer(), "{token}");
            assert_eq!(
                value.nonnegative_integer_u64_saturating(),
                text.nonnegative_integer_u64_saturating(),
                "{token}"
            );
        }
        let state = std::collections::hash_map::RandomState::new();
        let hash = |value: &Decimal| std::hash::BuildHasher::hash_one(&state, value);
        for (left, (value, text)) in tokens.iter().zip(parsed.iter().zip(&texts)) {
            for (right, (other, other_text)) in tokens.iter().zip(parsed.iter().zip(&texts)) {
                let pair = format!("{left} {right}");
                assert_eq!(value.compare(other), text.compare(other_text), "{pair}");
                // One representation per value, so equality and hashing are numeric.
                assert_eq!(value == other, text == other_text, "{pair}");
                if value == other {
                    assert_eq!(hash(value), hash(other), "{pair}");
                }
                if !other.is_zero() {
                    assert_eq!(
                        value.multiple_of(other),
                        text.multiple_of(other_text),
                        "{pair}"
                    );
                }
            }
        }
    }
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
