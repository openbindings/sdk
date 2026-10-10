//! Exact JSON values and source-backed views.
use crate::raw::{Arena, Id, Kind, Limits};
use jsonschema_value::ob_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashMap, fmt, sync::Arc};

/// Finite input limits. Nesting counts arrays and objects, including the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonLimits {
    /// Maximum admitted source bytes; default 64 MiB (67,108,864 bytes).
    pub max_bytes: usize,
    /// Maximum nested arrays/objects, including a container root; default 10,000.
    pub max_depth: usize,
    /// Maximum parsed nodes, including object-name string tokens; default 1,000,000. The internal index also caps this at `u32::MAX`.
    pub max_nodes: usize,
}
impl Default for JsonLimits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_depth: 10_000,
            max_nodes: 1_000_000,
        }
    }
}
/// Why an exact input could not be represented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputErrorKind {
    /// Input bytes are not valid UTF-8.
    InvalidUtf8,
    /// A leading UTF-8 byte order mark is not admitted.
    ByteOrderMark,
    /// The input does not satisfy JSON grammar.
    Syntax,
    /// An input byte, nesting or node limit was reached.
    Limit,
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Exact JSON admission failure. It is separate from document conformance and schema-instance outcomes.
pub struct InputError {
    /// Broad admission failure category.
    pub kind: InputErrorKind,
    /// Zero-based original-input byte offset; an end-of-input failure may equal the input length.
    pub byte_offset: usize,
    /// Stable detailed parse/admission code, suitable for branching.
    pub code: &'static str,
}
impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.code, self.byte_offset)
    }
}
impl std::error::Error for InputError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Exact JSON token category, without coercion or interpretation.
pub enum JsonKind {
    /// The JSON literal `null`.
    Null,
    /// A JSON `true` or `false` literal.
    Boolean,
    /// An exact JSON number token, with no binary64 rounding.
    Number,
    /// A JSON string, including preserved escaped unpaired UTF-16 units.
    String,
    /// An ordered JSON array.
    Array,
    /// An ordered member sequence; duplicate names remain representable.
    Object,
}
/// Original-source coordinates: zero-based byte offset, one-based line and byte column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceLocation {
    /// RFC 6901 pointer when representable; `Some("")` means root and `None` means unavailable, not root. Duplicate member occurrences can share a pointer, so use byte coordinates to distinguish them.
    pub pointer: Option<String>,
    /// Zero-based token-start offset into original UTF-8 bytes.
    pub byte_offset: usize,
    /// One-based line number in original source, counting newline bytes.
    pub line: usize,
    /// One-based UTF-8 byte column; convert against source before using a UTF-16 editor column.
    pub byte_column: usize,
}
/// An immutable exact JSON value. Cloning retains storage, without re-parsing.
#[derive(Clone)]
pub struct JsonValue {
    pub(crate) owner: Arc<Arena>,
    pub(crate) id: Id,
}
/// A cheap borrowed view. Use `to_owned` to retain it beyond its parent's lifetime.
#[derive(Clone, Copy)]
pub struct JsonRef<'a> {
    pub(crate) owner: &'a Arc<Arena>,
    pub(crate) id: Id,
}
/// An object member; names are exact JSON strings and can preserve lone UTF-16 units.
#[derive(Clone, Copy)]
pub struct JsonMember<'a> {
    /// Exact original member-name string, including its token coordinates.
    pub name: JsonRef<'a>,
    /// Borrowed exact member value in source order.
    pub value: JsonRef<'a>,
}
impl JsonValue {
    /// Parse UTF-8 JSON under [`JsonLimits::default`], copying source into an immutable arena. Preserves number spelling, duplicate members and escaped unpaired UTF-16 units; no normative assessment is performed.
    pub fn parse(input: impl AsRef<[u8]>) -> Result<Self, InputError> {
        Self::parse_with_limits(input, JsonLimits::default())
    }
    /// Parse with explicit finite admission limits. Refuses invalid UTF-8, a byte order mark, invalid syntax or exhausted limits; does not coerce or discard JSON data.
    pub fn parse_with_limits(
        input: impl AsRef<[u8]>,
        limits: JsonLimits,
    ) -> Result<Self, InputError> {
        let bytes = input.as_ref();
        if bytes.len() > limits.max_bytes {
            return Err(InputError {
                kind: InputErrorKind::Limit,
                byte_offset: limits.max_bytes,
                code: "byte-limit",
            });
        }
        let owner = Arena::parse(
            bytes,
            Limits {
                max_depth: limits.max_depth,
                max_nodes: limits.max_nodes.min(u32::MAX as usize),
            },
        )
        .map_err(|p| InputError {
            kind: match p.code {
                "utf8" => InputErrorKind::InvalidUtf8,
                "bom" => InputErrorKind::ByteOrderMark,
                "depth-limit" | "node-limit" => InputErrorKind::Limit,
                _ => InputErrorKind::Syntax,
            },
            byte_offset: p.offset,
            code: p.code,
        })?;
        Ok(Self {
            owner: Arc::new(owner),
            id: 0,
        })
    }
    /// Borrow this value without incrementing its storage owner count.
    pub fn view(&self) -> JsonRef<'_> {
        JsonRef {
            owner: &self.owner,
            id: self.id,
        }
    }
    /// Borrow this value's exact original token text, excluding surrounding whitespace; use [`Self::original_source`] for the complete source.
    pub fn text(&self) -> &str {
        self.owner.raw(self.id)
    }
    /// The original snapshot cannot be mutated through a borrowed byte slice.
    /// ```compile_fail,E0594
    /// # use openbindings_internal_json as openbindings;
    /// let value = openbindings::JsonValue::parse("7").unwrap();
    /// value.bytes()[0] = b'8';
    /// ```
    pub fn bytes(&self) -> &[u8] {
        self.text().as_bytes()
    }
    /// Entire source backing the view, including whitespace and any containing value.
    /// Source locations refer to these bytes; `bytes` returns only this value's token.
    pub fn original_source(&self) -> &[u8] {
        self.owner.source().as_bytes()
    }
    /// Return the exact token category without conversion.
    pub fn kind(&self) -> JsonKind {
        self.view().kind()
    }
    /// Borrow the first member with this decoded name; returns `None` for absent or non-object lookups. Duplicate names are preserved; this inspection does not disambiguate them.
    pub fn get(&self, name: &str) -> Option<JsonRef<'_>> {
        self.view().get(name)
    }
    /// Resolve a relative RFC 6901 pointer; empty selects this value. Malformed or absent paths return `None`; repeated names select the first occurrence.
    pub fn at(&self, pointer: &str) -> Option<JsonRef<'_>> {
        self.view().at(pointer)
    }
    /// Allocate original-source coordinates, including its escaped pointer when representable.
    pub fn location(&self) -> SourceLocation {
        self.view().location()
    }
    /// Create a standalone exact boolean snapshot.
    pub fn boolean(value: bool) -> Self {
        Self::parse(if value { "true" } else { "false" }).expect("constant JSON")
    }
    /// Create a standalone exact JSON null snapshot.
    pub fn null() -> Self {
        Self::parse("null").expect("constant JSON")
    }
    /// Create an escaped JSON string from Unicode scalar text, subject to default input limits.
    pub fn string(value: &str) -> Result<Self, InputError> {
        Self::parse(serde_json::to_vec(value).expect("a Rust string serializes as JSON"))
    }
    /// Create a standalone exact signed 64-bit integer without floating-point conversion.
    pub fn integer(value: i64) -> Self {
        Self::parse(value.to_string()).expect("an integer is JSON")
    }
    /// Duplicate names are retained for inspection, never silently overwritten.
    pub fn has_duplicate_names(&self) -> bool {
        self.owner.has_duplicate_names(self.id)
    }
    /// Exact semantic equality; ambiguous duplicate-member values have no equality verdict.
    pub fn semantic_eq(&self, other: &Self) -> Option<bool> {
        if self.has_duplicate_names() || other.has_duplicate_names() {
            None
        } else {
            Some(equal(self.view(), other.view()))
        }
    }
}
impl<'a> JsonRef<'a> {
    /// Retain the backing arena for this subtree without reparsing. The returned owner remains valid after the parent is dropped and can retain the entire original source.
    pub fn to_owned(self) -> JsonValue {
        JsonValue {
            owner: self.owner.clone(),
            id: self.id,
        }
    }
    /// Borrow the exact token spelling, excluding surrounding whitespace.
    pub fn text(self) -> &'a str {
        self.owner.raw(self.id)
    }
    /// Return the JSON token category without conversion.
    pub fn kind(self) -> JsonKind {
        match self.owner.nodes[self.id].kind {
            Kind::Null => JsonKind::Null,
            Kind::Bool(_) => JsonKind::Boolean,
            Kind::Number => JsonKind::Number,
            Kind::String { .. } => JsonKind::String,
            Kind::Array(_) => JsonKind::Array,
            Kind::Object(_) => JsonKind::Object,
        }
    }
    /// Borrow decoded Unicode scalar text for a string; returns `None` for non-strings or unpaired UTF-16 units. Use [`Self::utf16_units`] for the latter.
    pub fn as_str(self) -> Option<&'a str> {
        self.owner.string(self.id)
    }
    /// Allocate decoded UTF-16 units for a string, preserving unpaired units; returns `None` for non-strings.
    pub fn utf16_units(self) -> Option<Vec<u16>> {
        self.owner.units(self.id)
    }
    /// Return the boolean value, or `None` for any other JSON kind.
    pub fn as_bool(self) -> Option<bool> {
        if let Kind::Bool(b) = self.owner.nodes[self.id].kind {
            Some(b)
        } else {
            None
        }
    }
    /// Borrow the exact numeric token without rounding; returns `None` for non-numbers.
    pub fn number_text(self) -> Option<&'a str> {
        (self.kind() == JsonKind::Number).then(|| self.text())
    }
    /// Borrow the first object member with this decoded name; absent or non-object lookup returns `None`. Use members() to inspect duplicate occurrences.
    pub fn get(self, name: &str) -> Option<Self> {
        self.owner.get(self.id, name).map(|id| Self {
            owner: self.owner,
            id,
        })
    }
    /// Resolve an RFC 6901 pointer relative to this view. Empty means this value; invalid escapes, noncanonical array indices or unavailable targets return `None`; repeated object names select the first occurrence.
    pub fn at(self, pointer: &str) -> Option<Self> {
        if pointer.is_empty() {
            return Some(self);
        }
        let mut current = self;
        for token in pointer.strip_prefix('/')?.split('/') {
            let mut name = String::with_capacity(token.len());
            let mut chars = token.chars();
            while let Some(c) = chars.next() {
                name.push(if c == '~' {
                    match chars.next()? {
                        '0' => '~',
                        '1' => '/',
                        _ => return None,
                    }
                } else {
                    c
                });
            }
            current = match current.kind() {
                JsonKind::Object => current.get(&name)?,
                JsonKind::Array => {
                    if name.is_empty()
                        || (name.len() > 1 && name.starts_with('0'))
                        || !name.bytes().all(|c| c.is_ascii_digit())
                    {
                        return None;
                    }
                    current.element(name.parse().ok()?)?
                }
                _ => return None,
            };
        }
        Some(current)
    }
    /// Return array element or object member count, including repeated object names; return `None` for scalars.
    pub fn len(self) -> Option<usize> {
        match &self.owner.nodes[self.id].kind {
            Kind::Array(v) => Some(v.len()),
            Kind::Object(v) => Some(v.len()),
            _ => None,
        }
    }
    /// Return whether an array/object has zero entries, or `None` for scalars.
    pub fn is_empty(self) -> Option<bool> {
        self.len().map(|n| n == 0)
    }
    /// Borrow a zero-based array element; return `None` for non-arrays or an out-of-range index.
    pub fn element(self, index: usize) -> Option<Self> {
        let Kind::Array(items) = &self.owner.nodes[self.id].kind else {
            return None;
        };
        items.get(index).map(|&id| Self {
            owner: self.owner,
            id,
        })
    }
    /// Iterate borrowed array elements in order; return `None` for non-arrays. The iterator borrows the original arena.
    pub fn elements(self) -> Option<impl ExactSizeIterator<Item = Self> + 'a> {
        let Kind::Array(items) = &self.owner.nodes[self.id].kind else {
            return None;
        };
        Some(items.iter().map(move |&id| Self {
            owner: self.owner,
            id,
        }))
    }
    /// Iterate every borrowed object member in source order, including duplicates; return `None` for non-objects.
    pub fn members(self) -> Option<impl ExactSizeIterator<Item = JsonMember<'a>> + 'a> {
        let Kind::Object(items) = &self.owner.nodes[self.id].kind else {
            return None;
        };
        Some(items.iter().map(move |m| JsonMember {
            name: Self {
                owner: self.owner,
                id: m.key,
            },
            value: Self {
                owner: self.owner,
                id: m.value,
            },
        }))
    }
    /// Allocate coordinates in the complete original source, even when this view is a subtree.
    pub fn location(self) -> SourceLocation {
        let byte_offset = self.owner.nodes[self.id].span.start;
        let (line, byte_column) = self
            .owner
            .position(byte_offset)
            .expect("a node begins on a UTF-8 boundary");
        SourceLocation {
            pointer: self.owner.pointer(self.id),
            byte_offset,
            line,
            byte_column,
        }
    }
}
#[derive(PartialEq, Eq, Hash)]
enum Key<'a> {
    Scalar(&'a str),
    Units(&'a [u16]),
}
fn key(value: JsonRef<'_>) -> Key<'_> {
    if let Some(s) = value.as_str() {
        Key::Scalar(s)
    } else {
        let Kind::String {
            unpaired: Some(units),
            ..
        } = &value.owner.nodes[value.id].kind
        else {
            unreachable!("member name is a string")
        };
        Key::Units(units)
    }
}
pub(crate) fn equal(left: JsonRef<'_>, right: JsonRef<'_>) -> bool {
    let mut stack = vec![(left, right)];
    while let Some((left, right)) = stack.pop() {
        if left.kind() != right.kind() {
            return false;
        }
        match left.kind() {
            JsonKind::Null => {}
            JsonKind::Boolean => {
                if left.as_bool() != right.as_bool() {
                    return false;
                }
            }
            JsonKind::String => {
                if key(left) != key(right) {
                    return false;
                }
            }
            JsonKind::Number => {
                if Decimal::parse(left.text()) != Decimal::parse(right.text()) {
                    return false;
                }
            }
            JsonKind::Array => {
                if left.len() != right.len() {
                    return false;
                }
                stack.extend(left.elements().unwrap().zip(right.elements().unwrap()));
            }
            JsonKind::Object => {
                if left.len() != right.len() {
                    return false;
                }
                let index: HashMap<_, _> = right
                    .members()
                    .unwrap()
                    .map(|m| (key(m.name), m.value))
                    .collect();
                for m in left.members().unwrap() {
                    let Some(value) = index.get(&key(m.name)) else {
                        return false;
                    };
                    stack.push((m.value, *value));
                }
            }
        }
    }
    true
}
impl fmt::Debug for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonValue")
            .field("kind", &self.kind())
            .field("bytes", &self.bytes().len())
            .field("location", &self.location())
            .finish()
    }
}
impl Serialize for JsonValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct ExactText<'a>(&'a str);
        impl Serialize for ExactText<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let raw: &serde_json::value::RawValue =
                    serde_json::from_str(self.0).map_err(serde::ser::Error::custom)?;
                raw.serialize(serializer)
            }
        }
        serializer.serialize_newtype_struct("$openbindings::exact", &ExactText(self.text()))
    }
}
impl<'de> Deserialize<'de> for JsonValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        Self::parse(raw.get()).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_child_and_exact_serde() {
        let parent = JsonValue::parse(r#"{"n":9007199254740993,"v":null}"#).unwrap();
        let child = parent.get("n").unwrap().to_owned();
        drop(parent);
        assert_eq!(child.text(), "9007199254740993");
        assert_eq!(serde_json::to_string(&child).unwrap(), child.text());
    }
    #[test]
    fn exact_deep_equality() {
        for depth in [1, 256, 10000] {
            let a =
                JsonValue::parse(format!("{}1{}", "[".repeat(depth), "]".repeat(depth))).unwrap();
            let b =
                JsonValue::parse(format!("{}1.0{}", "[".repeat(depth), "]".repeat(depth))).unwrap();
            assert_eq!(a.semantic_eq(&b), Some(true));
            assert_eq!(serde_json::to_string(&a).unwrap(), a.text());
        }
    }
    #[test]
    fn members_and_presence() {
        let a = JsonValue::parse(r#"{"x":null,"y":{},"\ud800":1,"\ud800":2}"#).unwrap();
        assert_eq!(a.get("x").unwrap().kind(), JsonKind::Null);
        assert!(a.get("missing").is_none());
        assert_eq!(a.get("y").unwrap().len(), Some(0));
        assert!(a.has_duplicate_names());
        assert_eq!(a.semantic_eq(&a), None);
    }
    #[test]
    fn duplicate_queries_match_container_membership_and_bound_search_work() {
        use std::sync::atomic::Ordering;

        let nested = r#"{"a":{"n":0,"n":1},"a":2,"b":[{}, {"\ud800":0,"\ud800":1}],"c":{"\u0061":0,"a":1},"tail":[]}"#;
        let wide = format!(
            "[{},{{\"clean\":true}}]",
            std::iter::repeat_n(r#"{"a":0,"a":1}"#, 4096)
                .collect::<Vec<_>>()
                .join(",")
        );
        for source in [nested, wide.as_str(), "[]", "null", r#"{"a":1}"#] {
            let root = JsonValue::parse(source).unwrap();
            let arena = &root.owner;
            assert!(arena.duplicates.windows(2).all(|pair| {
                arena.nodes[pair[0].1].span.start < arena.nodes[pair[1].1].span.start
            }));
            for id in 0..arena.nodes.len() {
                let node = &arena.nodes[id];
                // Independent slow oracle: an entire duplicate-containing
                // object must lie within this value. Includes member-name
                // string nodes, which must never claim nested duplicates.
                let expected = arena.duplicates.iter().any(|&(object, _)| {
                    let inner = &arena.nodes[object].span;
                    node.span.start <= inner.start && inner.end <= node.span.end
                });
                arena.duplicate_query_probes.store(0, Ordering::Relaxed);
                let value = JsonValue {
                    owner: arena.clone(),
                    id,
                };
                assert_eq!(value.has_duplicate_names(), expected, "node {id}");
                let probes = arena.duplicate_query_probes.load(Ordering::Relaxed);
                if matches!(node.kind, Kind::Array(_) | Kind::Object(_)) {
                    let bound = if arena.duplicates.is_empty() {
                        0
                    } else {
                        arena.duplicates.len().ilog2() as usize + 2
                    };
                    assert!(probes <= bound, "{probes} probes exceeds {bound}");
                } else {
                    assert_eq!(probes, 0);
                }
            }
        }
    }
    #[test]
    fn limits_are_not_syntax() {
        let e = JsonValue::parse_with_limits(
            "[1]",
            JsonLimits {
                max_depth: 0,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(e.kind, InputErrorKind::Limit);
    }
    #[test]
    fn objects_compare_without_order_or_numeric_spelling() {
        let a = JsonValue::parse(r#"{"a":1,"b":[0e99,"\u0061"]}"#).unwrap();
        let b = JsonValue::parse(r#"{"b":[-0.0,"a"],"a":10e-1}"#).unwrap();
        assert_eq!(a.semantic_eq(&b), Some(true));
    }
}
