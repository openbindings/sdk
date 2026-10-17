//! Shared internal adapter: the schema engine reads immutable flat JSON without re-parsing.
use crate::decimal::Decimal;
use crate::raw::{Arena, Id, Kind, Member};
use jsonschema::json::{Array, Json, JsonNumber, Node, NodeIdentity, Object};
use jsonschema_value::types::JsonType;
use serde_json::Value;
use std::borrow::Cow;

/// Borrow one object member by source position, without visiting preceding members.
/// This internal bridge helper preserves duplicate names and exact name tokens.
pub fn member_at(value: crate::JsonRef<'_>, index: usize) -> Option<crate::JsonMember<'_>> {
    let Kind::Object(members) = &value.owner.nodes[value.id].kind else {
        return None;
    };
    let member = members.get(index)?;
    Some(crate::JsonMember {
        name: crate::JsonRef {
            owner: value.owner,
            id: member.key,
        },
        value: crate::JsonRef {
            owner: value.owner,
            id: member.value,
        },
    })
}

// Fixed schemas only test ASCII patterns, ASCII constants and nonempty strings.
// Escape the marker itself and encode unpaired units, retaining every UTF-16 unit.
// This presentation is injective; it is not a Unicode replacement operation.
const STRING_MARKER: char = '\u{e000}';
#[derive(Clone, Copy, Default)]
struct StringAccess {
    fixed: bool,
    unsupported: bool,
}
thread_local! {
    static STRING_ACCESS: std::cell::Cell<StringAccess> = const {
        std::cell::Cell::new(StringAccess { fixed: false, unsupported: false })
    };
}
struct StringScope(StringAccess);
impl Drop for StringScope {
    fn drop(&mut self) {
        STRING_ACCESS.set(self.0);
    }
}
/// Run one runtime evaluation, recording only strings actually interpreted by it.
/// Nested calls restore their caller's state, including on unwinding.
pub fn string_scope<T>(evaluate: impl FnOnce() -> T) -> Result<T, UnsupportedString> {
    let _restore = StringScope(STRING_ACCESS.replace(StringAccess::default()));
    let result = evaluate();
    if STRING_ACCESS.get().unsupported {
        Err(UnsupportedString)
    } else {
        Ok(result)
    }
}
#[derive(Debug)]
pub struct UnsupportedString;
/// Use the lossless presentation only for the trusted fixed-schema constraints.
pub fn fixed_strings<T>(evaluate: impl FnOnce() -> T) -> T {
    let _restore = StringScope(STRING_ACCESS.replace(StringAccess {
        fixed: true,
        unsupported: false,
    }));
    evaluate()
}
fn interpreted_unpaired() {
    STRING_ACCESS.with(|cell| {
        let mut state = cell.get();
        if !state.fixed {
            state.unsupported = true;
        }
        cell.set(state);
    });
}
fn present_string(arena: &Arena, id: Id) -> Option<Cow<'_, str>> {
    if let Some(text) = arena.string(id) {
        if !text.contains(STRING_MARKER) {
            return Some(Cow::Borrowed(text));
        }
        return Some(Cow::Owned(
            text.replace(STRING_MARKER, &STRING_MARKER.to_string().repeat(2)),
        ));
    }
    let units = arena.units(id)?;
    let mut text = String::new();
    for unit in char::decode_utf16(units) {
        match unit {
            Ok(c) => {
                text.push(c);
                if c == STRING_MARKER {
                    text.push(c);
                }
            }
            Err(unit) => {
                use std::fmt::Write;
                let _ = write!(text, "{STRING_MARKER}{:04x};", unit.unpaired_surrogate());
            }
        }
    }
    Some(Cow::Owned(text))
}
fn read_string(arena: &Arena, id: Id) -> Option<Cow<'_, str>> {
    if STRING_ACCESS.get().fixed {
        return present_string(arena, id);
    }
    if let Some(text) = arena.string(id) {
        return Some(Cow::Borrowed(text));
    }
    if matches!(arena.nodes[id].kind, Kind::String { .. }) {
        interpreted_unpaired();
    }
    present_string(arena, id)
}
/// The same fixed-schema spelling for locating errors back in the exact arena.
pub fn fixed_string(value: crate::JsonRef<'_>) -> Option<Cow<'_, str>> {
    present_string(value.owner, value.id)
}

#[derive(Debug)]
pub struct FlatJson;
#[derive(Clone, Copy)]
pub enum View<'a> {
    Stored(&'a Arena, Id),
    Text(&'a str),
}
impl std::fmt::Debug for View<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

impl Json for FlatJson {
    type Node<'a> = View<'a>;
    type PreparedKey = String;
    type StringBuffer = String;
    const KEYS_PER_LOOKUP: usize = 1;
    fn prepare_key(key: &str) -> String {
        key.to_owned()
    }
    fn with_string_node<T>(buffer: &mut String, text: &str, f: impl FnOnce(View<'_>) -> T) -> T {
        buffer.clear();
        buffer.push_str(text);
        f(View::Text(buffer))
    }
}
pub struct Number<'a>(&'a str);
impl std::fmt::Debug for Number<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Number").finish_non_exhaustive()
    }
}

