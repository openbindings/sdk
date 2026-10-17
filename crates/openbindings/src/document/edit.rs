//! Atomic edits against the current exact source revision.
use super::*;
use crate::authoring::Part;
use std::{io::Write, ops::Range};

/// One edit in an atomic batch. All pointers address the revision before the batch.
/// Parents must exist. Missing object members and array `-` append compactly.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum Edit<'a> {
    /// Replace a value or insert a missing member; preserve the value's exact token bytes.
    Set {
        /// RFC 6901 pointer, with no URI-fragment decoding.
        pointer: &'a str,
        /// Exact replacement value. Duplicate members are refused.
        value: &'a JsonValue,
    },
    /// Replace the complete operations map using checked typed parts.
    SetOperations {
        /// Borrowed parts, serialized once directly into the bounded splice buffer.
        value: &'a BTreeMap<String, Operation>,
    },
    /// Replace the complete sources map using checked typed parts.
    SetSources {
        /// Borrowed parts, serialized once directly into the bounded splice buffer.
        value: &'a BTreeMap<String, Source>,
    },
    /// Replace the complete bindings map using checked typed parts.
    SetBindings {
        /// Borrowed parts, serialized once directly into the bounded splice buffer.
        value: &'a BTreeMap<String, Binding>,
    },
    /// Replace the complete dependencies map using checked typed parts.
    SetDependencies {
        /// Borrowed parts, serialized once directly into the bounded splice buffer.
        value: &'a BTreeMap<String, Dependency>,
    },
    /// Remove an existing member or array element, including an adjacent separator.
    Remove {
        /// RFC 6901 pointer. Removing the root is refused.
        pointer: &'a str,
    },
}

impl fmt::Debug for Edit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Set { value, .. } => f
                .debug_struct("Set")
                .field("value", value)
                .finish_non_exhaustive(),
            Self::Remove { .. } => f.debug_struct("Remove").finish_non_exhaustive(),
            Self::SetOperations { .. } => f.debug_struct("SetOperations").finish_non_exhaustive(),
            Self::SetSources { .. } => f.debug_struct("SetSources").finish_non_exhaustive(),
            Self::SetBindings { .. } => f.debug_struct("SetBindings").finish_non_exhaustive(),
            Self::SetDependencies { .. } => {
                f.debug_struct("SetDependencies").finish_non_exhaustive()
            }
        }
    }
}

/// Atomic editing failure. The document and all its caches remain unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditError {
    /// Rebuilt JSON exceeded admission limits or could not be parsed.
    Input(InputError),
    /// A typed part could not be serialized without collisions or within limits.
    Authoring(AuthoringError),
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
            Self::Input(error) => error.code,
            Self::Authoring(error) => match error.kind() {
                AuthoringErrorKind::FieldCollision => "field-collision",
                AuthoringErrorKind::InvalidField => "invalid-field",
                AuthoringErrorKind::DuplicateMembers => "duplicate-members",
                AuthoringErrorKind::Serialization => "serialization",
                AuthoringErrorKind::Limit => "byte-limit",
            },
            Self::InvalidPointer => "invalid-pointer",
            Self::MissingParent => "missing-parent",
            Self::MissingValue => "missing-value",
            Self::NotContainer => "not-container",
            Self::DuplicateMembers => "duplicate-members",
            Self::Overlap => "overlapping-edits",
        }
    }
}
impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "document edit refused: {}", self.code())
    }
}
impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Authoring(error) => Some(error),
            _ => None,
        }
    }
}
impl From<InputError> for EditError {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}
impl From<AuthoringError> for EditError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}
fn limit(limits: JsonLimits) -> EditError {
    backend::input_error(InputErrorKind::Limit, "byte-limit", limits.max_bytes).into()
}
fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
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

