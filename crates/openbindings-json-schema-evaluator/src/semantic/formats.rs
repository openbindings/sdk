//! Required format assertions are profile-controlled and never perform I/O.
//! Their syntax policy is separate from OBI's Unicode-mode pattern matching.
use super::*;
use openbindings_internal_json::backend::FlatJson;
use openbindings_internal_schema::Control;
use std::sync::OnceLock;
mod rfc3339;

const NAMES: [&str; 19] = [
    "date-time",
    "date",
    "time",
    "duration",
    "email",
    "idn-email",
    "hostname",
    "idn-hostname",
    "ipv4",
    "ipv6",
    "uri",
    "uri-reference",
    "iri",
    "iri-reference",
    "uuid",
    "uri-template",
    "json-pointer",
    "relative-json-pointer",
    "regex",
];
static COMPILED: [OnceLock<jsonschema::Validator<FlatJson>>; 19] = [const { OnceLock::new() }; 19];

#[derive(Clone, Copy)]
pub(super) struct Format(usize);
impl Format {
    pub(super) fn prepare(
        value: JsonRef<'_>,
        attempt: &dyn Control,
    ) -> Result<Self, PreparationFailure> {
        attempt.charge_work(value.text().len().saturating_add(1))?;
        if value.kind() != JsonKind::String {
            return Err(PreparationFailure::new(
                PreparationFailureKind::InvalidExternalSchema,
            ));
        }
        let index = value
            .as_str()
            .and_then(|name| NAMES.iter().position(|known| *known == name))
            .ok_or_else(|| {
                PreparationFailure::new(PreparationFailureKind::UnsupportedCapability {
                    capability: "required-format-attribute".into(),
                })
            })?;
        let format = Self(index);
        if (3..16).contains(&index) && COMPILED[index].get().is_none() {
            // Compilation input is one fixed keyword and one fixed attribute,
            // with no caller-supplied schema content or reference resolver.
            attempt.charge_allocation(256 * 1024)?;
            attempt.charge_work(4096)?;
            let validator = jsonschema::options_for::<FlatJson>()
                .with_draft(jsonschema::Draft::Draft202012)
                .with_retriever(crate::NoRetrieval)
                .should_validate_formats(true)
                .should_ignore_unknown_formats(false)
                .build(&serde_json::json!({"format": NAMES[index]}))
                .map_err(|_| {
                    PreparationFailure::new(PreparationFailureKind::Implementation {
                        message: "a fixed standard format validator could not be compiled".into(),
                    })
                })?;
            // Failed/interrupted initialization must not poison later attempts.
            attempt.check()?;
            let _ = COMPILED[index].set(validator);
        }
        Ok(format)
    }
    pub(super) fn evaluate(
        self,
        value: JsonRef<'_>,
        graph: &mut Graph<'_>,
    ) -> Result<bool, NoVerdict> {
        graph.tick()?;
        if value.kind() != JsonKind::String {
            return Ok(true);
        }
        graph.work(value.text().len().saturating_mul(64).saturating_add(1))?;
        graph.charge(value.text().len().saturating_mul(128).saturating_add(8192))?;
        let string = value.as_string().expect("string");
        if self.0 == 16 {
            return Ok(pointer(string.utf16_units()));
        }
        if self.0 == 17 {
            return Ok(relative_pointer(string.utf16_units()));
        }
        if self.0 == 18 {
            return super::regex_format::valid(string, graph);
        }
        // These formats describe ASCII or Unicode scalar encodings. Preserve
        // lone units as invalid input rather than replacing them with U+FFFD.
        let Some(text) = string.as_str() else {
            return Ok(false);
        };
        match self.0 {
            0 => return Ok(rfc3339::date_time(text.as_bytes())),
            1 => return Ok(rfc3339::date(text.as_bytes())),
            2 => return Ok(rfc3339::time(text.as_bytes())),
            _ => {}
        }
        let valid = COMPILED[self.0]
            .get()
            .expect("prepared fixed format")
            .is_valid(backend::view(&value.to_owned()));
        graph.tick()?;
        Ok(valid)
    }
}
fn pointer(mut units: impl Iterator<Item = u16>) -> bool {
    match units.next() {
        None => return true,
        Some(47) => {}
        _ => return false,
    }
    while let Some(unit) = units.next() {
        if unit == 126 && !matches!(units.next(), Some(48 | 49)) {
            return false;
        }
    }
    true
}
fn relative_pointer(units: impl Iterator<Item = u16>) -> bool {
    let mut units = units.peekable();
    match units.next() {
        Some(48) => {
            if units.peek().is_some_and(|n| (48..=57).contains(n)) {
                return false;
            }
        }
        Some(49..=57) => {
            while units.peek().is_some_and(|n| (48..=57).contains(n)) {
                units.next();
            }
        }
        _ => return false,
    }
    if units.peek() == Some(&35) {
        units.next();
        return units.next().is_none();
    }
    pointer(units)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings_internal_schema::preparation::Attempt;

    #[test]
    fn regex_format_preparation_needs_no_identifier_initialization() {
        for source in [r#""regex""#, r#""r\u0065gex""#] {
            let value = JsonValue::parse(source).unwrap();
            let prepare = |work, cancelled| {
                Attempt::run(work, 0, &|| cancelled, |attempt| {
                    Format::prepare(value.view(), attempt).map(|format| format.0)
                })
                .map_err(PreparationFailure::from)
                .and_then(|result| result)
            };
            let work = value.text().len() + 1;
            for _ in 0..2 {
                assert_eq!(prepare(work, false).map_err(|error| error.kind), Ok(18));
            }
            assert_eq!(
                prepare(work - 1, false).unwrap_err().kind,
                PreparationFailureKind::Limit {
                    resource: "preparation-work-units".into(),
                }
            );
            assert_eq!(
                prepare(work, true).unwrap_err().kind,
                PreparationFailureKind::Cancelled
            );
        }
    }
}
