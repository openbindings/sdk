//! Optional exact source facts for library-bounded explanations.
//!
//! A prepared contract never keeps its document, supplied resources or schema
//! space. It keeps a compact copy of what an explanation reads: every projected
//! definition with only the keywords the validator compiled, their values
//! copied from the original source. A projection writes each child subschema
//! position as a reference to the child's own definition, which starts at the
//! child's original location; the copy keeps those positions as short
//! references, so following them reaches the same structure as the original
//! source. Library errors name only compiled keywords, so nothing an
//! explanation reads is left out. `const` and `enum` values stay in the
//! projected arenas that compiled literals keep anyway. When the program maps
//! one prefix origin per definition, the copy also holds each definition's
//! original location and replaces the program's location map; otherwise (the
//! paired bounds of a oneOf over missing resources) the contract keeps that map.
use jsonschema::error::{TypeKind, ValidationErrorKind as Kind};
use openbindings::*;
use openbindings_internal_json::{backend, text::TextValue};
use openbindings_internal_schema::{
    ChildShape, Program, ProgramOrigin, ProjectedResource, child_shape,
};
use std::sync::OnceLock;

fn work(control: &dyn WorkControl, units: usize) -> bool {
    !control.is_cancelled() && jsonschema::ob_work::charge(units.max(1))
}
// A JSON Pointer segment can be compared to a decoded name without decoding it
// into another String. Every source scan is charged to the diagnostic work pass.
fn segment_eq(name: impl Iterator<Item = u8>, token: &str) -> bool {
    let mut expected = name;
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
fn utf8(c: char) -> impl Iterator<Item = u8> {
    let mut buffer = [0; 4];
    let len = c.encode_utf8(&mut buffer).len();
    buffer.into_iter().take(len)
}

/// What an explaining contract keeps of its compiled program.
pub(crate) struct Explanation {
    // `[<common resource URI prefix>,{<resource key>:[<standard>,<source>,{<definition>:[<pointer>,<schema>]}]}]`.
    // Each schema holds only compiled keywords. A child position holds
    // `"<resource key>/<definition name>"`; `const` and `enum` hold
    // `[<literal arena>,<arena position>]`. `<source>` and `<pointer>` are the
    // definition's original location (a `null` source is the document), or both
    // `null` when the program keeps its own location map. `<standard>` says
    // whether the source is a packaged meta-schema.
    text: Box<str>,
    // Byte offsets of the prefix's characters and of the resources object.
    prefix: (u32, u32),
    resources: u32,
    // Projected resources whose arenas compiled literals keep anyway.
    literals: Vec<JsonValue>,
    // Whether the text holds every original location, so the program's own
    // location map is not needed.
    locates: bool,
    // Built by the first explanation that needs it; contracts that never
    // explain build none.
    index: OnceLock<Index>,
}
// Resources and their definitions sorted by name, with the byte offsets an
// explanation reads, so a lookup parses no text.
struct Index {
    resources: Box<[ResourceEntry]>,
}
struct ResourceEntry {
    // The generated key between its quotes.
    name: (u32, u32),
    standard: bool,
    // `None` for the document.
    source: Option<Raw>,
    definitions: Box<[DefinitionEntry]>,
}
struct DefinitionEntry {
    name: (u32, u32),
    // `None` when the program keeps its own location map.
    pointer: Option<Raw>,
    schema: u32,
}
// A string of the text: the start of its token and its unescaped characters
// when it has no escapes.
#[derive(Clone, Copy)]
struct Raw {
    token: u32,
    plain: Option<(u32, u32)>,
}
// A compact definition and what its resource says about its original source.
struct Definition<'a> {
    standard: bool,
    source: Option<Raw>,
    pointer: Option<Raw>,
    schema: TextValue<'a>,
}
/// An explained failure's original location and the value there.
pub(crate) struct Located<'a> {
    pub(crate) location: SchemaLocation,
    /// The value at `location`, or `None` where facts are not reported.
    pub(crate) operand: Option<Node<'a>>,
}
/// A value in the compact text or in a retained literal arena.
#[derive(Clone, Copy)]
pub(crate) enum Node<'a> {
    Text(TextValue<'a>),
    Json(JsonRef<'a>),
}
// Where a walk stands.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    // A schema whose members are compiled keywords.
    Schema,
    // An applicator value holding child positions.
    Children(ChildShape),
    // A child position, referring to the child's own definition.
    Child,
    // A keyword value copied from the original source.
    Copied,
}
// A definition's original location: from this text or from the program's map.
enum Original<'a> {
    Text {
        source: Option<Raw>,
        pointer: Raw,
        standard: bool,
    },
    Map(&'a openbindings_internal_schema::Location),
}