impl JsonNumber for Number<'_> {
    fn as_u64(&self) -> Option<u64> {
        self.0.parse().ok()
    }
    fn as_i64(&self) -> Option<i64> {
        self.0.parse().ok()
    }
    fn as_f64(&self) -> Option<f64> {
        self.0.parse::<f64>().ok().filter(|v| v.is_finite())
    }
    fn as_str(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.0)
    }
    // Opaque cold-path payload, never an authoritative number or SDK report.
    fn to_number(&self) -> Cow<'_, serde_json::Number> {
        Cow::Owned(0.into())
    }
    fn is_integer(&self) -> bool {
        // The arena has already admitted JSON number syntax. Without a decimal
        // point or exponent, even -0 and tokens longer than i64 are integers.
        if !self.0.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
            return true;
        }
        Decimal::parse(self.0).is_integer()
    }
}
impl<'a> Node<'a, FlatJson> for View<'a> {
    type Object = ObjectView<'a>;
    type Array = ArrayView<'a>;
    type Number = Number<'a>;
    fn as_object(&self) -> Option<ObjectView<'a>> {
        match self {
            Self::Stored(a, id) => match &a.nodes[*id].kind {
                Kind::Object(m) => Some(ObjectView(a, m)),
                _ => None,
            },
            _ => None,
        }
    }
    fn as_array(&self) -> Option<ArrayView<'a>> {
        match self {
            Self::Stored(a, id) => match &a.nodes[*id].kind {
                Kind::Array(m) => Some(ArrayView(a, m)),
                _ => None,
            },
            _ => None,
        }
    }
    fn as_string(&self) -> Option<Cow<'a, str>> {
        match self {
            Self::Text(s) => Some(Cow::Borrowed(s)),
            Self::Stored(a, id) => read_string(a, *id),
        }
    }
    fn as_number(&self) -> Option<Number<'a>> {
        match self {
            Self::Stored(a, id) if matches!(a.nodes[*id].kind, Kind::Number) => {
                Some(Number(a.raw(*id)))
            }
            _ => None,
        }
    }
    fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Stored(a, id) => match a.nodes[*id].kind {
                Kind::Bool(b) => Some(b),
                _ => None,
            },
            _ => None,
        }
    }
    fn is_null(&self) -> bool {
        matches!(self,Self::Stored(a,id) if matches!(a.nodes[*id].kind,Kind::Null))
    }
    fn json_type(&self) -> JsonType {
        match self {
            Self::Text(_) => JsonType::String,
            Self::Stored(a, id) => match a.nodes[*id].kind {
                Kind::Null => JsonType::Null,
                Kind::Bool(_) => JsonType::Boolean,
                Kind::Number => JsonType::Number,
                Kind::String { .. } => JsonType::String,
                Kind::Array(_) => JsonType::Array,
                Kind::Object(_) => JsonType::Object,
            },
        }
    }
    fn equals_value(&self, expected: &Value) -> bool {
        let mut stack = vec![(*self, expected)];
        while let Some((left, right)) = stack.pop() {
            if !jsonschema::ob_work::charge(1) {
                return false;
            }
            match right {
                Value::Null => {
                    if !left.is_null() {
                        return false;
                    }
                }
                Value::Bool(b) => {
                    if left.as_boolean() != Some(*b) {
                        return false;
                    }
                }
                Value::String(s) => {
                    if left.as_string().as_deref() != Some(s) {
                        return false;
                    }
                }
                Value::Number(n) => {
                    if left
                        .as_number()
                        .is_none_or(|m| Decimal::parse(m.0) != Decimal::parse(&n.to_string()))
                    {
                        return false;
                    }
                }
                Value::Array(values) => {
                    let Some(items) = left.as_array() else {
                        return false;
                    };
                    if items.len() != values.len() {
                        return false;
                    }
                    stack.extend(items.elements().zip(values));
                }
                Value::Object(members) => {
                    let Some(items) = left.as_object() else {
                        return false;
                    };
                    if items.len() != members.len() {
                        return false;
                    }
                    for (k, v) in members {
                        let Some(item) = items.get(k) else {
                            return false;
                        };
                        stack.push((item, v));
                    }
                }
            }
        }
        true
    }
    // Documented cold-path payload only. The SDK never exposes or formats it.
    // Equality, uniqueness, integer classification and numeric constraints use
    // the exact overrides; schema facts are recovered from original sources.
    // Redact the entire value rather than materializing unrepresentable numbers.
    fn to_value(&self) -> Cow<'a, Value> {
        Cow::Borrowed(&Value::Null)
    }
    fn identity(&self) -> Option<NodeIdentity> {
        match self {
            Self::Stored(a, id) => Some(NodeIdentity::tagged(
                std::ptr::from_ref(*a) as usize,
                *id as u32,
            )),
            Self::Text(_) => None,
        }
    }
}
pub struct ObjectView<'a>(&'a Arena, &'a [Member]);
impl std::fmt::Debug for ObjectView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectView").finish_non_exhaustive()
    }
}

pub struct Members<'a>(&'a Arena, std::slice::Iter<'a, Member>);
impl std::fmt::Debug for Members<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Members").finish_non_exhaustive()
    }
}

