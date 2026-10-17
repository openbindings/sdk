//! Optional exact source facts for library-bounded upstream diagnostics.
use jsonschema::error::{TypeKind, ValidationErrorKind as Kind};
use openbindings::*;

fn work(control: &dyn WorkControl, units: usize) -> bool {
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
    control: &dyn WorkControl,
) -> Result<Option<(JsonRef<'a>, usize)>, ()> {
    let value = if let Some(uri) = &at.resource {
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
        request.document().value()
    };
    enclosing(value, &at.pointer, control)
}

// Walk once from the source root, retaining the longest existing pointer prefix.
// This is independent of the keyword and of how a library constructs its paths.
fn enclosing<'a>(
    mut value: JsonRef<'a>,
    pointer: &str,
    control: &dyn WorkControl,
) -> Result<Option<(JsonRef<'a>, usize)>, ()> {
    if pointer.is_empty() {
        return Ok(Some((value, 0)));
    }
    let Some(pointer) = pointer.strip_prefix('/') else {
        return Ok(None);
    };
    let mut end = 0;
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
                let Some(found) = found else { break };
                found
            }
            JsonKind::Array => {
                if token.is_empty()
                    || (token.len() > 1 && token.starts_with('0'))
                    || !token.bytes().all(|b| b.is_ascii_digit())
                {
                    break;
                }
                let Some(found) = token.parse().ok().and_then(|i| value.element(i)) else {
                    break;
                };
                found
            }
            _ => break,
        };
        end += 1 + token.len();
    }
    Ok(Some((value, end)))
}

