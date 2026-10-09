//! Checked, bounded admission from the Serde data model into exact JSON.
use crate::{JsonKind, JsonLimits, JsonValue};
use serde::{
    Serialize, Serializer,
    ser::{self, SerializeMap, SerializeSeq},
};
use std::{collections::HashSet, fmt};
const DIAGNOSTIC_LIMIT: usize = 4096;
const SERDE_MAX_DEPTH: usize = 128;
// Qualified against the pinned serde_json arbitrary_precision Number protocol.
const NUMBER_TOKEN: &str = "$serde_json::private::Number";

/// Stable reasons why a Serde value could not enter the JSON value model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueConversionErrorKind {
    NonFiniteNumber,
    NonStringKey,
    DuplicateKey,
    UnsupportedRepresentation,
    Limit,
    Serialization,
}
/// An owned diagnostic. Custom serializer text is available only through `message`.
#[derive(Clone, PartialEq, Eq)]
pub struct ValueConversionError {
    kind: ValueConversionErrorKind,
    pointer: Option<String>,
    message: String,
    message_truncated: bool,
    path_omitted_for_limit: bool,
}
impl ValueConversionError {
    pub fn kind(&self) -> ValueConversionErrorKind {
        self.kind
    }
    /// Emitted JSON pointer; `Some("")` is the root. Never a draft-field path.
    pub fn pointer(&self) -> Option<&str> {
        self.pointer.as_deref()
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn message_truncated(&self) -> bool {
        self.message_truncated
    }
    pub fn path_omitted_for_limit(&self) -> bool {
        self.path_omitted_for_limit
    }
    fn new(kind: ValueConversionErrorKind, message: &str) -> Self {
        Self {
            kind,
            pointer: None,
            message: message.into(),
            message_truncated: false,
            path_omitted_for_limit: false,
        }
    }
}
impl fmt::Debug for ValueConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValueConversionError")
            .field("kind", &self.kind)
            .field("message_truncated", &self.message_truncated)
            .field("path_omitted_for_limit", &self.path_omitted_for_limit)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for ValueConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON value conversion failed ({:?})", self.kind)
    }
}
impl std::error::Error for ValueConversionError {}
struct BoundedMessage {
    text: String,
    limit: usize,
    truncated: bool,
}
impl fmt::Write for BoundedMessage {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.truncated {
            return Err(fmt::Error);
        }
        let remaining = self.limit - self.text.len();
        if value.len() <= remaining {
            self.text.push_str(value);
            return Ok(());
        }
        let mut end = remaining;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&value[..end]);
        self.truncated = true;
        Err(fmt::Error)
    }
}
fn capture(
    value: &(impl fmt::Display + ?Sized),
    limit: usize,
) -> Result<String, ValueConversionError> {
    use fmt::Write;
    let mut output = BoundedMessage {
        text: String::new(),
        limit,
        truncated: false,
    };
    let result = write!(&mut output, "{value}");
    if output.truncated {
        return Err(ValueConversionError::new(
            ValueConversionErrorKind::Limit,
            "formatted string exceeds byte limit",
        ));
    }
    result.map_err(|_| {
        ValueConversionError::new(ValueConversionErrorKind::Serialization, "formatter failed")
    })?;
    Ok(output.text)
}
impl ser::Error for ValueConversionError {
    fn custom<T: fmt::Display>(value: T) -> Self {
        use fmt::Write;
        let mut message = BoundedMessage {
            text: String::new(),
            limit: DIAGNOSTIC_LIMIT,
            truncated: false,
        };
        let _ = write!(&mut message, "{value}");
        Self {
            kind: ValueConversionErrorKind::Serialization,
            pointer: None,
            message: message.text,
            message_truncated: message.truncated,
            path_omitted_for_limit: false,
        }
    }
}
struct State {
    bytes: Vec<u8>,
    limits: JsonLimits,
    nodes: usize,
    depth: usize,
    path: Option<String>,
    first: Option<ValueConversionError>,
}
impl State {
    fn guard(&self) -> Result<(), ValueConversionError> {
        match &self.first {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
    fn latch(&mut self, error: ValueConversionError) -> ValueConversionError {
        let error = self.locate(error);
        self.first.get_or_insert(error).clone()
    }
    fn locate(&self, mut error: ValueConversionError) -> ValueConversionError {
        if error.pointer.is_none() && !error.path_omitted_for_limit {
            error.pointer = self.path.clone();
            error.path_omitted_for_limit = self.path.is_none();
        }
        error
    }
    fn fail(&mut self, kind: ValueConversionErrorKind, message: &str) -> ValueConversionError {
        self.latch(ValueConversionError::new(kind, message))
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), ValueConversionError> {
        self.guard()?;
        if bytes.len() > self.limits.max_bytes.saturating_sub(self.bytes.len()) {
            return Err(self.fail(ValueConversionErrorKind::Limit, "byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn node(&mut self) -> Result<(), ValueConversionError> {
        self.guard()?;
        if self.nodes >= self.limits.max_nodes {
            return Err(self.fail(ValueConversionErrorKind::Limit, "node limit exceeded"));
        }
        self.nodes += 1;
        Ok(())
    }
    fn string(&mut self, value: &str) -> Result<(), ValueConversionError> {
        self.write(b"\"")?;
        for ch in value.chars() {
            match ch {
                '"' => self.write(b"\\\"")?,
                '\\' => self.write(b"\\\\")?,
                ch if ch <= '\u{1f}' => {
                    let n = ch as u8;
                    let hex = b"0123456789abcdef";
                    self.write(&[
                        b'\\',
                        b'u',
                        b'0',
                        b'0',
                        hex[(n >> 4) as usize],
                        hex[(n & 15) as usize],
                    ])?;
                }
                ch => {
                    let mut buf = [0; 4];
                    self.write(ch.encode_utf8(&mut buf).as_bytes())?;
                }
            }
        }
        self.write(b"\"")
    }
    fn child<T: Serialize + ?Sized>(
        &mut self,
        key: &str,
        value: &T,
    ) -> Result<(), ValueConversionError> {
        self.guard()?;
        let parent = self.path.take();
        self.path = parent.as_deref().and_then(|p| pointer_child(p, key));
        let result = value.serialize(&mut *self).map_err(|e| self.locate(e));
        if let Err(e) = &result {
            self.first.get_or_insert_with(|| e.clone());
        }
        self.path = parent;
        result
    }
    fn begin(&mut self, object: bool) -> Result<(), ValueConversionError> {
        self.node()?;
        if self.depth >= self.limits.max_depth {
            return Err(self.fail(
                ValueConversionErrorKind::Limit,
                "container depth exceeds configured limit or checked Serde profile ceiling (128)",
            ));
        }
        self.depth += 1;
        self.write(if object { b"{" } else { b"[" })
    }
}
fn pointer_child(parent: &str, key: &str) -> Option<String> {
    let mut result = String::with_capacity(
        parent
            .len()
            .saturating_add(key.len())
            .saturating_add(1)
            .min(DIAGNOSTIC_LIMIT),
    );
    result.push_str(parent);
    if result.len() == DIAGNOSTIC_LIMIT {
        return None;
    }
    result.push('/');
    for ch in key.chars() {
        let mut buffer = [0; 4];
        let encoded = match ch {
            '~' => "~0",
            '/' => "~1",
            _ => ch.encode_utf8(&mut buffer),
        };
        if encoded.len() > DIAGNOSTIC_LIMIT - result.len() {
            return None;
        }
        result.push_str(encoded);
    }
    Some(result)
}
impl JsonValue {
    /// Checked Serde profile: finite numbers (including exact i128/u128), string
    /// map keys, unique members, standard containers/enums, and bytes as arrays.
    /// Pinned `serde_json::Number` tokens preserve their exact spelling.
    /// Exact `JsonValue`/Serde RawValue wrappers are refused; retain or parse exact
    /// values directly. Limits bound emitted storage, not user serializer work.
    /// This recursive Serde profile has a ceiling of 128 containers, independent
    /// of the larger iterative exact-parser depth limit.
    pub fn from_serializable<T: Serialize + ?Sized>(
        value: &T,
    ) -> Result<Self, ValueConversionError> {
        Self::from_serializable_with_limits(value, JsonLimits::default())
    }
    /// Checked conversion with explicit byte/node/depth bounds. Effective container
    /// depth is `min(limits.max_depth, 128)`; larger requested depths do not lift
    /// the recursive Serde profile ceiling. Exact parsing is unaffected.
    pub fn from_serializable_with_limits<T: Serialize + ?Sized>(
        value: &T,
        mut limits: JsonLimits,
    ) -> Result<Self, ValueConversionError> {
        limits.max_depth = limits.max_depth.min(SERDE_MAX_DEPTH);
        let mut state = State {
            bytes: Vec::new(),
            limits,
            nodes: 0,
            depth: 0,
            path: Some(String::new()),
            first: None,
        };
        let result = value.serialize(&mut state).map_err(|e| state.locate(e));
        if let Some(error) = state.first {
            return Err(error);
        }
        result?;
        Self::parse_with_limits(&state.bytes, limits).map_err(|e| {
            state.locate(ValueConversionError::new(
                ValueConversionErrorKind::Serialization,
                e.code,
            ))
        })
    }
}
struct Compound<'a> {
    state: &'a mut State,
    object: bool,
    number_token: bool,
    index: usize,
    key: Option<String>,
    seen: HashSet<String>,
    variant_parent: Option<Option<String>>,
}
impl Compound<'_> {
    fn prefix(&mut self) -> Result<(), ValueConversionError> {
        self.state.guard()?;
        if self.index > 0 {
            self.state.write(b",")?;
        }
        Ok(())
    }
    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ValueConversionError> {
        self.prefix()?;
        self.state.child(&self.index.to_string(), value)?;
        self.index += 1;
        Ok(())
    }
    fn key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), ValueConversionError> {
        self.state.guard()?;
        if self.key.is_some() {
            return Err(self
                .state
                .fail(ValueConversionErrorKind::Serialization, "map value missing"));
        }
        let key = key
            .serialize(KeySerializer {
                string_only: false,
                limit: self
                    .state
                    .limits
                    .max_bytes
                    .saturating_sub(self.state.bytes.len()),
            })
            .map_err(|e| {
                let e = self.state.locate(e);
                self.state.first.get_or_insert_with(|| e.clone());
                e
            })?;
        if self.seen.contains(&key) {
            let parent = self.state.path.take();
            self.state.path = parent.as_deref().and_then(|p| pointer_child(p, &key));
            let e = self.state.fail(
                ValueConversionErrorKind::DuplicateKey,
                "duplicate emitted member",
            );
            self.state.path = parent;
            return Err(e);
        }
        self.prefix()?;
        self.state.node()?;
        self.state.string(&key)?;
        self.state.write(b":")?;
        self.seen.insert(key.clone());
        self.key = Some(key);
        Ok(())
    }
    fn value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ValueConversionError> {
        self.state.guard()?;
        let key = self.key.take().ok_or_else(|| {
            self.state
                .fail(ValueConversionErrorKind::Serialization, "map key missing")
        })?;
        self.state.child(&key, value)?;
        self.index += 1;
        Ok(())
    }
    fn number<T: Serialize + ?Sized>(
        &mut self,
        key: &str,
        value: &T,
    ) -> Result<(), ValueConversionError> {
        self.state.guard()?;
        if key != NUMBER_TOKEN || self.index != 0 {
            return Err(self.state.fail(
                ValueConversionErrorKind::UnsupportedRepresentation,
                "invalid Number serialization protocol",
            ));
        }
        let remaining = self
            .state
            .limits
            .max_bytes
            .saturating_sub(self.state.bytes.len());
        let token = value
            .serialize(KeySerializer {
                limit: remaining,
                string_only: true,
            })
            .map_err(|e| {
                if e.kind() == ValueConversionErrorKind::NonStringKey {
                    self.state.fail(
                        ValueConversionErrorKind::UnsupportedRepresentation,
                        "Number protocol requires a string token",
                    )
                } else {
                    self.state.latch(e)
                }
            })?;
        // Validate in isolation: a forged token cannot inject another value or a
        // private protocol path. Preserve the caller's current emitted location.
        let number = JsonValue::parse_with_limits(
            &token,
            JsonLimits {
                max_bytes: remaining,
                max_nodes: 1,
                max_depth: 0,
            },
        )
        .map_err(|_| {
            self.state.fail(
                ValueConversionErrorKind::UnsupportedRepresentation,
                "Number protocol requires one complete JSON number",
            )
        })?;
        if number.kind() != JsonKind::Number || number.text() != token {
            return Err(self.state.fail(
                ValueConversionErrorKind::UnsupportedRepresentation,
                "Number protocol requires one complete JSON number",
            ));
        }
        self.state.write(token.as_bytes())?;
        self.index = 1;
        Ok(())
    }
    fn finish(self) -> Result<(), ValueConversionError> {
        self.state.guard()?;
        if self.number_token {
            return if self.index == 1 {
                Ok(())
            } else {
                Err(self.state.fail(
                    ValueConversionErrorKind::UnsupportedRepresentation,
                    "Number protocol token missing",
                ))
            };
        }
        if self.key.is_some() {
            return Err(self
                .state
                .fail(ValueConversionErrorKind::Serialization, "map value missing"));
        }
        self.state.write(if self.object { b"}" } else { b"]" })?;
        self.state.depth -= 1;
        if let Some(parent) = self.variant_parent {
            self.state.write(b"}")?;
            self.state.depth -= 1;
            self.state.path = parent;
        }
        Ok(())
    }
}
fn compound(state: &mut State, object: bool) -> Result<Compound<'_>, ValueConversionError> {
    state.begin(object)?;
    Ok(Compound {
        state,
        object,
        number_token: false,
        index: 0,
        key: None,
        seen: HashSet::new(),
        variant_parent: None,
    })
}
macro_rules! integer { ($($name:ident:$ty:ty),*)=>{$(fn $name(self,v:$ty)->Result<(),Self::Error>{self.node()?;self.write(v.to_string().as_bytes())})*}; }
impl<'a> Serializer for &'a mut State {
    type Ok = ();
    type Error = ValueConversionError;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;
    integer!(serialize_i8:i8,serialize_i16:i16,serialize_i32:i32,serialize_i64:i64,serialize_i128:i128,serialize_u8:u8,serialize_u16:u16,serialize_u32:u32,serialize_u64:u64,serialize_u128:u128);
    fn serialize_bool(self, v: bool) -> Result<(), Self::Error> {
        self.node()?;
        self.write(if v { b"true" } else { b"false" })
    }
    fn serialize_f32(self, v: f32) -> Result<(), Self::Error> {
        if !v.is_finite() {
            return Err(self.fail(
                ValueConversionErrorKind::NonFiniteNumber,
                "non-finite number",
            ));
        }
        self.node()?;
        self.write(serde_json::to_string(&v).expect("finite float").as_bytes())
    }
    fn serialize_f64(self, v: f64) -> Result<(), Self::Error> {
        if !v.is_finite() {
            return Err(self.fail(
                ValueConversionErrorKind::NonFiniteNumber,
                "non-finite number",
            ));
        }
        self.node()?;
        self.write(serde_json::to_string(&v).expect("finite float").as_bytes())
    }
    fn serialize_char(self, v: char) -> Result<(), Self::Error> {
        self.serialize_str(v.encode_utf8(&mut [0; 4]))
    }
    fn serialize_str(self, v: &str) -> Result<(), Self::Error> {
        self.node()?;
        self.string(v)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), Self::Error> {
        let mut seq = self.serialize_seq(Some(v.len()))?;
        for n in v {
            seq.serialize_element(n)?;
        }
        SerializeSeq::end(seq)
    }
    fn serialize_none(self) -> Result<(), Self::Error> {
        self.serialize_unit()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), Self::Error> {
        self.guard()?;
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Self::Error> {
        self.node()?;
        self.write(b"null")
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Self::Error> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        v: &'static str,
    ) -> Result<(), Self::Error> {
        self.serialize_str(v)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        if name == "$openbindings::exact" {
            return Err(self.fail(
                ValueConversionErrorKind::UnsupportedRepresentation,
                "exact values use the exact JSON lane",
            ));
        }
        self.guard()?;
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        k: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        let mut map = self.serialize_map(Some(1))?;
        map.serialize_entry(k, v)?;
        SerializeMap::end(map)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        compound(self, false)
    }
    fn serialize_tuple(self, n: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(n))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        n: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(n))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        compound(self, true)
    }
    fn serialize_struct(
        self,
        name: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        if name == NUMBER_TOKEN {
            self.node()?;
            return Ok(Compound {
                state: self,
                object: false,
                number_token: true,
                index: 0,
                key: None,
                seen: HashSet::new(),
                variant_parent: None,
            });
        }
        if name.starts_with("$serde_json::private::") {
            return Err(self.fail(
                ValueConversionErrorKind::UnsupportedRepresentation,
                "raw-token serialization requires the exact JSON lane",
            ));
        }
        self.serialize_map(None)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        k: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        variant(self, k, false)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        k: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        variant(self, k, true)
    }
    fn collect_str<T: fmt::Display + ?Sized>(self, v: &T) -> Result<(), Self::Error> {
        self.guard()?;
        let text = capture(v, self.limits.max_bytes.saturating_sub(self.bytes.len()))
            .map_err(|e| self.latch(e))?;
        self.serialize_str(&text)
    }
}
fn variant<'a>(
    state: &'a mut State,
    key: &str,
    object: bool,
) -> Result<Compound<'a>, ValueConversionError> {
    state.begin(true)?;
    state.node()?;
    state.string(key)?;
    state.write(b":")?;
    let parent = state.path.take();
    state.path = parent.as_deref().and_then(|p| pointer_child(p, key));
    let mut c = compound(state, object)?;
    c.variant_parent = Some(parent);
    Ok(c)
}
impl SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = ValueConversionError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl ser::SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = ValueConversionError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = ValueConversionError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = ValueConversionError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = ValueConversionError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, k: &T) -> Result<(), Self::Error> {
        self.key(k)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.value(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl ser::SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = ValueConversionError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        if self.number_token {
            self.number(k, v)
        } else {
            self.key(k)?;
            self.value(v)
        }
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = ValueConversionError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        self.key(k)?;
        self.value(v)
    }
    fn end(self) -> Result<(), Self::Error> {
        self.finish()
    }
}
struct KeySerializer {
    limit: usize,
    string_only: bool,
}
fn key_error() -> ValueConversionError {
    ValueConversionError::new(
        ValueConversionErrorKind::NonStringKey,
        "map keys must serialize as strings",
    )
}
macro_rules! invalid_keys {($($name:ident:$ty:ty),*)=>{$(fn $name(self,_:$ty)->Result<Self::Ok,Self::Error>{Err(key_error())})*};}
impl Serializer for KeySerializer {
    type Ok = String;
    type Error = ValueConversionError;
    type SerializeSeq = ser::Impossible<String, ValueConversionError>;
    type SerializeTuple = Self::SerializeSeq;
    type SerializeTupleStruct = Self::SerializeSeq;
    type SerializeTupleVariant = Self::SerializeSeq;
    type SerializeMap = Self::SerializeSeq;
    type SerializeStruct = Self::SerializeSeq;
    type SerializeStructVariant = Self::SerializeSeq;
    invalid_keys!(serialize_bool:bool,serialize_i8:i8,serialize_i16:i16,serialize_i32:i32,serialize_i64:i64,serialize_i128:i128,serialize_u8:u8,serialize_u16:u16,serialize_u32:u32,serialize_u64:u64,serialize_u128:u128,serialize_f32:f32,serialize_f64:f64,serialize_bytes:&[u8]);
    fn serialize_str(self, v: &str) -> Result<String, Self::Error> {
        if v.len() > self.limit {
            return Err(ValueConversionError::new(
                ValueConversionErrorKind::Limit,
                "map key exceeds byte limit",
            ));
        }
        Ok(v.into())
    }
    fn serialize_char(self, v: char) -> Result<String, Self::Error> {
        if self.string_only {
            return Err(key_error());
        }
        self.serialize_str(v.encode_utf8(&mut [0; 4]))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<String, Self::Error> {
        if self.string_only {
            return Err(key_error());
        }
        v.serialize(self)
    }
    fn serialize_none(self) -> Result<String, Self::Error> {
        Err(key_error())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<String, Self::Error> {
        Err(key_error())
    }
    fn serialize_unit(self) -> Result<String, Self::Error> {
        Err(key_error())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<String, Self::Error> {
        Err(key_error())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<String, Self::Error> {
        Err(key_error())
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<String, Self::Error> {
        Err(key_error())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(key_error())
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(key_error())
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(key_error())
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(key_error())
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(key_error())
    }
    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Err(key_error())
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(key_error())
    }
    fn collect_str<T: fmt::Display + ?Sized>(self, v: &T) -> Result<String, Self::Error> {
        if self.string_only {
            return Err(key_error());
        }
        capture(v, self.limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refused_map_never_accumulates_new_keys_or_output() {
        let mut state = State {
            bytes: Vec::new(),
            limits: JsonLimits {
                max_bytes: 2,
                ..Default::default()
            },
            nodes: 0,
            depth: 0,
            path: Some(String::new()),
            first: None,
        };
        let mut map = compound(&mut state, true).unwrap();
        assert!(map.key("first").is_err());
        let before = (map.seen.len(), map.state.bytes.len(), map.state.nodes);
        for n in 0..10000 {
            assert!(map.key(&n.to_string()).is_err());
        }
        assert_eq!(
            (map.seen.len(), map.state.bytes.len(), map.state.nodes),
            before
        );
        assert_eq!(map.seen.len(), 0);
    }
}