// Iterating values need not interpret their member names. Mark unsupported
// access only when the engine requests the scalar name (matching or locating it).
pub struct MemberName<'a> {
    text: Cow<'a, str>,
    unpaired: bool,
}
impl std::fmt::Debug for MemberName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemberName").finish_non_exhaustive()
    }
}

impl AsRef<str> for MemberName<'_> {
    fn as_ref(&self) -> &str {
        if self.unpaired {
            interpreted_unpaired();
        }
        &self.text
    }
}
impl<'a> From<MemberName<'a>> for Cow<'a, str> {
    fn from(name: MemberName<'a>) -> Self {
        if name.unpaired {
            interpreted_unpaired();
        }
        name.text
    }
}
impl<'a> Iterator for Members<'a> {
    type Item = (MemberName<'a>, View<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        if !jsonschema::ob_work::charge(1) {
            return None;
        }
        self.1.next().map(|m| {
            (
                MemberName {
                    text: if STRING_ACCESS.get().fixed {
                        present_string(self.0, m.key).expect("member name is a string")
                    } else {
                        self.0.string(m.key).map(Cow::Borrowed).unwrap_or_else(|| {
                            present_string(self.0, m.key).expect("member name is a string")
                        })
                    },
                    unpaired: self.0.string(m.key).is_none(),
                },
                View::Stored(self.0, m.value),
            )
        })
    }
}
impl<'a> Object<'a, FlatJson> for ObjectView<'a> {
    type Node = View<'a>;
    type MemberName = MemberName<'a>;
    type MembersIter = Members<'a>;
    fn len(&self) -> usize {
        self.1.len()
    }
    fn get(&self, key: &String) -> Option<View<'a>> {
        // recover the containing object from its first key token, so the
        // adapter shares the arena index and charges every collision candidate.
        let (object, _) = self.0.nodes[self.1.first()?.key].parent?;
        self.0
            .get_with_probe(object, key, || jsonschema::ob_work::charge(1))
            .map(|id| View::Stored(self.0, id))
    }
    fn members(&self) -> Members<'a> {
        Members(self.0, self.1.iter())
    }
}
pub struct ArrayView<'a>(&'a Arena, &'a [Id]);
impl std::fmt::Debug for ArrayView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArrayView").finish_non_exhaustive()
    }
}

pub struct Elements<'a>(&'a Arena, std::slice::Iter<'a, Id>);
impl std::fmt::Debug for Elements<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Elements").finish_non_exhaustive()
    }
}

impl<'a> Iterator for Elements<'a> {
    type Item = View<'a>;
    fn next(&mut self) -> Option<View<'a>> {
        if !jsonschema::ob_work::charge(1) {
            return None;
        }
        self.1.next().map(|id| View::Stored(self.0, *id))
    }
}
impl<'a> Array<'a, FlatJson> for ArrayView<'a> {
    type Node = View<'a>;
    type ElementsIter = Elements<'a>;
    fn len(&self) -> usize {
        self.1.len()
    }
    fn elements(&self) -> Elements<'a> {
        Elements(self.0, self.1.iter())
    }
    fn is_unique(&self) -> bool {
        use std::{collections::hash_map::RandomState, hash::BuildHasher};
        let Some(&first) = self.1.first() else {
            return true;
        };
        let last = *self.1.last().unwrap();
        let end = self
            .0
            .nodes
            .partition_point(|n| n.span.start < self.0.nodes[last].span.end);
        let hasher = RandomState::new();
        let mut hashes = vec![0u64; end - first];
        for id in (first..end).rev() {
            if !jsonschema::ob_work::charge(1) {
                return false;
            }
            let hash = match &self.0.nodes[id].kind {
                Kind::Null => hasher.hash_one((0u8, 0u8)),
                Kind::Bool(b) => hasher.hash_one((1u8, b)),
                Kind::Number => hasher.hash_one((2u8, Decimal::parse(self.0.raw(id)))),
                Kind::String {
                    unpaired: Some(units),
                    ..
                } => hasher.hash_one((3u8, 1u8, units)),
                Kind::String { .. } => hasher.hash_one((3u8, 0u8, self.0.string(id))),
                Kind::Array(items) => hasher.hash_one((
                    4u8,
                    items.iter().map(|&i| hashes[i - first]).collect::<Vec<_>>(),
                )),
                Kind::Object(members) => {
                    let mut pairs: Vec<_> = members
                        .iter()
                        .map(|m| (hashes[m.key - first], hashes[m.value - first]))
                        .collect();
                    pairs.sort_unstable();
                    hasher.hash_one((5u8, pairs))
                }
            };
            hashes[id - first] = hash;
        }
        self.unique_by_hash(first, &hashes)
    }
}
impl ArrayView<'_> {
    // Hashes only select candidates. Keep the first ID inline, then retain every
    // unequal collision and charge/verify exact equality exactly as before.
    fn unique_by_hash(&self, first: Id, hashes: &[u64]) -> bool {
        use std::collections::{HashMap, hash_map::Entry};
        enum Bucket {
            One(Id),
            Many(Vec<Id>),
        }
        impl Bucket {
            fn ids(&self) -> &[Id] {
                match self {
                    Self::One(id) => std::slice::from_ref(id),
                    Self::Many(ids) => ids,
                }
            }
            fn push(&mut self, id: Id) {
                match self {
                    Self::One(prior) => *self = Self::Many(vec![*prior, id]),
                    Self::Many(ids) => ids.push(id),
                }
            }
        }
        let mut seen: HashMap<u64, Bucket> = HashMap::new();
        for &id in self.1 {
            let bucket = match seen.entry(hashes[id - first]) {
                Entry::Vacant(entry) => {
                    entry.insert(Bucket::One(id));
                    continue;
                }
                Entry::Occupied(entry) => entry.into_mut(),
            };
            for &prior in bucket.ids() {
                if !jsonschema::ob_work::charge(1) {
                    return false;
                }
                if equals(View::Stored(self.0, id), View::Stored(self.0, prior)) {
                    return false;
                }
            }
            bucket.push(id);
        }
        true
    }
}

