//! Resolve profile references in their containing schema resource. Only schema
//! positions declare identifiers; instance data never changes the reference base.
use crate::{ComparisonFailure as Failure, SchemaInput, budget::Attempt};
use openbindings::{JsonKind, JsonRef, JsonValue};
use openbindings_internal_json::backend;
use openbindings_internal_schema::{schema_index::each_schema_child, uri};
use std::collections::BTreeMap;

struct Resource {
    base: Option<String>,
    document: JsonValue,
    keep_defs: bool,
}
pub(crate) struct Resources {
    entries: Vec<Resource>,
    scopes: BTreeMap<(usize, usize), usize>,
    anchors: BTreeMap<(usize, String), JsonValue>,
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
// Mark only schema keyword positions. Extension names may contain non-scalar
// UTF-16 units, so recognizing x- must not depend on conversion to Rust text.
fn outside_keyword(name: JsonRef<'_>, attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    attempt.work(name.text().len().saturating_add(512))?;
    let name = name.as_string().expect("member name");
    if name
        .utf16_units()
        .take(2)
        .eq([u16::from(b'x'), u16::from(b'-')])
    {
        return Ok(false);
    }
    let Some(text) = name.as_str() else {
        return Ok(true);
    };
    if matches!(text, "$ref" | "$defs") {
        return Ok(false);
    }
    let mut units = [0u16; 32];
    if text.len() > units.len() || !text.is_ascii() {
        return Ok(true);
    }
    for (target, byte) in units.iter_mut().zip(text.bytes()) {
        *target = u16::from(byte);
    }
    Ok(!crate::normalize::known(&units[..text.len()])
        && !crate::normalize::stripped(&units[..text.len()]))
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
            keep_defs: false,
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
                keep_defs: false,
            });
        }
        let mut resources = Self {
            entries,
            scopes: BTreeMap::new(),
            anchors: BTreeMap::new(),
        };
        for scope in 0..resources.entries.len() {
            let root = resources.entries[scope].document.clone();
            if root.get("openbindings").is_some() && root.get("operations").is_some() {
                if let Some(schemas) = root.get("schemas").and_then(|v| v.members()) {
                    for schema in schemas {
                        resources.scan(schema.value, scope, 0, attempt)?;
                    }
                }
                for operation in root
                    .get("operations")
                    .and_then(|v| v.members())
                    .ok_or(Failure::InvalidSchema)?
                {
                    for slot in ["input", "output"] {
                        if let Some(schema) = operation.value.get(slot) {
                            resources.scan(schema, scope, 0, attempt)?;
                        }
                    }
                }
            } else {
                resources.scan(root.view(), scope, 0, attempt)?;
            }
        }
        if !resources
            .scopes
            .contains_key(&backend::occurrence(input.schema.view()))
        {
            resources.scan(input.schema.view(), 0, 0, attempt)?;
        }
        Ok(resources)
    }
    pub(crate) fn enter(
        &mut self,
        schema: JsonRef<'_>,
        fallback: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<usize, Failure> {
        if !self.scopes.contains_key(&backend::occurrence(schema)) {
            self.scan(schema, fallback, 0, attempt)?;
        }
        self.scope(schema, fallback, attempt)
    }

    pub(crate) fn context<'a>(
        &'a self,
        scope: usize,
        schema: JsonRef<'a>,
        spelling: &'a str,
    ) -> crate::ReferenceContext<'a> {
        crate::ReferenceContext {
            spelling,
            schema,
            resource: &self.entries[scope].document,
            base: self.entries[scope].base.as_deref(),
        }
    }
    pub(crate) fn keep_defs(&self, scope: usize) -> bool {
        self.entries[scope].keep_defs
    }

    pub(crate) fn scope(
        &self,
        schema: JsonRef<'_>,
        fallback: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<usize, Failure> {
        attempt.work(self.scopes.len().saturating_add(1).ilog2() as usize + 1)?;
        Ok(self
            .scopes
            .get(&backend::occurrence(schema))
            .copied()
            .unwrap_or(fallback))
    }

    fn declare(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<usize, Failure> {
        let node = backend::occurrence(schema);
        if let Some(id) = schema.get("$id") {
            let text = id.as_str().ok_or(Failure::InvalidReference)?;
            attempt.allocate(
                text.len()
                    .saturating_add(self.entries[scope].base.as_ref().map_or(0, String::len))
                    .saturating_mul(8)
                    .saturating_add(128),
            )?;
            let resolved = if uri::absolute(text) {
                text.to_owned()
            } else {
                uri::resolve(
                    self.entries[scope]
                        .base
                        .as_deref()
                        .ok_or(Failure::InvalidReference)?,
                    text,
                )
                .ok_or(Failure::InvalidReference)?
            };
            let base = identity(&resolved, attempt)?;
            let mut existing = None;
            for (index, entry) in self.entries.iter().enumerate() {
                attempt.work(base.len().saturating_add(1))?;
                if entry.base.as_deref() == Some(&base) {
                    existing = Some(index);
                    break;
                }
            }
            return if let Some(index) = existing {
                if backend::occurrence(self.entries[index].document.view()) != node {
                    return Err(Failure::InvalidReference);
                }
                Ok(index)
            } else {
                attempt.allocate(std::mem::size_of::<Resource>().saturating_mul(2))?;
                self.entries.push(Resource {
                    base: Some(base),
                    document: schema.to_owned(),
                    keep_defs: false,
                });
                Ok(self.entries.len() - 1)
            };
        }
        Ok(scope)
    }

    fn scan(
        &mut self,
        schema: JsonRef<'_>,
        mut scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        attempt.depth(depth)?;
        let node = backend::occurrence(schema);
        attempt.work(self.scopes.len().saturating_add(1).ilog2() as usize + 1)?;
        if self.scopes.contains_key(&node) {
            return Ok(());
        }
        scope = self.declare(schema, scope, attempt)?;
        attempt.allocate(std::mem::size_of::<((usize, usize), usize)>().saturating_mul(4))?;
        self.scopes.insert(node, scope);
        if let Some(members) = schema.members() {
            for member in members {
                self.entries[scope].keep_defs |= outside_keyword(member.name, attempt)?;
            }
        }
        for keyword in ["$anchor", "$dynamicAnchor"] {
            if let Some(anchor) = schema.get(keyword) {
                let name = anchor.as_str().ok_or(Failure::InvalidReference)?;
                attempt.allocate(name.len().saturating_add(
                    std::mem::size_of::<((usize, String), JsonValue)>().saturating_mul(4),
                ))?;
                if let Some(previous) = self
                    .anchors
                    .insert((scope, name.to_owned()), schema.to_owned())
                    && backend::occurrence(previous.view()) != node
                {
                    return Err(Failure::InvalidReference);
                }
            }
        }
        let mut children = attempt.vector(schema.members().map_or(0, |m| m.len()))?;
        let mut failure = None;
        each_schema_child(schema, |child| {
            if failure.is_none() {
                if let Err(error) =
                    attempt.allocate(std::mem::size_of::<JsonRef<'_>>().saturating_mul(2))
                {
                    failure = Some(error);
                } else {
                    children.push(child);
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        for child in children {
            self.scan(child, scope, depth + 1, attempt)?;
        }
        Ok(())
    }

    fn target(
        &self,
        scope: usize,
        fragment: &str,
        attempt: &mut Attempt<'_>,
    ) -> Result<(JsonValue, usize), Failure> {
        attempt.allocate(fragment.len())?;
        let decoded = uri::decode_fragment(fragment).ok_or(Failure::InvalidReference)?;
        let target = if decoded.is_empty() || decoded.starts_with('/') {
            fragment_target(&self.entries[scope].document, fragment, attempt)?
        } else {
            attempt.work(
                self.anchors
                    .len()
                    .saturating_add(1)
                    .saturating_mul(decoded.len().saturating_add(1)),
            )?;
            self.anchors
                .get(&(scope, decoded))
                .cloned()
                .ok_or(Failure::MissingReference)?
        };
        Ok((target.clone(), self.scope(target.view(), scope, attempt)?))
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
        if text.is_empty() {
            return self.target(scope, "", attempt);
        }
        if let Some(fragment) = text.strip_prefix('#') {
            return self.target(scope, fragment, attempt);
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
                return self.target(index, fragment, attempt);
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
            json(r##"{"$id":"https://example.test/canonical/a","$ref":"b"}"##),
        )
        .with_resource("https://example.test/canonical/b", json("false"));
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
        assert_eq!(scope, 3);
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
        let input = {
            let schema = json(r##"{"a/b":[true,false],"duplicate":true,"duplicate":false}"##);
            SchemaInput::new(schema.clone(), schema)
        };
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
        let duplicate = {
            let schema = json("true");
            SchemaInput::new(schema.clone(), schema)
        }
        .with_resource("https://x/a/../b", json("true"))
        .with_resource("https://x/b#", json("false"));
        assert!(matches!(
            Resources::new(&duplicate, &mut attempt),
            Err(Failure::InvalidReference)
        ));
    }
}
