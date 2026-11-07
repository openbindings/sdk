//! Atomic edits against the current exact source revision.
use super::*;
use std::{io::Write, ops::Range};

/// The RFC 6901 pointer an edit addresses, borrowed from escaped text such as
/// `"/a~1b/0"` (with no URI-fragment decoding) or from an exact [`Pointer`].
/// A finding's or problem's pointer can name a member whose name holds an
/// unpaired UTF-16 unit, which no Rust text spells; the exact form addresses it.
/// Convert either with `into()`. `Debug` shows no segment.
#[derive(Clone, Copy)]
pub struct PointerRef<'a>(PointerRepr<'a>);
#[derive(Clone, Copy)]
enum PointerRepr<'a> {
    Text(&'a str),
    Exact(&'a Pointer),
}
impl<'a> From<&'a str> for PointerRef<'a> {
    fn from(text: &'a str) -> Self {
        Self(PointerRepr::Text(text))
    }
}
impl<'a> From<&'a String> for PointerRef<'a> {
    fn from(text: &'a String) -> Self {
        Self(PointerRepr::Text(text))
    }
}
impl<'a> From<&'a Pointer> for PointerRef<'a> {
    fn from(pointer: &'a Pointer) -> Self {
        Self(PointerRepr::Exact(pointer))
    }
}
impl fmt::Debug for PointerRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PointerRef { .. }")
    }
}

/// One owned, exact JSON edit. Paths address the snapshot before the batch.
/// Creating an edit does not read or change a document.
#[derive(Clone)]
pub struct Edit {
    pointer: EditPointer,
    value: Option<JsonValue>,
}
#[derive(Clone)]
enum EditPointer {
    Text(String),
    Exact(Pointer),
}
impl EditPointer {
    fn borrow(&self) -> PointerRef<'_> {
        match self {
            Self::Text(value) => value.as_str().into(),
            Self::Exact(value) => value.into(),
        }
    }
    fn retain(pointer: PointerRef<'_>) -> Self {
        match pointer.0 {
            PointerRepr::Text(value) => Self::Text(value.to_owned()),
            PointerRepr::Exact(value) => Self::Exact(value.clone()),
        }
    }
    fn segments(path: &[&str]) -> Self {
        let mut pointer = String::new();
        for segment in path {
            pointer.push('/');
            for character in segment.chars() {
                match character {
                    '~' => pointer.push_str("~0"),
                    '/' => pointer.push_str("~1"),
                    other => pointer.push(other),
                }
            }
        }
        Self::Text(pointer)
    }
}
impl Edit {
    /// Set an exact value at decoded path segments. Parents must already exist.
    #[must_use]
    pub fn set(path: &[&str], value: JsonValue) -> Self {
        Self {
            pointer: EditPointer::segments(path),
            value: Some(value),
        }
    }
    /// Remove an existing value at decoded path segments.
    #[must_use]
    pub fn remove(path: &[&str]) -> Self {
        Self {
            pointer: EditPointer::segments(path),
            value: None,
        }
    }
    /// Set at an RFC 6901 pointer, including an exact pointer from a finding.
    /// The edit owns its path; invalid text is refused when applying the batch.
    #[must_use]
    pub fn set_at<'p>(pointer: impl Into<PointerRef<'p>>, value: JsonValue) -> Self {
        Self {
            pointer: EditPointer::retain(pointer.into()),
            value: Some(value),
        }
    }
    /// Remove at an RFC 6901 pointer, retaining exact unpaired path units when present.
    #[must_use]
    pub fn remove_at<'p>(pointer: impl Into<PointerRef<'p>>) -> Self {
        Self {
            pointer: EditPointer::retain(pointer.into()),
            value: None,
        }
    }
}
impl fmt::Debug for Edit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Edit")
            .field("sets_value", &self.value.is_some())
            .finish_non_exhaustive()
    }
}
impl Document {
    /// Apply an atomic batch and return a new document snapshot.
    /// Parents must exist; untouched bytes and earlier snapshots stay exact.
    ///
    /// # Errors
    /// Invalid or ambiguous paths, overlapping edits and admission limits return
    /// an error without changing the document or the reusable edits.
    pub fn edit(&self, edits: &[Edit]) -> Result<Self, EditError> {
        let limits = self.limits();
        if edits.len() > limits.max_nodes {
            return Err(backend::input_error(InputErrorKind::Limit, "node-limit", 0).into());
        }
        let requests: Vec<_> = edits
            .iter()
            .map(|edit| Request {
                pointer: edit.pointer.borrow(),
                value: edit.value.as_ref().map(|value| value as &dyn WriteJson),
            })
            .collect();
        self.apply(&requests, limits)
    }
}

