//! Optional exact source facts. Borrow and preflight first; copy only atomic facts.
use jsonschema::error::{TypeKind, ValidationErrorKind as Kind};
use openbindings::*;

// Reserve this marker with the base problem before any optional detail work.
pub(crate) const TRUNCATED_BYTES: usize = "truncated".len();
pub(crate) enum Collection {
    Available(ValueProblemDetails, usize),
    Unavailable,
    Truncated,
    Interrupted,
}
fn work(control: &WorkControl, units: usize) -> bool {
    !control.is_cancelled() && jsonschema::ob_work::charge(units.max(1))
}
// A JSON Pointer segment can be compared to a borrowed name without decoding it
// into another String. Every source scan is charged to the diagnostic work pass.
fn segment_eq(name: &str, token: &str) -> bool {
    let mut expected = name.bytes();
    let mut bytes = token.bytes();
    while let Some(byte) = bytes.next() {
        let decoded = if byte == b'~' {
            match bytes.next() {
                Some(b'0') => b'~',
                Some(b'1') => b'/',
                _ => return false,
            }
        } else {
            byte
        };
        if expected.next() != Some(decoded) {
            return false;
        }
    }
    expected.next().is_none()
}
fn original<'a>(
    request: &'a SchemaRequest,
    at: &SchemaLocation,
    control: &WorkControl,
) -> Result<Option<JsonRef<'a>>, ()> {
    let mut value = if let Some(uri) = &at.resource {
        let mut found = None;
        for resource in request.supplied_resources().iter() {
            if !work(control, uri.len().saturating_add(resource.uri.len())) {
                return Err(());
            }
            if resource.uri == *uri {
                found = Some(resource.document.view());
                break;
            }
        }
        let Some(found) = found else { return Ok(None) };
        found
    } else {
        request.document().value().view()
    };
    if at.pointer.is_empty() {
        return Ok(Some(value));
    }
    let Some(pointer) = at.pointer.strip_prefix('/') else {
        return Ok(None);
    };
    for token in pointer.split('/') {
        if !work(control, token.len()) {
            return Err(());
        }
        value = match value.kind() {
            JsonKind::Object => {
                let mut found = None;
                for member in value.members().expect("object") {
                    let Some(name) = member.name.as_str() else {
                        continue;
                    };
                    if !work(control, name.len().saturating_add(token.len())) {
                        return Err(());
                    }
                    if segment_eq(name, token) {
                        found = Some(member.value);
                        break;
                    }
                }
                let Some(found) = found else { return Ok(None) };
                found
            }
            JsonKind::Array => {
                if token.is_empty()
                    || (token.len() > 1 && token.starts_with('0'))
                    || !token.bytes().all(|b| b.is_ascii_digit())
                {
                    return Ok(None);
                }
                let Some(found) = token.parse().ok().and_then(|i| value.element(i)) else {
                    return Ok(None);
                };
                found
            }
            _ => return Ok(None),
        };
    }
    Ok(Some(value))
}
#[cfg(test)]
thread_local! { static COPIED_DETAIL_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; static COPIED_ENUM_ITEMS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
fn copied(bytes: usize, items: usize) {
    #[cfg(test)]
    {
        COPIED_DETAIL_BYTES.set(COPIED_DETAIL_BYTES.get() + bytes);
        COPIED_ENUM_ITEMS.set(COPIED_ENUM_ITEMS.get() + items);
    }
    #[cfg(not(test))]
    let _ = (bytes, items);
}
fn owned(detail: ValueProblemDetails, bytes: usize) -> Collection {
    Collection::Available(detail, bytes)
}
pub(crate) fn collect(
    request: &SchemaRequest,
    at: Option<&SchemaLocation>,
    kind: &Kind,
    budget: usize,
    control: &WorkControl,
) -> Collection {
    if let Kind::Type { kind } = kind {
        let cost = match kind {
            TypeKind::Single(expected) => "type".len() + expected.as_str().len(),
            TypeKind::Multiple(expected) => {
                "type".len() + expected.iter().map(|t| t.as_str().len()).sum::<usize>()
            }
        };
        if cost > budget {
            return Collection::Truncated;
        }
        if !work(control, cost) {
            return Collection::Interrupted;
        }
        copied(cost, 0);
        let expected = match kind {
            TypeKind::Single(expected) => vec![expected.as_str().to_owned()],
            TypeKind::Multiple(expected) => {
                expected.iter().map(|t| t.as_str().to_owned()).collect()
            }
        };
        return owned(ValueProblemDetails::Type { expected }, cost);
    }
    let code = kind.keyword();
    if !matches!(
        code,
        "required"
            | "minimum"
            | "maximum"
            | "exclusiveMinimum"
            | "exclusiveMaximum"
            | "minLength"
            | "maxLength"
            | "minItems"
            | "maxItems"
            | "minProperties"
            | "maxProperties"
            | "enum"
    ) {
        return Collection::Unavailable;
    }
    let Some(at) = at else {
        return Collection::Unavailable;
    };
    let operand = match original(request, at, control) {
        Ok(Some(value)) => value,
        Ok(None) => return Collection::Unavailable,
        Err(()) => return Collection::Interrupted,
    };
    match code {
        "required" => {
            let Some(required) = operand.elements() else {
                return Collection::Unavailable;
            };
            let Kind::Required { property } = kind else {
                return Collection::Unavailable;
            };
            // Null in opt-in metadata means the established missing name did not
            // fit scratch admission; retain a truthful marker, never infer a name.
            let Some(member) = property.as_str() else {
                return Collection::Truncated;
            };
            let cost = "required".len().saturating_add(member.len());
            if cost > budget {
                return Collection::Truncated;
            }
            let mut verified = false;
            for required in required {
                let Some(name) = required.as_str() else {
                    return Collection::Unavailable;
                };
                if !work(control, name.len().saturating_add(member.len())) {
                    return Collection::Interrupted;
                }
                if name == member {
                    verified = true;
                    break;
                }
            }
            if !verified {
                return Collection::Unavailable;
            }
            copied(cost, 0);
            owned(
                ValueProblemDetails::Required {
                    member: member.to_owned(),
                },
                cost,
            )
        }
        "enum" => {
            let Some(choices) = operand.elements() else {
                return Collection::Unavailable;
            };
            let mut cost = "enum".len();
            let mut count = 0usize;
            for choice in choices {
                // Exact JSON tokens are nonempty: item count <= token bytes.
                // Thus collection/wire framing is bounded without a full Vec.
                cost = cost.saturating_add(choice.text().len());
                if cost > budget {
                    return Collection::Truncated;
                }
                if !work(control, choice.text().len()) {
                    return Collection::Interrupted;
                }
                count += 1;
            }
            if !work(control, 1) {
                return Collection::Interrupted;
            }
            copied(cost, count);
            let mut choices = Vec::with_capacity(count);
            for choice in operand.elements().expect("preflight array") {
                choices.push(choice.text().to_owned());
            }
            owned(ValueProblemDetails::Enum { choices }, cost)
        }
        _ => {
            let Some(bound) = operand.number_text() else {
                return Collection::Unavailable;
            };
            let numeric = matches!(
                code,
                "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum"
            );
            let tag = if numeric {
                "numeric-bound"
            } else {
                "size-bound"
            };
            let cost = tag.len().saturating_add(bound.len());
            if cost > budget {
                return Collection::Truncated;
            }
            if !work(control, bound.len()) {
                return Collection::Interrupted;
            }
            copied(cost, 0);
            owned(
                if numeric {
                    ValueProblemDetails::NumericBound {
                        bound: bound.to_owned(),
                    }
                } else {
                    ValueProblemDetails::SizeBound {
                        bound: bound.to_owned(),
                    }
                },
                cost,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DefaultEvaluator, Limits};
    use std::sync::Arc;
    #[test]
    fn rejected_enum_is_not_copied_before_admission() {
        let choices = serde_json::json!(["x".repeat(20000), "y".repeat(20000)]);
        let doc = ParsedDocument::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"enum":{choices}}}}}}}}}"#
        ))
        .unwrap();
        let context = doc
            .value_contracts(
                Arc::new(
                    DefaultEvaluator::with_limits(Limits {
                        diagnostic_bytes: 1024,
                        ..Limits::default()
                    })
                    .with_schema_details(true),
                ),
                ResourceSet::default(),
            )
            .unwrap();
        let ContractPreparation::Ready(contract) = context.prepare("op", Side::Input) else {
            panic!()
        };
        COPIED_DETAIL_BYTES.set(0);
        COPIED_ENUM_ITEMS.set(0);
        let ValueOutcome::Fails {
            problems,
            problems_complete,
        } = contract.validate(&JsonValue::null())
        else {
            panic!()
        };
        assert!(matches!(
            problems[0].details,
            Some(ValueProblemDetails::Truncated)
        ));
        assert!(!problems_complete);
        assert_eq!(COPIED_DETAIL_BYTES.get(), 0);
        assert_eq!(COPIED_ENUM_ITEMS.get(), 0);
    }
}
