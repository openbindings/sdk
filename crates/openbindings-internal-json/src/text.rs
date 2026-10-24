//! Borrowed reads over JSON text that an arena has already admitted.
//!
//! A retained text costs only its bytes, while its parsed arena also costs a
//! node per value. These reads walk admitted text in place and never allocate;
//! they do not validate, so callers pass only text that parsed successfully.
use crate::JsonKind;

/// One value in admitted JSON text.
#[derive(Clone, Copy)]
pub struct TextValue<'a> {
    text: &'a str,
    start: usize,
}
impl std::fmt::Debug for TextValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextValue")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}
/// One object member, with its exact name token and value.
#[derive(Clone, Copy, Debug)]
pub struct TextMember<'a> {
    /// The member's name as a string token.
    pub name: TextValue<'a>,
    /// The member's value.
    pub value: TextValue<'a>,
}
/// A string token whose escapes denote Unicode scalar text only.
#[derive(Clone, Copy)]
pub struct TextString<'a> {
    // Between the quotes, escapes not yet decoded.
    raw: &'a str,
    len: usize,
    escaped: bool,
}
impl std::fmt::Debug for TextString<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextString").finish_non_exhaustive()
    }
}

fn space(bytes: &[u8], mut at: usize) -> usize {
    while matches!(bytes.get(at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        at += 1;
    }
    at
}
// Index just past the string token starting at `at`.
fn string_end(bytes: &[u8], at: usize) -> usize {
    string_scan(bytes, at).0
}
// Index just past the string token starting at `at`, and whether it escapes.
fn string_scan(bytes: &[u8], mut at: usize) -> (usize, bool) {
    let mut escaped = false;
    at += 1;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'\\' => {
                escaped = true;
                at += 2;
            }
            b'"' => return (at + 1, escaped),
            _ => at += 1,
        }
    }
    (bytes.len(), escaped)
}
// Index just past the value starting at `at`. Containers are skipped by
// counting brackets, so deep values need no recursion.
fn value_end(bytes: &[u8], at: usize) -> usize {
    match bytes.get(at) {
        Some(b'"') => string_end(bytes, at),
        Some(b'{' | b'[') => {
            let mut depth = 0usize;
            let mut at = at;
            while let Some(&byte) = bytes.get(at) {
                match byte {
                    b'"' => {
                        at = string_end(bytes, at);
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return at + 1;
                        }
                    }
                    _ => {}
                }
                at += 1;
            }
            bytes.len()
        }
        _ => {
            let mut at = at;
            while bytes
                .get(at)
                .is_some_and(|b| !matches!(b, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'))
            {
                at += 1;
            }
            at
        }
    }
}
fn hex(raw: &[u8], at: usize) -> Option<u32> {
    let digits = std::str::from_utf8(raw.get(at..at + 4)?).ok()?;
    u32::from_str_radix(digits, 16).ok()
}
// Decode one character at `at` in a string's interior; returns it and the next
// index, or None when the escape denotes an unpaired surrogate.
fn decode(raw: &[u8], text: &str, at: usize) -> Option<(char, usize)> {
    if raw[at] != b'\\' {
        let c = text[at..].chars().next()?;
        return Some((c, at + c.len_utf8()));
    }
    let c = match raw[at + 1] {
        b'"' => '"',
        b'\\' => '\\',
        b'/' => '/',
        b'b' => '\u{8}',
        b'f' => '\u{c}',
        b'n' => '\n',
        b'r' => '\r',
        b't' => '\t',
        b'u' => {
            let unit = hex(raw, at + 2)?;
            if (0xd800..0xdc00).contains(&unit) {
                if raw.get(at + 6..at + 8) != Some(b"\\u") {
                    return None;
                }
                let low = hex(raw, at + 8)?;
                if !(0xdc00..0xe000).contains(&low) {
                    return None;
                }
                let scalar = 0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
                return Some((char::from_u32(scalar)?, at + 12));
            }
            return Some((char::from_u32(unit)?, at + 6));
        }
        _ => return None,
    };
    Some((c, at + 2))
}

// Where a container iteration resumes: its first entry, or after the value
// that starts at an offset, which is skipped only when the next entry is asked for.
#[derive(Clone, Copy)]
enum Cursor {
    First(usize),
    After(usize),
}
impl Cursor {
    fn next(self, bytes: &[u8]) -> Option<usize> {
        match self {
            Self::First(at) => Some(at),
            Self::After(value) => {
                let next = space(bytes, value_end(bytes, value));
                (bytes.get(next) == Some(&b',')).then_some(next + 1)
            }
        }
    }
}

impl<'a> TextValue<'a> {
    /// The value starting at `start`, after any JSON whitespace.
    pub fn at(text: &'a str, start: usize) -> Self {
        Self {
            text,
            start: space(text.as_bytes(), start),
        }
    }
    /// The value of a whole admitted text.
    pub fn root(text: &'a str) -> Self {
        Self::at(text, 0)
    }
    /// Byte offset of this value's first character in its text.
    pub fn start(self) -> usize {
        self.start
    }
    /// Return the JSON token category without conversion.
    pub fn kind(self) -> JsonKind {
        match self.text.as_bytes().get(self.start) {
            Some(b'{') => JsonKind::Object,
            Some(b'[') => JsonKind::Array,
            Some(b'"') => JsonKind::String,
            Some(b't' | b'f') => JsonKind::Boolean,
            Some(b'n') => JsonKind::Null,
            _ => JsonKind::Number,
        }
    }
    /// Borrow the exact token spelling, excluding surrounding whitespace.
    pub fn text(self) -> &'a str {
        &self.text[self.start..value_end(self.text.as_bytes(), self.start)]
    }
    /// Borrow the exact number token, or None for other kinds.
    pub fn number_text(self) -> Option<&'a str> {
        (self.kind() == JsonKind::Number).then(|| self.text())
    }
    /// Iterate object members in text order; empty for other kinds. Each
    /// member is yielded before its value is skipped, so finding a member
    /// scans only the members before it.
    pub fn members(self) -> impl Iterator<Item = TextMember<'a>> + 'a {
        let text = self.text;
        let bytes = text.as_bytes();
        let mut cursor = (self.kind() == JsonKind::Object).then_some(Cursor::First(self.start + 1));
        std::iter::from_fn(move || {
            let start = space(bytes, cursor?.next(bytes)?);
            if bytes.get(start) != Some(&b'"') {
                cursor = None;
                return None;
            }
            let name = TextValue { text, start };
            let colon = space(bytes, string_end(bytes, start));
            let value = TextValue::at(text, colon + 1);
            cursor = Some(Cursor::After(value.start));
            Some(TextMember { name, value })
        })
    }
    /// Iterate array elements in order; empty for other kinds. Each element is
    /// yielded before it is skipped.
    pub fn elements(self) -> impl Iterator<Item = TextValue<'a>> + 'a {
        let text = self.text;
        let bytes = text.as_bytes();
        let mut cursor = (self.kind() == JsonKind::Array).then_some(Cursor::First(self.start + 1));
        std::iter::from_fn(move || {
            let start = space(bytes, cursor?.next(bytes)?);
            if matches!(bytes.get(start), None | Some(b']')) {
                cursor = None;
                return None;
            }
            cursor = Some(Cursor::After(start));
            Some(TextValue { text, start })
        })
    }
    /// Return a string token's scalar text, or None for other kinds and for
    /// strings whose escapes denote an unpaired surrogate.
    pub fn string(self) -> Option<TextString<'a>> {
        if self.kind() != JsonKind::String {
            return None;
        }
        let (end, escaped) = string_scan(self.text.as_bytes(), self.start);
        let raw = &self.text[self.start + 1..end - 1];
        if !escaped {
            return Some(TextString {
                raw,
                len: raw.len(),
                escaped: false,
            });
        }
        let bytes = raw.as_bytes();
        let mut len = 0;
        let mut at = 0;
        while at < bytes.len() {
            let (c, next) = decode(bytes, raw, at)?;
            len += c.len_utf8();
            at = next;
        }
        Some(TextString {
            raw,
            len,
            escaped: true,
        })
    }
    /// Return a string token's UTF-16 units, keeping each unit an escape
    /// denotes unpaired, or None for other kinds.
    pub fn utf16_units(self) -> Option<Vec<u16>> {
        if self.kind() != JsonKind::String {
            return None;
        }
        let end = string_end(self.text.as_bytes(), self.start);
        let raw = &self.text[self.start + 1..end - 1];
        let bytes = raw.as_bytes();
        let mut units = Vec::with_capacity(raw.len());
        let mut at = 0;
        while at < bytes.len() {
            // Each `\u` escape is one unit, paired or not.
            if bytes[at] == b'\\' && bytes.get(at + 1) == Some(&b'u') {
                units.push(u16::try_from(hex(bytes, at + 2)?).ok()?);
                at += 6;
                continue;
            }
            let (c, next) = decode(bytes, raw, at)?;
            units.extend_from_slice(c.encode_utf16(&mut [0; 2]));
            at = next;
        }
        Some(units)
    }
}
impl<'a> TextString<'a> {
    /// Decoded UTF-8 length in bytes.
    pub fn len(self) -> usize {
        self.len
    }
    /// Whether the decoded text is empty.
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
    /// Borrow the text when it contains no escapes.
    pub fn as_unescaped(self) -> Option<&'a str> {
        (!self.escaped).then_some(self.raw)
    }
    /// Iterate the decoded characters without allocating.
    pub fn chars(self) -> impl Iterator<Item = char> + 'a {
        let raw = self.raw;
        let mut at = 0;
        std::iter::from_fn(move || {
            if at >= raw.len() {
                return None;
            }
            let (c, next) = decode(raw.as_bytes(), raw, at).expect("checked scalar string");
            at = next;
            Some(c)
        })
    }
    /// Compare the decoded text with Unicode scalar text.
    pub fn eq_str(self, other: &str) -> bool {
        match self.as_unescaped() {
            Some(raw) => raw == other,
            None => self.len == other.len() && self.chars().eq(other.chars()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JsonValue;

    // Every read agrees with the arena that admitted the same text.
    fn agree(arena: crate::JsonRef<'_>, text: TextValue<'_>) {
        assert_eq!(arena.kind(), text.kind());
        assert_eq!(arena.text(), text.text());
        assert_eq!(arena.number_text(), text.number_text());
        assert_eq!(arena.utf16_units(), text.utf16_units());
        assert_eq!(
            arena.as_str(),
            text.string()
                .map(|s| s.chars().collect::<String>())
                .as_deref()
        );
        if let Some(s) = text.string() {
            assert_eq!(s.len(), arena.as_str().unwrap().len());
            assert!(s.eq_str(arena.as_str().unwrap()));
            assert!(!s.eq_str("not this text at all"));
        }
        let members: Vec<_> = arena.members().into_iter().flatten().collect();
        let text_members: Vec<_> = text.members().collect();
        assert_eq!(members.len(), text_members.len());
        for (a, t) in members.iter().zip(&text_members) {
            agree(a.name, t.name);
            agree(a.value, t.value);
        }
        let elements: Vec<_> = arena.elements().into_iter().flatten().collect();
        let text_elements: Vec<_> = text.elements().collect();
        assert_eq!(elements.len(), text_elements.len());
        for (a, t) in elements.iter().zip(&text_elements) {
            agree(*a, *t);
        }
    }

    #[test]
    fn text_reads_agree_with_the_admitting_arena() {
        for source in [
            r#"  { "a" : [ 1 , -2.5e+3, true, false, null, "x\"y" ] , "b\u0041\/" : {} , "" : [] }  "#,
            r#"{"nested":{"deep":[[[{"k":"\uD83D\uDE00 snow \u2603 雪"}]]]},"esc":"\\\b\f\n\r\t"}"#,
            r#"["lone \ud800 unit", "\udc00", {"\ud83d": 1, "ok": 2}]"#,
            r#""top""#,
            "0",
            "[]",
            "{}",
            r#"{"a":{"}":"]","[":"{"},"b":"\"}"}"#,
        ] {
            let arena = JsonValue::parse(source).unwrap();
            agree(arena.view(), TextValue::root(source));
        }
    }

    #[test]
    fn unpaired_escapes_have_no_scalar_text() {
        for source in [r#""\ud800""#, r#""\udc00x""#, r#""\ud800\u0041""#] {
            assert!(TextValue::root(source).string().is_none(), "{source}");
        }
        // The units keep them, an escaped pair as its two units.
        assert_eq!(
            TextValue::root(r#""\ud800\u0041\ud83d\ude00é\n""#).utf16_units(),
            Some(vec![0xd800, 0x41, 0xd83d, 0xde00, 0xe9, 0x0a])
        );
        assert_eq!(TextValue::root("[]").utf16_units(), None);
        let pair = TextValue::root(r#""\ud83d\ude00""#).string().unwrap();
        assert_eq!(pair.len(), 4);
        assert!(pair.eq_str("\u{1f600}"));
        assert!(pair.as_unescaped().is_none());
    }

    #[test]
    fn deep_containers_are_skipped_without_recursion() {
        let depth = 100_000;
        let source = format!(
            "{{\"a\":{}0{},\"b\":7}}",
            "[".repeat(depth),
            "]".repeat(depth)
        );
        let root = TextValue::root(&source);
        let b = root
            .members()
            .find(|m| m.name.string().unwrap().eq_str("b"))
            .unwrap();
        assert_eq!(b.value.number_text(), Some("7"));
    }
}