/// Borrow an admitted exact value without parsing or allocating.
pub fn view(value: &crate::JsonValue) -> View<'_> {
    View::Stored(&value.owner, value.id)
}
/// Build only the JSON Pointer; source line/column scanning is unnecessary here.
pub fn pointer(value: &crate::JsonValue) -> Option<String> {
    value.owner.pointer(value.id)
}
/// Build only the exact JSON Pointer from the source's root, without line and
/// column, including names that hold unpaired units.
pub fn exact_pointer(value: crate::JsonRef<'_>) -> crate::Pointer {
    value.owner.exact_pointer(value.id)
}
/// Wrap escaped RFC 6901 text built from original sources, without checking it
/// again.
pub fn escaped_pointer(text: String) -> crate::Pointer {
    crate::Pointer::escaped(text)
}
/// Read a pointer from an exact JSON string, keeping unpaired units. `None` for
/// a non-string or text that is not an RFC 6901 pointer.
pub fn string_pointer(value: crate::JsonRef<'_>) -> Option<crate::Pointer> {
    crate::Pointer::parse_units(&value.utf16_units()?)
}
/// The exact pointer of `value` from its ancestor `root`, a value's own frame.
pub fn relative_pointer(value: crate::JsonRef<'_>, root: crate::JsonRef<'_>) -> crate::Pointer {
    debug_assert!(std::sync::Arc::ptr_eq(value.owner, root.owner));
    value.owner.exact_pointer_from(value.id, root.id)
}
/// Append escaped RFC 6901 text, such as a keyword's `/minimum`.
pub fn push_pointer(pointer: &mut crate::Pointer, escaped: &str) {
    pointer.push_escaped(escaped);
}
/// A pointer followed by escaped RFC 6901 text, in one exact allocation.
pub fn joined_pointer(pointer: &crate::Pointer, escaped: &str) -> crate::Pointer {
    pointer.joined(escaped)
}
/// The bytes a pointer retains: its UTF-8 text, or two per UTF-16 unit.
pub fn pointer_bytes(pointer: &crate::Pointer) -> usize {
    pointer.size()
}
/// Whether two views read the same source.
pub fn same_source(value: crate::JsonRef<'_>, source: &crate::JsonValue) -> bool {
    std::sync::Arc::ptr_eq(value.owner, &source.owner)
}
/// Measure separately owned decoded UTF-8 strings in a borrowed exact subtree,
/// including member names. Raw source bytes are excluded. Unescaped strings borrow
/// their source; escaped scalar strings own their complete decoded value. This
/// visits the existing flat nodes without parsing or allocating. `None` means the
/// byte limit is exceeded or the subtree contains non-scalar UTF-16 strings.
pub fn decoded_string_size(value: crate::JsonRef<'_>, limit: usize) -> Option<usize> {
    let end = value.owner.nodes[value.id].span.end;
    let mut size = 0usize;
    for node in value.owner.nodes[value.id..]
        .iter()
        .take_while(|node| node.span.start < end)
    {
        if let Kind::String { decoded, unpaired } = &node.kind {
            if unpaired.is_some() {
                return None;
            }
            size = size.checked_add(decoded.as_ref().map_or(0, |s| s.len()))?;
            if size > limit {
                return None;
            }
        }
    }
    Some(size)
}
/// Measure an escaped source pointer before allocating it. `None` means that
/// the pointer is unavailable or would exceed the caller's remaining byte budget.
pub fn pointer_size(value: crate::JsonRef<'_>, limit: usize) -> Option<usize> {
    use crate::raw::Edge;
    let mut size = 0usize;
    let mut current = value.id;
    while let Some((parent, edge)) = value.owner.nodes[current].parent {
        let remaining = limit.checked_sub(size)?.checked_sub(1)?;
        let segment = match edge {
            Edge::Index(index) => {
                if index == 0 {
                    1
                } else {
                    index.ilog10() as usize + 1
                }
            }
            Edge::Key(key) => {
                let name = value.owner.string(key)?;
                if name.len() > remaining {
                    return None;
                }
                name.len() + name.bytes().filter(|c| matches!(c, b'~' | b'/')).count()
            }
        };
        if segment > remaining {
            return None;
        }
        size += 1 + segment;
        current = parent;
    }
    Some(size)
}