// How the compact text keeps one compiled keyword.
#[derive(Clone, Copy)]
enum Kept {
    Children(ChildShape),
    // `const` and `enum`, kept in their literal arenas.
    Literal,
    // Reference targets and patterns: strings no explanation reads.
    Placeholder,
    Copied,
}
// One match for what `operands::role` and [`child_shape`] say together; a unit test
// keeps the three tables in agreement.
fn kept(keyword: &str) -> Option<Kept> {
    Some(match keyword {
        "properties" | "patternProperties" | "dependentSchemas" => Kept::Children(ChildShape::Map),
        "allOf" | "anyOf" | "oneOf" | "prefixItems" => Kept::Children(ChildShape::List),
        "additionalProperties"
        | "propertyNames"
        | "unevaluatedProperties"
        | "items"
        | "contains"
        | "unevaluatedItems"
        | "not"
        | "if"
        | "then"
        | "else" => Kept::Children(ChildShape::Single),
        "const" | "enum" => Kept::Literal,
        "$ref" | "$dynamicRef" | "pattern" => Kept::Placeholder,
        "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "multipleOf"
        | "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties"
        | "maxProperties" | "minContains" | "maxContains" | "$dynamicAnchor" | "type"
        | "uniqueItems" | "required" | "dependentRequired" => Kept::Copied,
        _ => return None,
    })
}
// Compact text writer, run once to measure and once to fill an exact buffer.
struct Writer<'r, W> {
    out: W,
    prefix: usize,
    // Literal arenas named so far; kept only when filling.
    named: usize,
    literals: Option<&'r mut Vec<JsonValue>>,
}
// Where the writer puts the text: a length count, then its exact buffer.
trait Sink {
    fn put(&mut self, text: &str);
    fn put_char(&mut self, c: char);
    // ASCII bytes, such as digits.
    fn put_ascii(&mut self, bytes: &[u8]);
}
struct Count(usize);
impl Sink for Count {
    fn put(&mut self, text: &str) {
        self.0 += text.len();
    }
    fn put_char(&mut self, c: char) {
        self.0 += c.len_utf8();
    }
    fn put_ascii(&mut self, bytes: &[u8]) {
        self.0 += bytes.len();
    }
}
impl Sink for String {
    fn put(&mut self, text: &str) {
        self.push_str(text);
    }
    fn put_char(&mut self, c: char) {
        self.push(c);
    }
    fn put_ascii(&mut self, bytes: &[u8]) {
        debug_assert!(bytes.is_ascii());
        self.push_str(std::str::from_utf8(bytes).expect("ASCII"));
    }
}
impl<W: Sink> Writer<'_, W> {
    fn put(&mut self, text: &str) {
        self.out.put(text);
    }
    fn put_char(&mut self, c: char) {
        self.out.put_char(c);
    }
    fn quoted(&mut self, text: &str) {
        // Generated names and resource URI suffixes need no escapes.
        debug_assert!(!text.contains(['"', '\\']) && !text.contains(char::is_control));
        self.quoted_raw(text);
    }
    fn quoted_raw(&mut self, text: &str) {
        self.put_char('"');
        self.put(text);
        self.put_char('"');
    }
    fn put_number(&mut self, mut n: usize) {
        let mut digits = [0u8; 20];
        let mut at = digits.len();
        loop {
            at -= 1;
            digits[at] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        self.out.put_ascii(&digits[at..]);
    }
    // Original text as a JSON string.
    fn escaped(&mut self, text: &str) {
        if !text.bytes().any(|b| b == b'"' || b == b'\\' || b < 0x20) {
            return self.quoted_raw(text);
        }
        self.put_char('"');
        for c in text.chars() {
            match c {
                '"' => self.put("\\\""),
                '\\' => self.put("\\\\"),
                c if (c as u32) < 0x20 => {
                    const HEX: &[u8; 16] = b"0123456789abcdef";
                    let low = c as usize;
                    self.out
                        .put_ascii(&[b'\\', b'u', b'0', b'0', HEX[low >> 4], HEX[low & 15]]);
                }
                c => self.put_char(c),
            }
        }
        self.put_char('"');
    }
    fn begin(&mut self, resources: &[ProjectedResource]) {
        let prefix = &resources.first().map_or("", |r| r.uri.as_str())[..self.prefix];
        self.put_char('[');
        self.quoted(prefix);
        self.put(",{");
    }
    // One prefix origin per generated definition: write each resource's
    // definitions with their original locations. `Ok(false)` when the map holds
    // anything else, such as the exact summaries of paired oneOf bounds.
    fn located(
        &mut self,
        resources: &[ProjectedResource],
        locations: &std::collections::BTreeMap<String, ProgramOrigin>,
        control: &dyn WorkControl,
    ) -> Result<bool, NoVerdict> {
        use std::ops::Bound;
        self.begin(resources);
        let mut written = 0usize;
        let mut first_resource = true;
        for resource in resources {
            let uri = resource.uri.as_str();
            // A resource's keys are `<uri>#/$defs/<name>`. Resource URIs hold no
            // `#`, which sorts below every character another URI could add, so
            // its keys are adjacent and start the map's range from `<uri>`.
            let mut keys = locations
                .range::<str, _>((Bound::Included(uri), Bound::Unbounded))
                .map_while(|(key, origin)| {
                    Some((key.strip_prefix(uri)?.strip_prefix('#')?, origin))
                })
                .peekable();
            // A resource no location maps into holds nothing an error can name,
            // such as the constant resource of a partial bound.
            let Some(&(_, first)) = keys.peek() else {
                continue;
            };
            let (ProgramOrigin::Prefix(first), Some(definitions)) =
                (first, resource.document.get("$defs"))
            else {
                return Ok(false);
            };
            let source = first.resource.as_deref();
            if !first_resource {
                self.put_char(',');
            }
            first_resource = false;
            self.quoted(&uri[self.prefix..]);
            self.put(":[");
            let standard = source.is_some_and(|uri| {
                openbindings_internal_schema::embedded_resources()
                    .iter()
                    .any(|resource| resource.uri == uri)
            });
            self.put(if standard { "true," } else { "false," });
            match source {
                Some(source) => self.escaped(source),
                None => self.put("null"),
            }
            self.put(",{");
            // Projections write definitions in the map's order.
            let mut next = 0;
            let mut arena = None;
            for (position, (fragment, origin)) in keys.enumerate() {
                control.check()?;
                let ProgramOrigin::Prefix(at) = origin else {
                    return Ok(false);
                };
                let Some(name) = fragment
                    .strip_prefix("/$defs/")
                    .filter(|name| !name.contains('/'))
                else {
                    return Ok(false);
                };
                if at.resource.as_deref() != source {
                    return Ok(false);
                }
                let expected = backend::member_at(definitions, next)
                    .filter(|member| member.name.as_str() == Some(name));
                let schema = match expected {
                    Some(member) => {
                        next += 1;
                        member.value
                    }
                    None => match definitions.get(name) {
                        Some(schema) => schema,
                        None => return Ok(false),
                    },
                };
                if position != 0 {
                    self.put_char(',');
                }
                self.quoted(name);
                self.put(":[");
                self.escaped(&at.pointer);
                self.put_char(',');
                self.schema(resource, schema, &mut arena);
                self.put_char(']');
                written += 1;
            }
            self.put("}]");
        }
        self.put("}]");
        // Every key names a definition of one program resource.
        Ok(written == locations.len())
    }
    // Every projected definition without original locations, in projected order.
    fn unlocated(
        &mut self,
        resources: &[ProjectedResource],
        control: &dyn WorkControl,
    ) -> Result<(), NoVerdict> {
        self.begin(resources);
        for (position, resource) in resources.iter().enumerate() {
            if position != 0 {
                self.put_char(',');
            }
            self.quoted(&resource.uri[self.prefix..]);
            self.put(":[false,null,{");
            let mut arena = None;
            let definitions = resource.document.get("$defs").and_then(|d| d.members());
            for (i, definition) in definitions.into_iter().flatten().enumerate() {
                control.check()?;
                if i != 0 {
                    self.put_char(',');
                }
                self.put(definition.name.text());
                self.put(":[null,");
                self.schema(resource, definition.value, &mut arena);
                self.put_char(']');
            }
            self.put("}]");
        }
        self.put("}]");
        Ok(())
    }
    fn schema(
        &mut self,
        resource: &ProjectedResource,
        schema: JsonRef<'_>,
        arena: &mut Option<usize>,
    ) {
        let Some(members) = schema.members() else {
            return self.put(schema.text());
        };
        self.put_char('{');
        let mut first = true;
        for field in members {
            let Some(kept) = field.name.as_str().and_then(kept) else {
                continue;
            };
            if !first {
                self.put_char(',');
            }
            first = false;
            self.put(field.name.text());
            self.put_char(':');
            match kept {
                Kept::Children(ChildShape::Map) => {
                    self.put_char('{');
                    for (i, child) in field.value.members().into_iter().flatten().enumerate() {
                        if i != 0 {
                            self.put_char(',');
                        }
                        self.put(child.name.text());
                        self.put_char(':');
                        self.child(child.value);
                    }
                    self.put_char('}');
                }
                Kept::Children(ChildShape::List) => {
                    self.put_char('[');
                    for (i, child) in field.value.elements().into_iter().flatten().enumerate() {
                        if i != 0 {
                            self.put_char(',');
                        }
                        self.child(child);
                    }
                    self.put_char(']');
                }
                Kept::Children(ChildShape::Single) => self.child(field.value),
                Kept::Literal => {
                    let index = match *arena {
                        Some(index) => index,
                        None => {
                            let index = self.named;
                            self.named += 1;
                            if let Some(literals) = &mut self.literals {
                                literals.push(resource.document.clone());
                            }
                            *arena = Some(index);
                            index
                        }
                    };
                    self.put_char('[');
                    self.put_number(index);
                    self.put_char(',');
                    self.put_number(backend::ref_id(field.value));
                    self.put_char(']');
                }
                Kept::Placeholder => self.put("\"\""),
                Kept::Copied => self.put(field.value.text()),
            }
        }
        self.put_char('}');
    }
    // A child position written by the projection as `{"$ref":<address>}`, its
    // only member.
    fn child(&mut self, child: JsonRef<'_>) {
        let address = backend::member_at(child, 0)
            .filter(|member| member.name.as_str() == Some("$ref"))
            .and_then(|member| member.value.as_str())
            // Every program resource URI starts with the shared prefix.
            .and_then(|address| address.get(self.prefix..)?.split_once('#'))
            .and_then(|(key, fragment)| Some((key, fragment.strip_prefix("/$defs/")?)));
        match address {
            Some((key, name)) => {
                self.put_char('"');
                self.put(key);
                self.put_char('/');
                self.put(name);
                self.put_char('"');
            }
            // Generated glue that no original location maps into.
            None => self.put("null"),
        }
    }
}
// Decode a string of this text into a new owner with room for `extra` bytes.
fn owned(value: TextValue<'_>, extra: usize) -> Option<String> {
    let text = value.string()?;
    let mut out = String::with_capacity(text.len() + extra);
    match text.as_unescaped() {
        Some(raw) => out.push_str(raw),
        None => out.extend(text.chars()),
    }
    Some(out)
}

