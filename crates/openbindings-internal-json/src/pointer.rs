//! Exact JSON Pointers.
use serde::{Serialize, Serializer};
use std::{fmt, str::FromStr};

/// An RFC 6901 JSON Pointer whose segments are held exactly.
///
/// A member name in a caller value, a draft document or a supplied resource can
/// hold an escape denoting an unpaired UTF-16 surrogate, which Rust text cannot
/// spell. Such a pointer keeps every unit: [`Self::as_str`] returns `None` and
/// [`Self::utf16_units`] reads it exactly. The empty pointer denotes a root.
/// Segments are data, often source-controlled: escape them for display.
///
/// `Display` writes the escaped pointer, with each unpaired unit as `\uXXXX` in
/// lowercase hexadecimal; this presentation is not reversible. Serialization
/// writes a JSON string, using a `\uXXXX` escape for each unpaired unit, so a
/// consumer that keeps UTF-16 strings (such as JavaScript) receives the exact
/// pointer. A `serde_json::Value` cannot hold such a string, as for an exact
/// [`crate::JsonValue`] with one, so serialize records that may carry one to
/// JSON text (`serde_json::to_string`, `to_writer`); `to_value` refuses it and
/// `json!` panics. Another serializer receives that string token as
/// `serde_json`'s raw-value newtype, as for an exact value. `Debug` shows no
/// segment.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Pointer(Repr);
#[derive(Clone, PartialEq, Eq, Hash)]
enum Repr {
    // Escaped RFC 6901 text whose segments are all Unicode scalar text.
    Text(String),
    // The escaped pointer's UTF-16 units; only when one is an unpaired surrogate.
    Units(Box<[u16]>),
}
/// Text that is not an RFC 6901 JSON Pointer: it neither is empty nor begins
/// with `/`, or a `~` is not followed by `0` or `1`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PointerError;
impl fmt::Display for PointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid JSON Pointer")
    }
}
impl std::error::Error for PointerError {}