/// Bound the bytes a retained location's exact pointer keeps before
/// constructing it: its UTF-8 bytes, or two bytes per UTF-16 unit when an
/// ancestor's name holds an unpaired unit. None means the budget is exceeded.
pub fn location_pointer_size(value: crate::JsonRef<'_>, limit: usize) -> Option<usize> {
    if let Some(size) = pointer_size(value, limit) {
        return Some(size);
    }
    // Distinguish a pointer through an unpaired name, held as units, from an
    // oversized one without allocating either. This second walk is needed only
    // after the ordinary bounded measure failed; normal locations take one walk.
    let escaped = |unit: u16| unit == u16::from(b'~') || unit == u16::from(b'/');
    let (mut units, mut unpaired) = (0usize, false);
    let mut current = value.id;
    while let Some((parent, edge)) = value.owner.nodes[current].parent {
        let segment = match edge {
            crate::raw::Edge::Index(index) => index
                .checked_ilog10()
                .map_or(1, |digits| digits as usize + 1),
            crate::raw::Edge::Key(key) => match &value.owner.nodes[key].kind {
                Kind::String {
                    unpaired: Some(name),
                    ..
                } => {
                    unpaired = true;
                    name.len() + name.iter().filter(|&&unit| escaped(unit)).count()
                }
                _ => value
                    .owner
                    .string(key)?
                    .encode_utf16()
                    .map(|unit| 1 + usize::from(escaped(unit)))
                    .sum(),
            },
        };
        units = units.saturating_add(1).saturating_add(segment);
        current = parent;
    }
    let size = units.saturating_mul(2);
    (unpaired && size <= limit).then_some(size)
}