impl Explanation {
    /// Copy what explanations read from a compiled program's projected
    /// resources, before they are released, with the program's original
    /// locations when its map holds one prefix origin per definition. The text
    /// is measured first, so it takes one exact allocation.
    pub(crate) fn new(program: &Program, control: &dyn WorkControl) -> Result<Self, NoVerdict> {
        let resources = &program.resources;
        let mut prefix = resources.first().map_or(0, |r| r.uri.len());
        for resource in resources {
            let common = resource
                .uri
                .bytes()
                .zip(resources[0].uri.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            prefix = prefix.min(common);
        }
        while !resources
            .first()
            .is_none_or(|r| r.uri.is_char_boundary(prefix))
        {
            prefix -= 1;
        }
        let writer = |out| Writer {
            out,
            prefix,
            named: 0,
            literals: None,
        };
        let mut count = writer(Count(0));
        let locates = count.located(resources, &program.locations, control)?;
        if !locates {
            count = writer(Count(0));
            count.unlocated(resources, control)?;
        }
        let len = count.out.0;
        let mut literals = Vec::with_capacity(count.named);
        let mut fill = Writer {
            out: String::with_capacity(len),
            prefix,
            named: 0,
            literals: Some(&mut literals),
        };
        if locates {
            fill.located(resources, &program.locations, control)?;
        } else {
            fill.unlocated(resources, control)?;
        }
        debug_assert_eq!(fill.out.len(), len);
        // The text starts `["<prefix>",{`.
        let prefix_end = 2 + prefix;
        Ok(Self {
            text: fill.out.into_boxed_str(),
            prefix: (2, prefix_end as u32),
            resources: (prefix_end + 2) as u32,
            literals,
            locates,
            index: OnceLock::new(),
        })
    }
    /// Whether explanations need the program's own location map.
    pub(crate) fn needs_locations(&self) -> bool {
        !self.locates
    }
    fn resources(&self) -> TextValue<'_> {
        TextValue::at(&self.text, self.resources as usize)
    }
    fn index(&self) -> &Index {
        self.index.get_or_init(|| {
            let name = |member: &openbindings_internal_json::text::TextMember<'_>| {
                let start = member.name.start() + 1;
                (start as u32, (start + member.name.text().len() - 2) as u32)
            };
            let mut resources: Vec<_> = self
                .resources()
                .members()
                .map(|resource| {
                    let mut parts = resource.value.elements();
                    let standard = parts.next().is_some_and(|flag| flag.text() == "true");
                    let source = parts.next().and_then(|source| self.raw(source));
                    let mut definitions: Vec<_> = parts
                        .next()
                        .into_iter()
                        .flat_map(TextValue::members)
                        .map(|definition| {
                            let mut parts = definition.value.elements();
                            let pointer = parts.next().and_then(|p| self.raw(p));
                            let schema = parts.next().map_or(0, |s| s.start() as u32);
                            DefinitionEntry {
                                name: name(&definition),
                                pointer,
                                schema,
                            }
                        })
                        .collect();
                    definitions.sort_unstable_by(|a, b| self.slice(a.name).cmp(self.slice(b.name)));
                    ResourceEntry {
                        name: name(&resource),
                        standard,
                        source,
                        definitions: definitions.into_boxed_slice(),
                    }
                })
                .collect();
            resources.sort_unstable_by(|a, b| self.slice(a.name).cmp(self.slice(b.name)));
            Index {
                resources: resources.into_boxed_slice(),
            }
        })
    }
    fn slice(&self, range: (u32, u32)) -> &str {
        &self.text[range.0 as usize..range.1 as usize]
    }
    // Where a string of this text starts, and its characters when unescaped.
    fn raw(&self, value: TextValue<'_>) -> Option<Raw> {
        let text = value.string()?;
        let plain = text.as_unescaped().map(|plain| {
            let start = value.start() as u32 + 1;
            (start, start + plain.len() as u32)
        });
        Some(Raw {
            token: value.start() as u32,
            plain,
        })
    }
    // A new owner of a string of this text, with room for `extra` bytes.
    fn owned(&self, raw: Raw, extra: usize) -> Option<String> {
        match raw.plain {
            Some(plain) => {
                let plain = self.slice(plain);
                let mut out = String::with_capacity(plain.len() + extra);
                out.push_str(plain);
                Some(out)
            }
            None => owned(self.at(raw.token), extra),
        }
    }
    fn at(&self, offset: u32) -> TextValue<'_> {
        TextValue::at(&self.text, offset as usize)
    }
    // A definition of the resource with this generated key.
    fn definition(
        &self,
        key: &str,
        name: &str,
        control: &dyn WorkControl,
    ) -> Result<Option<Definition<'_>>, ()> {
        if !work(control, key.len().saturating_add(name.len())) {
            return Err(());
        }
        let index = self.index();
        let Ok(resource) = index
            .resources
            .binary_search_by(|entry| self.slice(entry.name).cmp(key))
            .map(|i| &index.resources[i])
        else {
            return Ok(None);
        };
        let Ok(definition) = resource
            .definitions
            .binary_search_by(|entry| self.slice(entry.name).cmp(name))
            .map(|i| &resource.definitions[i])
        else {
            return Ok(None);
        };
        Ok(Some(Definition {
            standard: resource.standard,
            source: resource.source,
            pointer: definition.pointer,
            schema: self.at(definition.schema),
        }))
    }
    // The resource key of a generated base URI.
    fn key<'a>(&self, base: &'a str) -> Option<&'a str> {
        base.strip_prefix(&self.text[self.prefix.0 as usize..self.prefix.1 as usize])
    }
    // The schema a child position refers to.
    fn follow<'a>(
        &'a self,
        child: TextValue<'a>,
        control: &dyn WorkControl,
    ) -> Result<Option<TextValue<'a>>, ()> {
        let Some(reference) = child.string().and_then(|s| s.as_unescaped()) else {
            return Ok(None);
        };
        let Some((key, name)) = reference.rsplit_once('/') else {
            return Ok(None);
        };
        Ok(self
            .definition(key, name, control)?
            .map(|definition| definition.schema))
    }
    // A `const` or `enum` value kept in its projected arena.
    fn literal(&self, reference: TextValue<'_>) -> Option<JsonRef<'_>> {
        let mut parts = reference.elements();
        let arena: usize = parts.next()?.number_text()?.parse().ok()?;
        let position: usize = parts.next()?.number_text()?.parse().ok()?;
        backend::node(self.literals.get(arena)?, position)
    }
    // Take one pointer token from `node`, or `None` when nothing exists there.
    fn step<'a>(
        &'a self,
        node: Node<'a>,
        mode: Mode,
        token: &str,
        control: &dyn WorkControl,
    ) -> Result<Option<(Node<'a>, Mode)>, ()> {
        let (node, mode) = match (node, mode) {
            (Node::Text(child), Mode::Child) => match self.follow(child, control)? {
                Some(schema) => (Node::Text(schema), Mode::Schema),
                None => return Ok(None),
            },
            other => other,
        };
        match node.kind() {
            JsonKind::Object => {
                let Some((value, name)) = member(node, token, control)? else {
                    return Ok(None);
                };
                Ok(Some(match mode {
                    Mode::Schema => match name.and_then(child_shape) {
                        Some(ChildShape::Single) => (value, Mode::Child),
                        Some(shape) => (value, Mode::Children(shape)),
                        // `const` and `enum` values live in their literal arenas.
                        None if matches!(name, Some("const" | "enum")) => match value {
                            Node::Text(reference) => match self.literal(reference) {
                                Some(literal) => (Node::Json(literal), Mode::Copied),
                                None => return Ok(None),
                            },
                            Node::Json(_) => (value, Mode::Copied),
                        },
                        None => (value, Mode::Copied),
                    },
                    Mode::Children(ChildShape::Map) => (value, Mode::Child),
                    _ => (value, Mode::Copied),
                }))
            }
            JsonKind::Array => {
                if token.is_empty()
                    || (token.len() > 1 && token.starts_with('0'))
                    || !token.bytes().all(|b| b.is_ascii_digit())
                {
                    return Ok(None);
                }
                let Some(value) = token.parse().ok().and_then(|i| node.element(i)) else {
                    return Ok(None);
                };
                Ok(Some(match mode {
                    Mode::Children(ChildShape::List) => (value, Mode::Child),
                    _ => (value, Mode::Copied),
                }))
            }
            _ => Ok(None),
        }
    }
    /// Map a library keyword location to its original location, keeping the
    /// nearest existing ancestor when the library names a path the source
    /// lacks. Interrupted lookup reports no location rather than an unchecked
    /// one. Packaged meta-schemas are original sources for location validity,
    /// but their facts are not reported. `program` is the compiled program when
    /// its own location map is kept.
    // Out of line, so the diagnostic pass that calls it stays small enough for
    // the compiler to inline that pass's budget scopes.
    #[inline(never)]
    pub(crate) fn locate(
        &self,
        program: Option<&Program>,
        uri: &str,
        control: &dyn WorkControl,
    ) -> Result<Option<Located<'_>>, ()> {
        if self.locates {
            // Every mapped origin is a definition's prefix: the generated URI
            // names `<resource>#/$defs/<definition>` and then a suffix.
            let uri = match uri.split_once('#') {
                Some((base, fragment)) if fragment.contains('%') => {
                    let Some(mut decoded) =
                        openbindings_internal_schema::uri::decode_fragment(fragment)
                    else {
                        return Ok(None);
                    };
                    decoded.insert(0, '#');
                    decoded.insert_str(0, base);
                    std::borrow::Cow::Owned(decoded)
                }
                _ => std::borrow::Cow::Borrowed(uri),
            };
            let Some((base, fragment)) = uri.split_once('#') else {
                return Ok(None);
            };
            let Some(fragment) = fragment.strip_prefix("/$defs/") else {
                return Ok(None);
            };
            let (name, suffix) = fragment.split_at(fragment.find('/').unwrap_or(fragment.len()));
            let Some(key) = self.key(base) else {
                return Ok(None);
            };
            let Some(definition) = self.definition(key, name, control)? else {
                return Ok(None);
            };
            let Some(pointer) = definition.pointer else {
                return Ok(None);
            };
            let original = Original::Text {
                source: definition.source,
                pointer,
                standard: definition.standard,
            };
            return self.walk(definition.schema, "", suffix, original, control);
        }
        let Some(Ok(Some(found))) = program.map(|program| program.origin(uri, usize::MAX)) else {
            return Ok(None);
        };
        let suffix = found.suffix();
        let at = match found.origin {
            ProgramOrigin::Prefix(at) => at,
            ProgramOrigin::Exact(at) if suffix.is_empty() => at,
            ProgramOrigin::Exact(_) | ProgramOrigin::Synthetic => return Ok(None),
        };
        // Every mapped key is a generated definition, or a keyword inside one.
        let key = &found.uri[..found.key];
        let Some((resource, name, rest)) = key.split_once('#').and_then(|(base, fragment)| {
            let fragment = fragment.strip_prefix("/$defs/")?;
            let (name, rest) = fragment.split_once('/').unwrap_or((fragment, ""));
            Some((self.key(base)?, name, rest))
        }) else {
            return Ok(None);
        };
        let Some(schema) = self
            .definition(resource, name, control)?
            .map(|definition| definition.schema)
        else {
            return Ok(None);
        };
        self.walk(schema, rest, suffix, Original::Map(at), control)
    }
    // Walk the keyword path inside a mapped key, which exists, then as much of
    // the generated suffix as exists.
    fn walk<'a>(
        &'a self,
        schema: TextValue<'a>,
        key_rest: &str,
        suffix: &str,
        original: Original<'_>,
        control: &dyn WorkControl,
    ) -> Result<Option<Located<'a>>, ()> {
        let mut node = Node::Text(schema);
        let mut mode = Mode::Schema;
        if !key_rest.is_empty() {
            for token in key_rest.split('/') {
                if !work(control, token.len()) {
                    return Err(());
                }
                let Some(next) = self.step(node, mode, token, control)? else {
                    return Ok(None);
                };
                (node, mode) = next;
            }
        }
        let mut end = 0;
        if let Some(rest) = suffix.strip_prefix('/') {
            for token in rest.split('/') {
                if !work(control, token.len()) {
                    return Err(());
                }
                let Some(next) = self.step(node, mode, token, control)? else {
                    break;
                };
                (node, mode) = next;
                end += 1 + token.len();
            }
        }
        let (resource, mut pointer, standard) = match original {
            Original::Map(at) => {
                let mut pointer = String::with_capacity(at.pointer.len() + end);
                pointer.push_str(&at.pointer);
                let mut standard = false;
                if let Some(uri) = &at.resource {
                    for resource in openbindings_internal_schema::embedded_resources() {
                        if !work(control, uri.len().saturating_add(resource.uri.len())) {
                            return Err(());
                        }
                        if resource.uri == *uri {
                            standard = true;
                            break;
                        }
                    }
                }
                (at.resource.clone(), pointer, standard)
            }
            Original::Text {
                source,
                pointer,
                standard,
            } => {
                let resource = match source {
                    Some(source) => Some(self.owned(source, 0).ok_or(())?),
                    None => None,
                };
                (resource, self.owned(pointer, end).ok_or(())?, standard)
            }
        };
        pointer.push_str(&suffix[..end]);
        Ok(Some(Located {
            location: SchemaLocation { resource, pointer },
            operand: (!standard).then_some(node),
        }))
    }
}
impl<'a> Node<'a> {
    fn kind(self) -> JsonKind {
        match self {
            Self::Text(value) => value.kind(),
            Self::Json(value) => value.kind(),
        }
    }
    fn text(self) -> &'a str {
        match self {
            Self::Text(value) => value.text(),
            Self::Json(value) => value.text(),
        }
    }
    fn number_text(self) -> Option<&'a str> {
        match self {
            Self::Text(value) => value.number_text(),
            Self::Json(value) => value.number_text(),
        }
    }
    fn element(self, index: usize) -> Option<Self> {
        match self {
            Self::Text(value) => value.elements().nth(index).map(Self::Text),
            Self::Json(value) => value.element(index).map(Self::Json),
        }
    }
    fn elements(self) -> impl Iterator<Item = Self> + 'a {
        let (text, json) = match self {
            Self::Text(value) => (Some(value.elements()), None),
            Self::Json(value) => (None, value.elements()),
        };
        text.into_iter()
            .flatten()
            .map(Self::Text)
            .chain(json.into_iter().flatten().map(Self::Json))
    }
    // Decoded string length and an equality test, or `None` for non-strings and
    // strings with unpaired escapes.
    fn string_eq(self, other: &str) -> Option<(usize, bool)> {
        match self {
            Self::Text(value) => value.string().map(|s| (s.len(), s.eq_str(other))),
            Self::Json(value) => value.as_str().map(|s| (s.len(), s == other)),
        }
    }
}
// A member found by a pointer token: its value and its name when unescaped.
type Member<'a> = Option<(Node<'a>, Option<&'a str>)>;
// The first member named by a pointer token, with its name when unescaped.
// Names with unpaired surrogates never match, as in the arena's reads.
fn member<'a>(node: Node<'a>, token: &str, control: &dyn WorkControl) -> Result<Member<'a>, ()> {
    match node {
        Node::Text(node) => {
            for m in node.members() {
                let Some(name) = m.name_string() else {
                    continue;
                };
                if !work(control, name.len().saturating_add(token.len())) {
                    return Err(());
                }
                let found = match name.as_unescaped() {
                    Some(raw) => segment_eq(raw.bytes(), token),
                    None => segment_eq(name.chars().flat_map(utf8), token),
                };
                if found {
                    return Ok(Some((Node::Text(m.value), name.as_unescaped())));
                }
            }
        }
        Node::Json(node) => {
            for m in node.members().into_iter().flatten() {
                let Some(name) = m.name.as_str() else {
                    continue;
                };
                if !work(control, name.len().saturating_add(token.len())) {
                    return Err(());
                }
                if segment_eq(name.bytes(), token) {
                    return Ok(Some((Node::Json(m.value), Some(name))));
                }
            }
        }
    }
    Ok(None)
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
/// Copy the schema facts a failure's keyword has at its located operand.
pub(crate) fn collect(
    operand: Option<Node<'_>>,
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
    let operand = operand?;
    match code {
        "required" => {
            if operand.kind() != JsonKind::Array {
                return None;
            }
            let Kind::Required { property } = kind else {
                return None;
            };
            // Recover only the missing member established by the library.
            let member = property.as_str()?;
            let cost = "required".len().saturating_add(member.len());
            let mut verified = false;
            for required in operand.elements() {
                let (len, equal) = required.string_eq(member)?;
                if !work(control, len.saturating_add(member.len())) {
                    return None;
                }
                if equal {
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
            if operand.kind() != JsonKind::Array {
                return None;
            }
            let mut cost = "enum".len();
            let mut count = 0usize;
            for choice in operand.elements() {
                // Exact JSON tokens are nonempty: item count <= token bytes.
                // Thus collection/wire framing is bounded without a full Vec.
                let text = choice.text();
                cost = cost.saturating_add(text.len());
                if !work(control, text.len()) {
                    return None;
                }
                count += 1;
            }
            if !work(control, 1) {
                return None;
            }
            copied(cost, count);
            let mut choices = Vec::with_capacity(count);
            for choice in operand.elements() {
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
    use crate::operands::{Role, role};
    use std::sync::Arc;

    // The projected program a default preparation compiles for `schema`.
    fn projected(schema: &str) -> (Program, Explanation) {
        #[derive(Default)]
        struct Capture(std::sync::Mutex<Option<SchemaRequest>>);
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
        let capture = Capture::default();
        let document = Document::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
        ))
        .unwrap();
        let document = document.validate().unwrap().conformant().unwrap();
        let operation = document.resolve_operation("op").unwrap();
        assert!(operation.input().unwrap().prepare(&capture).is_ok());
        let request = capture.0.lock().unwrap().take().unwrap();
        let (space, entry) = crate::planner::space(&request);
        let mut program = space.program(entry, &|| false).unwrap();
        let control = std::sync::atomic::AtomicBool::new(false);
        let explanation = Explanation::new(&program, &control).unwrap();
        // A strict projection maps one prefix origin per definition, so the
        // explanation holds every location itself.
        assert!(!explanation.needs_locations());
        program.resources = Vec::new();
        (program, explanation)
    }
    // The generated keyword URI for an original document pointer and suffix.
    fn generated(program: &Program, pointer: &str, suffix: &str) -> String {
        let key = program
            .locations
            .iter()
            .find_map(|(key, origin)| match origin {
                ProgramOrigin::Prefix(at) if at.resource.is_none() && at.pointer == pointer => {
                    Some(key.clone())
                }
                _ => None,
            })
            .expect("projected node");
        key + suffix
    }

    #[test]
    fn kept_keywords_are_exactly_the_compiled_keywords_with_their_child_shapes() {
        // Every 2020-12 keyword, plus spellings the projection copies unchanged.
        for keyword in [
            "$id",
            "$schema",
            "$ref",
            "$anchor",
            "$dynamicRef",
            "$dynamicAnchor",
            "$vocabulary",
            "$comment",
            "$defs",
            "allOf",
            "anyOf",
            "oneOf",
            "not",
            "if",
            "then",
            "else",
            "dependentSchemas",
            "prefixItems",
            "items",
            "contains",
            "properties",
            "patternProperties",
            "additionalProperties",
            "propertyNames",
            "unevaluatedItems",
            "unevaluatedProperties",
            "type",
            "enum",
            "const",
            "multipleOf",
            "maximum",
            "exclusiveMaximum",
            "minimum",
            "exclusiveMinimum",
            "maxLength",
            "minLength",
            "pattern",
            "maxItems",
            "minItems",
            "uniqueItems",
            "maxContains",
            "minContains",
            "maxProperties",
            "minProperties",
            "required",
            "dependentRequired",
            "format",
            "contentEncoding",
            "contentMediaType",
            "contentSchema",
            "title",
            "description",
            "default",
            "deprecated",
            "readOnly",
            "writeOnly",
            "examples",
            "x-unknown",
        ] {
            let kept = kept(keyword);
            assert_eq!(kept.is_some(), role(keyword).is_some(), "{keyword}");
            let shape = match kept {
                Some(Kept::Children(shape)) => Some(shape),
                _ => None,
            };
            assert_eq!(
                shape,
                child_shape(keyword).filter(|_| role(keyword).is_some()),
                "{keyword}"
            );
            assert_eq!(
                matches!(kept, Some(Kept::Literal)),
                matches!(role(keyword), Some(Role::Const | Role::Enum)),
                "{keyword}"
            );
        }
    }

    #[test]
    fn interrupted_lookup_and_enum_collection_copy_no_details() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct StopAfter {
            calls: AtomicUsize,
            limit: usize,
        }
        impl WorkControl for StopAfter {
            fn is_cancelled(&self) -> bool {
                self.calls.fetch_add(1, Ordering::Relaxed) >= self.limit
            }
        }
        let (program, projection) = projected(r#"{"enum":[0,1,2]}"#);
        let uri = generated(&program, "/operations/op/input", "/enum");
        let lookup = StopAfter {
            calls: AtomicUsize::new(0),
            limit: usize::MAX,
        };
        let located = projection
            .locate(Some(&program), &uri, &lookup)
            .unwrap()
            .unwrap();
        assert_eq!(located.location.pointer, "/operations/op/input/enum");
        let lookup_calls = lookup.calls.load(Ordering::Relaxed);
        // Interrupted lookup reports nothing rather than an unchecked location.
        for limit in 0..lookup_calls {
            let control = StopAfter {
                calls: AtomicUsize::new(0),
                limit,
            };
            assert!(
                projection.locate(Some(&program), &uri, &control).is_err(),
                "{limit}"
            );
        }
        let validator = jsonschema::validator_for(&serde_json::json!({"enum":[0,1,2]})).unwrap();
        let value = serde_json::Value::Null;
        let error = validator.validate(&value).unwrap_err();
        for limit in [0, 1] {
            COPIED_DETAIL_BYTES.set(0);
            COPIED_ENUM_ITEMS.set(0);
            let control = StopAfter {
                calls: AtomicUsize::new(0),
                limit,
            };
            assert!(collect(located.operand, error.kind(), &control).is_none());
            assert_eq!(COPIED_DETAIL_BYTES.get(), 0);
            assert_eq!(COPIED_ENUM_ITEMS.get(), 0);
            assert_eq!(control.calls.load(Ordering::Relaxed), limit + 1);
        }
        assert!(matches!(collect(located.operand, error.kind(), &lookup),
            Some(ValueProblemDetails::Enum { choices }) if choices == ["0", "1", "2"]));
    }

    #[test]
    fn nonexistent_locations_use_the_nearest_existing_pointer_prefix() {
        // A literal is walked in the arena its compiled keyword keeps.
        let data = r#"{"object":{"a/~雪":[null],"0":false},"":true}"#;
        let (program, projection) = projected(&format!(r#"{{"const":{data}}}"#));
        let source = JsonValue::parse(data).unwrap();
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
            let uri = generated(
                &program,
                "/operations/op/input",
                &format!("/const{reported}"),
            );
            let located = projection
                .locate(Some(&program), &uri, &control)
                .unwrap()
                .unwrap();
            assert_eq!(
                located.location.pointer,
                format!("/operations/op/input/const{expected}"),
                "{reported}"
            );
            assert_eq!(
                located.operand.unwrap().text(),
                source.at(expected).unwrap().text()
            );
        }
        let uri = generated(&program, "/operations/op/input", "/const/object/absent");
        control.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(projection.locate(Some(&program), &uri, &control).is_err());
        control.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(matches!(
            jsonschema::ob_work::bounded(1, 10, || projection.locate(
                Some(&program),
                &uri,
                &control
            )),
            Err(jsonschema::ob_work::Stop::Work)
        ));
    }

    #[test]
    fn child_positions_continue_in_the_original_subschema() {
        // A projection writes each subschema position as a reference to the
        // child's own definition. Paths below that position read the child,
        // never the reference written in its place.
        let (program, projection) = projected(
            r##"{"properties":{"a":{"type":"string"},"b":{"$ref":"#/operations/op/input/properties/a"}},"allOf":[{"minimum":3}],"not":false}"##,
        );
        let control = std::sync::atomic::AtomicBool::new(false);
        for (suffix, expected) in [
            ("/properties/a/type", "/properties/a/type"),
            ("/properties/a/$ref", "/properties/a"),
            ("/properties/b/$ref", "/properties/b/$ref"),
            ("/properties/b/$ref/x", "/properties/b/$ref"),
            ("/properties/c", "/properties"),
            ("/allOf/0/minimum", "/allOf/0/minimum"),
            ("/allOf/1", "/allOf"),
            ("/not/type", "/not"),
        ] {
            let uri = generated(&program, "/operations/op/input", suffix);
            let located = projection
                .locate(Some(&program), &uri, &control)
                .unwrap()
                .unwrap();
            assert_eq!(
                located.location.pointer,
                format!("/operations/op/input{expected}"),
                "{suffix}"
            );
        }
        let uri = generated(&program, "/operations/op/input", "/allOf/0/minimum");
        let located = projection
            .locate(Some(&program), &uri, &control)
            .unwrap()
            .unwrap();
        assert_eq!(located.operand.unwrap().number_text(), Some("3"));
    }

    #[test]
    fn every_resource_keeps_its_own_definitions() {
        // Twelve embedded resources: generated URIs whose keys sort apart from
        // their numeric order (`r1`, `r10`, `r11`, `r2`, ...).
        let items: Vec<String> = (0..12)
            .map(|i| format!(r#"{{"$id":"https://many.test/c{i}","minimum":{i}}}"#))
            .collect();
        let (program, projection) = projected(&format!(r#"{{"allOf":[{}]}}"#, items.join(",")));
        let generated_resources: std::collections::BTreeSet<_> = program
            .locations
            .keys()
            .map(|key| key.split_once('#').unwrap().0)
            .collect();
        assert_eq!(generated_resources.len(), 13);
        let control = std::sync::atomic::AtomicBool::new(false);
        for i in 0..12 {
            let pointer = format!("/operations/op/input/allOf/{i}");
            let uri = generated(&program, &pointer, "/minimum");
            let located = projection
                .locate(Some(&program), &uri, &control)
                .unwrap()
                .unwrap();
            assert_eq!(located.location.resource, None);
            assert_eq!(located.location.pointer, format!("{pointer}/minimum"));
            assert_eq!(
                located.operand.unwrap().number_text(),
                Some(i.to_string().as_str())
            );
        }
    }

    #[test]
    fn original_locations_keep_quotes_backslashes_and_control_characters() {
        let (program, projection) = projected(r#"{"properties":{"q\"\\\n\u0001":{"minimum":3}}}"#);
        let pointer = "/operations/op/input/properties/q\"\\\n\u{1}";
        let uri = generated(&program, pointer, "/minimum");
        let control = std::sync::atomic::AtomicBool::new(false);
        let located = projection
            .locate(Some(&program), &uri, &control)
            .unwrap()
            .unwrap();
        assert_eq!(located.location.pointer, format!("{pointer}/minimum"));
        assert_eq!(located.operand.unwrap().number_text(), Some("3"));
        assert!(projection.text.contains(r#"q\"\\\u000a\u0001"#));
    }

    #[test]
    fn copied_keyword_values_keep_their_structure_and_annotations_are_not_kept() {
        let (program, projection) = projected(
            r#"{"dependentRequired":{"a/~b":["x","y"],"0":[]},"description":"not compiled","x-data":{"k":1}}"#,
        );
        let control = std::sync::atomic::AtomicBool::new(false);
        for (suffix, expected) in [
            ("/dependentRequired/a~1~0b/1", "/dependentRequired/a~1~0b/1"),
            ("/dependentRequired/a~1~0b/2", "/dependentRequired/a~1~0b"),
            ("/dependentRequired/0/0", "/dependentRequired/0"),
            // Percent-encoded generated paths are decoded first.
            (
                "/dependentRequired/a~1~0%62/0",
                "/dependentRequired/a~1~0b/0",
            ),
            // No library error can name a keyword the validator never compiled.
            ("/description", ""),
            ("/x-data/k", ""),
        ] {
            let uri = generated(&program, "/operations/op/input", suffix);
            let located = projection
                .locate(Some(&program), &uri, &control)
                .unwrap()
                .unwrap();
            assert_eq!(
                located.location.pointer,
                format!("/operations/op/input{expected}"),
                "{suffix}"
            );
        }
        assert!(!projection.text.contains("not compiled"));
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
            // Projections are 2020-12, where the library names the same kind of path.
            if draft == jsonschema::Draft::Draft202012 {
                let (program, projection) = projected(r#"{"dependentRequired":{"a":["b"]}}"#);
                let uri = generated(&program, "/operations/op/input", pointer);
                let located = projection
                    .locate(Some(&program), &uri, &control)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    located.location.pointer,
                    format!("/operations/op/input/{keyword}")
                );
            }
        }
    }

    #[test]
    fn cheap_checks_never_copy_source_facts() {
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
        // The same contract copies them when asked to explain.
        let ValueOutcome::Fails { problems, .. } = contract.explain(&JsonValue::null()) else {
            panic!()
        };
        assert!(
            matches!(&problems[0].details, Some(ValueProblemDetails::Enum { choices }) if choices.len() == 2)
        );
        assert_eq!(COPIED_ENUM_ITEMS.get(), 2);
    }
}