/// Retain an existing source location, or its nearest existing ancestor. Missing
/// resources remain unmapped; interrupted lookup never reports an unchecked path.
pub(crate) fn enclosing_location(
    request: &SchemaRequest,
    mut at: SchemaLocation,
    control: &dyn WorkControl,
) -> Result<Option<SchemaLocation>, ()> {
    let mut found = original(request, &at, control)?;
    if found.is_none()
        && let Some(uri) = &at.resource
    {
        // Packaged meta-schemas are original sources too. Consult them only for
        // location validity; this does not expand the optional source facts.
        for resource in openbindings_internal_schema::embedded_resources() {
            if !work(control, uri.len().saturating_add(resource.uri.len())) {
                return Err(());
            }
            if resource.uri == *uri {
                found = enclosing(resource.document.view(), &at.pointer, control)?;
                break;
            }
        }
    }
    let Some((_, end)) = found else {
        return Ok(None);
    };
    at.pointer.truncate(end);
    Ok(Some(at))
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
pub(crate) fn collect(
    request: &SchemaRequest,
    at: Option<&SchemaLocation>,
    kind: &Kind,
    control: &dyn WorkControl,
) -> Option<ValueProblemDetails> {
    if let Kind::Type { kind } = kind {
        let cost = match kind {
            TypeKind::Single(expected) => "type".len() + expected.as_str().len(),
            TypeKind::Multiple(expected) => {
                "type".len() + expected.iter().map(|t| t.as_str().len()).sum::<usize>()
            }
        };
        if !work(control, cost) {
            return None;
        }
        copied(cost, 0);
        let expected = match kind {
            TypeKind::Single(expected) => vec![expected.as_str().to_owned()],
            TypeKind::Multiple(expected) => {
                expected.iter().map(|t| t.as_str().to_owned()).collect()
            }
        };
        return Some(ValueProblemDetails::Type { expected });
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
        return None;
    }
    let at = at?;
    let operand = match original(request, at, control) {
        Ok(Some((value, end))) if end == at.pointer.len() => value,
        Ok(_) => return None,
        Err(()) => return None,
    };
    match code {
        "required" => {
            let required = operand.elements()?;
            let Kind::Required { property } = kind else {
                return None;
            };
            // Recover only the missing member established by the library.
            let member = property.as_str()?;
            let cost = "required".len().saturating_add(member.len());
            let mut verified = false;
            for required in required {
                let name = required.as_str()?;
                if !work(control, name.len().saturating_add(member.len())) {
                    return None;
                }
                if name == member {
                    verified = true;
                    break;
                }
            }
            if !verified {
                return None;
            }
            copied(cost, 0);
            Some(ValueProblemDetails::Required {
                member: member.to_owned(),
            })
        }
        "enum" => {
            let choices = operand.elements()?;
            let mut cost = "enum".len();
            let mut count = 0usize;
            for choice in choices {
                // Exact JSON tokens are nonempty: item count <= token bytes.
                // Thus collection/wire framing is bounded without a full Vec.
                cost = cost.saturating_add(choice.text().len());
                if !work(control, choice.text().len()) {
                    return None;
                }
                count += 1;
            }
            if !work(control, 1) {
                return None;
            }
            copied(cost, count);
            let mut choices = Vec::with_capacity(count);
            for choice in operand.elements().expect("preflight array") {
                choices.push(choice.text().to_owned());
            }
            Some(ValueProblemDetails::Enum { choices })
        }
        _ => {
            let bound = operand.number_text()?;
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
            if !work(control, bound.len()) {
                return None;
            }
            copied(cost, 0);
            Some(if numeric {
                ValueProblemDetails::NumericBound {
                    bound: bound.to_owned(),
                }
            } else {
                ValueProblemDetails::SizeBound {
                    bound: bound.to_owned(),
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DefaultEvaluator;
    use std::sync::Arc;
    #[test]
    fn interrupted_lookup_and_enum_collection_copy_no_details() {
        use std::sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        };
        #[derive(Default)]
        struct Capture(Mutex<Option<SchemaRequest>>);
        impl SchemaEvaluator for Capture {
            fn prepare(
                &self,
                request: &SchemaRequest,
                control: &dyn WorkControl,
            ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
                *self.0.lock().unwrap() = Some(request.clone());
                DefaultEvaluator::new().prepare(request, control)
            }
        }
        struct StopAfter {
            calls: AtomicUsize,
            limit: usize,
        }
        impl WorkControl for StopAfter {
            fn is_cancelled(&self) -> bool {
                self.calls.fetch_add(1, Ordering::Relaxed) >= self.limit
            }
        }
        let capture = Arc::new(Capture::default());
        let document = Document::parse(
            r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"enum":[0,1,2]}}}}"#,
        )
        .unwrap();
        let document = document.validate().unwrap().conformant().unwrap();
        let operation = document.resolve_operation("op").unwrap();
        assert!(operation.input().unwrap().prepare(&*capture).is_ok());
        let request = capture.0.lock().unwrap().take().unwrap();
        let at = SchemaLocation {
            resource: None,
            pointer: "/operations/op/input/enum".into(),
        };
        let lookup = StopAfter {
            calls: AtomicUsize::new(0),
            limit: usize::MAX,
        };
        assert!(original(&request, &at, &lookup).unwrap().is_some());
        let lookup_calls = lookup.calls.load(Ordering::Relaxed);
        let validator = jsonschema::validator_for(&serde_json::json!({"enum":[0,1,2]})).unwrap();
        let value = serde_json::Value::Null;
        let error = validator.validate(&value).unwrap_err();
        for limit in [0, lookup_calls + 1] {
            COPIED_DETAIL_BYTES.set(0);
            COPIED_ENUM_ITEMS.set(0);
            let control = StopAfter {
                calls: AtomicUsize::new(0),
                limit,
            };
            assert!(collect(&request, Some(&at), error.kind(), &control).is_none());
            assert_eq!(COPIED_DETAIL_BYTES.get(), 0);
            assert_eq!(COPIED_ENUM_ITEMS.get(), 0);
            assert_eq!(control.calls.load(Ordering::Relaxed), limit + 1);
        }
        assert!(
            matches!(collect(&request, Some(&at), error.kind(), &lookup),
            Some(ValueProblemDetails::Enum { choices }) if choices == ["0", "1", "2"])
        );
    }

    #[test]
    fn nonexistent_locations_use_the_nearest_existing_pointer_prefix() {
        let source = JsonValue::parse(r#"{"object":{"a/~雪":[null],"0":false},"":true}"#).unwrap();
        let control = std::sync::atomic::AtomicBool::new(false);
        for (reported, expected) in [
            ("", ""),
            ("/", "/"),
            ("/missing/deep/path", ""),
            ("/object/missing/deep/path", "/object"),
            ("/object/0", "/object/0"),
            ("/object/0/child", "/object/0"),
            ("/object/a~1~0雪/0", "/object/a~1~0雪/0"),
            ("/object/a~1~0雪/0/child", "/object/a~1~0雪/0"),
            ("/object/a~1~0雪/1/child", "/object/a~1~0雪"),
            ("/object/a~1~0雪/00", "/object/a~1~0雪"),
            ("/object/a~1~0雪/-", "/object/a~1~0雪"),
        ] {
            let (value, end) = enclosing(source.view(), reported, &control)
                .unwrap()
                .unwrap();
            assert_eq!(&reported[..end], expected, "{reported}");
            assert_eq!(value.text(), source.at(expected).unwrap().text());
        }
        control.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(enclosing(source.view(), "/object/absent", &control).is_err());
        control.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(matches!(
            jsonschema::ob_work::bounded(1, 10, || {
                enclosing(source.view(), "/object/absent", &control)
            }),
            Err(jsonschema::ob_work::Stop::Work)
        ));
    }

    #[test]
    fn dependency_drafts_can_report_nonexistent_locations() {
        let control = std::sync::atomic::AtomicBool::new(false);
        for (draft, keyword) in [
            (jsonschema::Draft::Draft7, "dependencies"),
            (jsonschema::Draft::Draft202012, "dependentRequired"),
        ] {
            let schema = serde_json::json!({
                "$id": "https://example.test/schema",
                keyword: {"a": ["b"]}
            });
            let validator = jsonschema::options()
                .with_draft(draft)
                .build(&schema)
                .unwrap();
            let instance = serde_json::json!({"a": 1});
            let error = validator.validate(&instance).unwrap_err();
            let (_, pointer) = error
                .absolute_keyword_location()
                .unwrap()
                .as_str()
                .split_once('#')
                .unwrap();
            let source = JsonValue::parse(schema.to_string()).unwrap();
            assert!(source.at(pointer).is_none(), "{keyword}: {pointer}");
            let (_, end) = enclosing(source.view(), pointer, &control)
                .unwrap()
                .unwrap();
            assert_eq!(&pointer[..end], format!("/{keyword}"));
        }
    }

    #[test]
    fn coarse_mode_never_copies_source_facts() {
        let choices = serde_json::json!(["x".repeat(20000), "y".repeat(20000)]);
        let doc = Document::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"enum":{choices}}}}}}}}}"#
        ))
        .unwrap();
        let doc = doc
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant document");
        let operation = doc.resolve_operation("op").unwrap();
        let contract = operation
            .input()
            .unwrap()
            .prepare(&DefaultEvaluator::new())
            .unwrap();
        COPIED_DETAIL_BYTES.set(0);
        COPIED_ENUM_ITEMS.set(0);
        let ValueOutcome::Fails {
            problems,
            problems_complete,
            ..
        } = contract.validate(&JsonValue::null())
        else {
            panic!()
        };
        assert!(problems.is_empty());
        assert!(!problems_complete);
        assert_eq!(COPIED_DETAIL_BYTES.get(), 0);
        assert_eq!(COPIED_ENUM_ITEMS.get(), 0);
    }
}
