//! Exact JSON values and source-backed views.
use crate::raw::{Arena, Id, Kind, Limits};
use jsonschema_value::ob_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashMap, fmt, sync::Arc};

/// Finite input limits. Nesting counts arrays and objects, including the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonLimits {
    pub max_bytes: usize,
    pub max_depth: usize,
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
    InvalidUtf8,
    ByteOrderMark,
    Syntax,
    Limit,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputError {
    pub kind: InputErrorKind,
    pub byte_offset: usize,
    pub code: &'static str,
}
impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.code, self.byte_offset)
    }
}
impl std::error::Error for InputError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonKind {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}
/// Original-source coordinates: zero-based byte offset, one-based line and byte column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceLocation {
    pub pointer: Option<String>,
    pub byte_offset: usize,
    pub line: usize,
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
    pub name: JsonRef<'a>,
    pub value: JsonRef<'a>,
}
impl JsonValue {
    pub fn parse(input: impl AsRef<[u8]>) -> Result<Self, InputError> {
        Self::parse_with_limits(input, JsonLimits::default())
    }
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
    pub fn view(&self) -> JsonRef<'_> {
        JsonRef {
            owner: &self.owner,
            id: self.id,
        }
    }
    pub fn text(&self) -> &str {
        self.owner.raw(self.id)
    }
    /// The original snapshot cannot be mutated through a borrowed byte slice.
    /// ```compile_fail
    /// let value = openbindings_internal_json::JsonValue::parse("7").unwrap();
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
    pub fn kind(&self) -> JsonKind {
        self.view().kind()
    }
    pub fn get(&self, name: &str) -> Option<JsonRef<'_>> {
        self.view().get(name)
    }
    pub fn at(&self, pointer: &str) -> Option<JsonRef<'_>> {
        self.view().at(pointer)
    }
    pub fn location(&self) -> SourceLocation {
        self.view().location()
    }
    pub fn boolean(value: bool) -> Self {
        Self::parse(if value { "true" } else { "false" }).expect("constant JSON")
    }
    pub fn null() -> Self {
        Self::parse("null").expect("constant JSON")
    }
    pub fn string(value: &str) -> Result<Self, InputError> {
        Self::parse(serde_json::to_vec(value).expect("a Rust string serializes as JSON"))
    }
    pub fn integer(value: i64) -> Self {
        Self::parse(value.to_string()).expect("an integer is JSON")
    }
    /// Duplicate names are retained for inspection, never silently overwritten.
    pub fn has_duplicate_names(&self) -> bool {
        self.owner
            .duplicates
            .iter()
            .any(|(object, _)| self.contains(*object))
    }
    fn contains(&self, id: Id) -> bool {
        let outer = &self.owner.nodes[self.id].span;
        let inner = &self.owner.nodes[id].span;
        outer.start <= inner.start && inner.end <= outer.end
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
    pub fn to_owned(self) -> JsonValue {
        JsonValue {
            owner: self.owner.clone(),
            id: self.id,
        }
    }
    pub fn text(self) -> &'a str {
        self.owner.raw(self.id)
    }
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
    pub fn as_str(self) -> Option<&'a str> {
        self.owner.string(self.id)
    }
    pub fn utf16_units(self) -> Option<Vec<u16>> {
        self.owner.units(self.id)
    }
    pub fn as_bool(self) -> Option<bool> {
        if let Kind::Bool(b) = self.owner.nodes[self.id].kind {
            Some(b)
        } else {
            None
        }
    }
    pub fn number_text(self) -> Option<&'a str> {
        (self.kind() == JsonKind::Number).then(|| self.text())
    }
    pub fn get(self, name: &str) -> Option<Self> {
        self.owner.get(self.id, name).map(|id| Self {
            owner: self.owner,
            id,
        })
    }
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
    pub fn len(self) -> Option<usize> {
        match &self.owner.nodes[self.id].kind {
            Kind::Array(v) => Some(v.len()),
            Kind::Object(v) => Some(v.len()),
            _ => None,
        }
    }
    pub fn is_empty(self) -> Option<bool> {
        self.len().map(|n| n == 0)
    }
    pub fn element(self, index: usize) -> Option<Self> {
        let Kind::Array(items) = &self.owner.nodes[self.id].kind else {
            return None;
        };
        items.get(index).map(|&id| Self {
            owner: self.owner,
            id,
        })
    }
    pub fn elements(self) -> Option<impl ExactSizeIterator<Item = Self> + 'a> {
        let Kind::Array(items) = &self.owner.nodes[self.id].kind else {
            return None;
        };
        Some(items.iter().map(move |&id| Self {
            owner: self.owner,
            id,
        }))
    }
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