/// Atomic editing failure. The document and all its caches remain unchanged.
///
/// [`Self::code`] gives each case a stable identifier. `Debug` shows no caller
/// values or pointer segments.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditError {
    /// Rebuilt JSON exceeded admission limits or could not be parsed.
    Input(InputError),
    /// Invalid RFC 6901 syntax, array index, or removal of the root.
    InvalidPointer,
    /// The pointer's parent does not exist.
    MissingParent,
    /// Removal requires an existing value.
    MissingValue,
    /// Insertion requires an object or array append position.
    NotContainer,
    /// Duplicate decoded member names prevent unambiguous editing.
    DuplicateMembers,
    /// Edits touch overlapping ranges or set the same object member twice.
    Overlap,
}
impl EditError {
    /// Stable kebab-case identifier; branch on this rather than display text.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Input(error) => error.code(),
            Self::InvalidPointer => "invalid-pointer",
            Self::MissingParent => "missing-parent",
            Self::MissingValue => "missing-value",
            Self::NotContainer => "not-container",
            Self::DuplicateMembers => "duplicate-members",
            Self::Overlap => "overlapping-edits",
        }
    }
}
impl fmt::Debug for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => f.debug_tuple("Input").field(error).finish(),
            Self::InvalidPointer => f.write_str("InvalidPointer"),
            Self::MissingParent => f.write_str("MissingParent"),
            Self::MissingValue => f.write_str("MissingValue"),
            Self::NotContainer => f.write_str("NotContainer"),
            Self::DuplicateMembers => f.write_str("DuplicateMembers"),
            Self::Overlap => f.write_str("Overlap"),
        }
    }
}
impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // An exceeded limit is the document's: name the setting it was
        // admitted with.
        if let Self::Input(error) = self
            && let Some((rust, typescript)) = backend::limit_setting(error.code())
        {
            let limit = match error.code() {
                "byte-limit" => "byte",
                "node-limit" => "node",
                _ => "nesting",
            };
            return write!(
                f,
                "document edit refused: the edited document exceeds its {limit} limit; \
                 admit the document with a larger JsonLimits::{rust} (limits.{typescript} \
                 in TypeScript)"
            );
        }
        write!(f, "document edit refused: {}", self.code())
    }
}
impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            _ => None,
        }
    }
}
impl From<InputError> for EditError {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}
fn limit(limits: JsonLimits) -> EditError {
    backend::input_error(InputErrorKind::Limit, "byte-limit", limits.max_bytes).into()
}
fn decode(s: &str) -> Result<String, EditError> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '~' {
            match chars.next() {
                Some('0') => '~',
                Some('1') => '/',
                _ => return Err(EditError::InvalidPointer),
            }
        } else {
            c
        });
    }
    Ok(out)
}

