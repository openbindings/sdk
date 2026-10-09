//! Narrow public-keyword adapter for exact numeric bounds and division.
use jsonschema::json::{Json, JsonNumber, Node};
use jsonschema::{Keyword, ValidationError, ValidationOptions};
use jsonschema_value::ob_decimal::Decimal;
use std::cmp::Ordering;
struct Constraint {
    keyword: &'static str,
    limit: Decimal,
}
impl<'i, F: Json> Keyword<'i, F> for Constraint {
    fn is_valid(&self, instance: F::Node<'i>) -> bool {
        let Some(number) = instance.as_number() else {
            return true;
        };
        let value = Decimal::parse(&number.as_str());
        match self.keyword {
            "minimum" => value.compare(&self.limit) != Ordering::Less,
            "maximum" => value.compare(&self.limit) != Ordering::Greater,
            "exclusiveMinimum" => value.compare(&self.limit) == Ordering::Greater,
            "exclusiveMaximum" => value.compare(&self.limit) == Ordering::Less,
            "multipleOf" => value.multiple_of(&self.limit).unwrap_or_else(|| {
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
pub fn apply<R, F: Json>(mut options: ValidationOptions<'_, R, F>) -> ValidationOptions<'_, R, F> {
    for keyword in [
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ] {
        options = options.with_keyword(keyword, move |_, value, _| {
            let Some(number) = value.as_number() else {
                return Err(ValidationError::custom("numeric keyword requires a number"));
            };
            let limit = Decimal::parse(number.as_str());
            if keyword == "multipleOf" && limit.compare(&Decimal::parse("0")) != Ordering::Greater {
                return Err(ValidationError::custom("multipleOf must be positive"));
            }
            Ok(Box::new(Constraint { keyword, limit }))
        });
    }
    options
}