impl Pointer {
    /// Borrow the escaped RFC 6901 text, or `None` when a segment holds an
    /// unpaired UTF-16 unit. Use [`Self::utf16_units`] for the latter.
    pub fn as_str(&self) -> Option<&str> {
        match &self.0 {
            Repr::Text(text) => Some(text),
            Repr::Units(_) => None,
        }
    }
    /// Iterate the escaped pointer's UTF-16 units without allocating, keeping
    /// unpaired units.
    pub fn utf16_units(&self) -> impl Iterator<Item = u16> + '_ {
        let (text, units) = match &self.0 {
            Repr::Text(text) => (Some(text.as_str()), None),
            Repr::Units(units) => (None, Some(units.iter().copied())),
        };
        text.into_iter()
            .flat_map(str::encode_utf16)
            .chain(units.into_iter().flatten())
    }
    /// Check escaped RFC 6901 UTF-16 units, as [`Self::utf16_units`] gives them,
    /// keeping unpaired units; the counterpart of `String::from_utf16`.
    ///
    /// # Errors
    /// Returns [`PointerError`] when the units are not an RFC 6901 pointer.
    pub fn from_utf16(units: &[u16]) -> Result<Self, PointerError> {
        Self::parse_units(units).ok_or(PointerError)
    }
    /// Wrap escaped RFC 6901 text the caller has built or checked.
    pub(crate) fn escaped(text: String) -> Self {
        debug_assert!(valid(text.encode_utf16()));
        Self(Repr::Text(text))
    }
    /// Escaped units; the scalar form when every unit pairs.
    pub(crate) fn from_escaped_units(units: Vec<u16>) -> Self {
        match String::from_utf16(&units) {
            Ok(text) => Self::escaped(text),
            Err(_) => {
                debug_assert!(valid(units.iter().copied()));
                Self(Repr::Units(units.into_boxed_slice()))
            }
        }
    }
    /// Check and wrap escaped RFC 6901 units.
    pub(crate) fn parse_units(units: &[u16]) -> Option<Self> {
        valid(units.iter().copied()).then(|| Self::from_escaped_units(units.to_vec()))
    }
    /// Append escaped RFC 6901 text, such as `/minimum`.
    pub(crate) fn push_escaped(&mut self, escaped: &str) {
        match &mut self.0 {
            Repr::Text(text) => {
                debug_assert!(escaped.is_empty() || valid(escaped.encode_utf16()));
                text.push_str(escaped);
            }
            Repr::Units(_) => *self = self.joined(escaped),
        }
    }
    /// This pointer followed by escaped RFC 6901 text, in one exact allocation.
    pub(crate) fn joined(&self, escaped: &str) -> Self {
        debug_assert!(escaped.is_empty() || valid(escaped.encode_utf16()));
        match &self.0 {
            Repr::Text(text) => {
                let mut joined = String::with_capacity(text.len() + escaped.len());
                joined.push_str(text);
                joined.push_str(escaped);
                Self(Repr::Text(joined))
            }
            Repr::Units(units) => {
                let mut joined = Vec::with_capacity(units.len() + escaped.len());
                joined.extend_from_slice(units);
                joined.extend(escaped.encode_utf16());
                Self(Repr::Units(joined.into_boxed_slice()))
            }
        }
    }
    /// Retained bytes: the UTF-8 text, or two per unit.
    pub(crate) fn size(&self) -> usize {
        match &self.0 {
            Repr::Text(text) => text.len(),
            Repr::Units(units) => 2 * units.len(),
        }
    }
    pub(crate) fn is_root(&self) -> bool {
        matches!(&self.0, Repr::Text(text) if text.is_empty())
    }
    /// Decoded segments as UTF-16 units; the root has none.
    pub(crate) fn unit_segments(&self) -> impl Iterator<Item = Vec<u16>> + '_ {
        let units: Vec<u16> = self.utf16_units().collect();
        let mut segments = Vec::new();
        if !units.is_empty() {
            for raw in units[1..].split(|&unit| unit == u16::from(b'/')) {
                let mut segment = Vec::with_capacity(raw.len());
                let mut raw = raw.iter().copied();
                while let Some(unit) = raw.next() {
                    if unit == u16::from(b'~') {
                        segment.push(match raw.next() {
                            Some(0x30) => u16::from(b'~'),
                            _ => u16::from(b'/'),
                        });
                    } else {
                        segment.push(unit);
                    }
                }
                segments.push(segment);
            }
        }
        segments.into_iter()
    }
}
// Empty, or `/` then segments in which every `~` begins `~0` or `~1`.
fn valid(mut units: impl Iterator<Item = u16>) -> bool {
    match units.next() {
        None => return true,
        Some(0x2f) => {}
        Some(_) => return false,
    }
    while let Some(unit) = units.next() {
        if unit == u16::from(b'~') && !matches!(units.next(), Some(0x30 | 0x31)) {
            return false;
        }
    }
    true
}
impl FromStr for Pointer {
    type Err = PointerError;
    /// Check escaped RFC 6901 text, such as `/a~1b/0`; the empty string is the
    /// root. No URI-fragment decoding is applied.
    fn from_str(text: &str) -> Result<Self, PointerError> {
        if valid(text.encode_utf16()) {
            Ok(Self(Repr::Text(text.to_owned())))
        } else {
            Err(PointerError)
        }
    }
}
impl PartialEq<str> for Pointer {
    /// Whether this pointer is exactly this escaped text; a pointer with an
    /// unpaired unit equals no text.
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}
impl PartialEq<&str> for Pointer {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}
impl PartialEq<String> for Pointer {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == Some(other.as_str())
    }
}
impl fmt::Display for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Repr::Text(text) => f.write_str(text),
            Repr::Units(units) => {
                for unit in char::decode_utf16(units.iter().copied()) {
                    match unit {
                        Ok(c) => fmt::Write::write_char(f, c)?,
                        Err(unit) => write!(f, "\\u{:04x}", unit.unpaired_surrogate())?,
                    }
                }
                Ok(())
            }
        }
    }
}
impl fmt::Debug for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pointer { .. }")
    }
}
impl Serialize for Pointer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Repr::Text(text) => serializer.serialize_str(text),
            Repr::Units(units) => {
                // JSON text can denote an unpaired unit with an escape, which no
                // Rust string holds; write the exact string token. Every run of
                // scalar text takes serde_json's own escapes.
                let mut token = String::with_capacity(units.len() + 2);
                let mut run = String::new();
                let flush = |token: &mut String, run: &mut String| {
                    let quoted = serde_json::to_string(run.as_str()).expect("text serializes");
                    token.push_str(&quoted[1..quoted.len() - 1]);
                    run.clear();
                };
                token.push('"');
                for unit in char::decode_utf16(units.iter().copied()) {
                    match unit {
                        Ok(c) => run.push(c),
                        Err(unit) => {
                            flush(&mut token, &mut run);
                            use std::fmt::Write;
                            let _ = write!(token, "\\u{:04x}", unit.unpaired_surrogate());
                        }
                    }
                }
                flush(&mut token, &mut run);
                token.push('"');
                serde_json::value::RawValue::from_string(token)
                    .map_err(serde::ser::Error::custom)?
                    .serialize(serializer)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn text_is_checked_and_read_back_exactly() {
        for text in ["", "/", "//", "/a~0b~1c/0", "/é/雪"] {
            let pointer: Pointer = text.parse().unwrap();
            assert_eq!(pointer.as_str(), Some(text));
            assert_eq!(pointer, text);
            assert_eq!(pointer.to_string(), text);
            assert_eq!(pointer.utf16_units().collect::<Vec<_>>(), units(text));
            assert_eq!(
                serde_json::to_string(&pointer).unwrap(),
                serde_json::to_string(text).unwrap()
            );
        }
        for text in ["a", "/~", "/~2", "/a~"] {
            assert_eq!(text.parse::<Pointer>(), Err(PointerError));
        }
    }

    #[test]
    fn unpaired_units_are_kept_and_written_as_escapes() {
        let mut exact = units("/a\"\\\n/");
        exact.push(0xd800);
        exact.extend(units("~0x"));
        let mut pointer = Pointer::from_utf16(&exact).unwrap();
        assert_eq!(pointer.as_str(), None);
        assert_ne!(pointer, "/a\"\\\n/\\ud800~0x");
        assert_eq!(pointer.utf16_units().collect::<Vec<_>>(), exact);
        assert_eq!(pointer.to_string(), "/a\"\\\n/\\ud800~0x");
        assert_eq!(
            serde_json::to_string(&pointer).unwrap(),
            r#""/a\"\\\n/\ud800~0x""#
        );
        // A character takes the same escape in either form.
        let scalar: Pointer = "/a\"\\\n/x".parse().unwrap();
        assert_eq!(serde_json::to_string(&scalar).unwrap(), r#""/a\"\\\n/x""#);
        assert_eq!(format!("{pointer:?}"), "Pointer { .. }");
        assert_eq!(
            pointer.unit_segments().collect::<Vec<_>>(),
            vec![units("a\"\\\n"), [&[0xd800][..], &units("~x")].concat()]
        );
        pointer.push_escaped("/~1");
        assert_eq!(pointer.to_string(), "/a\"\\\n/\\ud800~0x/~1");
        assert_eq!(pointer.size(), 2 * (exact.len() + 3));
        // Paired units are scalar text.
        let paired = Pointer::from_utf16(&units("/😀")).unwrap();
        assert_eq!(paired.as_str(), Some("/😀"));
        assert_eq!(Pointer::from_utf16(&[0xd800]), Err(PointerError));
        assert_eq!(Pointer::from_utf16(&units("/~2")), Err(PointerError));
    }
}