// Exact inputs have a known byte extent. Every write reserves the untouched
// bytes and replacements still to come before growing the final splice buffer.
trait WriteJson {
    fn size(&self) -> Result<Option<usize>, EditError> {
        Ok(None)
    }
    fn write(&self, out: &mut BoundedWriter) -> Result<(), EditError>;
}
impl WriteJson for JsonValue {
    fn size(&self) -> Result<Option<usize>, EditError> {
        if self.has_duplicate_names() {
            return Err(EditError::DuplicateMembers);
        }
        Ok(Some(self.bytes().len()))
    }
    fn write(&self, out: &mut BoundedWriter) -> Result<(), EditError> {
        out.write_all(self.bytes()).map_err(|_| limit(out.limits))
    }
}
struct BoundedWriter {
    bytes: Vec<u8>,
    reserved: usize,
    limits: JsonLimits,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let available = self.limits.max_bytes - self.reserved;
        if bytes.len() > available.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("byte limit"));
        }
        let needed = self.bytes.len() + bytes.len();
        if needed > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(needed)
                .min(available);
            self.bytes.reserve_exact(capacity - self.bytes.len());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl BoundedWriter {
    fn known(&mut self, bytes: &[u8]) -> Result<(), EditError> {
        self.reserved -= bytes.len();
        self.write_all(bytes).map_err(|_| limit(self.limits))
    }
}
fn key_size(key: &str) -> usize {
    2 + key
        .bytes()
        .map(|b| match b {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
            0..=31 => 6,
            _ => 1,
        })
        .sum::<usize>()
}
const SLASH: u16 = b'/' as u16;
const TILDE: u16 = b'~' as u16;
// A checked edit pointer: escaped text, or the escaped UTF-16 units of an exact
// pointer whose segment holds an unpaired unit.
enum Address<'a> {
    Text(&'a str),
    Units(&'a Pointer, Vec<u16>),
}
impl<'a> Address<'a> {
    fn check(pointer: PointerRef<'a>, limits: JsonLimits) -> Result<Self, EditError> {
        let text = match pointer.0 {
            PointerRepr::Text(text) => text,
            PointerRepr::Exact(exact) => match exact.as_str() {
                Some(text) => text,
                // A `Pointer` is already well formed. One unit takes at least one
                // byte, so its units bound the bytes it can name.
                None => {
                    let units: Vec<u16> = exact.utf16_units().collect();
                    if units.len() > limits.max_bytes {
                        return Err(limit(limits));
                    }
                    return Ok(Self::Units(exact, units));
                }
            },
        };
        if text.len() > limits.max_bytes {
            return Err(limit(limits));
        }
        if !text.is_empty() && !text.starts_with('/') {
            return Err(EditError::InvalidPointer);
        }
        // Validate every escape, including segments leading to existing values.
        for token in text.split('/').skip(1) {
            decode(token)?;
        }
        Ok(Self::Text(text))
    }
    fn target<'v>(&self, root: &'v JsonValue) -> Option<JsonRef<'v>> {
        match self {
            Self::Text(text) => root.at(text),
            Self::Units(exact, _) => root.at_pointer(exact),
        }
    }
    fn parent<'v>(&self, root: &'v JsonValue) -> Result<JsonRef<'v>, EditError> {
        let parent = match self {
            Self::Text(text) => {
                let (parent, _) = text.rsplit_once('/').ok_or(EditError::InvalidPointer)?;
                root.at(parent)
            }
            Self::Units(_, units) => {
                let slash = units.iter().rposition(|&unit| unit == SLASH);
                let parent = Pointer::from_utf16(&units[..slash.ok_or(EditError::InvalidPointer)?])
                    .map_err(|_| EditError::InvalidPointer)?;
                root.at_pointer(&parent)
            }
        };
        parent.ok_or(EditError::MissingParent)
    }
    // The decoded last segment, as the member name an insertion writes.
    fn name(&self) -> Result<Name, EditError> {
        match self {
            Self::Text(text) => {
                let (_, token) = text.rsplit_once('/').ok_or(EditError::InvalidPointer)?;
                decode(token).map(Name::Text)
            }
            Self::Units(_, units) => {
                let start = units
                    .iter()
                    .rposition(|&unit| unit == SLASH)
                    .map_or(0, |i| i + 1);
                let mut name = Vec::with_capacity(units.len() - start);
                let mut token = units[start..].iter().copied();
                // A checked pointer follows every `~` with `0` or `1`.
                while let Some(unit) = token.next() {
                    name.push(if unit == TILDE {
                        if token.next() == Some(u16::from(b'0')) {
                            TILDE
                        } else {
                            SLASH
                        }
                    } else {
                        unit
                    });
                }
                Ok(match String::from_utf16(&name) {
                    Ok(text) => Name::Text(text),
                    Err(_) => Name::Token(units_token(&name)),
                })
            }
        }
    }
}
// An inserted member's name: decoded text, or the JSON string token of a name
// holding an unpaired UTF-16 unit, which only a `\u` escape can write. A token
// never denotes the same name as text.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Name {
    Text(String),
    Token(String),
}
impl Name {
    fn size(&self) -> usize {
        match self {
            Self::Text(name) => key_size(name),
            Self::Token(token) => token.len(),
        }
    }
    fn write(&self, out: &mut BoundedWriter) -> Result<(), EditError> {
        match self {
            Self::Text(name) => {
                let limits = out.limits;
                // A scalar string has no serialization failure except writer capacity.
                serde_json::to_writer(out, name).map_err(|_| limit(limits))
            }
            Self::Token(token) => out
                .write_all(token.as_bytes())
                .map_err(|_| limit(out.limits)),
        }
    }
}
// Scalar runs take serde_json's escapes; each unpaired unit is a `\u` escape.
fn units_token(units: &[u16]) -> String {
    let mut token = String::from("\"");
    let mut run = String::new();
    let flush = |token: &mut String, run: &mut String| {
        let quoted = serde_json::to_string(run.as_str()).expect("text serializes");
        token.push_str(&quoted[1..quoted.len() - 1]);
        run.clear();
    };
    for unit in char::decode_utf16(units.iter().copied()) {
        match unit {
            Ok(c) => run.push(c),
            Err(unit) => {
                flush(&mut token, &mut run);
                let _ = fmt::Write::write_fmt(
                    &mut token,
                    format_args!("\\u{:04x}", unit.unpaired_surrogate()),
                );
            }
        }
    }
    flush(&mut token, &mut run);
    token.push('"');
    token
}
// Build each parent's removal range index once per batch. Removed children are
// planned together once every request is known, so adjacent removals share
// separators instead of each claiming one alone.
struct RemovalIndex {
    children: Vec<Range<usize>>,
    by_end: HashMap<usize, usize>,
    removed: Vec<usize>,
}
#[cfg(test)]
thread_local! { static PLANNING_WORK: std::cell::Cell<(usize, usize, usize)> = const { std::cell::Cell::new((0, 0, 0)) }; }
impl RemovalIndex {
    fn new(parent: JsonRef<'_>) -> Result<Self, EditError> {
        let children: Vec<_> = if let Some(members) = parent.members() {
            members
                .map(|m| backend::byte_range(m.name).start..backend::byte_range(m.value).end)
                .collect()
        } else if let Some(items) = parent.elements() {
            items.map(backend::byte_range).collect()
        } else {
            return Err(EditError::NotContainer);
        };
        #[cfg(test)]
        PLANNING_WORK.with(|work| {
            let (p, c, l) = work.get();
            work.set((p + 1, c + children.len(), l));
        });
        let by_end = children
            .iter()
            .enumerate()
            .map(|(i, r)| (r.end, i))
            .collect();
        Ok(Self {
            children,
            by_end,
            removed: Vec::new(),
        })
    }
    fn remove(&mut self, end: usize) -> Result<(), EditError> {
        #[cfg(test)]
        PLANNING_WORK.with(|work| {
            let (p, c, l) = work.get();
            work.set((p, c, l + 1));
        });
        let i = *self.by_end.get(&end).ok_or(EditError::MissingValue)?;
        self.removed.push(i);
        Ok(())
    }
    // Each run of adjacent removed children takes the separator after it. A run
    // reaching the last child takes the separator before it instead, and with no
    // child kept the removal spans first to last child. Removing one child twice
    // overlaps. Returns whether every child is removed.
    fn plan(&mut self, splices: &mut Vec<Splice<'_>>) -> Result<bool, EditError> {
        let (children, removed) = (&self.children, &mut self.removed);
        removed.sort_unstable();
        if removed.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(EditError::Overlap);
        }
        let last = children.len() - 1;
        let mut runs = removed.iter().copied().peekable();
        while let Some(first) = runs.next() {
            let mut end = first;
            while runs.next_if_eq(&(end + 1)).is_some() {
                end += 1;
            }
            let range = if end < last {
                children[first].start..children[end + 1].start
            } else if first > 0 {
                children[first - 1].end..children[end].end
            } else {
                children[0].start..children[last].end
            };
            splices.push(Splice {
                range,
                value: None,
                insertion: None,
                size: 0,
            });
        }
        Ok(removed.len() == children.len())
    }
}
struct Request<'a> {
    pointer: PointerRef<'a>,
    value: Option<&'a dyn WriteJson>,
}
struct Splice<'a> {
    range: Range<usize>,
    value: Option<&'a dyn WriteJson>,
    // Insertions are ordered by batch order when their boundaries coincide.
    insertion: Option<Insertion>,
    size: usize,
}
struct Insertion {
    key: Option<Name>,
    comma: bool,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}