// Exact inputs have a known byte extent. Typed parts stream once into the final
// splice buffer; every write reserves the untouched bytes and known replacements
// still to come, and refuses before growing beyond the final byte budget.
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
struct Typed<'a, T: ?Sized>(&'a T);
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
impl<T: Serialize + ?Sized> WriteJson for Typed<'_, T> {
    fn write(&self, out: &mut BoundedWriter) -> Result<(), EditError> {
        serde_json::to_writer(out, self.0).map_err(|error| {
            AuthoringError::plain(
                if error.is_io() {
                    AuthoringErrorKind::Limit
                } else {
                    AuthoringErrorKind::Serialization
                },
                "part could not be serialized within the supported JSON profile",
            )
            .into()
        })
    }
}
impl WriteJson for Edit<'_> {
    fn size(&self) -> Result<Option<usize>, EditError> {
        match self {
            Self::Set { value, .. } => value.size(),
            _ => Ok(None),
        }
    }
    fn write(&self, out: &mut BoundedWriter) -> Result<(), EditError> {
        match self {
            Self::Set { value, .. } => value.write(out),
            Self::SetOperations { value } => Typed(value).write(out),
            Self::SetSources { value } => Typed(value).write(out),
            Self::SetBindings { value } => Typed(value).write(out),
            Self::SetDependencies { value } => Typed(value).write(out),
            Self::Remove { .. } => Ok(()),
        }
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
    pointer: &'a str,
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
    key: Option<String>,
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
    /// Replace or insert an exact value at an RFC 6901 pointer.
    ///
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set(&mut self, pointer: &str, value: &JsonValue) -> Result<(), EditError> {
        self.edit(&[Edit::Set { pointer, value }])
    }
    /// Remove an existing member or array element.
    ///
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn remove(&mut self, pointer: &str) -> Result<(), EditError> {
        self.edit(&[Edit::Remove { pointer }])
    }
    /// Commit a batch with one splice and one parse, under the default JSON limits.
    /// All paths refer to the pre-edit revision. Sibling insertions share a boundary
    /// in batch order. Removing adjacent siblings, up to every member of a container,
    /// is not an overlap. Overlapping replacements/removals are refused. Untouched bytes
    /// retain their exact spelling, formatting and order. Positions describe current
    /// bytes; retained views and proofs continue to describe their earlier revision.
    ///
    /// # Errors
    /// Invalid paths, duplicates, overlap and admission failures leave the document
    /// unchanged. Known byte extents are preflighted; typed parts serialize once
    /// into a bounded writer that reserves all remaining known bytes before growing.
    pub fn edit(&mut self, edits: &[Edit<'_>]) -> Result<(), EditError> {
        let limits = JsonLimits::default();
        if edits.len() > limits.max_nodes {
            return Err(backend::input_error(InputErrorKind::Limit, "node-limit", 0).into());
        }
        let mut requests = Vec::with_capacity(edits.len());
        for edit in edits {
            let pointer = match edit {
                Edit::Set { pointer, .. } | Edit::Remove { pointer } => *pointer,
                Edit::SetOperations { value } => {
                    value.check(&["operations"])?;
                    "/operations"
                }
                Edit::SetSources { value } => {
                    value.check(&["sources"])?;
                    "/sources"
                }
                Edit::SetBindings { value } => {
                    value.check(&["bindings"])?;
                    "/bindings"
                }
                Edit::SetDependencies { value } => {
                    value.check(&["dependencies"])?;
                    "/dependencies"
                }
            };
            requests.push(Request {
                pointer,
                value: if matches!(edit, Edit::Remove { .. }) {
                    None
                } else {
                    Some(edit)
                },
            });
        }
        self.apply(&requests, limits)
    }
    fn apply(&mut self, edits: &[Request<'_>], limits: JsonLimits) -> Result<(), EditError> {
        if edits.is_empty() {
            return Ok(());
        }
        if self.value().has_duplicate_names() {
            return Err(EditError::DuplicateMembers);
        }
        let mut splices = Vec::with_capacity(edits.len());
        // Removals are indexed by their parent's closing position.
        let mut removal_indexes = HashMap::new();
        for edit in edits {
            let pointer = edit.pointer;
            if pointer.len() > limits.max_bytes {
                return Err(limit(limits));
            }
            if !pointer.is_empty() && !pointer.starts_with('/') {
                return Err(EditError::InvalidPointer);
            }
            // Validate every escape, including segments leading to existing values.
            for token in pointer.split('/').skip(1) {
                decode(token)?;
            }
            let size = edit
                .value
                .map(|v| v.size())
                .transpose()?
                .flatten()
                .unwrap_or(0);
            if let Some(target) = self.value().at(pointer) {
                let range = backend::byte_range(target);
                if edit.value.is_none() {
                    let (parent, _) = pointer.rsplit_once('/').ok_or(EditError::InvalidPointer)?;
                    let parent = self.value().at(parent).ok_or(EditError::MissingParent)?;
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
                let (parent, token) = pointer.rsplit_once('/').ok_or(EditError::InvalidPointer)?;
                let key = decode(token)?;
                let parent = self.value().at(parent).ok_or(EditError::MissingParent)?;
                let comma = parent.is_empty() == Some(false);
                let key = match parent.kind() {
                    JsonKind::Object => Some(key),
                    JsonKind::Array if key == "-" => None,
                    JsonKind::Array => return Err(EditError::InvalidPointer),
                    _ => return Err(EditError::NotContainer),
                };
                let prefix = match &key {
                    Some(key) => key_size(key).checked_add(1).ok_or_else(|| limit(limits))?,
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
        let mut inserted = HashSet::new();
        for splice in &splices {
            if let Some(Insertion { key: Some(key), .. }) = &splice.insertion
                && !inserted.insert((splice.range.start, key.as_str()))
            {
                return Err(EditError::Overlap);
            }
        }
        let old = self.to_bytes();
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
                    Typed(&key).write(&mut out)?;
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
        let next = JsonValue::parse_with_limits(out.bytes, limits)?;
        if next.has_duplicate_names() {
            return Err(EditError::DuplicateMembers);
        }
        *self = Self::from_json(next);
        Ok(())
    }
    fn set_typed<T: Serialize + ?Sized>(
        &mut self,
        pointer: &str,
        value: &T,
    ) -> Result<(), EditError> {
        self.apply(
            &[Request {
                pointer,
                value: Some(&Typed(value)),
            }],
            JsonLimits::default(),
        )
    }
    fn set_part<T: Part>(
        &mut self,
        namespace: &str,
        key: &str,
        value: &T,
    ) -> Result<(), EditError> {
        value.check(&[namespace, key])?;
        if self.value().get(namespace).is_some() {
            self.set_typed(&format!("/{namespace}/{}", escape(key)), value)
        } else {
            self.set_typed(&format!("/{namespace}"), &BTreeMap::from([(key, value)]))
        }
    }
    /// Set the specification version declaration. Conformance is assessed separately.
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set_openbindings(&mut self, value: &str) -> Result<(), EditError> {
        self.set_typed("/openbindings", value)
    }
    /// Set the interface name.
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set_name(&mut self, value: &str) -> Result<(), EditError> {
        self.set_typed("/name", value)
    }
    /// Set the interface version label.
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set_version(&mut self, value: &str) -> Result<(), EditError> {
        self.set_typed("/version", value)
    }
    /// Set the interface description.
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set_description(&mut self, value: &str) -> Result<(), EditError> {
        self.set_typed("/description", value)
    }
    /// Set an existing operation's description, escaping its key as an RFC 6901 token.
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set_operation_description(&mut self, key: &str, value: &str) -> Result<(), EditError> {
        self.set_typed(&format!("/operations/{}/description", escape(key)), value)
    }
    /// Set an existing operation's exact input schema. Validation is separate.
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set_operation_input(&mut self, key: &str, value: &JsonValue) -> Result<(), EditError> {
        self.set(&format!("/operations/{}/input", escape(key)), value)
    }
    /// Set an existing operation's exact output schema. Validation is separate.
    /// # Errors
    /// Returns an editing error without changing the document.
    pub fn set_operation_output(&mut self, key: &str, value: &JsonValue) -> Result<(), EditError> {
        self.set(&format!("/operations/{}/output", escape(key)), value)
    }
    /// Set one whole operation, creating its namespace if absent.
    /// # Errors
    /// Returns an editing or typed-part error without changing the document.
    pub fn set_operation(&mut self, key: &str, value: &Operation) -> Result<(), EditError> {
        self.set_part("operations", key, value)
    }
    /// Commit a complete operations map in one splice and parse, for bulk synthesis.
    /// # Errors
    /// Returns an editing or typed-part error without changing the document.
    pub fn set_operations(&mut self, value: &BTreeMap<String, Operation>) -> Result<(), EditError> {
        self.edit(&[Edit::SetOperations { value }])
    }
    /// Set one whole binding, creating its namespace if absent.
    /// # Errors
    /// Returns an editing or typed-part error without changing the document.
    pub fn set_binding(&mut self, key: &str, value: &Binding) -> Result<(), EditError> {
        self.set_part("bindings", key, value)
    }
    /// Set one whole source, creating its namespace if absent.
    /// # Errors
    /// Returns an editing or typed-part error without changing the document.
    pub fn set_source(&mut self, key: &str, value: &Source) -> Result<(), EditError> {
        self.set_part("sources", key, value)
    }
    /// Set one whole dependency, creating its namespace if absent.
    /// # Errors
    /// Returns an editing or typed-part error without changing the document.
    pub fn set_dependency(&mut self, key: &str, value: &Dependency) -> Result<(), EditError> {
        self.set_part("dependencies", key, value)
    }
    /// Set an example on an existing operation, creating its examples map if absent.
    /// # Errors
    /// Returns an editing or typed-part error without changing the document.
    pub fn set_example(
        &mut self,
        operation: &str,
        key: &str,
        value: &Example,
    ) -> Result<(), EditError> {
        value.check(&["operations", operation, "examples", key])?;
        let namespace = format!("/operations/{}/examples", escape(operation));
        if self.value().at(&namespace).is_some() {
            self.set_typed(&format!("{namespace}/{}", escape(key)), value)
        } else {
            self.set_typed(&namespace, &BTreeMap::from([(key, value)]))
        }
    }
}

macro_rules! set_namespace {
    ($method:ident, $field:literal, $part:ty, $variant:ident) => {
        impl Document {
            #[doc = concat!("Commit a complete `", $field, "` map in one splice and parse.\n\n# Errors\nReturns an editing or typed-part error without changing the document.")]
            pub fn $method(&mut self, value: &BTreeMap<String, $part>) -> Result<(), EditError> {
                self.edit(&[Edit::$variant { value }])
            }
        }
    };
}
set_namespace!(set_bindings, "bindings", Binding, SetBindings);
set_namespace!(set_sources, "sources", Source, SetSources);
set_namespace!(
    set_dependencies,
    "dependencies",
    Dependency,
    SetDependencies
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn byte_budget_is_preflighted_before_writing_any_replacement() {
        // Known extents refuse before encoding; typed writes stop at the budget.
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
        let mut document = Document::new();
        let original = document.to_bytes().to_vec();
        let witness = Witness(Cell::new(false));
        let error = document
            .apply(
                &[Request {
                    pointer: "/description",
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
        assert_eq!(document.to_bytes(), original);
        let error = document
            .apply(
                &[Request {
                    pointer: "/description",
                    value: Some(&Typed(&"a".repeat(100))),
                }],
                JsonLimits {
                    max_bytes: original.len() + 5,
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert_eq!(error.code(), "byte-limit");
        assert_eq!(document.to_bytes(), original);
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
            let mut doc = Document::new();
            let before = doc.to_bytes().to_vec();
            let value = JsonValue::parse("[[[1]]]").unwrap();
            let error = doc
                .apply(
                    &[Request {
                        pointer: "/x",
                        value: Some(&value),
                    }],
                    limits,
                )
                .unwrap_err();
            assert!(matches!(error.code(), "node-limit" | "depth-limit"));
            assert_eq!(doc.to_bytes(), before);
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
            let edits: Vec<_> = paths
                .iter()
                .map(|pointer| Edit::Remove { pointer })
                .collect();
            PLANNING_WORK.with(|work| work.set((0, 0, 0)));
            document.edit(&edits).unwrap();
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
            assert_eq!(document.to_bytes(), expected.as_bytes());
        }
    }
    #[test]
    fn typed_serialization_runs_once_and_stops_before_the_byte_budget() {
        // No counting serialization pass and no over-budget write.
        struct Witness {
            calls: Cell<usize>,
            fields: Cell<usize>,
        }
        impl Serialize for Witness {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeSeq;
                self.calls.set(self.calls.get() + 1);
                let mut seq = serializer.serialize_seq(Some(100))?;
                for _ in 0..100 {
                    self.fields.set(self.fields.get() + 1);
                    seq.serialize_element("0123456789")?;
                }
                seq.end()
            }
        }
        let witness = Witness {
            calls: Cell::new(0),
            fields: Cell::new(0),
        };
        let mut document = Document::new();
        document.set_typed("/x", &witness).unwrap();
        assert_eq!(witness.calls.get(), 1);
        let before = document.to_bytes().to_vec();
        witness.calls.set(0);
        witness.fields.set(0);
        let error = document
            .apply(
                &[Request {
                    pointer: "/x",
                    value: Some(&Typed(&witness)),
                }],
                JsonLimits {
                    max_bytes: 100,
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert_eq!(error.code(), "byte-limit");
        assert_eq!(witness.calls.get(), 1);
        assert!(witness.fields.get() < 10);
        assert_eq!(document.to_bytes(), before);
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
