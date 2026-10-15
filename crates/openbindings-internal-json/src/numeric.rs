//! Narrow public-keyword adapter for exact numeric bounds and division.
use crate::decimal::Decimal;
use jsonschema::json::{Json, JsonNumber, Node};
use jsonschema::{Keyword, ValidationError, ValidationOptions};
use serde_json::Value;
use std::{cmp::Ordering, sync::Arc};
struct Constraint {
    keyword: &'static str,
    limit: Operand,
}
impl<'i, F: Json> Keyword<'i, F> for Constraint {
    fn is_valid(&self, instance: F::Node<'i>) -> bool {
        let Some(number) = instance.as_number() else {
            return true;
        };
        let token = number.as_str();
        // Both tokens are exact signed integers. No rounding or exponent
        // conversion is involved; every other spelling uses Decimal below.
        if let (Some(limit), Ok(value)) = (self.limit.integer, token.parse::<i64>()) {
            return match self.keyword {
                "minimum" => value >= limit,
                "maximum" => value <= limit,
                "exclusiveMinimum" => value > limit,
                "exclusiveMaximum" => value < limit,
                // Compilation requires a positive divisor, excluding zero and
                // MIN / -1 overflow. Keep the arithmetic checked nevertheless.
                "multipleOf" => value.checked_rem(limit) == Some(0),
                _ => unreachable!(),
            };
        }
        let value = Decimal::parse(&token);
        match self.keyword {
            "minimum" => value.compare(&self.limit.decimal) != Ordering::Less,
            "maximum" => value.compare(&self.limit.decimal) != Ordering::Greater,
            "exclusiveMinimum" => value.compare(&self.limit.decimal) == Ordering::Greater,
            "exclusiveMaximum" => value.compare(&self.limit.decimal) == Ordering::Less,
            "multipleOf" => value.multiple_of(&self.limit.decimal).unwrap_or_else(|| {
                jsonschema::ob_work::arithmetic_limit();
                false
            }),
            _ => unreachable!(),
        }
    }
    fn validate(&self, instance: F::Node<'i>) -> Result<(), ValidationError<'i>> {
        if <Self as Keyword<'i, F>>::is_valid(self, instance) {
            Ok(())
        } else {
            Err(ValidationError::custom(format!(
                "exact numeric {} constraint failed",
                self.keyword
            )))
        }
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
    pub fn insert(&mut self, token: &str) -> Value {
        self.0.push(Operand::parse(token));
        Value::from(self.0.len())
    }
    pub fn apply<R, F: Json>(
        self,
        options: ValidationOptions<'_, R, F>,
    ) -> ValidationOptions<'_, R, F> {
        configure(options, Some(Arc::new(self.0)))
    }
}
/// Trusted fixed schemas have only small, exactly representable operands.
pub fn apply<R, F: Json>(options: ValidationOptions<'_, R, F>) -> ValidationOptions<'_, R, F> {
    configure(options, None)
}
fn configure<R, F: Json>(
    mut options: ValidationOptions<'_, R, F>,
    operands: Option<Arc<Vec<Operand>>>,
) -> ValidationOptions<'_, R, F> {
    for keyword in [
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ] {
        let operands = operands.clone();
        options = options.with_keyword(keyword, move |_, value, _| {
            let Some(number) = value.as_number() else {
                return Err(ValidationError::custom("numeric keyword requires a number"));
            };
            let limit = if let Some(operands) = &operands {
                value
                    .as_u64()
                    .and_then(|id| usize::try_from(id).ok())
                    .and_then(|id| id.checked_sub(1))
                    .and_then(|id| operands.get(id))
                    .cloned()
                    .ok_or_else(|| ValidationError::custom("invalid internal numeric reference"))?
            } else {
                Operand::parse(&number.to_string())
            };
            if keyword == "multipleOf"
                && limit.decimal.compare(&Decimal::parse("0")) != Ordering::Greater
            {
                return Err(ValidationError::custom("multipleOf must be positive"));
            }
            Ok(Box::new(Constraint { keyword, limit }))
        });
    }
    options
}