impl Document {
    /// Create an empty OBI declaring [`AUTHORING_VERSION`]. Validate to obtain proof.
    pub fn new() -> Self {
        Self::parse(format!(
            r#"{{"openbindings":"{AUTHORING_VERSION}","operations":{{}}}}"#
        ))
        .expect("constant OBI")
    }
    /// Replace or insert an exact value at an RFC 6901 pointer: escaped text,
    /// or an exact [`Pointer`] such as a finding's.
    ///
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set<'p>(
        &self,
        pointer: impl Into<PointerRef<'p>>,
        value: &JsonValue,
    ) -> Result<Self, EditError> {
        self.edit(&[Edit::set_at(pointer, value.clone())])
    }
    /// Remove an existing member or array element at an RFC 6901 pointer:
    /// escaped text, or an exact [`Pointer`] such as a finding's.
    ///
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn remove<'p>(&self, pointer: impl Into<PointerRef<'p>>) -> Result<Self, EditError> {
        self.edit(&[Edit::remove_at(pointer)])
    }
    fn apply(&self, edits: &[Request<'_>], limits: JsonLimits) -> Result<Self, EditError> {
        if edits.is_empty() {
            return Ok(self.clone());
        }
        if self.value().has_duplicate_names() {
            return Err(EditError::DuplicateMembers);
        }
        let mut splices = Vec::with_capacity(edits.len());
        // Removals are indexed by their parent's closing position.
        let mut removal_indexes = HashMap::new();
        for edit in edits {
            let address = Address::check(edit.pointer, limits)?;
            let size = edit
                .value
                .map(|v| v.size())
                .transpose()?
                .flatten()
                .unwrap_or(0);
            if let Some(target) = address.target(self.value()) {
                let range = backend::byte_range(target);
                if edit.value.is_none() {
                    let parent = address.parent(self.value())?;
                    let index = match removal_indexes.entry(backend::byte_range(parent).end - 1) {
                        std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(RemovalIndex::new(parent)?)
                        }
                    };
                    index.remove(range.end)?;
                    continue;
                }
                splices.push(Splice {
                    range,
                    value: edit.value,
                    insertion: None,
                    size,
                });
            } else {
                let value = edit.value.ok_or(EditError::MissingValue)?;
                let key = address.name()?;
                let parent = address.parent(self.value())?;
                let comma = parent.is_empty() == Some(false);
                let key = match parent.kind() {
                    JsonKind::Object => Some(key),
                    JsonKind::Array if matches!(&key, Name::Text(key) if key == "-") => None,
                    JsonKind::Array => return Err(EditError::InvalidPointer),
                    _ => return Err(EditError::NotContainer),
                };
                let prefix = match &key {
                    Some(key) => key.size().checked_add(1).ok_or_else(|| limit(limits))?,
                    None => 0,
                };
                let size = size
                    .checked_add(prefix)
                    .and_then(|n| n.checked_add(usize::from(comma)))
                    .ok_or_else(|| limit(limits))?;
                let at = backend::byte_range(parent).end - 1;
                splices.push(Splice {
                    range: at..at,
                    value: Some(value),
                    insertion: Some(Insertion { key, comma }),
                    size,
                });
            }
        }
        // An insertion into a parent whose every child is removed needs no comma.
        let mut emptied = HashSet::new();
        for (close, index) in &mut removal_indexes {
            if index.plan(&mut splices)? {
                emptied.insert(*close);
            }
        }
        for splice in &mut splices {
            if emptied.contains(&splice.range.start)
                && let Some(insertion) = &mut splice.insertion
                && insertion.comma
            {
                insertion.comma = false;
                splice.size -= 1;
            }
        }
        splices.sort_by_key(|s| s.range.start);

        for i in 0..splices.len() {
            if i > 0 {
                let (left, right) = splices.split_at_mut(i);
                let prev = &left[i - 1];
                let next = &mut right[0];
                if prev.range.end > next.range.start {
                    return Err(EditError::Overlap);
                }
                if prev.range.start == next.range.start {
                    if prev.insertion.is_none() || next.insertion.is_none() {
                        return Err(EditError::Overlap);
                    }
                    let insertion = next.insertion.as_mut().unwrap();
                    if !insertion.comma {
                        insertion.comma = true;
                        next.size = next.size.checked_add(1).ok_or_else(|| limit(limits))?;
                    }
                }
            }
        }
        let mut inserted = BTreeSet::new();
        for splice in &splices {
            if let Some(Insertion { key: Some(key), .. }) = &splice.insertion
                && !inserted.insert((splice.range.start, key))
            {
                return Err(EditError::Overlap);
            }
        }
        let old = self.as_bytes();
        // Subtract all removed bytes first, so a replacement does not temporarily
        // charge both revisions or depend on the order of requests.
        let removed: usize = splices.iter().map(|s| s.range.len()).sum();
        let size = splices
            .iter()
            .try_fold(old.len() - removed, |n, s| n.checked_add(s.size))
            .filter(|&n| n <= limits.max_bytes)
            .ok_or_else(|| limit(limits))?;
        let mut out = BoundedWriter {
            bytes: Vec::with_capacity(size),
            reserved: size,
            limits,
        };
        let mut from = 0;
        for splice in splices {
            out.known(&old[from..splice.range.start])?;
            out.reserved -= splice.size;
            if let Some(insert) = splice.insertion {
                if insert.comma {
                    out.write_all(b",").map_err(|_| limit(limits))?;
                }
                if let Some(key) = insert.key {
                    key.write(&mut out)?;
                    out.write_all(b":").map_err(|_| limit(limits))?;
                }
            }
            if let Some(value) = splice.value {
                value.write(&mut out)?;
            }
            from = splice.range.end;
        }
        out.known(&old[from..])?;
        debug_assert_eq!(out.reserved, 0);
        let next = JsonValue::parse_owned_with_limits(out.bytes, limits)?;
        if next.has_duplicate_names() {
            return Err(EditError::DuplicateMembers);
        }
        Ok(Self::admitted(next, limits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn byte_budget_is_preflighted_before_writing_any_replacement() {
        // Known extents refuse before any replacement write.
        struct Witness(Cell<bool>);
        impl WriteJson for Witness {
            fn size(&self) -> Result<Option<usize>, EditError> {
                Ok(Some(100))
            }
            fn write(&self, _: &mut BoundedWriter) -> Result<(), EditError> {
                self.0.set(true);
                panic!("preflight must refuse");
            }
        }
        let document = Document::new();
        let original = document.as_bytes().to_vec();
        let witness = Witness(Cell::new(false));
        let error = document
            .apply(
                &[Request {
                    pointer: "/description".into(),
                    value: Some(&witness),
                }],
                JsonLimits {
                    max_bytes: original.len() + 5,
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert_eq!(error.code(), "byte-limit");
        assert!(!witness.0.get());
        assert_eq!(document.as_bytes(), original);
        let error = document
            .apply(
                &[Request {
                    pointer: "/description".into(),
                    value: Some(&JsonValue::parse(format!("\"{}\"", "a".repeat(100))).unwrap()),
                }],
                JsonLimits {
                    max_bytes: original.len() + 5,
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert_eq!(error.code(), "byte-limit");
        assert_eq!(document.as_bytes(), original);
    }

    #[test]
    fn final_parse_charges_node_and_depth_limits_atomically() {
        // The same parser limits govern the committed revision.
        for limits in [
            JsonLimits {
                max_nodes: 6,
                ..Default::default()
            },
            JsonLimits {
                max_depth: 1,
                ..Default::default()
            },
        ] {
            let doc = Document::new();
            let before = doc.as_bytes().to_vec();
            let value = JsonValue::parse("[[[1]]]").unwrap();
            let error = doc
                .apply(
                    &[Request {
                        pointer: "/x".into(),
                        value: Some(&value),
                    }],
                    limits,
                )
                .unwrap_err();
            assert!(matches!(error.code(), "node-limit" | "depth-limit"));
            assert_eq!(doc.as_bytes(), before);
        }
    }
    #[test]
    fn removal_planning_indexes_each_parent_once() {
        // Count planner work, independent of machine timing.
        for object in [false, true] {
            let items: Vec<_> = (0..1000)
                .map(|i| {
                    if object {
                        format!("\"k{i}\":{i}")
                    } else {
                        i.to_string()
                    }
                })
                .collect();
            let text = if object {
                format!("{{{}}}", items.join(","))
            } else {
                format!("[{}]", items.join(","))
            };
            let mut document = Document::parse(text).unwrap();
            let paths: Vec<_> = (0..1000)
                .step_by(2)
                .map(|i| {
                    if object {
                        format!("/k{i}")
                    } else {
                        format!("/{i}")
                    }
                })
                .collect();
            let edits: Vec<_> = paths.iter().map(Edit::remove_at).collect();
            PLANNING_WORK.with(|work| work.set((0, 0, 0)));
            document = document.edit(&edits).unwrap();
            assert_eq!(PLANNING_WORK.with(Cell::get), (1, 1000, 500));
            let remaining = items
                .iter()
                .skip(1)
                .step_by(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(",");
            let expected = if object {
                format!("{{{remaining}}}")
            } else {
                format!("[{remaining}]")
            };
            assert_eq!(document.as_bytes(), expected.as_bytes());
        }
    }
    #[test]
    fn bounded_writer_reserves_remaining_exact_bytes() {
        let limits = JsonLimits {
            max_bytes: 7,
            ..Default::default()
        };
        let mut writer = BoundedWriter {
            bytes: Vec::new(),
            reserved: 2,
            limits,
        };
        writer.write_all(b"12345").unwrap();
        assert!(writer.write_all(b"6").is_err());
        assert_eq!(writer.bytes, b"12345");
        assert!(writer.bytes.capacity() <= limits.max_bytes);
    }
}
