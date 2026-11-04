//! Exact numeric bounds and division for the five keywords that take a number.
//! Each schema library's adapter registers them through that library's
//! documented keyword API; this module holds only the arithmetic.
use crate::decimal::Decimal;
use serde_json::Value;
use std::cmp::Ordering;

/// The keywords whose operand is a number, in registration order.
pub const KEYWORDS: [&str; 5] = [
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
];
/// The four comparisons, which [`Bound::admits`] always decides.
pub const COMPARISONS: [&str; 4] = ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"];

#[derive(Clone, Copy)]
enum Keyword {
    Minimum,
    Maximum,
    ExclusiveMinimum,
    ExclusiveMaximum,
    MultipleOf,
}
impl Keyword {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "minimum" => Self::Minimum,
            "maximum" => Self::Maximum,
            "exclusiveMinimum" => Self::ExclusiveMinimum,
            "exclusiveMaximum" => Self::ExclusiveMaximum,
            "multipleOf" => Self::MultipleOf,
            _ => return None,
        })
    }
}

/// One numeric keyword with its exact operand.
#[derive(Clone)]
pub struct Bound {
    keyword: Keyword,
    limit: Operand,
}
impl std::fmt::Debug for Bound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bound").finish_non_exhaustive()
    }
}
impl Bound {
    /// The bound `keyword` states with the operand spelled `token`.
    pub fn parse(keyword: &str, token: &str) -> Result<Self, &'static str> {
        Self::new(keyword, Operand::parse(token))
    }
    fn new(keyword: &str, limit: Operand) -> Result<Self, &'static str> {
        let keyword = Keyword::named(keyword).ok_or("not a numeric keyword")?;
        if matches!(keyword, Keyword::MultipleOf)
            && limit.decimal.compare(&Decimal::parse("0")) != Ordering::Greater
        {
            return Err("multipleOf must be positive");
        }
        Ok(Self { keyword, limit })
    }
    /// Whether the number spelled `token` meets the bound. `None` when deciding
    /// `multipleOf` would exceed the exact arithmetic's finite admission domain;
    /// the comparisons always decide. Each library's keyword calls it for every
    /// number it checks, so it is a candidate for inlining there.
    #[inline]
    pub fn admits(&self, token: &str) -> Option<bool> {
        if let Ok(value) = token.parse::<i64>()
            && let Some(result) = self.admits_integer(value)
        {
            return Some(result);
        }
        let value = Decimal::parse(token);
        let limit = &self.limit.decimal;
        Some(match self.keyword {
            Keyword::Minimum => value.compare(limit) != Ordering::Less,
            Keyword::Maximum => value.compare(limit) != Ordering::Greater,
            Keyword::ExclusiveMinimum => value.compare(limit) == Ordering::Greater,
            Keyword::ExclusiveMaximum => value.compare(limit) == Ordering::Less,
            Keyword::MultipleOf => return value.multiple_of(limit),
        })
    }
    /// Check an exact signed integer without allocating, when the bound has
    /// that same representation. `None` requires the general decimal path;
    /// it does not mean that the value fails the bound.
    #[inline]
    pub fn admits_integer(&self, value: i64) -> Option<bool> {
        let limit = self.limit.integer?;
        Some(match self.keyword {
            Keyword::Minimum => value >= limit,
            Keyword::Maximum => value <= limit,
            Keyword::ExclusiveMinimum => value > limit,
            Keyword::ExclusiveMaximum => value < limit,
            // Construction requires a positive divisor. Keep the operation
            // checked so no signed-remainder overflow can be introduced.
            Keyword::MultipleOf => value.checked_rem(limit) == Some(0),
        })
    }
    /// The keyword's name, for messages.
    #[inline]
    pub fn keyword(&self) -> &'static str {
        KEYWORDS[self.keyword as usize]
    }
}

/// Original decimal operands, replaced by positive integer handles only in the
/// private compiler projection. The handles satisfy the schema's numeric shape.
#[derive(Default)]
pub struct Operands(Vec<Operand>);

#[derive(Clone)]
struct Operand {
    decimal: Decimal,
    integer: Option<i64>,
}
impl Operand {
    fn parse(token: &str) -> Self {
        Self {
            decimal: Decimal::parse(token),
            integer: token.parse().ok(),
        }
    }
}
impl std::fmt::Debug for Operands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Operands").finish_non_exhaustive()
    }
}

