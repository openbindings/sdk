//! Exact JSON values and source-backed views.
use crate::Pointer;
use crate::decimal::Decimal;
use crate::raw::{Arena, Id, Kind, Limits};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashMap, fmt, sync::Arc};

/// Finite input limits. Nesting counts arrays and objects, including the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonLimits {
    /// Maximum admitted source bytes; default 32 MiB (33,554,432 bytes).
    pub max_bytes: usize,
    /// Maximum nested arrays/objects, including a container root; default 10,000.
    pub max_depth: usize,
    /// Maximum parsed nodes, including object-name string tokens; default 1,000,000. The internal index also caps this at `u32::MAX`.
    pub max_nodes: usize,
}
impl Default for JsonLimits {
    fn default() -> Self {
        Self {
            max_bytes: 32 * 1024 * 1024,
            max_depth: 10_000,
            max_nodes: 1_000_000,
        }
    }
}
/// Why an exact input could not be represented. Later versions may add kinds,
/// so a `match` needs a fallback arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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

#[derive(Clone, PartialEq, Eq)]
/// Exact JSON admission failure. It is separate from document conformance and schema-instance outcomes.
/// `Display` writes what the parser expected or found and the byte where, such as
/// `expected a colon after the member name at byte 5`; branch on `code` and
/// `byte_offset`, which do not change with that wording.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct InputError {
    /// Broad admission failure category.
    pub kind: InputErrorKind,
    /// Zero-based original-input byte offset; an end-of-input failure may equal the input length.
    pub byte_offset: usize,
    /// Stable detailed parse/admission code, suitable for branching.
    pub code: &'static str,
}
impl std::fmt::Debug for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Only fixed parser/admission reasons are safe; the public code can be reassigned.
        let code = match self.code {
            "bom"
            | "byte-limit"
            | "colon"
            | "comma-or-close"
            | "depth-limit"
            | "escape"
            | "exponent"
            | "fraction"
            | "literal"
            | "node-limit"
            | "number"
            | "object-key"
            | "string-control"
            | "trailing-token"
            | "unicode-escape"
            | "unterminated-string"
            | "utf8"
            | "value" => self.code,
            _ => "<redacted>",
        };
        f.debug_struct("InputError")
            .field("code", &code)
            .field("kind", &self.kind)
            .field("byte_offset", &self.byte_offset)
            .finish_non_exhaustive()
    }
}
/// What a parser or admission code reports, as a fixed phrase; `None` for a
/// code the parser and admission never emit.
pub(crate) fn input_error_description(code: &str) -> Option<&'static str> {
    Some(match code {
        "bom" => "JSON text must not begin with a byte-order mark",
        "utf8" => "JSON text must use valid UTF-8",
        "comma-or-close" => "expected a comma or the end of the container",
        "object-key" => "expected a quoted object member name",
        "colon" => "expected a colon after the member name",
        "trailing-token" => "unexpected content after the JSON value",
        "number" => "invalid JSON number",
        "fraction" => "expected a digit after the decimal point",
        "exponent" => "expected a digit in the number exponent",
        "value" => "expected a JSON value",
        "literal" => "expected true, false or null",
        "unterminated-string" => "expected the closing quote of the string",
        "string-control" => "control characters in strings must be escaped",
        "escape" => "invalid JSON string escape",
        "unicode-escape" => "expected four hexadecimal digits in the Unicode escape",
        "byte-limit" => "the input exceeds the admitted byte limit",
        "depth-limit" => "the input nests deeper than the admitted depth limit",
        "node-limit" => "the input holds more nodes than the admitted node limit",
        _ => return None,
    })
}
impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match input_error_description(self.code) {
            Some(description) => write!(f, "{description} at byte {}", self.byte_offset),
            None => write!(f, "{} at byte {}", self.code, self.byte_offset),
        }
    }
}
impl std::error::Error for InputError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Exact JSON token category, without coercion or interpretation.
///
/// JSON has exactly these six kinds, so this closed partition may be
/// exhaustively matched:
/// ```
/// # use openbindings_internal_json as openbindings;
/// use openbindings::JsonKind;
/// fn container(kind: JsonKind) -> bool {
///     match kind {
///         JsonKind::Array | JsonKind::Object => true,
///         JsonKind::Null | JsonKind::Boolean | JsonKind::Number | JsonKind::String => false,
///     }
/// }
/// ```
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
///
/// `Display` writes `pointer (line L, column C, byte B)`, or `line L, column C,
/// byte B` when there is no pointer or it is a root's empty pointer. The column
/// counts UTF-8 bytes. Pointers are source-controlled text: quote them for display.
///
/// Serialization writes JSON text. A pointer through a member name with an
/// unpaired UTF-16 unit is written with an escape that no `serde_json::Value`
/// can hold, so `serde_json::to_value` refuses such a location (`json!` panics);
/// serialize with `serde_json::to_string` or `to_writer`.
///
/// Later versions may add fields: read them by name and match with `..`.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SourceLocation {
    /// Exact RFC 6901 pointer: from the value, document or resource the location was asked of (`locate`), or from the source's root (findings, [`JsonRef::location`]); an empty pointer means that root. `None` only when the location precedes any value, as a syntax error does. Duplicate member occurrences can share a pointer, so use byte coordinates to distinguish them.
    pub pointer: Option<Pointer>,
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
impl std::fmt::Debug for JsonRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonRef")
            .field("kind", &self.kind())
            .field("bytes", &self.text().len())
            .finish_non_exhaustive()
    }
}

