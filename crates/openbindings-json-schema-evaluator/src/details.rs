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
//! projected arenas that compiled literals keep anyway. The copy also holds the
//! program location map's entries, including the exact and synthetic ones of
//! paired oneOf bounds, and replaces that map; a map it cannot carry stays with
//! the contract. Lookup slots, written with the copy, give every offset a lookup
//! reads, so no explanation parses or indexes text first.
use jsonschema::error::{TypeKind, ValidationErrorKind as Kind};
use openbindings::*;
use openbindings_internal_json::{backend, text::TextValue};
use openbindings_internal_schema::{
    ChildShape, Program, ProgramOrigin, ProjectedResource, SchemaSpace, child_shape,
};

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
    // `[<common resource URI prefix>,{<resource key>:[<standard>,<source>,{<definition>:[<origin>,<schema>(,<keyword origins>)]}]}]`.
    //
    // Each schema holds only compiled keywords. A child position holds
    // `"<resource key>/<definition name>"`. In a resource whose every origin is
    // a prefix origin in its source, the children of `properties`,
    // `patternProperties` and `dependentSchemas` are a list: each child's own
    // pointer ends with its name, so the name is not kept twice. `const` and `enum` hold
    // `[<literal arena>,<arena position>]`; a `required` array, or a
    // `dependentRequired` list, whose names the library reports exactly holds
    // `true`. `<source>` is the resource's original source (`null` for the
    // document) and `<standard>` says whether it is a packaged meta-schema.
    //
    // An origin is the program map's entry for a generated URI: `"<pointer>"` is
    // a prefix origin in the resource's source, `null` a synthetic one, and
    // `[<exact>,"<pointer>"(,<source>,<standard>)]` an exact origin or one in
    // another source. `<keyword origins>` lists `["<keyword path>",<origin>]` for
    // the map's entries inside a definition, as paired oneOf bounds write. When
    // the map holds anything else, every origin is `null` and the contract keeps
    // the program's own map.
    text: Box<str>,
    // Resource slots sorted by key, then each resource's definition slots in
    // name order, a closing definition slot, and every definition's keyword
    // slots: every offset a lookup reads, so it parses no text to find one.
    slots: Box<[Slot]>,
    resources: u32,
    // Whether each resource's definitions are in name order, as the map writes
    // them; the projected order of an unlocated text need not be.
    ordered: bool,
    // Byte length of the shared resource URI prefix, which starts at offset 2.
    prefix: u32,
    // Projected resources whose arenas compiled literals keep anyway.
    literals: Vec<JsonValue>,
    // Whether the text holds every origin, so the program's map is not needed.
    locates: bool,
    // The entry definition's slot, or `NONE`: the selected schema, whose own
    // origin names it when a keyword cannot be mapped.
    entry: u32,
}
// A resource, definition or keyword: its name, between its quotes in the text,
// then for a resource the range of its definition slots; for a definition its
// schema's offset and its first keyword slot (its last is before the next
// definition's first); for a keyword its value's offset. A keyword slot with
// a `NONE` name holds the offset of its definition's keyword origins. The value
// after a resource's name starts two bytes past its end, and a definition's
// origin three.
#[derive(Clone, Copy, Default)]
struct Slot {
    name: u32,
    len: u32,
    first: u32,
    last: u32,
}
const NONE: u32 = u32::MAX;
// A definition found by name, with its resource.
#[derive(Clone, Copy)]
struct Definition {
    resource: Slot,
    index: u32,
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
    // A definition's schema, whose members are compiled keywords.
    Schema(u32),
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
        source: Option<Token>,
        pointer: Token,
        standard: bool,
    },
    Map(&'a openbindings_internal_schema::Location),
}
// A string token of the text by its byte range, so copying it scans no text.
#[derive(Clone, Copy)]
struct Token {
    start: u32,
    end: u32,
}
impl Token {
    fn of(value: TextValue<'_>) -> Self {
        let start = value.start();
        Self {
            start: offset(start),
            end: offset(start + value.text().len()),
        }
    }
}