impl Operands {
    /// Keep the operand spelled `token`; the returned handle replaces it in the
    /// projection.
    pub fn insert(&mut self, token: &str) -> Value {
        self.0.push(Operand::parse(token));
        Value::from(self.0.len())
    }
    /// The bound `keyword` states with the operand a projected `handle` names.
    pub fn bound(&self, keyword: &str, handle: &Value) -> Result<Bound, &'static str> {
        let limit = handle
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
            .and_then(|id| id.checked_sub(1))
            .and_then(|id| self.0.get(id))
            .cloned()
            .ok_or("invalid internal numeric reference")?;
        Bound::new(keyword, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_compare_exact_spellings() {
        for (keyword, limit, token, admits) in [
            ("minimum", "0", "-0", Some(true)),
            (
                "minimum",
                "9007199254740993",
                "9007199254740992",
                Some(false),
            ),
            ("maximum", "1e400", "1e399", Some(true)),
            ("exclusiveMinimum", "0.1", "0.10", Some(false)),
            ("exclusiveMaximum", "-1", "-1.5", Some(true)),
            ("multipleOf", "0.1", "0.3", Some(true)),
            ("multipleOf", "7", "-21", Some(true)),
            ("multipleOf", "2", "1e-400", Some(false)),
            ("multipleOf", "1e-1000000000", "1e1000000000", None),
        ] {
            let bound = Bound::parse(keyword, limit).unwrap();
            assert_eq!(bound.keyword(), keyword);
            assert_eq!(bound.admits(token), admits, "{keyword} {limit} {token}");
        }
        assert!(Bound::parse("multipleOf", "0").is_err());
        assert!(Bound::parse("multipleOf", "-2").is_err());
        assert!(Bound::parse("minLength", "1").is_err());
    }

    #[test]
    fn integer_fast_path_preserves_exact_boundaries_and_decimal_fallback() {
        for limit in [i64::MIN, -1, 0, 1, i64::MAX] {
            for value in [i64::MIN, -1, 0, 1, i64::MAX] {
                let expected = [value >= limit, value <= limit, value > limit, value < limit];
                for (keyword, expected) in COMPARISONS.into_iter().zip(expected) {
                    let bound = Bound::parse(keyword, &limit.to_string()).unwrap();
                    assert_eq!(bound.admits_integer(value), Some(expected));
                    assert_eq!(bound.admits(&value.to_string()), Some(expected));
                }
            }
        }
        for (limit, token, expected) in [
            ("9223372036854775807", "9223372036854775808", true),
            ("-9223372036854775808", "-9223372036854775809", false),
            ("0", "-0", true),
            ("0", "0.0", true),
            ("1", "1e0", true),
            ("1.0", "1", true),
            ("1e0", "0", false),
        ] {
            let bound = Bound::parse("minimum", limit).unwrap();
            assert_eq!(bound.admits(token), Some(expected), "{limit}/{token}");
            if limit.parse::<i64>().is_err() {
                assert_eq!(bound.admits_integer(1), None);
            }
        }
        for divisor in [1, 2, 7, i64::MAX] {
            let bound = Bound::parse("multipleOf", &divisor.to_string()).unwrap();
            for value in [i64::MIN, -21, -1, 0, 1, 21, i64::MAX] {
                let expected = (value as i128) % (divisor as i128) == 0;
                assert_eq!(bound.admits_integer(value), Some(expected));
                assert_eq!(bound.admits(&value.to_string()), Some(expected));
            }
        }
        for divisor in ["0", "-0", "-1", "-9223372036854775808"] {
            assert!(Bound::parse("multipleOf", divisor).is_err());
        }
        let decimal = Bound::parse("multipleOf", "0.1").unwrap();
        assert_eq!(decimal.admits_integer(1), None);
        assert_eq!(decimal.admits("1"), Some(true));
    }

    #[test]
    fn handles_name_their_operands() {
        let mut operands = Operands::default();
        let first = operands.insert("12.5");
        let second = operands.insert("3");
        let bound = operands.bound("maximum", &first).unwrap();
        assert_eq!(bound.admits("12.50"), Some(true));
        assert_eq!(
            operands.bound("multipleOf", &second).unwrap().admits("9"),
            Some(true)
        );
        for handle in [
            Value::from(0),
            Value::from(3),
            Value::from(-1),
            Value::from("1"),
        ] {
            assert_eq!(
                operands.bound("minimum", &handle).err(),
                Some("invalid internal numeric reference")
            );
        }
    }
}