/// An exact borrowed JSON string. Equality compares decoded UTF-16 units;
/// scalar conversion is optional and never replaces unpaired units.
#[derive(Clone, Copy)]
pub struct JsonStringRef<'a> {
    json: JsonRef<'a>,
}
impl fmt::Debug for JsonStringRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("JsonStringRef").field(&self.json).finish()
    }
}
impl<'a> JsonStringRef<'a> {
    /// Borrow Unicode scalar text, or None when the string contains an unpaired unit.
    pub fn as_str(self) -> Option<&'a str> {
        self.json.as_str()
    }
    /// Iterate decoded UTF-16 units without allocating, preserving unpaired units.
    pub fn utf16_units(self) -> impl Iterator<Item = u16> + 'a {
        let unpaired = self.json.owner.unpaired_units(self.json.id);
        self.as_str()
            .into_iter()
            .flat_map(str::encode_utf16)
            .chain(unpaired.into_iter().flatten())
    }
    /// Borrow the exact authored token and its original source location.
    pub fn json(self) -> JsonRef<'a> {
        self.json
    }
    /// Compare decoded units with Unicode scalar text, independent of escape spelling.
    pub fn eq_str(self, scalar: &str) -> bool {
        self.utf16_units().eq(scalar.encode_utf16())
    }
}
impl PartialEq for JsonStringRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.utf16_units().eq(other.utf16_units())
    }
}
impl Eq for JsonStringRef<'_> {}
impl PartialEq<&str> for JsonStringRef<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.eq_str(other)
    }
}
/// An object member; names are exact JSON strings and can preserve lone UTF-16 units.
///
/// [`JsonRef::members`] yields these. A JSON member is exactly a name and a
/// value, so, unlike the SDK's other output records, this one stays exhaustive:
/// a pattern may name both fields without `..`.
/// ```
/// # use openbindings_internal_json as openbindings;
/// use openbindings::{JsonMember, JsonValue};
/// let object = JsonValue::parse(r#"{"id":7}"#).unwrap();
/// for JsonMember { name, value } in object.view().members().unwrap() {
///     assert_eq!((name.text(), value.text()), (r#""id""#, "7"));
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct JsonMember<'a> {
    /// Exact original member-name string, including its token coordinates.
    pub name: JsonRef<'a>,
    /// Borrowed exact member value in source order.
    pub value: JsonRef<'a>,
}
impl JsonValue {
    /// Parse UTF-8 JSON under [`JsonLimits::default`], copying source into an immutable arena. Preserves number spelling, duplicate members and escaped unpaired UTF-16 units; no normative validation is performed.
    ///
    /// # Errors
    /// Returns an input error for invalid UTF-8, a byte order mark, invalid JSON syntax, or an exceeded admission limit.
    pub fn parse(input: impl AsRef<[u8]>) -> Result<Self, InputError> {
        Self::parse_with_limits(input, JsonLimits::default())
    }
    /// Parse with explicit finite admission limits. Refuses invalid UTF-8, a byte order mark, invalid syntax or exhausted limits; does not coerce or discard JSON data.
    ///
    /// # Errors
    /// Returns an input error for invalid UTF-8, a byte order mark, invalid JSON syntax, or an exceeded admission limit.
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
        Ok(Self { owner, id: 0 })
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
    /// Allocate original-source coordinates, including its exact escaped pointer.
    pub fn location(&self) -> SourceLocation {
        self.view().location()
    }
    /// Resolve an exact pointer relative to this value, as [`Self::at`] resolves
    /// text; a segment holding an unpaired UTF-16 unit names exactly its member.
    pub fn at_pointer(&self, pointer: &Pointer) -> Option<JsonRef<'_>> {
        self.view().at_pointer(pointer)
    }
    /// Locate an exact pointer relative to this value, such as a problem's
    /// instance: see [`JsonRef::locate`].
    pub fn locate(&self, pointer: &Pointer) -> Option<SourceLocation> {
        self.view().locate(pointer)
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
    ///
    /// # Errors
    /// Returns an input error if the encoded string exceeds the default JSON admission limits.
    pub fn string(value: &str) -> Result<Self, InputError> {
        Self::string_with_limits(value, JsonLimits::default())
    }
    fn string_with_limits(value: &str, limits: JsonLimits) -> Result<Self, InputError> {
        // Count with the same serializer before allocating any encoded output.
        // A string is immutable and its serialization cannot fail except when
        // this writer stops at the byte limit. This also preserves serde_json's
        // exact escape spelling for accepted strings.
        struct EncodedSize {
            bytes: usize,
            max: usize,
        }
        impl std::io::Write for EncodedSize {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() > self.max - self.bytes {
                    return Err(std::io::ErrorKind::FileTooLarge.into());
                }
                self.bytes += bytes.len();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut size = EncodedSize {
            bytes: 0,
            max: limits.max_bytes,
        };
        serde_json::to_writer(&mut size, value).map_err(|_| InputError {
            kind: InputErrorKind::Limit,
            byte_offset: limits.max_bytes,
            code: "byte-limit",
        })?;
        let mut bytes = Vec::with_capacity(size.bytes);
        serde_json::to_writer(&mut bytes, value).expect("a Rust string serializes as JSON");
        Self::parse_with_limits(bytes, limits)
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
            Kind::Array { .. } => JsonKind::Array,
            Kind::Object { .. } => JsonKind::Object,
        }
    }
    /// Borrow decoded Unicode scalar text for a string; returns `None` for non-strings or unpaired UTF-16 units. Use [`Self::utf16_units`] for the latter.
    pub fn as_str(self) -> Option<&'a str> {
        self.owner.string(self.id)
    }
    /// Borrow an exact string, including unpaired UTF-16; None means a non-string.
    pub fn as_string(self) -> Option<JsonStringRef<'a>> {
        (self.kind() == JsonKind::String).then_some(JsonStringRef { json: self })
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
        self.owner.at_from(self.id, pointer).map(|id| Self {
            owner: self.owner,
            id,
        })
    }
    /// Return array element or object member count, including repeated object names; return `None` for scalars.
    pub fn len(self) -> Option<usize> {
        match self.owner.nodes[self.id].kind {
            Kind::Array { len, .. } | Kind::Object { len, .. } => Some(len as usize),
            _ => None,
        }
    }
    /// Return whether an array/object has zero entries, or `None` for scalars.
    pub fn is_empty(self) -> Option<bool> {
        self.len().map(|n| n == 0)
    }
    /// Borrow a zero-based array element; return `None` for non-arrays or an out-of-range index.
    pub fn element(self, index: usize) -> Option<Self> {
        self.owner.elements(self.id)?.get(index).map(|&id| Self {
            owner: self.owner,
            id: id as usize,
        })
    }
    /// Iterate borrowed array elements in order; return `None` for non-arrays. The iterator borrows the original arena.
    pub fn elements(self) -> Option<impl ExactSizeIterator<Item = Self> + 'a> {
        let items = self.owner.elements(self.id)?;
        Some(items.iter().map(move |&id| Self {
            owner: self.owner,
            id: id as usize,
        }))
    }
    /// Iterate every borrowed object member in source order, including duplicates; return `None` for non-objects.
    pub fn members(self) -> Option<impl ExactSizeIterator<Item = JsonMember<'a>> + 'a> {
        let names = self.owner.member_names(self.id)?;
        // A member's value is the node after its name.
        Some(names.iter().map(move |&name| JsonMember {
            name: Self {
                owner: self.owner,
                id: name as usize,
            },
            value: Self {
                owner: self.owner,
                id: name as usize + 1,
            },
        }))
    }
    /// Resolve an exact pointer relative to this view, as [`Self::at`] resolves
    /// text; a segment holding an unpaired UTF-16 unit names exactly its member.
    /// Repeated object names select the first occurrence.
    pub fn at_pointer(self, pointer: &Pointer) -> Option<Self> {
        if let Some(text) = pointer.as_str() {
            return self.at(text);
        }
        let mut current = self;
        for segment in pointer.unit_segments() {
            current = match current.kind() {
                JsonKind::Object => {
                    current
                        .members()?
                        .find(|member| {
                            member
                                .name
                                .as_string()
                                .is_some_and(|name| name.utf16_units().eq(segment.iter().copied()))
                        })?
                        .value
                }
                JsonKind::Array => {
                    let index = String::from_utf16(&segment).ok()?;
                    if index.is_empty()
                        || (index.len() > 1 && index.starts_with('0'))
                        || !index.bytes().all(|b| b.is_ascii_digit())
                    {
                        return None;
                    }
                    current.element(index.parse().ok()?)?
                }
                _ => return None,
            };
        }
        Some(current)
    }
    /// Locate an exact pointer relative to this view, such as a value problem's
    /// instance, in the text the view was parsed from: the node's byte offset,
    /// line and column, found by one pass over the bytes before it, with the
    /// given pointer. Nothing is computed until asked, and nothing is retained.
    /// `None` when the pointer names no node.
    pub fn locate(self, pointer: &Pointer) -> Option<SourceLocation> {
        let node = self.at_pointer(pointer)?;
        let byte_offset = node.owner.nodes[node.id].span.start;
        let (line, byte_column) = node.owner.line_and_column(byte_offset);
        Some(SourceLocation {
            pointer: Some(pointer.clone()),
            byte_offset,
            line,
            byte_column,
        })
    }
    /// Allocate coordinates in the complete original source, even when this view
    /// is a subtree: its exact pointer from that source's root, with line and
    /// column found from the byte offset. A value built rather than parsed is
    /// located in its exact serialization.
    pub fn location(self) -> SourceLocation {
        let byte_offset = self.owner.nodes[self.id].span.start;
        let (line, byte_column) = self
            .owner
            .position(byte_offset)
            .expect("a node begins on a UTF-8 boundary");
        SourceLocation {
            pointer: Some(self.owner.exact_pointer(self.id)),
            byte_offset,
            line,
            byte_column,
        }
    }
}
/// A string's exact text: its scalar text, or, for one holding an unpaired
/// unit, its UTF-16 units read from its token, compared and hashed as read.
enum Key<'a> {
    Scalar(&'a str),
    Units(crate::raw::Utf16<'a>),
}
impl PartialEq for Key<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Key::Scalar(a), Key::Scalar(b)) => a == b,
            (Key::Units(a), Key::Units(b)) => a.clone().eq(b.clone()),
            _ => false,
        }
    }
}
impl Eq for Key<'_> {}
impl std::hash::Hash for Key<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Key::Scalar(text) => text.hash(state),
            Key::Units(units) => {
                for unit in units.clone() {
                    state.write_u16(unit);
                }
                // Units end where no scalar text's hash does.
                state.write_u8(0xff);
            }
        }
    }
}
fn key(value: JsonRef<'_>) -> Key<'_> {
    if let Some(s) = value.as_str() {
        Key::Scalar(s)
    } else {
        Key::Units(
            value
                .owner
                .unpaired_units(value.id)
                .expect("member name is a string"),
        )
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

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (line, column, byte) = (self.line, self.byte_column, self.byte_offset);
        match self.pointer.as_ref().filter(|pointer| !pointer.is_root()) {
            Some(pointer) => write!(f, "{pointer} (line {line}, column {column}, byte {byte})"),
            None => write!(f, "line {line}, column {column}, byte {byte}"),
        }
    }
}
impl std::fmt::Debug for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceLocation")
            .field("byte_offset", &self.byte_offset)
            .field("line", &self.line)
            .field("byte_column", &self.byte_column)
            .finish_non_exhaustive()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn string_encoding_and_admission_match_serialized_input() {
        let controls: String = (0..=0x7f).map(char::from).collect();
        for value in ["", "plain text", "\"\\\n\r\t\u{8}\u{c}", "é𝄞", &controls] {
            let encoded = serde_json::to_vec(value).unwrap();
            for max_bytes in 0..=encoded.len() + 1 {
                let limits = JsonLimits {
                    max_bytes,
                    ..JsonLimits::default()
                };
                let expected = JsonValue::parse_with_limits(&encoded, limits);
                let actual = JsonValue::string_with_limits(value, limits);
                assert_eq!(
                    actual.as_ref().map(JsonValue::text),
                    expected.as_ref().map(JsonValue::text)
                );
                if let Ok(actual) = actual {
                    assert_eq!(actual.view().as_str(), Some(value));
                }
            }
        }
    }
    #[test]
    fn input_errors_display_what_was_expected_and_where() {
        let error = JsonValue::parse(r#"{"a" "#).unwrap_err();
        assert_eq!(
            (error.code, error.byte_offset, error.to_string()),
            (
                "colon",
                5,
                "expected a colon after the member name at byte 5".to_owned()
            )
        );
        // Every code the parser and admission emit, from real input.
        let limited = |limits| JsonValue::parse_with_limits("[0]", limits).unwrap_err();
        let errors = [
            JsonValue::parse(b"\xff").unwrap_err(),
            JsonValue::parse(b"\xef\xbb\xbf{}").unwrap_err(),
            JsonValue::parse("[0 1]").unwrap_err(),
            JsonValue::parse("{1:0}").unwrap_err(),
            error,
            JsonValue::parse("0 0").unwrap_err(),
            JsonValue::parse("-x").unwrap_err(),
            JsonValue::parse("1.").unwrap_err(),
            JsonValue::parse("1e").unwrap_err(),
            JsonValue::parse("]").unwrap_err(),
            JsonValue::parse("nulx").unwrap_err(),
            JsonValue::parse("\"").unwrap_err(),
            JsonValue::parse("\"\u{0}\"").unwrap_err(),
            JsonValue::parse(r#""\q""#).unwrap_err(),
            JsonValue::parse(r#""\uGGGG""#).unwrap_err(),
            limited(JsonLimits {
                max_bytes: 0,
                ..JsonLimits::default()
            }),
            limited(JsonLimits {
                max_depth: 0,
                ..JsonLimits::default()
            }),
            limited(JsonLimits {
                max_nodes: 0,
                ..JsonLimits::default()
            }),
        ];
        let mut codes = std::collections::BTreeSet::new();
        for error in errors {
            let description = input_error_description(error.code).expect(error.code);
            assert_eq!(
                error.to_string(),
                format!("{description} at byte {}", error.byte_offset)
            );
            codes.insert(error.code);
        }
        assert_eq!(codes.len(), 18);
        // A code the parser does not emit is written as itself.
        let mut renamed = JsonValue::parse("]").unwrap_err();
        renamed.code = "application-code";
        assert_eq!(renamed.to_string(), "application-code at byte 0");
    }
    #[test]
    fn locations_display_their_pointer_and_coordinates() {
        let value = JsonValue::parse("{\n \"a/b\": [true],\n \"\\ud800\": 0}").unwrap();
        let element = value.at("/a~1b/0").unwrap().location();
        assert_eq!(element.to_string(), "/a~1b/0 (line 2, column 10, byte 11)");
        // The root's empty pointer is not written, as for a location with none.
        assert_eq!(value.location().to_string(), "line 1, column 1, byte 0");
        let syntax = SourceLocation {
            pointer: None,
            ..value.location()
        };
        assert_eq!(syntax.to_string(), "line 1, column 1, byte 0");
        let unpaired = value.view().members().unwrap().nth(1).unwrap().value;
        assert_eq!(
            unpaired.location().to_string(),
            "/\\ud800 (line 3, column 12, byte 29)"
        );
        assert_eq!(
            serde_json::to_string(&unpaired.location()).unwrap(),
            r#"{"pointer":"/\ud800","byte_offset":29,"line":3,"byte_column":12}"#
        );
    }
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
                arena.nodes[pair[0].1 as usize].span.start
                    < arena.nodes[pair[1].1 as usize].span.start
            }));
            for id in 0..arena.nodes.len() {
                let node = &arena.nodes[id];
                // Independent slow oracle: an entire duplicate-containing
                // object must lie within this value. Includes member-name
                // string nodes, which must never claim nested duplicates.
                let expected = arena.duplicates.iter().any(|&(object, _)| {
                    let inner = &arena.nodes[object as usize].span;
                    node.span.start <= inner.start && inner.end <= node.span.end
                });
                arena.duplicate_query_probes.store(0, Ordering::Relaxed);
                let value = JsonValue {
                    owner: arena.clone(),
                    id,
                };
                assert_eq!(value.has_duplicate_names(), expected, "node {id}");
                let probes = arena.duplicate_query_probes.load(Ordering::Relaxed);
                if node.is_container() {
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
