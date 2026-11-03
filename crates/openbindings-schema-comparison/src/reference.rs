//! Caller-supplied profile contexts, not core schema resources. No `$id`, dialect
//! or schema-position inventory participates in this resolver.
use crate::{ComparisonFailure as Failure, SchemaInput, budget::Attempt};
use openbindings::{JsonKind, JsonRef, JsonValue};
use openbindings_internal_schema::uri;

struct Resource {
    base: Option<String>,
    document: JsonValue,
}
pub(crate) struct Resources {
    entries: Vec<Resource>,
}
fn identity(text: &str, attempt: &mut Attempt<'_>) -> Result<String, Failure> {
    attempt.work(text.len().saturating_mul(8).saturating_add(1))?;
    attempt.allocate(text.len().saturating_mul(8).saturating_add(128))?;
    if !uri::absolute(text) || uri::fragment(text).is_some_and(|fragment| !fragment.is_empty()) {
        return Err(Failure::InvalidReference);
    }
    let mut resolved = uri::resolve(text, text).ok_or(Failure::InvalidReference)?;
    if resolved.ends_with('#') {
        resolved.pop();
    }
    Ok(resolved)
}
impl Resources {
    pub(crate) fn new(input: &SchemaInput, attempt: &mut Attempt<'_>) -> Result<Self, Failure> {
        let mut entries = attempt.vector(input.resources.len().saturating_add(1))?;
        entries.push(Resource {
            base: input
                .base
                .as_deref()
                .map(|base| identity(base, attempt))
                .transpose()?,
            document: input.document.clone(),
        });
        for (base, document) in &input.resources {
            let base = identity(base, attempt)?;
            for known in &entries {
                attempt.work(base.len().saturating_add(1))?;
                if known.base.as_deref() == Some(base.as_str()) {
                    return Err(Failure::InvalidReference);
                }
            }
            entries.push(Resource {
                base: Some(base),
                document: document.clone(),
            });
        }
        Ok(Self { entries })
    }
    /// A scope index identifies the immutable document/base pair for this side.
    /// Active-reference keys include this index and the original node occurrence.
    pub(crate) fn resolve(
        &self,
        scope: usize,
        reference: JsonRef<'_>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(JsonValue, usize), Failure> {
        let text = reference.as_str().ok_or(Failure::InvalidReference)?;
        attempt.work(text.len().saturating_mul(8).saturating_add(1))?;
        if !uri::valid(text) {
            return Err(Failure::InvalidReference);
        }
        let current = &self.entries[scope];
        if let Some(fragment) = text.strip_prefix('#') {
            return Ok((
                fragment_target(&current.document, fragment, attempt)?,
                scope,
            ));
        }
        let bytes = text
            .len()
            .saturating_add(current.base.as_ref().map_or(0, String::len));
        attempt.allocate(bytes.saturating_mul(8).saturating_add(128))?;
        attempt.work(bytes.saturating_mul(8).saturating_add(1))?;
        let resolved = if uri::absolute(text) {
            uri::resolve(text, text)
        } else {
            uri::resolve(
                current.base.as_deref().ok_or(Failure::InvalidReference)?,
                text,
            )
        }
        .ok_or(Failure::InvalidReference)?;
        let (resource, fragment) = resolved.split_once('#').unwrap_or((&resolved, ""));
        for (index, candidate) in self.entries.iter().enumerate() {
            attempt.work(resource.len().saturating_add(1))?;
            if candidate.base.as_deref() == Some(resource) {
                return Ok((
                    fragment_target(&candidate.document, fragment, attempt)?,
                    index,
                ));
            }
        }
        Err(Failure::MissingReference)
    }
}
fn fragment_target(
    root: &JsonValue,
    fragment: &str,
    attempt: &mut Attempt<'_>,
) -> Result<JsonValue, Failure> {
    attempt.work(fragment.len().saturating_mul(4).saturating_add(1))?;
    attempt.allocate(fragment.len())?;
    let pointer = uri::decode_fragment(fragment).ok_or(Failure::InvalidReference)?;
    if pointer.is_empty() {
        return Ok(root.clone());
    }
    let path = pointer
        .strip_prefix('/')
        .ok_or(Failure::UnsupportedReference)?;
    let mut node = root.view();
    for segment in path.split('/') {
        attempt.work(segment.len().saturating_mul(4).saturating_add(1))?;
        let mut decoded = attempt.vector::<u16>(segment.len())?;
        let mut units = segment.encode_utf16();
        while let Some(unit) = units.next() {
            decoded.push(if unit == b'~' as u16 {
                match units.next() {
                    Some(unit) if unit == b'0' as u16 => b'~' as u16,
                    Some(unit) if unit == b'1' as u16 => b'/' as u16,
                    _ => return Err(Failure::InvalidReference),
                }
            } else {
                unit
            });
        }
        node = match node.kind() {
            JsonKind::Object => {
                let mut found = None;
                for member in node.members().expect("object") {
                    attempt.work(
                        member
                            .name
                            .text()
                            .len()
                            .saturating_add(decoded.len())
                            .saturating_add(1),
                    )?;
                    if member
                        .name
                        .as_string()
                        .expect("name")
                        .utf16_units()
                        .eq(decoded.iter().copied())
                    {
                        if found.is_some() {
                            return Err(Failure::DuplicateMember);
                        }
                        found = Some(member.value);
                    }
                }
                found.ok_or(Failure::MissingReference)?
            }
            JsonKind::Array => {
                // Index syntax is ASCII and must use the canonical array spelling.
                if decoded.is_empty() || (decoded.len() > 1 && decoded[0] == b'0' as u16) {
                    return Err(Failure::InvalidReference);
                }
                let mut index = 0usize;
                for unit in decoded {
                    if !(b'0' as u16..=b'9' as u16).contains(&unit) {
                        return Err(Failure::InvalidReference);
                    }
                    index = index
                        .checked_mul(10)
                        .and_then(|n| n.checked_add((unit - b'0' as u16) as usize))
                        .ok_or(Failure::MissingReference)?;
                }
                let mut elements = node.elements().expect("array");
                if index >= elements.len() {
                    return Err(Failure::MissingReference);
                }
                attempt.work(index.saturating_add(1))?;
                elements.nth(index).expect("existing element")
            }
            _ => return Err(Failure::MissingReference),
        };
    }
    Ok(node.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComparisonLimits;
    use std::sync::atomic::AtomicBool;
    fn json(text: &str) -> JsonValue {
        JsonValue::parse(text).unwrap()
    }
    #[test]
    fn references_use_each_supplied_root_base_and_external_context() {
        let selected = json(r##"{"$ref":"#/schemas/A"}"##);
        let input = SchemaInput::new(
            selected.clone(),
            json(r##"{"schemas":{"A":{"type":"string"}}}"##),
        )
        .with_base("https://example.test/root")
        .with_resource(
            "https://example.test/other/a",
            json(r##"{"$id":"https://wrong.test/","$ref":"b"}"##),
        )
        .with_resource("https://example.test/other/b", json("false"));
        let control = AtomicBool::new(false);
        let mut attempt = Attempt::new(ComparisonLimits::default(), &control);
        let resources = Resources::new(&input, &mut attempt).unwrap();
        let reference = json(r##""#/schemas/A""##);
        assert_eq!(
            resources
                .resolve(0, reference.view(), &mut attempt)
                .unwrap()
                .0
                .text(),
            r##"{"type":"string"}"##
        );
        let external = json(r##""other/a""##);
        let (a, scope) = resources.resolve(0, external.view(), &mut attempt).unwrap();
        assert_eq!(scope, 1);
        let (b, scope) = resources
            .resolve(scope, a.get("$ref").unwrap(), &mut attempt)
            .unwrap();
        assert_eq!((b.text(), scope), ("false", 2));
        let other = SchemaInput::new(selected, json(r##"{"schemas":{"A":true}}"##));
        assert_eq!(
            Resources::new(&other, &mut attempt)
                .unwrap()
                .resolve(0, reference.view(), &mut attempt)
                .unwrap()
                .0
                .text(),
            "true"
        );
    }
    #[test]
    fn pointer_ambiguity_missing_context_and_resource_aliases_are_failures() {
        let control = AtomicBool::new(false);
        let mut attempt = Attempt::new(ComparisonLimits::default(), &control);
        let input = SchemaInput::standalone(json(
            r##"{"a/b":[true,false],"duplicate":true,"duplicate":false}"##,
        ));
        let resources = Resources::new(&input, &mut attempt).unwrap();
        let reference = json(r##""#/a~1b/1""##);
        assert_eq!(
            resources
                .resolve(0, reference.view(), &mut attempt)
                .unwrap()
                .0
                .text(),
            "false"
        );
        for (text, error) in [
            (r##""#/duplicate""##, Failure::DuplicateMember),
            (r##""#/a~2b""##, Failure::InvalidReference),
            (r##""relative""##, Failure::InvalidReference),
            (r##""https://missing.test/""##, Failure::MissingReference),
        ] {
            assert!(
                matches!(resources.resolve(0,json(text).view(),&mut attempt),Err(found) if found==error)
            );
        }
        let duplicate = SchemaInput::standalone(json("true"))
            .with_resource("https://x/a/../b", json("true"))
            .with_resource("https://x/b#", json("false"));
        assert!(matches!(
            Resources::new(&duplicate, &mut attempt),
            Err(Failure::InvalidReference)
        ));
    }
}