/// Locate a bounded diagnostic batch while scanning each arena's source prefix
/// once. Sorting is validation-local; healthy documents build no line index.
/// Output order matches input order, including repeated nodes and mixed arenas.
pub fn locations(values: &[crate::JsonRef<'_>]) -> Vec<crate::SourceLocation> {
    locations_with_work(values).0
}
fn locations_with_work(values: &[crate::JsonRef<'_>]) -> (Vec<crate::SourceLocation>, usize) {
    let mut order: Vec<_> = (0..values.len()).collect();
    order.sort_unstable_by_key(|&index| {
        let value = values[index];
        (
            std::sync::Arc::as_ptr(value.owner),
            value.owner.nodes[value.id].span.start,
        )
    });
    let mut result = vec![None; values.len()];
    let mut previous: Option<&Arena> = None;
    let (mut cursor, mut line, mut line_start, mut scanned) = (0, 1, 0, 0);
    for index in order {
        let value = values[index];
        let arena = value.owner.as_ref();
        if !previous.is_some_and(|old| std::ptr::eq(old, arena)) {
            (cursor, line, line_start) = (0, 1, 0);
            previous = Some(arena);
        }
        let byte_offset = arena.nodes[value.id].span.start;
        for (relative, &byte) in arena.source().as_bytes()[cursor..byte_offset]
            .iter()
            .enumerate()
        {
            if byte == b'\n' {
                line += 1;
                line_start = cursor + relative + 1;
            }
        }
        scanned += byte_offset - cursor;
        cursor = byte_offset;
        result[index] = Some(crate::SourceLocation {
            pointer: Some(arena.exact_pointer(value.id)),
            byte_offset,
            line,
            byte_column: byte_offset - line_start + 1,
        });
    }
    (
        result
            .into_iter()
            .map(|at| at.expect("each input index is visited once"))
            .collect(),
        scanned,
    )
}
pub fn has_unpaired(value: &crate::JsonValue) -> bool {
    let span = &value.owner.nodes[value.id].span;
    value.owner.unpaired.iter().any(|id| {
        let inner = &value.owner.nodes[*id].span;
        span.start <= inner.start && inner.end <= span.end
    })
}

/// Borrow string and member-name tokens containing unpaired UTF-16 units.
/// The exact parser retains these for values and draft inspection; document
/// conformance applies its own OBI-01 exclusion at the original token locations.
pub fn unpaired_strings(value: &crate::JsonValue) -> impl Iterator<Item = crate::JsonRef<'_>> {
    let span = &value.owner.nodes[value.id].span;
    value.owner.unpaired.iter().filter_map(move |&id| {
        let inner = &value.owner.nodes[id].span;
        (span.start <= inner.start && inner.end <= span.end).then_some(crate::JsonRef {
            owner: &value.owner,
            id,
        })
    })
}
/// Stack-safe exact comparison, charged to the enclosing evaluator work scope.
pub fn equals(left: View<'_>, right: View<'_>) -> bool {
    let mut stack = vec![(left, right)];
    while let Some((left, right)) = stack.pop() {
        if !jsonschema::ob_work::charge(1) {
            return false;
        }
        if left.json_type() != right.json_type() {
            return false;
        }
        if let Some(items) = left.as_array() {
            let other = right.as_array().unwrap();
            if items.len() != other.len() {
                return false;
            }
            stack.extend(items.elements().zip(other.elements()));
        } else if let Some(items) = left.as_object() {
            let other = right.as_object().unwrap();
            if items.len() != other.len() {
                return false;
            }
            // Exact object equality does not round-trip names through the scalar
            // lookup API. In particular, an encoded surrogate cannot alias a
            // genuine scalar member name containing the presentation marker.
            for member in items.1 {
                if !jsonschema::ob_work::charge(1) {
                    return false;
                }
                let matched = if let Some(key) = items.0.string(member.key) {
                    other.get(&key.to_owned())
                } else {
                    interpreted_unpaired();
                    let key = items.0.units(member.key).unwrap();
                    let mut matched = None;
                    for candidate in other.1 {
                        if !jsonschema::ob_work::charge(1) {
                            return false;
                        }
                        if other.0.units(candidate.key).as_deref() == Some(&key) {
                            matched = Some(View::Stored(other.0, candidate.value));
                            break;
                        }
                    }
                    matched
                };
                let Some(other) = matched else {
                    return false;
                };
                stack.push((View::Stored(items.0, member.value), other));
            }
        } else if let Some(n) = left.as_number() {
            let other = right.as_number().unwrap().0;
            // Parsing both tokens as i64 proves exact representability, including
            // -0 == 0. Mixed, fractional, exponent and wider pairs use Decimal.
            let same = match (n.0.parse::<i64>(), other.parse::<i64>()) {
                (Ok(left), Ok(right)) => left == right,
                _ => Decimal::parse(n.0) == Decimal::parse(other),
            };
            if !same {
                return false;
            }
        } else if left.json_type() == JsonType::String {
            fn scalar(value: View<'_>) -> Option<&str> {
                match value {
                    View::Stored(arena, id) => arena.string(id),
                    View::Text(text) => Some(text),
                }
            }
            fn unpaired(value: View<'_>) -> Option<&[u16]> {
                match value {
                    View::Stored(arena, id) => match &arena.nodes[id].kind {
                        Kind::String { unpaired, .. } => unpaired.as_deref(),
                        _ => None,
                    },
                    View::Text(_) => None,
                }
            }
            match (scalar(left), scalar(right)) {
                (Some(a), Some(b)) => {
                    if a != b {
                        return false;
                    }
                }
                _ => {
                    interpreted_unpaired();
                    if unpaired(left) != unpaired(right) {
                        return false;
                    }
                }
            }
        } else if left.as_boolean() != right.as_boolean() {
            return false;
        }
    }
    true
}

pub fn node_id(value: &crate::JsonValue) -> usize {
    value.id
}
/// The arena position of a borrowed value, for a later [`node`] lookup.
pub fn ref_id(value: crate::JsonRef<'_>) -> usize {
    value.id
}
/// Borrow the value at an arena position recorded by [`ref_id`] from any value
/// sharing that arena. `None` when the position does not exist.
pub fn node(value: &crate::JsonValue, id: usize) -> Option<crate::JsonRef<'_>> {
    (id < value.owner.nodes.len()).then_some(crate::JsonRef {
        owner: &value.owner,
        id,
    })
}
/// Rebase a retained child when it becomes an independent document resource.
pub fn standalone(value: crate::JsonValue) -> crate::JsonValue {
    if value.id == 0 {
        return value;
    }
    crate::JsonValue::parse_with_limits(
        value.bytes(),
        crate::JsonLimits {
            max_bytes: usize::MAX,
            max_depth: usize::MAX,
            max_nodes: u32::MAX as usize,
        },
    )
    .expect("an admitted value's source span is a complete JSON value")
}
/// Borrow duplicate-containing objects in parse order without materializing
/// pointers or coordinates. Callers cap this iterator before collecting nodes.
/// Document validation passes a standalone root, so this covers its whole arena.
pub fn duplicate_nodes(
    value: &crate::JsonValue,
) -> impl ExactSizeIterator<Item = crate::JsonRef<'_>> {
    value.owner.duplicates.iter().map(|(id, _)| crate::JsonRef {
        owner: &value.owner,
        id: *id,
    })
}

/// Borrow each offending repeated member-name token in parse order. Distinct
/// occurrences can share a JSON Pointer but retain distinct original byte spans.
/// This exposes the parser's existing key identities without rescanning objects.
pub fn duplicate_member_names(
    value: &crate::JsonValue,
) -> impl ExactSizeIterator<Item = crate::JsonRef<'_>> {
    value
        .owner
        .duplicates
        .iter()
        .map(|(_, key)| crate::JsonRef {
            owner: &value.owner,
            id: *key,
        })
}

pub fn depth(value: &crate::JsonValue) -> usize {
    let span = &value.owner.nodes[value.id].span;
    let end = value
        .owner
        .nodes
        .partition_point(|node| node.span.start < span.end);
    let mut depths = vec![0; end - value.id];
    let mut maximum = 0;
    for (offset, node) in value.owner.nodes[value.id..end].iter().enumerate() {
        let own = usize::from(matches!(node.kind, Kind::Array(_) | Kind::Object(_)));
        let parent = if offset == 0 {
            0
        } else {
            node.parent
                .map_or(0, |(parent, _)| depths[parent - value.id])
        };
        depths[offset] = parent + own;
        maximum = maximum.max(depths[offset]);
    }
    maximum
}

