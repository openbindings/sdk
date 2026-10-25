//! Original locations for the library's errors.
//!
//! The library names a failing keyword by its location in the bundled
//! resource, `<bundle>#/$defs/<definition>/<keyword>`, which means nothing to
//! the document's author. A prepared value contract keeps, for each bundled
//! definition, the original location of the schema it was projected from, and
//! nothing else of the document: the error maps back to that location and the
//! keyword. Every subschema position is a definition of its own, so the
//! keyword is the first segment of the suffix. A copy of a definition, which
//! an in-place cycle's unfolding or a bounds program makes, maps to the
//! definition it copies. A definition a schema transform adds stands for one
//! keyword of a projected definition, and maps to that keyword.
use crate::bundle;
use openbindings::*;
use openbindings_internal_json::backend;
use openbindings_internal_schema::SchemaSpace;
use std::collections::HashMap;

/// The original schema locations a prepared value contract's explanations name.
pub(crate) struct Locations {
    // Original pointers, escaped and concatenated.
    text: String,
    // Distinct original resource URIs (`None` for the document).
    resources: Vec<Option<String>>,
    // By projected definition number: its resource and pointer span, or a
    // pointer through a member name with an unpaired unit.
    projected: Vec<(u32, Stored)>,
}
enum Stored {
    Text { resource: u32, start: u32, end: u32 },
    Exact { resource: u32, pointer: Pointer },
}

impl Locations {
    /// Record where every definition of `bundle` came from; `nodes` maps each
    /// projected definition number to its node in `space`.
    pub(crate) fn new(
        space: &SchemaSpace,
        nodes: &HashMap<u32, usize>,
        bundle: &bundle::Bundle,
    ) -> Self {
        let mut numbers: Vec<u32> = bundle.definitions.iter().map(|d| d.number).collect();
        numbers.sort_unstable();
        numbers.dedup();
        let mut locations = Locations {
            text: String::new(),
            resources: Vec::new(),
            projected: Vec::with_capacity(numbers.len()),
        };
        let mut resources: HashMap<Option<&str>, u32> = HashMap::new();
        let mut bytes = Vec::new();
        let mut scratch = Vec::new();
        for number in numbers {
            let Some(&node) = nodes.get(&number) else {
                continue;
            };
            let uri = space.source_uri(node);
            let next = resources.len() as u32;
            let resource = *resources.entry(uri).or_insert_with(|| {
                locations.resources.push(uri.map(str::to_owned));
                next
            });
            bytes.clear();
            let stored = if space.location_into(node, &mut bytes, &mut scratch)
                && let Ok(text) = std::str::from_utf8(&bytes)
            {
                let start = locations.text.len() as u32;
                locations.text.push_str(text);
                Stored::Text {
                    resource,
                    start,
                    end: locations.text.len() as u32,
                }
            } else {
                Stored::Exact {
                    resource,
                    pointer: space.location(node).pointer,
                }
            };
            locations.projected.push((number, stored));
        }
        locations.text.shrink_to_fit();
        locations.projected.shrink_to_fit();
        locations
    }

    /// The original location of the keyword the library names by `uri`, and
    /// the keyword a transform's definition stands for.
    pub(crate) fn locate(&self, uri: &str) -> Option<(SchemaLocation, Option<&'static str>)> {
        let fragment = uri.strip_prefix(bundle::URI)?.strip_prefix('#')?;
        let decoded;
        let fragment = if fragment.contains('%') {
            decoded = percent_decoded(fragment)?;
            decoded.as_str()
        } else {
            fragment
        };
        let rest = fragment.strip_prefix("/$defs/")?;
        let (name, suffix) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let (number, keyword) = bundle::parse_name(name)?;
        let at = self
            .projected
            .binary_search_by_key(&number, |(number, _)| *number)
            .ok()?;
        let (resource, mut pointer) = match &self.projected[at].1 {
            Stored::Text {
                resource,
                start,
                end,
            } => (
                *resource,
                backend::escaped_pointer(self.text[*start as usize..*end as usize].to_owned()),
            ),
            Stored::Exact { resource, pointer } => (*resource, pointer.clone()),
        };
        // A projected definition holds its original keywords, so the first
        // segment names one; a transform's definition stands for its keyword.
        let first = suffix
            .strip_prefix('/')
            .map(|rest| rest.split('/').next().unwrap_or(rest))
            .filter(|segment| !segment.is_empty());
        if let Some(segment) = keyword.or(first) {
            backend::push_pointer(&mut pointer, &format!("/{segment}"));
        }
        let resource = self.resources[resource as usize].clone();
        Some((SchemaLocation::new(resource, pointer), keyword))
    }
}

// Percent-decode a URI fragment the library wrote; `None` if it is not UTF-8.
fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = std::str::from_utf8(bytes.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}