// How the compact text keeps one compiled keyword.
#[derive(Clone, Copy)]
enum Kept {
    Children(ChildShape),
    // `const` and `enum`, kept in their literal arenas.
    Literal,
    // Reference targets, patterns and dynamic anchors: strings no explanation
    // reads.
    Placeholder,
    // `required` names, and `dependentRequired` lists of names.
    Names,
    NameLists,
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
        "$ref" | "$dynamicRef" | "pattern" | "$dynamicAnchor" => Kept::Placeholder,
        "required" => Kept::Names,
        "dependentRequired" => Kept::NameLists,
        "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "multipleOf"
        | "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties"
        | "maxProperties" | "minContains" | "maxContains" | "type" | "uniqueItems" => Kept::Copied,
        _ => return None,
    })
}
// Whether a `dependentRequired` map names a list with a key spelled as an
// array index. The library reports a failed list at the keyword followed by an
// index, so such a key, which the location walk then reaches, must keep its
// list for the membership check main makes there.
fn index_named(lists: JsonRef<'_>) -> bool {
    lists.members().is_some_and(|mut lists| {
        lists.any(|list| {
            list.name.as_str().is_some_and(|name| {
                !name.is_empty()
                    && name.bytes().all(|b| b.is_ascii_digit())
                    && (name == "0" || !name.starts_with('0'))
            })
        })
    })
}
fn standard(uri: &str) -> bool {
    openbindings_internal_schema::embedded_resources()
        .iter()
        .any(|resource| resource.uri == uri)
}
// Where the writer puts the text: a length count, then its exact buffer.
trait Sink {
    fn put(&mut self, text: &str);
    fn put_char(&mut self, c: char);
    // ASCII bytes, such as digits.
    fn put_ascii(&mut self, bytes: &[u8]);
    fn len(&self) -> usize;
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
    fn len(&self) -> usize {
        self.0
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
    fn len(&self) -> usize {
        String::len(self)
    }
}
// Where the text takes each definition's origin from: the program's location
// map, or the space it was projected from, with each projected resource's
// definition nodes and their original pointers, built once for both passes:
// definition `k`'s pointer ends at `ends[k]` in `pointers`.
enum Origins<'a> {
    Map(&'a std::collections::BTreeMap<String, ProgramOrigin>),
    Space {
        space: &'a SchemaSpace,
        definitions: &'a [Vec<usize>],
        pointers: Vec<u8>,
        ends: Vec<usize>,
    },
}
// Slots the filling pass writes: resources from 0, definitions from
// `definitions`, keywords from `keywords`.
struct Slots<'r> {
    slots: &'r mut [Slot],
    definitions: usize,
    keywords: usize,
}
// Compact text writer, run once to measure and once to fill an exact buffer and
// its slots.
struct Writer<'r, W> {
    out: W,
    prefix: usize,
    // Literal arenas named so far; kept only when filling.
    named: usize,
    literals: Option<&'r mut Vec<JsonValue>>,
    // Resources, definitions and keywords written so far.
    resources: usize,
    definitions: usize,
    keywords: usize,
    slots: Option<Slots<'r>>,
    // Whether child maps are written as lists, their names left to each
    // child's pointer.
    unnamed: bool,
}
// An offset of the text being filled. [`Explanation::new`] keeps no text whose
// offsets would not fit.
fn offset(at: usize) -> u32 {
    u32::try_from(at).expect("measured text fits 32-bit offsets")
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
    // A generated name between quotes: where it starts and its length.
    fn name(&mut self, name: &str) -> (usize, usize) {
        let start = self.out.len() + 1;
        self.quoted(name);
        (start, name.len())
    }
    fn resource_slot(&mut self, name: (usize, usize), first_definition: usize) {
        if let Some(Slots {
            slots, definitions, ..
        }) = &mut self.slots
        {
            slots[self.resources] = Slot {
                name: offset(name.0),
                len: offset(name.1),
                first: offset(*definitions + first_definition),
                last: offset(*definitions + self.definitions),
            };
        }
        self.resources += 1;
    }
    fn definition_slot(&mut self, name: (usize, usize), schema: usize, first_keyword: usize) {
        if let Some(Slots {
            slots,
            definitions,
            keywords,
        }) = &mut self.slots
        {
            slots[*definitions + self.definitions] = Slot {
                name: offset(name.0),
                len: offset(name.1),
                first: offset(schema),
                last: offset(*keywords + first_keyword),
            };
        }
        self.definitions += 1;
    }
    // A keyword's name and value offsets, or with no name the offset of its
    // definition's keyword origins.
    fn keyword_slot(&mut self, name: Option<(usize, usize)>, value: usize) {
        if let Some(Slots {
            slots, keywords, ..
        }) = &mut self.slots
        {
            let (name, len) = name.map_or((NONE, 0), |(name, len)| (offset(name), offset(len)));
            slots[*keywords + self.keywords] = Slot {
                name,
                len,
                first: offset(value),
                last: 0,
            };
        }
        self.keywords += 1;
    }
    fn begin(&mut self, resources: &[ProjectedResource]) {
        let prefix = &resources.first().map_or("", |r| r.uri.as_str())[..self.prefix];
        self.put_char('[');
        self.quoted(prefix);
        self.put(",{");
    }
    // A program map entry, relative to the resource's original source. `false`,
    // writing nothing, for a pointer through a name with an unpaired unit, which
    // this text does not spell: the program's own map keeps it exactly.
    fn origin(&mut self, origin: &ProgramOrigin, source: Option<&str>) -> bool {
        let (exact, at) = match origin {
            ProgramOrigin::Synthetic => {
                self.put("null");
                return true;
            }
            ProgramOrigin::Prefix(at) | ProgramOrigin::Exact(at)
                if at.pointer.as_str().is_none() =>
            {
                return false;
            }
            ProgramOrigin::Prefix(at) if at.resource.as_deref() == source => {
                self.escaped(at.pointer.as_str().expect("text pointer"));
                return true;
            }
            ProgramOrigin::Prefix(at) => (false, at),
            ProgramOrigin::Exact(at) => (true, at),
        };
        self.put(if exact { "[true," } else { "[false," });
        self.escaped(at.pointer.as_str().expect("text pointer"));
        let resource = at.resource.as_deref();
        if resource != source {
            match resource {
                Some(uri) => {
                    self.put_char(',');
                    self.escaped(uri);
                    self.put(if standard(uri) { ",true" } else { ",false" });
                }
                None => self.put(",null,false"),
            }
        }
        self.put_char(']');
        true
    }
    // Write each resource's definitions with the program map's entries for
    // them. `Ok(false)` when the map holds an entry this text cannot carry.
    fn located(
        &mut self,
        resources: &[ProjectedResource],
        locations: &std::collections::BTreeMap<String, ProgramOrigin>,
        control: &dyn WorkControl,
    ) -> Result<bool, NoVerdict> {
        use std::ops::Bound;
        self.begin(resources);
        let mut written = 0usize;
        for resource in resources {
            let uri = resource.uri.as_str();
            // A resource's entries are `<uri>` and `<uri>#<fragment>`. Resource
            // URIs hold no `#`, which sorts below every character another URI
            // could add, so its entries are adjacent and start the map's range
            // from `<uri>`.
            let entries = || {
                locations
                    .range::<str, _>((Bound::Included(uri), Bound::Unbounded))
                    .map_while(move |(key, origin)| match key.strip_prefix(uri)? {
                        "" => Some((None, origin)),
                        rest => Some((Some(rest.strip_prefix('#')?), origin)),
                    })
            };
            let mut keys = entries().peekable();
            // A URI with no fragment names no library error; its entry, which
            // sorts first, can only be synthetic glue.
            if let Some(&(None, origin)) = keys.peek() {
                if !matches!(origin, ProgramOrigin::Synthetic) {
                    return Ok(false);
                }
                keys.next();
                written += 1;
            }
            // A resource no other entry maps into holds nothing an error can
            // name, such as the constant resource of a partial bound.
            if keys.peek().is_none() {
                continue;
            }
            let Some(definitions) = resource.document.get("$defs") else {
                return Ok(false);
            };
            let source = entries()
                .find_map(|(_, origin)| match origin {
                    ProgramOrigin::Prefix(at) | ProgramOrigin::Exact(at) => Some(at),
                    ProgramOrigin::Synthetic => None,
                })
                .and_then(|at| at.resource.as_deref());
            // A projection places each child at its parent's pointer, keyword and
            // name, so a child's own prefix origin names it.
            self.unnamed = entries().all(|(fragment, origin)| {
                fragment
                    .and_then(|fragment| fragment.strip_prefix("/$defs/"))
                    .is_some_and(|name| !name.contains('/'))
                    && matches!(origin, ProgramOrigin::Prefix(at) if at.resource.as_deref() == source)
            });
            if self.resources != 0 {
                self.put_char(',');
            }
            let key = self.name(&uri[self.prefix..]);
            self.put(":[");
            self.put(if source.is_some_and(standard) {
                "true,"
            } else {
                "false,"
            });
            match source {
                Some(source) => self.escaped(source),
                None => self.put("null"),
            }
            self.put(",{");
            let first_definition = self.definitions;
            // Projections write definitions in the map's order.
            let mut next = 0;
            let mut arena = None;
            while let Some((fragment, origin)) = keys.next() {
                control.check()?;
                // A definition's own entry precedes the entries inside it.
                let Some(name) = fragment
                    .and_then(|fragment| fragment.strip_prefix("/$defs/"))
                    .filter(|name| !name.contains('/'))
                else {
                    return Ok(false);
                };
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
                if self.definitions != first_definition {
                    self.put_char(',');
                }
                let at = self.name(name);
                self.put(":[");
                if !self.origin(origin, source) {
                    return Ok(false);
                }
                self.put_char(',');
                let schema_at = self.out.len();
                let first_keyword = self.keywords;
                self.schema(resource, schema, &mut arena);
                written += 1;
                let mut keywords = None;
                while let Some(&(Some(fragment), origin)) = keys.peek() {
                    let Some(path) = fragment
                        .strip_prefix("/$defs/")
                        .and_then(|path| path.strip_prefix(name))
                        .and_then(|path| path.strip_prefix('/'))
                    else {
                        break;
                    };
                    keys.next();
                    control.check()?;
                    if keywords.is_none() {
                        self.put_char(',');
                        keywords = Some(self.out.len());
                        self.put_char('[');
                    } else {
                        self.put_char(',');
                    }
                    self.put_char('[');
                    self.escaped(path);
                    self.put_char(',');
                    if !self.origin(origin, source) {
                        return Ok(false);
                    }
                    self.put_char(']');
                    written += 1;
                }
                if let Some(keywords) = keywords {
                    self.put_char(']');
                    self.keyword_slot(None, keywords);
                }
                self.put_char(']');
                self.definition_slot(at, schema_at, first_keyword);
            }
            self.put("}]");
            self.resource_slot(key, first_definition);
        }
        self.put("}]");
        // Every entry belongs to one program resource.
        Ok(written == locations.len())
    }
    // Write every definition with its origin; `Ok(false)` when the program's
    // map holds an entry this text cannot carry.
    fn write(
        &mut self,
        resources: &[ProjectedResource],
        origins: &mut Origins<'_>,
        control: &dyn WorkControl,
    ) -> Result<bool, NoVerdict> {
        match origins {
            Origins::Map(locations) => self.located(resources, locations, control),
            Origins::Space {
                space,
                definitions,
                pointers,
                ends,
            } => {
                self.projected(resources, space, definitions, pointers, ends, control)?;
                Ok(true)
            }
        }
    }
    // Every projected definition with its node's origin in the space it was
    // projected from: a prefix origin in its resource's source, which every
    // definition of one projected resource shares.
    fn projected(
        &mut self,
        resources: &[ProjectedResource],
        space: &SchemaSpace,
        definitions: &[Vec<usize>],
        pointers: &[u8],
        ends: &[usize],
        control: &dyn WorkControl,
    ) -> Result<(), NoVerdict> {
        self.begin(resources);
        // A projection places each child at its parent's pointer, keyword and
        // name, so a child's own origin names it.
        self.unnamed = true;
        let mut next = 0;
        for (resource, nodes) in resources.iter().zip(definitions) {
            let first_node = next;
            next += nodes.len();
            let (Some(&first), Some(members)) = (
                nodes.first(),
                resource.document.get("$defs").and_then(|d| d.members()),
            ) else {
                continue;
            };
            let source = space.source_uri(first);
            if self.resources != 0 {
                self.put_char(',');
            }
            let key = self.name(&resource.uri[self.prefix..]);
            self.put(":[");
            self.put(if source.is_some_and(standard) {
                "true,"
            } else {
                "false,"
            });
            match source {
                Some(source) => self.escaped(source),
                None => self.put("null"),
            }
            self.put(",{");
            let first_definition = self.definitions;
            let mut arena = None;
            for (k, (definition, &node)) in members.zip(nodes).enumerate() {
                control.check()?;
                let Some(name) = definition.name.as_str() else {
                    continue;
                };
                debug_assert_eq!(space.source_uri(node), source);
                let k = first_node + k;
                let pointer = &pointers[k.checked_sub(1).map_or(0, |k| ends[k])..ends[k]];
                if self.definitions != first_definition {
                    self.put_char(',');
                }
                let at = self.name(name);
                self.put(":[");
                self.escaped(std::str::from_utf8(pointer).expect("pointers are UTF-8"));
                self.put_char(',');
                let schema_at = self.out.len();
                let first_keyword = self.keywords;
                self.schema(resource, definition.value, &mut arena);
                self.put_char(']');
                self.definition_slot(at, schema_at, first_keyword);
            }
            self.put("}]");
            self.resource_slot(key, first_definition);
        }
        self.put("}]");
        Ok(())
    }
    // Every projected definition without origins, in projected order.
    fn unlocated(
        &mut self,
        resources: &[ProjectedResource],
        control: &dyn WorkControl,
    ) -> Result<(), NoVerdict> {
        self.begin(resources);
        self.unnamed = false;
        for resource in resources {
            if self.resources != 0 {
                self.put_char(',');
            }
            let key = self.name(&resource.uri[self.prefix..]);
            self.put(":[false,null,{");
            let first_definition = self.definitions;
            let mut arena = None;
            let definitions = resource.document.get("$defs").and_then(|d| d.members());
            for definition in definitions.into_iter().flatten() {
                control.check()?;
                let Some(name) = definition.name.as_str() else {
                    continue;
                };
                if self.definitions != first_definition {
                    self.put_char(',');
                }
                let at = self.name(name);
                self.put(":[null,");
                let schema_at = self.out.len();
                let first_keyword = self.keywords;
                self.schema(resource, definition.value, &mut arena);
                self.put_char(']');
                self.definition_slot(at, schema_at, first_keyword);
            }
            self.put("}]");
            self.resource_slot(key, first_definition);
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
            let keyword = field.name.text();
            let name = self.out.len() + 1;
            self.put(keyword);
            self.put_char(':');
            // Projections write keyword names unescaped.
            let len = keyword.len().saturating_sub(2);
            self.keyword_slot(Some((name, len)), self.out.len());
            match kept {
                Kept::Children(ChildShape::Map) if self.unnamed => {
                    self.put_char('[');
                    for (i, child) in field.value.members().into_iter().flatten().enumerate() {
                        if i != 0 {
                            self.put_char(',');
                        }
                        self.child(child.value);
                    }
                    self.put_char(']');
                }
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
                // The library reports the missing member exactly as the
                // original spells it, so a list keeps only that it exists.
                Kept::Names => self.put("true"),
                Kept::NameLists => match field.value.members() {
                    Some(lists) if !index_named(field.value) => {
                        self.put_char('{');
                        for (i, list) in lists.enumerate() {
                            if i != 0 {
                                self.put_char(',');
                            }
                            self.put(list.name.text());
                            self.put(":true");
                        }
                        self.put_char('}');
                    }
                    _ => self.put(field.value.text()),
                },
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
// Whether a generated suffix (`/a/b`) continues with a keyword path's segments
// (`a`), ending there or at a `/`.
fn continues_with(suffix: &str, path: TextValue<'_>) -> Option<usize> {
    let path = path.string()?;
    let rest = suffix.strip_prefix('/')?;
    let matched = match path.as_unescaped() {
        Some(path) => rest.starts_with(path).then_some(path.len()),
        None => {
            let mut chars = rest.chars();
            let mut len = 0;
            path.chars()
                .all(|c| {
                    len += c.len_utf8();
                    chars.next() == Some(c)
                })
                .then_some(len)
        }
    }?;
    matches!(rest.as_bytes().get(matched), None | Some(b'/')).then_some(1 + matched)
}

impl Explanation {
    /// Copy what explanations read from a compiled program's projected
    /// resources, before they are released, with the program map's entries when
    /// the text can carry them all. The text is measured first, so it and its
    /// lookup slots take one exact allocation each. `None` when the text would
    /// pass 4 GiB: explanations then report no problems.
    pub(crate) fn new(
        program: &Program,
        control: &dyn WorkControl,
    ) -> Result<Option<Self>, NoVerdict> {
        Self::build(program, Origins::Map(&program.locations), control)
    }
    /// The same text for a program projected without a location map, its
    /// origins written from `space`: `definitions` holds each projected
    /// resource's definition nodes in `$defs` order. When an origin passes
    /// through a name holding unpaired units, which the text cannot carry, the
    /// program takes the map [`SchemaSpace::program`] builds and the
    /// explanation is [`Self::new`]'s, reading that map.
    pub(crate) fn projected(
        program: &mut Program,
        space: &SchemaSpace,
        definitions: &[Vec<usize>],
        control: &dyn WorkControl,
    ) -> Result<Option<Self>, NoVerdict> {
        // Each pointer is built once, for both passes.
        let mut ends = Vec::with_capacity(definitions.iter().map(Vec::len).sum());
        let (mut pointers, mut scratch) = (Vec::new(), Vec::new());
        for &node in definitions.iter().flatten() {
            control.check()?;
            if !space.location_into(node, &mut pointers, &mut scratch) {
                return Self::mapped(program, space, definitions, control);
            }
            ends.push(pointers.len());
        }
        let origins = Origins::Space {
            space,
            definitions,
            pointers,
            ends,
        };
        Self::build(program, origins, control)
    }
    #[cold]
    #[inline(never)]
    fn mapped(
        program: &mut Program,
        space: &SchemaSpace,
        definitions: &[Vec<usize>],
        control: &dyn WorkControl,
    ) -> Result<Option<Self>, NoVerdict> {
        program.locations = space.definition_locations(program, definitions);
        Self::new(program, control)
    }
    fn build(
        program: &Program,
        mut origins: Origins<'_>,
        control: &dyn WorkControl,
    ) -> Result<Option<Self>, NoVerdict> {
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
            resources: 0,
            definitions: 0,
            keywords: 0,
            slots: None,
            unnamed: false,
        };
        let mut count = writer(Count(0));
        let locates = count.write(resources, &mut origins, control)?;
        if !locates {
            count = writer(Count(0));
            count.unlocated(resources, control)?;
        }
        let len = count.out.0;
        if u32::try_from(len).is_err() {
            return Ok(None);
        }
        let (resource_count, definition_count, keyword_count) =
            (count.resources, count.definitions, count.keywords);
        // Definitions close with a slot whose first keyword ends the last one's.
        let keywords = resource_count + definition_count + 1;
        let mut slots = vec![Slot::default(); keywords + keyword_count].into_boxed_slice();
        slots[keywords - 1].last = offset(keywords + keyword_count);
        let mut literals = Vec::with_capacity(count.named);
        let mut fill = Writer {
            out: String::with_capacity(len),
            prefix,
            named: 0,
            literals: Some(&mut literals),
            resources: 0,
            definitions: 0,
            keywords: 0,
            slots: Some(Slots {
                slots: &mut slots,
                definitions: resource_count,
                keywords,
            }),
            unnamed: false,
        };
        if locates {
            fill.write(resources, &mut origins, control)?;
        } else {
            fill.unlocated(resources, control)?;
        }
        debug_assert_eq!(
            (
                fill.out.len(),
                fill.resources,
                fill.definitions,
                fill.keywords
            ),
            (len, resource_count, definition_count, keyword_count)
        );
        let text = fill.out.into_boxed_str();
        let name = |slot: &Slot| &text[slot.name as usize..(slot.name + slot.len) as usize];
        let (resource_slots, rest) = slots.split_at_mut(resource_count);
        let ordered = resource_slots.iter().all(|resource| {
            rest[resource.first as usize - resource_count..resource.last as usize - resource_count]
                .windows(2)
                .all(|pair| name(&pair[0]) < name(&pair[1]))
        });
        resource_slots.sort_unstable_by(|a, b| name(a).cmp(name(b)));
        let mut explanation = Self {
            text,
            slots,
            resources: offset(resource_count),
            ordered,
            prefix: offset(prefix),
            literals,
            locates,
            entry: NONE,
        };
        // The entry resource's root refers to the entry definition.
        let entry = resources
            .iter()
            .find(|resource| resource.uri == program.entry_uri)
            .and_then(|resource| resource.document.get("$ref")?.as_str())
            .and_then(|reference| {
                let (base, fragment) = reference.split_once('#')?;
                let name = fragment.strip_prefix("/$defs/")?;
                explanation.find(explanation.key(base)?, name)
            });
        if let Some(entry) = entry {
            explanation.entry = entry.index;
        }
        Ok(Some(explanation))
    }
    /// Whether explanations need the program's own location map.
    pub(crate) fn needs_locations(&self) -> bool {
        !self.locates
    }
    fn name(&self, slot: Slot) -> &str {
        &self.text[slot.name as usize..(slot.name + slot.len) as usize]
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
    ) -> Result<Option<Definition>, ()> {
        if !work(control, key.len().saturating_add(name.len())) {
            return Err(());
        }
        Ok(self.find(key, name))
    }
    fn find(&self, key: &str, name: &str) -> Option<Definition> {
        let resources = &self.slots[..self.resources as usize];
        let found = resources
            .binary_search_by(|slot| self.name(*slot).cmp(key))
            .ok()?;
        let resource = resources[found];
        let definitions = &self.slots[resource.first as usize..resource.last as usize];
        let found = if self.ordered {
            definitions
                .binary_search_by(|slot| self.name(*slot).cmp(name))
                .ok()
        } else {
            definitions.iter().position(|slot| self.name(*slot) == name)
        };
        found.map(|found| Definition {
            resource,
            index: resource.first + found as u32,
        })
    }
    // A resource's original source (`None` for the document) and whether it is a
    // packaged meta-schema, at fixed offsets: its value starts `[true,` or
    // `[false,` right after its name.
    fn resource_source(&self, resource: Slot) -> (Option<Token>, bool) {
        let flag = resource.name + resource.len + 3;
        let standard = self.text.as_bytes()[flag as usize] == b't';
        let start = flag + if standard { 5 } else { 6 };
        if self.text.as_bytes()[start as usize] != b'"' {
            return (None, standard);
        }
        // A source is followed by `,{"` and the name of the resource's first
        // definition.
        let token = match resource.first < resource.last {
            true => Token {
                start,
                end: self.slots[resource.first as usize].name - 3,
            },
            false => Token::of(self.at(start)),
        };
        (Some(token), standard)
    }
    // A new owner of a string token's characters with room for `extra` bytes;
    // one without escapes is copied as it is.
    fn owned(&self, token: Token, extra: usize) -> Option<String> {
        debug_assert_eq!(token.end, Token::of(self.at(token.start)).end);
        let raw = &self.text[token.start as usize..token.end as usize];
        match raw.strip_prefix('"').and_then(|raw| raw.strip_suffix('"')) {
            Some(plain) if !plain.as_bytes().contains(&b'\\') => {
                let mut out = String::with_capacity(plain.len() + extra);
                out.push_str(plain);
                Some(out)
            }
            _ => owned(self.at(token.start), extra),
        }
    }
    fn schema(&self, definition: Definition) -> TextValue<'_> {
        self.at(self.slots[definition.index as usize].first)
    }
    // A definition's keyword slots, the last of which may hold its keyword origins.
    fn keywords(&self, index: u32) -> &[Slot] {
        let index = index as usize;
        &self.slots[self.slots[index].last as usize..self.slots[index + 1].last as usize]
    }
    // A compiled keyword of a definition, by pointer token.
    fn keyword<'a>(
        &'a self,
        index: u32,
        token: &str,
        control: &dyn WorkControl,
    ) -> Result<Option<(&'a str, TextValue<'a>)>, ()> {
        // A definition has at most one slot per compiled keyword, so the scan
        // is charged once, for every name it compared.
        let mut compared = 0usize;
        let mut found = None;
        for slot in self.keywords(index) {
            if slot.name == NONE {
                continue;
            }
            let name = self.name(*slot);
            compared = compared
                .saturating_add(name.len())
                .saturating_add(token.len());
            if segment_eq(name.bytes(), token) {
                found = Some((name, self.at(slot.first)));
                break;
            }
        }
        if !work(control, compared) {
            return Err(());
        }
        Ok(found)
    }
    // The resource key of a generated base URI.
    fn key<'a>(&self, base: &'a str) -> Option<&'a str> {
        base.strip_prefix(&self.text[2..2 + self.prefix as usize])
    }
    // The definition a child position refers to.
    fn follow(
        &self,
        child: TextValue<'_>,
        control: &dyn WorkControl,
    ) -> Result<Option<Definition>, ()> {
        let Some(reference) = child.string().and_then(|s| s.as_unescaped()) else {
            return Ok(None);
        };
        let Some((key, name)) = reference.rsplit_once('/') else {
            return Ok(None);
        };
        self.definition(key, name, control)
    }
    // The first child of a list of named children whose own pointer ends with
    // the name a pointer token selects.
    fn named_child<'a>(
        &'a self,
        children: TextValue<'a>,
        token: &str,
        control: &dyn WorkControl,
    ) -> Result<Option<TextValue<'a>>, ()> {
        for child in children.elements() {
            let Some(definition) = self.follow(child, control)? else {
                continue;
            };
            let slot = self.slots[definition.index as usize];
            // A prefix origin is the pointer itself; the array form holds it second.
            let origin = self.at(slot.name + slot.len + 3);
            let origin = match origin.kind() {
                JsonKind::Array => origin.elements().nth(1),
                _ => Some(origin),
            };
            let Some(pointer) = origin.and_then(TextValue::string) else {
                continue;
            };
            if !work(control, pointer.len().saturating_add(token.len())) {
                return Err(());
            }
            let pointer: std::borrow::Cow<'_, str> = match pointer.as_unescaped() {
                Some(pointer) => pointer.into(),
                None => pointer.chars().collect::<String>().into(),
            };
            let Some((_, segment)) = pointer.rsplit_once('/') else {
                continue;
            };
            // Decode the segment's `~1` and `~0` escapes as the token's are.
            let mut name = segment.bytes().peekable();
            let decoded = std::iter::from_fn(|| match name.next()? {
                b'~' => match name.next() {
                    Some(b'1') => Some(b'/'),
                    Some(b'0') => Some(b'~'),
                    // A canonical pointer escapes nothing else.
                    _ => Some(0),
                },
                byte => Some(byte),
            });
            if segment_eq(decoded, token) {
                return Ok(Some(child));
            }
        }
        Ok(None)
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
                Some(definition) => (
                    Node::Text(self.schema(definition)),
                    Mode::Schema(definition.index),
                ),
                None => return Ok(None),
            },
            other => other,
        };
        if let Mode::Schema(index) = mode {
            let Some((name, value)) = self.keyword(index, token, control)? else {
                return Ok(None);
            };
            return Ok(Some(match child_shape(name) {
                Some(ChildShape::Single) => (Node::Text(value), Mode::Child),
                Some(shape) => (Node::Text(value), Mode::Children(shape)),
                // `const` and `enum` values live in their literal arenas.
                None if matches!(name, "const" | "enum") => match self.literal(value) {
                    Some(literal) => (Node::Json(literal), Mode::Copied),
                    None => return Ok(None),
                },
                None => (Node::Text(value), Mode::Copied),
            }));
        }
        if let (Mode::Children(ChildShape::Map), Node::Text(children)) = (mode, node)
            && children.kind() == JsonKind::Array
        {
            return Ok(self
                .named_child(children, token, control)?
                .map(|child| (Node::Text(child), Mode::Child)));
        }
        match node.kind() {
            JsonKind::Object => {
                let Some((value, _)) = member(node, token, control)? else {
                    return Ok(None);
                };
                Ok(Some(match mode {
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
                let Some(index) = token.parse().ok() else {
                    return Ok(None);
                };
                let Some(value) = node.element(index, control)? else {
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
    /// The original location of a problem's failing keyword, from the
    /// library's keyword URI. A keyword the original sources do not hold is not
    /// guessed: the problem names the selected schema itself, with no facts,
    /// except in a partial program (`original_only`), whose generated errors
    /// are not original schema evidence and yield no problem. `Ok(None)` when
    /// nothing can be named; interrupted lookup is `Err`.
    // Out of line, so the diagnostic pass that calls it stays small enough for
    // the compiler to inline that pass's budget scopes.
    #[inline(never)]
    pub(crate) fn problem_schema(
        &self,
        program: Option<&Program>,
        uri: Option<&str>,
        original_only: bool,
        control: &dyn WorkControl,
    ) -> Result<Option<Located<'_>>, ()> {
        if let Some(uri) = uri
            && let Some(located) = self.locate(program, uri, control)?
        {
            return Ok(Some(located));
        }
        if original_only {
            return Ok(None);
        }
        Ok(self.entry(program, control)?.map(|entry| Located {
            location: entry.location,
            operand: None,
        }))
    }
    /// Map a library keyword location to its original location, keeping the
    /// nearest existing ancestor when the library names a path the source
    /// lacks. Interrupted lookup reports no location rather than an unchecked
    /// one. Packaged meta-schemas are original sources for location validity,
    /// but their facts are not reported. `program` is the compiled program when
    /// its own location map is kept.
    pub(crate) fn locate(
        &self,
        program: Option<&Program>,
        uri: &str,
        control: &dyn WorkControl,
    ) -> Result<Option<Located<'_>>, ()> {
        if self.locates {
            return self.located(uri, control);
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
        let Some(definition) = self.definition(resource, name, control)? else {
            return Ok(None);
        };
        self.walk(definition, rest, suffix, Original::Map(at), control)
    }
    // The text's own entry for a generated URI: the longest map entry that
    // prefixes it, as `Program::origin` finds one.
    fn located(&self, uri: &str, control: &dyn WorkControl) -> Result<Option<Located<'_>>, ()> {
        let Some((base, fragment)) = uri.split_once('#') else {
            return Ok(None);
        };
        // Fragments are compared decoded; the base keeps its escapes.
        let decoded;
        let fragment = if fragment.contains('%') {
            let Some(fragment) = openbindings_internal_schema::uri::decode_fragment(fragment)
            else {
                return Ok(None);
            };
            decoded = fragment;
            decoded.as_str()
        } else {
            fragment
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
        self.origin(definition, suffix, control)
    }
    /// The selected schema's own original location, which names it when a
    /// library keyword cannot be mapped. `program` is the compiled program when
    /// its own location map is kept.
    #[cold]
    #[inline(never)]
    pub(crate) fn entry(
        &self,
        program: Option<&Program>,
        control: &dyn WorkControl,
    ) -> Result<Option<Located<'_>>, ()> {
        if self.entry == NONE {
            return Ok(None);
        }
        let resource = self.slots[..self.resources as usize]
            .iter()
            .find(|resource| (resource.first..resource.last).contains(&self.entry));
        let Some(&resource) = resource else {
            return Ok(None);
        };
        if self.locates {
            let definition = Definition {
                resource,
                index: self.entry,
            };
            return self.origin(definition, "", control);
        }
        // The program's map holds the origin, under the definition's address.
        let Some(program) = program else {
            return Ok(None);
        };
        let address = format!(
            "{}{}#/$defs/{}",
            &self.text[2..2 + self.prefix as usize],
            self.name(resource),
            self.name(self.slots[self.entry as usize])
        );
        self.locate(Some(program), &address, control)
    }
    // A definition's origin, or that of the longest of its keyword origins the
    // generated suffix continues with, then the walk of what is left.
    fn origin(
        &self,
        definition: Definition,
        mut suffix: &str,
        control: &dyn WorkControl,
    ) -> Result<Option<Located<'_>>, ()> {
        let slot = self.slots[definition.index as usize];
        let mut origin = self.at(slot.name + slot.len + 3);
        // The definition's own origin ends at the comma before its schema.
        let mut origin_end = Some(slot.first - 1);
        let mut path = None;
        let keyword_origins = self
            .keywords(definition.index)
            .last()
            .filter(|keyword| keyword.name == NONE);
        if let Some(keyword_origins) = keyword_origins {
            let mut longest = 0;
            for entry in self.at(keyword_origins.first).elements() {
                let mut parts = entry.elements();
                let (Some(keyword), Some(entry_origin)) = (parts.next(), parts.next()) else {
                    return Ok(None);
                };
                if !work(control, keyword.text().len()) {
                    return Err(());
                }
                if let Some(matched) = continues_with(suffix, keyword).filter(|&m| m > longest) {
                    longest = matched;
                    path = Some(keyword);
                    origin = entry_origin;
                    origin_end = None;
                }
            }
            suffix = &suffix[longest..];
        }
        let (exact, pointer, source, standard) = match origin.kind() {
            JsonKind::String => {
                let (source, standard) = self.resource_source(definition.resource);
                let pointer = match origin_end {
                    Some(end) => Token {
                        start: offset(origin.start()),
                        end,
                    },
                    None => Token::of(origin),
                };
                (false, pointer, source, standard)
            }
            JsonKind::Array => {
                let mut parts = origin.elements();
                let exact = parts.next().is_some_and(|flag| flag.text() == "true");
                let Some(pointer) = parts.next().filter(|p| p.kind() == JsonKind::String) else {
                    return Ok(None);
                };
                let (source, standard) = match parts.next() {
                    Some(source) => (
                        Some(source)
                            .filter(|source| source.kind() == JsonKind::String)
                            .map(Token::of),
                        parts.next().is_some_and(|flag| flag.text() == "true"),
                    ),
                    None => self.resource_source(definition.resource),
                };
                (exact, Token::of(pointer), source, standard)
            }
            // A synthetic entry maps nothing.
            _ => return Ok(None),
        };
        // An exact entry maps only itself, not what lies below it.
        if exact && !suffix.is_empty() {
            return Ok(None);
        }
        let path = match path {
            None => std::borrow::Cow::Borrowed(""),
            Some(path) => match path.string() {
                Some(path) => match path.as_unescaped() {
                    Some(path) => std::borrow::Cow::Borrowed(path),
                    None => std::borrow::Cow::Owned(path.chars().collect()),
                },
                None => return Ok(None),
            },
        };
        let original = Original::Text {
            source,
            pointer,
            standard,
        };
        self.walk(definition, &path, suffix, original, control)
    }
    // Walk the keyword path inside a mapped key, which exists, then as much of
    // the generated suffix as exists.
    fn walk<'a>(
        &'a self,
        definition: Definition,
        key_rest: &str,
        suffix: &str,
        original: Original<'_>,
        control: &dyn WorkControl,
    ) -> Result<Option<Located<'a>>, ()> {
        // Every step charges the names it compares with its token, or what it
        // passes to reach an element.
        let mut node = Node::Text(self.schema(definition));
        let mut mode = Mode::Schema(definition.index);
        if !key_rest.is_empty() {
            for token in key_rest.split('/') {
                let Some(next) = self.step(node, mode, token, control)? else {
                    return Ok(None);
                };
                (node, mode) = next;
            }
        }
        let mut end = 0;
        if let Some(rest) = suffix.strip_prefix('/') {
            for token in rest.split('/') {
                let Some(next) = self.step(node, mode, token, control)? else {
                    break;
                };
                (node, mode) = next;
                end += 1 + token.len();
            }
        }
        let (resource, pointer, standard) = match original {
            Original::Map(at) => {
                let pointer = backend::joined_pointer(&at.pointer, &suffix[..end]);
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
                let mut pointer = self.owned(pointer, end).ok_or(())?;
                pointer.push_str(&suffix[..end]);
                (resource, backend::escaped_pointer(pointer), standard)
            }
        };
        Ok(Some(Located {
            location: SchemaLocation::new(resource, pointer),
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
    // An array element; reaching one in the text scans the elements before it,
    // which is charged.
    fn element(self, index: usize, control: &dyn WorkControl) -> Result<Option<Self>, ()> {
        match self {
            Self::Text(value) => {
                let Some(element) = value.elements().nth(index) else {
                    return Ok(None);
                };
                if !work(control, element.start() - value.start()) {
                    return Err(());
                }
                Ok(Some(Self::Text(element)))
            }
            Self::Json(value) => Ok(value.element(index).map(Self::Json)),
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
            // Passing a member scans its value: charge it when the next begins.
            let mut passed = None;
            for m in node.members() {
                if let Some(value_start) = passed.replace(m.value.start())
                    && !work(control, m.name.start() - value_start)
                {
                    return Err(());
                }
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
            let Kind::Required { property } = kind else {
                return None;
            };
            // Recover only the missing member established by the library.
            let member = property.as_str()?;
            let cost = "required".len().saturating_add(member.len());
            match operand.kind() {
                // Names the library reports exactly as the original spells them.
                JsonKind::Boolean => {
                    if !work(control, cost) {
                        return None;
                    }
                }
                JsonKind::Array => {
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
                }
                _ => return None,
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
        let mut program = project(schema);
        let control = std::sync::atomic::AtomicBool::new(false);
        let explanation = Explanation::new(&program, &control).unwrap().unwrap();
        // A strict projection maps one prefix origin per definition, so the
        // explanation holds every location itself.
        assert!(!explanation.needs_locations());
        program.resources = Vec::new();
        (program, explanation)
    }
    // The strict projection of an operation input schema.
    fn project(schema: &str) -> Program {
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
        space.program(entry, &|| false).unwrap()
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
    fn the_entry_names_the_selected_schema_from_the_text_or_the_programs_map() {
        let schema = r##"{"properties":{"a":{"$ref":"#/operations/op/input/$defs/s"}},"$defs":{"s":{"type":"string"}}}"##;
        let control = std::sync::atomic::AtomicBool::new(false);
        let (program, explanation) = projected(schema);
        let entry = explanation
            .entry(Some(&program), &control)
            .unwrap()
            .unwrap();
        assert_eq!(entry.location.resource, None);
        assert_eq!(entry.location.pointer, "/operations/op/input");
        // A map entry the text cannot carry keeps the program's map, which
        // then names the entry.
        let mut program = project(schema);
        let mut locations = (*program.locations).clone();
        let uri = program.entry_uri.clone();
        locations.insert(
            uri,
            ProgramOrigin::Prefix(openbindings_internal_schema::Location {
                resource: None,
                pointer: "".parse().unwrap(),
            }),
        );
        program.locations = Arc::new(locations);
        let unlocated = Explanation::new(&program, &control).unwrap().unwrap();
        assert!(unlocated.needs_locations());
        program.resources = Vec::new();
        let entry = unlocated.entry(Some(&program), &control).unwrap().unwrap();
        assert_eq!(entry.location.resource, None);
        assert_eq!(entry.location.pointer, "/operations/op/input");
        assert!(unlocated.entry(None, &control).unwrap().is_none());
        // Interrupted lookup reports nothing rather than an unchecked location.
        control.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(unlocated.entry(Some(&program), &control).is_err());
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
            r#"{"type":["object","null"],"dependentRequired":{"a/~b":["x","y"],"0":[]},"description":"not compiled","x-data":{"k":1}}"#,
        );
        let control = std::sync::atomic::AtomicBool::new(false);
        for (suffix, expected) in [
            ("/type/1", "/type/1"),
            ("/type/2", "/type"),
            ("/dependentRequired/c", "/dependentRequired"),
            // A key spelled as an index keeps every list whole.
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
        assert!(
            projection
                .text
                .contains(r#""dependentRequired":{"a/~b":["x","y"],"0":[]}"#)
        );
    }

    #[test]
    fn origins_from_the_space_write_the_text_the_map_writes() {
        for schema in [
            r#"{"type":"object","properties":{"a/b":{"minimum":1},"~c":{"items":{"type":"string"}}},"required":["a/b"]}"#,
            r##"{"allOf":[{"$ref":"#/operations/op/input/$defs/x"},{"$id":"https://embedded.test/e","oneOf":[{"const":1},{"enum":[2,3]}]}],"$defs":{"x":{"pattern":"^a","dependentRequired":{"a":["b"]}}}}"##,
            r#"{"properties":{"q\"\\\n":{"maximum":2}},"$dynamicAnchor":"n"}"#,
        ] {
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
            let control = std::sync::atomic::AtomicBool::new(false);
            let mapped = space.program(entry, &|| false).unwrap();
            let mut unmapped = space.program_unmapped(entry, &|| false).unwrap();
            let from_map = Explanation::new(&mapped, &control).unwrap().unwrap();
            let from_space = Explanation::projected(
                &mut unmapped.program,
                &space,
                &unmapped.definitions,
                &control,
            )
            .unwrap()
            .unwrap();
            assert_eq!(from_map.text, from_space.text, "{schema}");
            assert!(!from_space.needs_locations());
        }
    }

    #[test]
    fn name_lists_keep_names_only_where_the_location_walk_reads_them() {
        // The library reports names, the marker character included, exactly.
        let (_, projection) =
            projected(r#"{"required":["a",""],"dependentRequired":{"a":["b"],"01":["c"]}}"#);
        assert!(projection.text.contains(r#""required":true"#));
        assert!(
            projection
                .text
                .contains(r#""dependentRequired":{"a":true,"01":true}"#)
        );
        // A failed list is reported at the keyword and an index, so a key
        // spelled as an index keeps every list for the membership check there.
        let explain = |schema: &str, value: &str| {
            let document = Document::parse(format!(
                r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
            ))
            .unwrap();
            let document = document.validate().unwrap().conformant().unwrap();
            let contract = document
                .resolve_operation("op")
                .unwrap()
                .input()
                .unwrap()
                .prepare(&DefaultEvaluator::new())
                .unwrap();
            let ValueOutcome::Fails { problems, .. } =
                contract.explain(&JsonValue::parse(value).unwrap())
            else {
                panic!("expected a failure")
            };
            let problem = problems.into_iter().next().unwrap();
            (problem.schema.pointer, problem.details)
        };
        let (pointer, details) = explain(
            r#"{"dependentRequired":{"a":["0"],"0":["1"]}}"#,
            r#"{"a":1}"#,
        );
        assert_eq!(pointer, "/operations/op/input/dependentRequired/0");
        assert!(details.is_none());
        let (pointer, details) = explain(
            r#"{"dependentRequired":{"a":["0"],"0":["0"]}}"#,
            r#"{"a":1}"#,
        );
        assert_eq!(pointer, "/operations/op/input/dependentRequired/0");
        assert!(matches!(details, Some(ValueProblemDetails::Required { member }) if member == "0"));
        let (pointer, details) = explain(r#"{"dependentRequired":{"a":["b"]}}"#, r#"{"a":1}"#);
        assert_eq!(pointer, "/operations/op/input/dependentRequired");
        assert!(details.is_none());
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