/// Logical live exact-storage owners, including process-lifetime fixed schemas.
pub fn live_arenas() -> usize {
    crate::raw::live_arenas()
}

/// Borrow a token's byte range without constructing its pointer or line coordinates.
pub fn byte_range(value: crate::JsonRef<'_>) -> std::ops::Range<usize> {
    value.owner.nodes[value.id].span.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_string_size_counts_owned_subtree_strings_without_ancestors_or_siblings() {
        let value = crate::JsonValue::parse(
            r#"{"outside\n":"sibling\t","nested":{"key\"":"raw雪","list":["a\/b","\uD83D\uDE00",{"\u006b":"end\n"}]},"after":"other\r"}"#,
        ).unwrap();
        let nested = value.at("/nested").unwrap();
        // key\" (4), a\/b (3), emoji (4 UTF-8), \u006b (1), end\n (4).
        assert_eq!(decoded_string_size(nested, 16), Some(16));
        assert_eq!(decoded_string_size(nested, 15), None);
        assert_eq!(
            decoded_string_size(value.at("/nested/key\"").unwrap(), 0),
            Some(0)
        );
        assert_eq!(
            decoded_string_size(value.at("/nested/list/1").unwrap(), 4),
            Some(4)
        );
        assert_eq!(decoded_string_size(value.view(), 38), Some(38));
        let unsupported = crate::JsonValue::parse(r#"["\ud800"]"#).unwrap();
        assert_eq!(decoded_string_size(unsupported.view(), usize::MAX), None);
    }
    #[test]
    fn indexed_members_preserve_each_exact_occurrence() {
        let source = r#"{"z":9007199254740993,"\ud800":null,"z":0.29000000000000001}"#;
        let value = crate::JsonValue::parse(source).unwrap();
        for (index, (name, text)) in [
            (r#""z""#, "9007199254740993"),
            (r#""\ud800""#, "null"),
            (r#""z""#, "0.29000000000000001"),
        ]
        .into_iter()
        .enumerate()
        {
            let member = member_at(value.view(), index).unwrap();
            assert_eq!(member.name.text(), name);
            assert_eq!(member.value.text(), text);
        }
        let first = member_at(value.view(), 0).unwrap();
        let last = member_at(value.view(), 2).unwrap();
        assert_ne!(
            first.name.location().byte_offset,
            last.name.location().byte_offset
        );
        let retained = last.value.to_owned();
        assert!(member_at(value.view(), 3).is_none());
        assert!(member_at(value.view(), usize::MAX).is_none());
        drop(value);
        assert_eq!(retained.text(), "0.29000000000000001");
        for source in ["{}", "[]", "[1]", "null", "1", "true", r#""text""#] {
            let value = crate::JsonValue::parse(source).unwrap();
            assert!(member_at(value.view(), 0).is_none());
        }
    }

    #[test]
    fn duplicate_member_names_keep_each_original_token_occurrence() {
        let source = r#"{"a/~":{"k":0,"k":1,"k":2},"\ud800":{"j":0,"j":1}}"#;
        let value = crate::JsonValue::parse(source).unwrap();
        assert_eq!(duplicate_member_names(&value).len(), 3);
        let nodes: Vec<_> = duplicate_member_names(&value).collect();
        let at = locations(&nodes);
        assert_eq!(at[0].pointer.as_ref().unwrap(), "/a~1~0/k");
        assert_eq!(at[1].pointer, at[0].pointer);
        assert_eq!(at[0].byte_offset, source.find("\"k\":1").unwrap());
        assert_eq!(at[1].byte_offset, source.find("\"k\":2").unwrap());
        // A name with an unpaired unit is named exactly.
        let exact = at[2].pointer.as_ref().unwrap();
        assert_eq!(exact.as_str(), None);
        assert_eq!(exact.to_string(), "/\\ud800/j");
        assert_eq!(at[2].byte_offset, source.find("\"j\":1").unwrap());
        assert_eq!(duplicate_member_names(&value).take(1).count(), 1);
    }

    #[test]
    fn duplicate_locations_are_materialized_only_for_retained_nodes() {
        let source = format!(
            "{{\"pad\":\"{}\",\"nested\":{{\"k\":0{}}}}}",
            "x".repeat(128 * 1024),
            ",\"k\":0".repeat(5000)
        );
        let value = crate::JsonValue::parse(&source).unwrap();
        let mut visited = 0;
        let nodes: Vec<_> = duplicate_nodes(&value)
            .inspect(|_| visited += 1)
            .take(4096)
            .collect();
        assert_eq!(visited, 4096);
        assert_eq!(nodes.len(), 4096);
        let (locations, scanned) = locations_with_work(&nodes);
        let offset = source.find("{\"k\"").unwrap();
        assert_eq!(scanned, offset);
        assert!(
            locations
                .iter()
                .all(|at| at.byte_offset == offset && at.pointer.as_ref().unwrap() == "/nested")
        );
        assert_eq!(duplicate_nodes(&value).take(0).count(), 0);
    }

    #[test]
    fn pointers_through_unpaired_names_are_exact_and_charge_their_units() {
        let value = crate::JsonValue::parse(r#"{"\ud800/":{"k":0,"k":1}}"#).unwrap();
        let node = duplicate_nodes(&value).next().unwrap();
        // The scalar measure has no spelling for it; the exact one holds the
        // escaped units `/`, U+D800, `~`, `1`, two bytes each.
        assert_eq!(pointer_size(node, usize::MAX), None);
        assert_eq!(location_pointer_size(node, 7), None);
        assert_eq!(location_pointer_size(node, 8), Some(8));
        let at = locations(&[node]).remove(0);
        let pointer = at.pointer.as_ref().unwrap();
        assert_eq!(
            pointer.utf16_units().collect::<Vec<_>>(),
            [0x2f, 0xd800, 0x7e, 0x31]
        );
        assert_eq!(at.byte_offset, 11);
        assert_eq!(
            value.at_pointer(pointer).map(byte_range),
            Some(byte_range(node))
        );
        assert_eq!(value.locate(pointer).as_ref(), Some(&at));

        let scalar = crate::JsonValue::parse(r#"{"a/~":{"k":0,"k":1}}"#).unwrap();
        let node = duplicate_nodes(&scalar).next().unwrap();
        assert_eq!(location_pointer_size(node, 5), None);
        assert_eq!(location_pointer_size(node, 6), Some(6));
    }

    #[test]
    fn batched_locations_match_original_coordinates_with_linear_source_scans() {
        let first = crate::JsonValue::parse(" {\r\n\"é/~\\n\":[1,\n2],\"z\":false}").unwrap();
        let second = crate::JsonValue::parse(" \n[true, false]").unwrap();
        let members: Vec<_> = first.view().members().unwrap().collect();
        let values = [
            members[1].value,
            second.view(),
            members[0].name,
            first.at("/é~1~0\n/1").unwrap(),
            members[1].name,
            members[1].value,
            second.at("/1").unwrap(),
        ];
        let (locations, scanned) = locations_with_work(&values);
        assert_eq!(
            locations,
            values.iter().map(|v| v.location()).collect::<Vec<_>>()
        );
        let max_first = values
            .iter()
            .filter(|v| std::sync::Arc::ptr_eq(v.owner, &first.owner))
            .map(|v| v.owner.nodes[v.id].span.start)
            .max()
            .unwrap();
        let max_second = values
            .iter()
            .filter(|v| std::sync::Arc::ptr_eq(v.owner, &second.owner))
            .map(|v| v.owner.nodes[v.id].span.start)
            .max()
            .unwrap();
        assert_eq!(scanned, max_first + max_second);
        assert!(locations_with_work(&[]).0.is_empty());
        for value in values {
            let size = value.location().pointer.unwrap().as_str().unwrap().len();
            assert_eq!(pointer_size(value, size), Some(size));
            if size > 0 {
                assert_eq!(pointer_size(value, size - 1), None);
            }
        }
    }

    #[test]
    fn uniqueness_verifies_every_forced_hash_collision() {
        for (text, expected) in [
            ("[]", true),
            ("[1,2,3,4]", true),
            ("[1,2,3,1.0]", false),
            ("[1,2,3,2e0]", false),
            ("[1,2,3,3]", false),
            (r#"[{"a":1},{"a":2},{"b":1}]"#, true),
            (r#"[{"a":1},{"a":2},{"a":2.0}]"#, false),
            ("[-0,1,2,0e999999]", false),
        ] {
            let value = crate::JsonValue::parse(text).unwrap();
            let array = view(&value).as_array().unwrap();
            // A constant hash forces One -> Many and hits both early and later
            // candidates. Hash equality alone must never decide uniqueness.
            assert_eq!(
                array.unique_by_hash(0, &vec![0; value.owner.nodes.len()]),
                expected,
                "{text}"
            );
        }
    }

    #[test]
    fn uniqueness_is_exact_and_stack_safe() {
        let validator = jsonschema::options_for::<FlatJson>()
            .with_draft(jsonschema::Draft::Draft202012)
            .offline()
            .build(&serde_json::json!({"uniqueItems":true}))
            .unwrap();
        for (text, want) in [
            ("[1,1.0]", false),
            (r#"[{"a":1,"b":2},{"b":2.0,"a":1e0}]"#, false),
            ("[9007199254740992,9007199254740993]", true),
        ] {
            let value = crate::JsonValue::parse(text).unwrap();
            assert_eq!(validator.is_valid(view(&value)), want);
        }
        let deep = format!("{}0{}", "[".repeat(9999), "]".repeat(9999));
        let value = crate::JsonValue::parse(format!("[{deep},{deep}]")).unwrap();
        assert!(!validator.is_valid(view(&value)));
    }
    #[test]
    fn deep_unevaluated_diagnostics_do_not_materialize_values() {
        let validator = jsonschema::options_for::<FlatJson>()
            .with_draft(jsonschema::Draft::Draft202012)
            .offline()
            .build(&serde_json::json!({"unevaluatedItems":false}))
            .unwrap();
        let value = crate::JsonValue::parse(format!("{}0{}", "[".repeat(10000), "]".repeat(10000)))
            .unwrap();
        let errors: Vec<_> = validator.iter_errors(view(&value)).collect();
        assert!(!errors.is_empty());
        assert_eq!(errors[0].instance_path().as_str(), "");
    }
}
