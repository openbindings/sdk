//! Private flat exact-JSON storage; iterative parsing and destruction.
//!
//! Every node is one fixed-size record. Containers name a range of the shared
//! `items` list (array elements, or object member-name tokens, each member's
//! value being the node after its name), escaped scalar strings name a range
//! of one shared decoded buffer, a string holding an unpaired UTF-16 unit is
//! read from its source token, and parsing keeps no per-level frame: the open
//! container's parent link is the way back. Ids are `u32` in storage because
//! admission caps the node count at `u32::MAX`.
use std::{
    cmp::Ordering,
    ops::Range,
    sync::{Arc, Mutex},
};

pub type Id = usize;

const INDEX_MIN_MEMBERS: usize = 16;
/// The pending items a parse first makes room for, when its first container
/// opens: a small document's containers then fit without growing the list, and
/// a document without containers allocates none.
const PENDING_START: usize = 64;
/// The parent link of a root node.
pub const NO_PARENT: u32 = u32::MAX;

/// First occurrences in key order, borrowing the arena's flat member index.
pub(crate) struct MemberCandidates<'a> {
    remaining: &'a [u32],
}
impl Iterator for MemberCandidates<'_> {
    type Item = Id;
    fn next(&mut self) -> Option<Id> {
        let (&name, rest) = self.remaining.split_first()?;
        self.remaining = rest;
        Some(name as usize)
    }
}

#[inline]
fn key_prefix(bytes: &[u8]) -> u64 {
    let mut prefix = [0u8; 8];
    let len = bytes.len().min(8);
    prefix[..len].copy_from_slice(&bytes[..len]);
    u64::from_be_bytes(prefix)
}

// Compare the leading eight bytes as one integer before the remaining slice.
// This preserves byte order and avoids a byte-at-a-time prefix scan on Wasm.
#[inline]
fn scalar_cmp(a: &str, b: &str) -> Ordering {
    bytes_cmp(a.as_bytes(), b.as_bytes())
}
#[inline]
fn bytes_cmp(a: &[u8], b: &[u8]) -> Ordering {
    if a.len() >= 8 && b.len() >= 8 {
        let x = u64::from_be_bytes(a[..8].try_into().expect("eight bytes"));
        let y = u64::from_be_bytes(b[..8].try_into().expect("eight bytes"));
        x.cmp(&y).then_with(|| a[8..].cmp(&b[8..]))
    } else {
        a.cmp(b)
    }
}
#[inline]
fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if a.len() >= 8 {
        a[..8] == b[..8] && a[8..] == b[8..]
    } else {
        a == b
    }
}

/// How a string token's decoded value is held: an index into the decoded
/// strings, or one of two marks. Every node but a container root can be an
/// escaped string, so an index stays below both marks. A plain number keeps
/// [`Kind`] a 12-byte record whose tag is read directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text(u32);
impl Text {
    /// No escapes: the value is the source between the quotes.
    pub const SOURCE: Self = Self(u32::MAX);
    /// Escapes that leave an unpaired UTF-16 unit; the units are read from
    /// the token whenever asked for.
    pub const UNPAIRED: Self = Self(u32::MAX - 1);
    /// Escaped scalar text: its index into the decoded strings.
    pub fn decoded(self) -> Option<u32> {
        (self.0 < u32::MAX - 1).then_some(self.0)
    }
}
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Kind {
    Null,
    Bool(bool),
    Number,
    /// `name` marks an object member-name token.
    String {
        text: Text,
        name: bool,
    },
    /// `len` items starting at `first` in [`Arena::items`].
    Array {
        first: u32,
        len: u32,
    },
    /// `len` member names starting at `first` in [`Arena::items`].
    Object {
        first: u32,
        len: u32,
    },
}
#[derive(Debug, Clone, Copy)]
pub enum Edge {
    Key(Id),
    Index(usize),
}
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    /// The containing array or object, or [`NO_PARENT`] for the root.
    pub parent: u32,
    pub span: Range<usize>,
}
impl Node {
    pub fn is_container(&self) -> bool {
        matches!(self.kind, Kind::Array { .. } | Kind::Object { .. })
    }
}
#[derive(Debug, Clone)]
pub struct Problem {
    pub code: &'static str,
    pub offset: usize,
}
impl Problem {
    pub fn is_incomplete(&self) -> bool {
        self.code == "incomplete"
    }
}
fn bom() -> Problem {
    Problem {
        code: "bom",
        offset: 0,
    }
}
fn utf8(offset: usize) -> Problem {
    Problem {
        code: "utf8",
        offset,
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_depth: usize,
    pub max_nodes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 10_000,
            max_nodes: 1_000_000,
        }
    }
}
static LIVE_ARENAS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[derive(Debug)]
struct Lifetime;
impl Lifetime {
    fn new() -> Self {
        LIVE_ARENAS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self
    }
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        LIVE_ARENAS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}
pub fn live_arenas() -> usize {
    LIVE_ARENAS.load(std::sync::atomic::Ordering::Relaxed)
}

const POSITION_STRIDE: usize = 4096;

#[derive(Debug, Clone, Copy)]
struct PositionCheckpoint {
    line: usize,
    line_start: usize,
}
impl PositionCheckpoint {
    const START: Self = Self {
        line: 1,
        line_start: 0,
    };
}

#[derive(Debug, Default)]
struct PositionIndex {
    // Slot n describes byte (n + 1) * POSITION_STRIDE. The zero checkpoint is
    // implicit. A boxed buffer gives an exact slot bound, including spare slots.
    checkpoints: Box<[PositionCheckpoint]>,
    populated: usize,
}
impl PositionIndex {
    fn reserve(&mut self, required: usize, maximum: usize) {
        if required <= self.checkpoints.len() {
            return;
        }
        let capacity = required
            .max(self.checkpoints.len().saturating_mul(2))
            .min(maximum);
        let mut checkpoints = vec![PositionCheckpoint::START; capacity].into_boxed_slice();
        checkpoints[..self.populated].copy_from_slice(&self.checkpoints[..self.populated]);
        self.checkpoints = checkpoints;
    }
}

#[derive(Debug)]
pub struct Arena {
    _lifetime: Lifetime,
    source: Box<str>,
    positions: Mutex<PositionIndex>,
    // Objects of at least INDEX_MIN_MEMBERS members publish sorted first
    // occurrences after parsing, in one exactly sized allocation. Each record
    // caches eight key bytes after the object's common scalar prefix. The
    // immutable index is shared by retained views and threads without locks.
    member_indexes: Vec<(u32, Range<usize>, usize)>,
    member_slots: Vec<(u64, u32)>,
    #[cfg(test)]
    key_comparisons: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    member_probes: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    position_scanned: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub duplicate_query_probes: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub unpaired_query_probes: std::sync::atomic::AtomicUsize,
    pub nodes: Vec<Node>,
    /// Every container's items, each container's contiguous and in source order.
    items: Vec<u32>,
    /// Escaped scalar strings' decoded text, back to back, and where each ends.
    decoded: String,
    decoded_ends: Vec<usize>,
    /// (object, repeated member-name token) in source order of the tokens.
    pub duplicates: Vec<(u32, u32)>,
    /// String tokens holding an unpaired UTF-16 unit, in source order.
    pub unpaired: Vec<u32>,
}
impl Arena {
    pub fn retained_bytes(&self) -> usize {
        use crate::backend::vec_bytes;
        // Arc header, arena allocation, and all separately allocated buffers.
        2 * size_of::<usize>()
            + size_of::<Self>()
            + self.source.len()
            + vec_bytes(&self.nodes)
            + vec_bytes(&self.items)
            + self.decoded.capacity()
            + vec_bytes(&self.decoded_ends)
            + vec_bytes(&self.duplicates)
            + vec_bytes(&self.unpaired)
            + vec_bytes(&self.member_indexes)
            + vec_bytes(&self.member_slots)
            + size_of_val(
                self.positions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .checkpoints
                    .as_ref(),
            )
    }
    /// Parse into a shared immutable owner.
    pub fn parse(bytes: &[u8], limits: Limits) -> Result<Arc<Self>, Problem> {
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(bom());
        }
        let text = std::str::from_utf8(bytes).map_err(|e| utf8(e.valid_up_to()))?;
        Self::parse_source(Box::from(text), limits).map_err(|(problem, _)| problem)
    }
    /// Parse owned bytes, which become the owner's source without a copy.
    /// A refusal hands the bytes back.
    pub fn parse_owned(bytes: Vec<u8>, limits: Limits) -> Result<Arc<Self>, (Problem, Vec<u8>)> {
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err((bom(), bytes));
        }
        let text = String::from_utf8(bytes).map_err(|e| {
            let offset = e.utf8_error().valid_up_to();
            (utf8(offset), e.into_bytes())
        })?;
        Self::parse_source(text.into_boxed_str(), limits)
            .map_err(|(problem, source)| (problem, source.into_string().into_bytes()))
    }
    fn new(source: Box<str>) -> Self {
        Self {
            _lifetime: Lifetime::new(),
            source,
            positions: Mutex::default(),
            member_indexes: Vec::new(),
            member_slots: Vec::new(),
            #[cfg(test)]
            key_comparisons: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            member_probes: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            position_scanned: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            duplicate_query_probes: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            unpaired_query_probes: std::sync::atomic::AtomicUsize::new(0),
            nodes: Vec::new(),
            items: Vec::new(),
            decoded: String::new(),
            decoded_ends: Vec::new(),
            duplicates: Vec::new(),
            unpaired: Vec::new(),
        }
    }
    // Parse `source` into a new owner; a refusal hands the source back.
    fn parse_source(source: Box<str>, limits: Limits) -> Result<Arc<Self>, (Problem, Box<str>)> {
        let mut owner = Self::new(Box::default());
        let parsed = Parser {
            out: &mut owner,
            source: Source::trusted(&source),
            pos: 0,
            open: NO_PARENT,
            state: 0,
            depth: 0,
            pending: Vec::new(),
            limits,
        }
        .document();
        match parsed {
            Ok(()) => {
                owner.source = source;
                Ok(Arc::new(owner))
            }
            Err(problem) => Err((problem, source)),
        }
    }
    /// Parse once over borrowed input, copying only the completed source.
    pub fn parse_prefix(source: &[u8], limits: Limits) -> Result<(Arc<Self>, usize), Problem> {
        if source.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(bom());
        }
        let mut owner = Self::new(Box::default());
        let mut parser = Parser {
            out: &mut owner,
            source: Source::prefix(source),
            pos: 0,
            open: NO_PARENT,
            state: 0,
            depth: 0,
            pending: Vec::new(),
            limits,
        };
        parser.value_end()?;
        if parser.pos == source.len() && matches!(parser.out.nodes[0].kind, Kind::Number) {
            return Err(parser.error("number"));
        }
        let end = parser.pos;
        parser.finish();
        parser.out.source = Box::from(prefix_utf8(&source[..end]).expect("validated JSON source"));
        Ok((Arc::new(owner), end))
    }
    fn compare_keys_in(&self, source: &Source<'_>, left: Id, right: Id) -> Ordering {
        #[cfg(test)]
        self.key_comparisons
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match (
            self.string_bytes_in(source, left),
            self.string_bytes_in(source, right),
        ) {
            (Some(a), Some(b)) => bytes_cmp(a, b),
            (None, None) => self
                .utf16_in(source, left)
                .cmp(self.utf16_in(source, right)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
        }
    }
    fn equal_keys_in(&self, source: &Source<'_>, left: Id, right: Id) -> bool {
        #[cfg(test)]
        self.key_comparisons
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match (
            self.string_bytes_in(source, left),
            self.string_bytes_in(source, right),
        ) {
            (Some(a), Some(b)) => bytes_eq(a, b),
            (None, None) => self.utf16_in(source, left).eq(self.utf16_in(source, right)),
            _ => false,
        }
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn raw(&self, id: Id) -> &str {
        &self.source[self.nodes[id].span.clone()]
    }
    /// An array's elements, or `None` for any other node.
    #[inline]
    pub fn elements(&self, id: Id) -> Option<&[u32]> {
        match self.nodes[id].kind {
            Kind::Array { first, len } => {
                Some(&self.items[first as usize..first as usize + len as usize])
            }
            _ => None,
        }
    }
    /// An object's member-name tokens, or `None` for any other node. Each
    /// member's value is the node after its name.
    #[inline]
    pub fn member_names(&self, id: Id) -> Option<&[u32]> {
        match self.nodes[id].kind {
            Kind::Object { first, len } => {
                Some(&self.items[first as usize..first as usize + len as usize])
            }
            _ => None,
        }
    }
    /// The parent container and the edge from it, or `None` for a root.
    pub fn edge(&self, id: Id) -> Option<(Id, Edge)> {
        let node = &self.nodes[id];
        if node.parent == NO_PARENT {
            return None;
        }
        let parent = node.parent as usize;
        let edge = match self.nodes[parent].kind {
            // A member's value is the node after its name.
            Kind::Object { .. } => Edge::Key(match node.kind {
                Kind::String { name: true, .. } => id,
                _ => id - 1,
            }),
            Kind::Array { first, len } => Edge::Index(
                self.items[first as usize..first as usize + len as usize]
                    .binary_search(&(id as u32))
                    .expect("an element of its parent"),
            ),
            _ => unreachable!("a parent is a container"),
        };
        Some((parent, edge))
    }
    pub fn has_duplicate_names(&self, id: Id) -> bool {
        let node = &self.nodes[id];
        if !node.is_container() {
            return false;
        }
        // Duplicate key tokens are in source order. Object IDs are not ordered
        // here: a nested duplicate can precede its parent's next duplicate. A
        // key inside a container belongs to it or a descendant.
        let first = self.duplicates.partition_point(|&(_, key)| {
            #[cfg(test)]
            self.duplicate_query_probes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.nodes[key as usize].span.start < node.span.start
        });
        self.duplicates
            .get(first)
            .is_some_and(|&(_, key)| self.nodes[key as usize].span.start < node.span.end)
    }
    // Append source text to the decoded buffer.
    #[inline]
    fn decode(&mut self, source: &Source<'_>, range: Range<usize>) {
        source.decode(self, range);
    }
    // Grow the decoded buffer geometrically, but never past the source's
    // length, which bounds all decoded text.
    fn reserve_decoded(&mut self, source_len: usize, additional: usize) {
        let (len, capacity) = (self.decoded.len(), self.decoded.capacity());
        if capacity - len >= additional {
            return;
        }
        let target = (len + additional).max(capacity.saturating_mul(2).min(source_len));
        self.decoded.reserve_exact(target - len);
    }
    fn decoded_text(&self, index: u32) -> &str {
        let index = index as usize;
        let start = if index == 0 {
            0
        } else {
            self.decoded_ends[index - 1]
        };
        &self.decoded[start..self.decoded_ends[index]]
    }
    /// A string's scalar text, or `None` for a non-string or one holding an
    /// unpaired unit.
    #[inline]
    pub fn string(&self, id: Id) -> Option<&str> {
        self.string_in(&Source::trusted(&self.source), id)
    }
    #[inline]
    fn string_in<'a>(&'a self, source: &Source<'a>, id: Id) -> Option<&'a str> {
        let node = &self.nodes[id];
        match node.kind {
            Kind::String { text, .. } => match text.decoded() {
                Some(index) => Some(self.decoded_text(index)),
                None if text == Text::SOURCE => {
                    Some(source.text(node.span.start + 1..node.span.end - 1))
                }
                None => None,
            },
            _ => None,
        }
    }
    // Parsed scalar names compare as UTF-8 bytes. Avoid rebuilding a string
    // view (and its validation route) for each internal key comparison.
    #[inline]
    fn string_bytes_in<'a>(&'a self, source: &Source<'a>, id: Id) -> Option<&'a [u8]> {
        let node = &self.nodes[id];
        match node.kind {
            Kind::String { text, .. } => match text.decoded() {
                Some(index) => Some(self.decoded_text(index).as_bytes()),
                None if text == Text::SOURCE => {
                    Some(&source.bytes[node.span.start + 1..node.span.end - 1])
                }
                None => None,
            },
            _ => None,
        }
    }
    /// The units of a string holding an unpaired unit, or `None` otherwise.
    pub fn unpaired_units(&self, id: Id) -> Option<Utf16<'_>> {
        match self.nodes[id].kind {
            Kind::String {
                text: Text::UNPAIRED,
                ..
            } => Some(self.utf16(id)),
            _ => None,
        }
    }
    /// A string token's UTF-16 units, decoded from its source.
    fn utf16(&self, id: Id) -> Utf16<'_> {
        self.utf16_in(&Source::trusted(&self.source), id)
    }
    fn utf16_in<'a>(&'a self, source: &Source<'a>, id: Id) -> Utf16<'a> {
        let span = &self.nodes[id].span;
        Utf16 {
            body: source.text(span.start + 1..span.end - 1),
            at: 0,
            low: None,
        }
    }
    /// Separately held decoded bytes of an escaped scalar string; zero for one
    /// that borrows its source.
    #[cfg(test)]
    pub fn decoded_len(&self, id: Id) -> usize {
        match self.nodes[id].kind {
            Kind::String { text, .. } => text.decoded().map_or(0, |i| self.decoded_text(i).len()),
            _ => 0,
        }
    }
    pub fn units(&self, id: Id) -> Option<Vec<u16>> {
        match self.unpaired_units(id) {
            Some(units) => Some(units.collect()),
            None => self.string(id).map(|s| s.encode_utf16().collect()),
        }
    }
    // Whether the member name token `name` spells `key`: `string(name) ==
    // Some(key)`, deciding an unescaped name by its length and first byte
    // before comparing the rest.
    #[inline]
    fn name_is(&self, name: Id, key: &str) -> bool {
        #[cfg(test)]
        self.key_comparisons
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let node = &self.nodes[name];
        match node.kind {
            Kind::String {
                text: Text::SOURCE, ..
            } => {
                let body = &self.source.as_bytes()[node.span.start + 1..node.span.end - 1];
                let key = key.as_bytes();
                body.len() == key.len() && body.first() == key.first() && body == key
            }
            _ => self.string(name) == Some(key),
        }
    }
    pub fn get(&self, id: Id, key: &str) -> Option<Id> {
        self.get_with_probe(id, key, || true)
    }
    fn member_index(&self, object: Id) -> Option<(&[(u64, u32)], usize)> {
        self.member_index_with_probe(object, &mut || true)
    }
    fn member_index_with_probe(
        &self,
        object: Id,
        probe: &mut impl FnMut() -> bool,
    ) -> Option<(&[(u64, u32)], usize)> {
        let mut range = 0..self.member_indexes.len();
        while range.start < range.end {
            if !probe() {
                return None;
            }
            let mid = range.start + range.len() / 2;
            let (id, slots, common) = &self.member_indexes[mid];
            match (*id as usize).cmp(&object) {
                Ordering::Less => range.start = mid + 1,
                Ordering::Greater => range.end = mid,
                Ordering::Equal => return Some((&self.member_slots[slots.clone()], *common)),
            }
        }
        None
    }
    pub(crate) fn member_candidates<E>(
        &self,
        id: Id,
        scalar: Option<&str>,
        units: impl Iterator<Item = u16>,
        token_bytes: usize,
        work: &mut impl FnMut(usize) -> Result<(), E>,
    ) -> Result<MemberCandidates<'_>, E> {
        let names = self.member_names(id).unwrap_or(&[]);
        let search = self.member_indexes.len().checked_ilog2().unwrap_or(0) as usize + 1;
        let steps = names.len().checked_ilog2().unwrap_or(0) as usize + 1;
        // Prepay decoding and every binary-search comparison, bounded by the
        // query token's length. Keep the same minimum payment for small objects.
        work(
            token_bytes
                .saturating_mul(16)
                .saturating_mul(steps)
                .saturating_add(search),
        )?;
        let Some((index, _)) = self.member_index(id) else {
            return Ok(MemberCandidates { remaining: names });
        };
        // Scalar text is already decoded in its arena; only unpaired tokens
        // need a copy of their units.
        let mut decoded = Vec::new();
        if scalar.is_none() {
            for (at, unit) in units.enumerate() {
                if at % 1024 == 0 {
                    work(1)?;
                }
                decoded.push(unit);
            }
        }
        work(1)?;
        let mut range = 0..index.len();
        while range.start < range.end {
            work(1)?;
            let mid = range.start + range.len() / 2;
            let name = index[mid].1 as usize;
            #[cfg(test)]
            self.key_comparisons
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let order = match (self.string(name), scalar) {
                (Some(a), Some(b)) => scalar_cmp(a, b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => {
                    let mut candidate = self.utf16(name);
                    let mut order = Ordering::Equal;
                    for (at, &unit) in decoded.iter().enumerate() {
                        if at % 1024 == 0 {
                            work(1)?;
                        }
                        order = candidate.next().cmp(&Some(unit));
                        if order != Ordering::Equal {
                            break;
                        }
                    }
                    if order == Ordering::Equal {
                        order = candidate.next().cmp(&None);
                    }
                    order
                }
            };
            match order {
                Ordering::Less => range.start = mid + 1,
                Ordering::Greater => range.end = mid,
                Ordering::Equal => {
                    return Ok(MemberCandidates {
                        remaining: std::slice::from_ref(&index[mid].1),
                    });
                }
            }
        }
        Ok(MemberCandidates { remaining: &[] })
    }
    pub(crate) fn get_with_probe(
        &self,
        id: Id,
        key: &str,
        mut probe: impl FnMut() -> bool,
    ) -> Option<Id> {
        let names = self.member_names(id)?;
        if names.len() >= INDEX_MIN_MEMBERS {
            let (index, common) = self.member_index_with_probe(id, &mut probe)?;
            let prefix = key_prefix(key.as_bytes().get(common..)?);
            let mut exhausted = false;
            let found = index
                .binary_search_by(|&(candidate, name)| {
                    if exhausted || !probe() {
                        exhausted = true;
                        return Ordering::Less;
                    }
                    #[cfg(test)]
                    {
                        self.member_probes
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        self.key_comparisons
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    // A key outside the common prefix can only miss; prefix ties
                    // still compare the full key before returning a match.
                    candidate.cmp(&prefix).then_with(|| {
                        self.string(name as usize)
                            .map_or(Ordering::Greater, |s| scalar_cmp(s, key))
                    })
                })
                .ok()?;
            return (!exhausted).then(|| index[found].1 as usize + 1);
        } else {
            for &name in names {
                if !probe() {
                    return None;
                }
                #[cfg(test)]
                self.member_probes
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if self.name_is(name as usize, key) {
                    return Some(name as usize + 1);
                }
            }
        }
        None
    }
    pub fn at(&self, pointer: &str) -> Option<Id> {
        self.at_from(0, pointer)
    }
    pub(crate) fn at_from(&self, id: Id, pointer: &str) -> Option<Id> {
        if pointer.is_empty() {
            return Some(id);
        }
        let rest = pointer.strip_prefix('/')?;
        let mut current = id;
        for segment in rest.split('/') {
            // Only a segment with escapes needs a decoded copy.
            let decoded;
            let name = if segment.contains('~') {
                let mut name = String::with_capacity(segment.len());
                let mut chars = segment.chars();
                while let Some(c) = chars.next() {
                    if c == '~' {
                        name.push(match chars.next()? {
                            '0' => '~',
                            '1' => '/',
                            _ => return None,
                        });
                    } else {
                        name.push(c);
                    }
                }
                decoded = name;
                decoded.as_str()
            } else {
                segment
            };
            current = match self.nodes[current].kind {
                Kind::Object { .. } => self.get(current, name)?,
                Kind::Array { .. } => {
                    if name.len() > 1 && name.starts_with('0') {
                        return None;
                    }
                    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
                        return None;
                    }
                    *self.elements(current)?.get(name.parse::<usize>().ok()?)? as usize
                }
                _ => return None,
            };
        }
        Some(current)
    }
    pub fn pointer(&self, id: Id) -> Option<String> {
        self.pointer_from(id, 0)
    }
    /// The scalar pointer of `id` from its ancestor `root`, which may be `id`.
    pub fn pointer_from(&self, id: Id, root: Id) -> Option<String> {
        // Measure first: an unrepresentable ancestor makes the whole pointer
        // unavailable before anything is copied, and the pointer then takes one
        // exact allocation, written from its last segment back.
        let size = self.pointer_len(id, root)?;
        let mut bytes = vec![0u8; size];
        self.fill_pointer(id, root, &mut bytes);
        Some(String::from_utf8(bytes).expect("segments are UTF-8"))
    }
    /// Append this node's pointer to `out`, as [`Self::pointer`] spells it;
    /// `false`, appending nothing, when it is unavailable.
    pub fn pointer_into(&self, id: Id, out: &mut Vec<u8>) -> bool {
        let Some(size) = self.pointer_len(id, 0) else {
            return false;
        };
        let start = out.len();
        out.resize(start + size, 0);
        self.fill_pointer(id, 0, &mut out[start..]);
        true
    }
    // The edges from `id` up to its ancestor `root`, last segment first.
    pub(crate) fn edges(&self, mut current: Id, root: Id) -> impl Iterator<Item = Edge> + '_ {
        std::iter::from_fn(move || {
            (current != root).then(|| {
                let (parent, edge) = self.edge(current).expect("an ancestor of the node");
                current = parent;
                edge
            })
        })
    }
    fn pointer_len(&self, id: Id, root: Id) -> Option<usize> {
        let mut size = 0usize;
        for edge in self.edges(id, root) {
            size += 1 + match edge {
                Edge::Index(index) => index
                    .checked_ilog10()
                    .map_or(1, |digits| digits as usize + 1),
                Edge::Key(key) => escaped_len(self.string(key)?),
            };
        }
        Some(size)
    }
    // Write a measured pointer into `bytes`, from its last segment back.
    fn fill_pointer(&self, id: Id, root: Id, bytes: &mut [u8]) {
        let mut end = bytes.len();
        for edge in self.edges(id, root) {
            match edge {
                Edge::Index(mut index) => loop {
                    end -= 1;
                    bytes[end] = b'0' + (index % 10) as u8;
                    index /= 10;
                    if index == 0 {
                        break;
                    }
                },
                Edge::Key(key) => {
                    let name = self.string(key).expect("measured segment");
                    let start = end - escaped_len(name);
                    if start + name.len() == end {
                        bytes[start..end].copy_from_slice(name.as_bytes());
                    } else {
                        let mut at = start;
                        for byte in name.bytes() {
                            let escape = match byte {
                                b'~' => Some(b'0'),
                                b'/' => Some(b'1'),
                                _ => None,
                            };
                            if let Some(code) = escape {
                                bytes[at] = b'~';
                                bytes[at + 1] = code;
                                at += 2;
                            } else {
                                bytes[at] = byte;
                                at += 1;
                            }
                        }
                    }
                    end = start;
                }
            }
            end -= 1;
            bytes[end] = b'/';
        }
        debug_assert_eq!(end, 0);
    }
    /// The exact pointer, including segments whose names hold unpaired units.
    pub fn exact_pointer(&self, id: Id) -> crate::Pointer {
        self.exact_pointer_from(id, 0)
    }
    /// The exact pointer of `id` from its ancestor `root`, which may be `id`.
    pub fn exact_pointer_from(&self, id: Id, root: Id) -> crate::Pointer {
        if let Some(text) = self.pointer_from(id, root) {
            return crate::Pointer::escaped(text);
        }
        // The relative path contains an unpaired name. Measure and fill only
        // the final buffer, avoiding decoded-name and ancestor vectors.
        let name_len = |key| {
            self.utf16(key)
                .try_fold(0usize, |size, unit| {
                    size.checked_add(
                        1 + usize::from(unit == u16::from(b'~') || unit == u16::from(b'/')),
                    )
                })
                .expect("pointer segment length fits address space")
        };
        let escaped = self
            .edges(id, root)
            .try_fold(0usize, |size, edge| {
                let segment = match edge {
                    Edge::Index(index) => index
                        .checked_ilog10()
                        .map_or(1, |digits| digits as usize + 1),
                    Edge::Key(key) => name_len(key),
                };
                size.checked_add(1)?.checked_add(segment)
            })
            .expect("pointer length fits address space");
        let mut units = vec![0u16; escaped];
        let mut end = units.len();
        for edge in self.edges(id, root) {
            match edge {
                Edge::Index(mut index) => loop {
                    end -= 1;
                    units[end] = u16::from(b'0') + (index % 10) as u16;
                    index /= 10;
                    if index == 0 {
                        break;
                    }
                },
                Edge::Key(key) => {
                    let start = end - name_len(key);
                    let mut at = start;
                    for unit in self.utf16(key) {
                        let escape = match unit {
                            0x7e => Some(u16::from(b'0')),
                            0x2f => Some(u16::from(b'1')),
                            _ => None,
                        };
                        if let Some(code) = escape {
                            units[at] = u16::from(b'~');
                            units[at + 1] = code;
                            at += 2;
                        } else {
                            units[at] = unit;
                            at += 1;
                        }
                    }
                    end = start;
                }
            }
            end -= 1;
            units[end] = u16::from(b'/');
        }
        debug_assert_eq!(end, 0);
        crate::Pointer::from_escaped_unpaired_units(units)
    }
    /// One-based line and byte column of a node's offset, found by one pass
    /// over the bytes before it; nothing is indexed or retained.
    pub fn line_and_column(&self, offset: usize) -> (usize, usize) {
        let before = &self.source.as_bytes()[..offset];
        // Counting compares every byte, which the compiler vectorizes.
        let lines = before.iter().filter(|&&byte| byte == b'\n').count();
        let line_start = before
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map_or(0, |at| at + 1);
        (1 + lines, offset - line_start + 1)
    }
    pub fn position(&self, offset: usize) -> Option<(usize, usize)> {
        if offset > self.source.len() || !self.source.is_char_boundary(offset) {
            return None;
        }
        let block = offset / POSITION_STRIDE;
        let start = block * POSITION_STRIDE;
        let checkpoint = if block == 0 {
            PositionCheckpoint::START
        } else {
            // Only complete checkpoints are published. Allocation and scanning
            // cannot expose a partial populated slot, including after unwinding.
            let mut index = self.positions.lock().unwrap_or_else(|e| e.into_inner());
            index.reserve(block, self.source.len() / POSITION_STRIDE);
            while index.populated < block {
                let cursor = index.populated * POSITION_STRIDE;
                let previous = if index.populated == 0 {
                    PositionCheckpoint::START
                } else {
                    index.checkpoints[index.populated - 1]
                };
                let next = self.scan_position(cursor..cursor + POSITION_STRIDE, previous);
                let slot = index.populated;
                index.checkpoints[slot] = next;
                index.populated += 1;
            }
            index.checkpoints[block - 1]
        };
        // The mutex is released before the bounded tail scan. Across all owners,
        // complete blocks scan once, plus < POSITION_STRIDE bytes per request.
        let at = self.scan_position(start..offset, checkpoint);
        Some((at.line, offset - at.line_start + 1))
    }
    fn scan_position(&self, range: Range<usize>, mut at: PositionCheckpoint) -> PositionCheckpoint {
        // Byte slices allow checkpoint boundaries inside UTF-8 scalars. LF is
        // always a single byte and columns count original bytes, including CR.
        let bytes = &self.source.as_bytes()[range.clone()];
        #[cfg(test)]
        self.position_scanned
            .fetch_add(bytes.len(), std::sync::atomic::Ordering::Relaxed);
        for (relative, &byte) in bytes.iter().enumerate() {
            if byte == b'\n' {
                at.line += 1;
                at.line_start = range.start + relative + 1;
            }
        }
        at
    }
}
// Both modes use the same direct byte slice and the same lexical loop. The
// optional text is proof of the whole-text entry point's single UTF-8 check.
struct Source<'a> {
    bytes: &'a [u8],
    text: Option<&'a str>,
}
impl<'a> Source<'a> {
    fn trusted(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            text: Some(text),
        }
    }
    fn prefix(bytes: &'a [u8]) -> Self {
        Self { bytes, text: None }
    }
    #[inline(always)]
    fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    #[inline]
    fn text(&self, range: Range<usize>) -> &'a str {
        match self.text {
            Some(text) => &text[range],
            None => prefix_utf8(&self.bytes[range]).expect("validated JSON source"),
        }
    }
    #[inline(always)]
    fn validate_plain(&self, range: Range<usize>) -> Result<(), Problem> {
        if self.text.is_some() {
            return Ok(());
        }
        prefix_utf8(&self.bytes[range.clone()])
            .map(|_| ())
            .map_err(|error| Problem {
                code: if error.error_len().is_none() && range.end == self.bytes.len() {
                    "incomplete"
                } else {
                    "utf8"
                },
                offset: range.start + error.valid_up_to(),
            })
    }
    #[inline]
    fn decode(&self, out: &mut Arena, range: Range<usize>) {
        out.reserve_decoded(self.bytes.len(), range.len());
        out.decoded.push_str(self.text(range));
    }
    fn incomplete_code(&self, code: &'static str) -> &'static str {
        if self.text.is_some() {
            code
        } else {
            "incomplete"
        }
    }
    // Non-ASCII bytes outside plain strings cannot extend JSON grammar.
    #[inline]
    fn unexpected(&self, at: usize) -> Option<Problem> {
        if self.text.is_some() {
            return None;
        }
        let first = *self.bytes.get(at)?;
        if first.is_ascii() {
            return None;
        }
        let width = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 1,
        };
        self.validate_plain(at..self.bytes.len().min(at.saturating_add(width)))
            .err()
            .map(|mut problem| {
                if problem.is_incomplete() {
                    problem.code = "utf8";
                }
                problem
            })
    }
}
#[cfg(test)]
thread_local! {
    static PREFIX_UTF8_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
fn prefix_utf8(bytes: &[u8]) -> Result<&str, std::str::Utf8Error> {
    #[cfg(test)]
    PREFIX_UTF8_BYTES.with(|count| count.set(count.get() + bytes.len()));
    std::str::from_utf8(bytes)
}
struct Parser<'a> {
    out: &'a mut Arena,
    source: Source<'a>,
    pos: usize,
    /// The innermost unclosed container, or `NO_PARENT`.
    open: u32,
    state: u8,
    depth: usize,
    /// Items of the unclosed containers, outermost first. While open, a
    /// container's `first` is where its items begin here.
    pending: Vec<u32>,
    limits: Limits,
}
impl Parser<'_> {
    /// Parse the whole source: one value, then only whitespace.
    fn document(&mut self) -> Result<(), Problem> {
        self.value_end()?;
        if self.space().is_some() {
            return Err(self.error("trailing-token"));
        }
        self.finish();
        Ok(())
    }
    /// Stop immediately after the first value, leaving all following bytes alone.
    #[inline(never)]
    fn value_end(&mut self) -> Result<(), Problem> {
        let first = self.space();
        self.value(NO_PARENT, false, first)?;
        // `open` is the innermost unclosed container; its state is 0 just
        // after it opens, 1 after an item and 2 after a separating comma.
        while self.open != NO_PARENT {
            let id = self.open as usize;
            let state = self.state;
            let object = matches!(self.out.nodes[id].kind, Kind::Object { .. });
            let next = self.space();
            let close = if object { b'}' } else { b']' };
            if next == Some(close) && state != 2 {
                self.pos += 1;
                self.close(id, object);
                continue;
            }
            if state == 1 {
                if next != Some(b',') {
                    return Err(self.error("comma-or-close"));
                }
                self.pos += 1;
                self.state = 2;
                continue;
            }
            if object {
                if next != Some(b'"') {
                    return Err(self.error("object-key"));
                }
                let key = self.string(id as u32, true)?;
                self.pending.push(key);
                if self.space() != Some(b':') {
                    return Err(self.error("colon"));
                }
                self.pos += 1;
                self.state = 1;
                let first = self.space();
                self.value(id as u32, false, first)?;
            } else {
                self.state = 1;
                self.value(id as u32, true, next)?;
            }
        }
        Ok(())
    }
    #[inline]
    fn finish(&mut self) {
        self.index_members();
        // Token ids follow source order, including duplicates in nested objects.
        self.out.duplicates.sort_unstable_by_key(|&(_, key)| key);
    }
    #[cold]
    fn error(&self, code: &'static str) -> Problem {
        if !matches!(code, "depth-limit" | "node-limit" | "escape")
            && let Some(problem) = self.source.unexpected(self.pos)
        {
            return problem;
        }
        let code = if self.pos == self.source.bytes().len()
            && !matches!(code, "depth-limit" | "node-limit")
        {
            self.source.incomplete_code(code)
        } else {
            code
        };
        Problem {
            code,
            offset: self.pos,
        }
    }
    #[inline(always)]
    fn peek(&self) -> Option<u8> {
        self.source.bytes().get(self.pos).copied()
    }
    fn space(&mut self) -> Option<u8> {
        loop {
            let next = self.peek();
            if !next.is_some_and(|c| matches!(c, b' ' | b'\n' | b'\r' | b'\t')) {
                return next;
            }
            self.pos += 1;
        }
    }
    fn push(&mut self, kind: Kind, start: usize, parent: u32) -> Result<u32, Problem> {
        if self.out.nodes.len() >= self.limits.max_nodes {
            return Err(self.error("node-limit"));
        }
        let id = self.out.nodes.len() as u32;
        self.out.nodes.push(Node {
            kind,
            parent,
            span: start..self.pos,
        });
        Ok(id)
    }
    /// Close the open container whose final byte was just consumed: move its
    /// items to their final place, record repeated member names and index a
    /// large object.
    #[inline(always)]
    fn close(&mut self, id: Id, object: bool) {
        let node = &mut self.out.nodes[id];
        node.span.end = self.pos;
        let parent = node.parent;
        let (Kind::Array { first, len } | Kind::Object { first, len }) = &mut node.kind else {
            unreachable!("an open node is a container")
        };
        let start = self.out.items.len();
        let pending = *first as usize;
        let count = self.pending.len() - pending;
        *first = u32::try_from(start).expect("items are bounded by the node count");
        *len = count as u32;
        self.out.items.extend_from_slice(&self.pending[pending..]);
        self.pending.truncate(pending);
        if object {
            self.members(id, start, count);
        }
        self.open = parent;
        self.state = 1;
        self.depth -= 1;
    }
    // Small objects compare names directly. Larger objects sort names by key
    // and source slot, record adjacent repeats, and retain each first occurrence
    // in one flat index. Member iteration always uses the source-order items.
    fn members(&mut self, object: Id, start: usize, count: usize) {
        let out = &mut *self.out;
        let names = &out.items[start..start + count];
        if count < INDEX_MIN_MEMBERS {
            let mut scalar = [None; INDEX_MIN_MEMBERS];
            let mut duplicates = [0; INDEX_MIN_MEMBERS];
            let mut repeats = 0;
            for (at, &name) in names.iter().enumerate() {
                scalar[at] = out.string_bytes_in(&self.source, name as usize);
                let repeated = (0..at).any(|prior| match (scalar[prior], scalar[at]) {
                    (Some(a), Some(b)) => {
                        #[cfg(test)]
                        out.key_comparisons
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        bytes_eq(a, b)
                    }
                    (None, None) => {
                        out.equal_keys_in(&self.source, names[prior] as usize, name as usize)
                    }
                    _ => {
                        #[cfg(test)]
                        out.key_comparisons
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        false
                    }
                });
                if repeated {
                    duplicates[repeats] = name;
                    repeats += 1;
                }
            }
            out.duplicates.extend(
                duplicates[..repeats]
                    .iter()
                    .map(|&name| (object as u32, name)),
            );
            return;
        }
        out.member_indexes
            .push((object as u32, start..start + count, 0));
    }
    // Allocate once for all indexed names, without per-object sorting scratch.
    // A record uses at most sixteen bytes; no per-object spare bucket capacity.
    fn index_members(&mut self) {
        let out = &mut *self.out;
        let mut indexes = std::mem::take(&mut out.member_indexes);
        let capacity = indexes.iter().map(|(_, names, _)| names.len()).sum();
        let mut slots: Vec<(u64, u32)> = Vec::with_capacity(capacity);
        for (object, range, common) in &mut indexes {
            let names = &out.items[range.clone()];
            // Removing a shared scalar prefix preserves byte order. Unpaired
            // keys have their own class, after every valid UTF-8 prefix.
            let mut scalar = names
                .iter()
                .filter_map(|&id| out.string_bytes_in(&self.source, id as usize));
            let first_key = scalar.next().unwrap_or(b"");
            *common = first_key.len();
            for key in scalar {
                *common = first_key[..*common]
                    .iter()
                    .zip(key)
                    .take_while(|(a, b)| a == b)
                    .count();
                if *common == 0 {
                    break;
                }
            }
            let first = slots.len();
            for &name in names {
                let prefix = out
                    .string_bytes_in(&self.source, name as usize)
                    .map_or(u64::MAX, |key| key_prefix(&key[*common..]));
                slots.push((prefix, name));
            }
            slots[first..].sort_unstable_by(|a, b| {
                #[cfg(test)]
                out.key_comparisons
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                a.cmp(b)
            });
            let mut begin = first;
            while begin < slots.len() {
                let mut end = begin + 1;
                while end < slots.len() && slots[end].0 == slots[begin].0 {
                    end += 1;
                }
                slots[begin..end].sort_unstable_by(|&(_, a), &(_, b)| {
                    out.compare_keys_in(&self.source, a as usize, b as usize)
                        .then_with(|| a.cmp(&b))
                });
                begin = end;
            }
            let mut end = first + 1;
            for at in first + 1..slots.len() {
                let (prefix, name) = slots[at];
                let (prior_prefix, prior) = slots[end - 1];
                if prefix == prior_prefix
                    && out.equal_keys_in(&self.source, prior as usize, name as usize)
                {
                    out.duplicates.push((*object, name));
                } else {
                    slots[end] = slots[at];
                    end += 1;
                }
            }
            slots.truncate(end);
            *range = first..end;
        }
        indexes.sort_unstable_by_key(|&(object, _, _)| object);
        out.member_slots = slots;
        out.member_indexes = indexes;
    }
    /// Parse one value after whitespace, reusing its already-read first byte;
    /// an `element` is listed as an item of its array parent.
    /// A container is left open, its items to follow.
    #[inline(always)]
    fn value(&mut self, parent: u32, element: bool, next: Option<u8>) -> Result<(), Problem> {
        let start = self.pos;
        let id = match next {
            Some(open @ (b'{' | b'[')) => {
                if self.depth >= self.limits.max_depth {
                    return Err(self.error("depth-limit"));
                }
                if self.pending.capacity() == 0 {
                    self.pending.reserve(PENDING_START);
                }
                // While open, `first` is where its items begin in `pending`,
                // after its own entry there as an element.
                let first = u32::try_from(self.pending.len() + usize::from(element))
                    .expect("pending items are bounded by the node count");
                let kind = if open == b'{' {
                    Kind::Object { first, len: 0 }
                } else {
                    Kind::Array { first, len: 0 }
                };
                self.pos += 1;
                let id = self.push(kind, start, parent)?;
                self.open = id;
                self.state = 0;
                self.depth += 1;
                id
            }
            Some(b'"') => self.string(parent, false)?,
            Some(b'n') => self.literal("null", Kind::Null, parent)?,
            Some(b't') => self.literal("true", Kind::Bool(true), parent)?,
            Some(b'f') => self.literal("false", Kind::Bool(false), parent)?,
            Some(first @ (b'-' | b'0'..=b'9')) => {
                let leading = if first == b'-' {
                    self.pos += 1;
                    self.peek()
                } else {
                    Some(first)
                };
                match leading {
                    Some(b'0') => self.pos += 1,
                    Some(b'1'..=b'9') => self.digits(),
                    _ => return Err(self.error("number")),
                }
                if self.peek() == Some(b'.') {
                    self.pos += 1;
                    if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        return Err(self.error("fraction"));
                    }
                    self.digits();
                }
                if self.peek().is_some_and(|c| c == b'e' || c == b'E') {
                    self.pos += 1;
                    if self.peek().is_some_and(|c| c == b'+' || c == b'-') {
                        self.pos += 1;
                    }
                    if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        return Err(self.error("exponent"));
                    }
                    self.digits();
                }
                self.push(Kind::Number, start, parent)?
            }
            _ => return Err(self.error("value")),
        };
        if element {
            self.pending.push(id);
        }
        Ok(())
    }
    fn digits(&mut self) {
        let bytes = self.source.bytes();
        // Every caller has already checked the first digit.
        let mut end = self.pos + 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        self.pos = end;
    }
    fn literal(&mut self, text: &str, kind: Kind, parent: u32) -> Result<u32, Problem> {
        let start = self.pos;
        let rest = &self.source.bytes()[self.pos..];
        if !rest.starts_with(text.as_bytes()) {
            let matched = rest
                .iter()
                .zip(text.as_bytes())
                .take_while(|(a, b)| a == b)
                .count();
            if let Some(problem) = self.source.unexpected(self.pos + matched) {
                return Err(problem);
            }
            let mut problem = self.error("literal");
            if text.as_bytes().starts_with(rest) {
                problem.code = self.source.incomplete_code("literal");
            }
            return Err(problem);
        }
        self.pos += text.len();
        self.push(kind, start, parent)
    }
    // Advance to the next quote, backslash or control byte, or the end.
    fn plain(&mut self) {
        let start = self.pos;
        let rest = &self.source.bytes()[start..];
        self.pos += rest
            .iter()
            .position(|&b| b == b'"' || b == b'\\' || b < 0x20)
            .unwrap_or(rest.len());
    }
    fn hex4(&mut self) -> Result<u16, Problem> {
        let mut n = 0u16;
        for _ in 0..4 {
            let Some(c) = self.peek() else {
                return Err(self.error("unicode-escape"));
            };
            let value = match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => return Err(self.error("unicode-escape")),
            };
            n = (n << 4) | u16::from(value);
            self.pos += 1;
        }
        Ok(n)
    }
    // A `\u` escape of a low surrogate starting at `at`, without consuming it.
    fn low_surrogate_at(&self, at: usize) -> Option<u16> {
        let escape = self.source.bytes().get(at..at + 6)?;
        if escape[0] != b'\\' || escape[1] != b'u' {
            return None;
        }
        let mut n = 0u16;
        for &c in &escape[2..] {
            n = (n << 4) | u16::from((c as char).to_digit(16)? as u8);
        }
        (0xdc00..0xe000).contains(&n).then_some(n)
    }
    fn string(&mut self, parent: u32, name: bool) -> Result<u32, Problem> {
        let start = self.pos;
        self.pos += 1;
        self.plain();
        self.source.validate_plain(start + 1..self.pos)?;
        match self.peek() {
            None => return Err(self.error("unterminated-string")),
            Some(b'"') => {
                self.pos += 1;
                return self.push(
                    Kind::String {
                        text: Text::SOURCE,
                        name,
                    },
                    start,
                    parent,
                );
            }
            Some(b'\\') => {}
            Some(_) => return Err(self.error("string-control")),
        }
        // Escaped: decode straight to UTF-8 in the shared buffer. A surrogate
        // escape that does not pair with the next unit leaves the value no
        // scalar form: nothing more is written, and its units are read from
        // the token whenever asked for.
        let decoded_start = self.out.decoded.len();
        let mut segment = start + 1;
        let mut unpaired = false;
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated-string")),
                Some(0..=0x1f) => return Err(self.error("string-control")),
                Some(b'"') => break,
                Some(b'\\') => {
                    if !unpaired {
                        self.out.decode(&self.source, segment..self.pos);
                    }
                    self.pos += 1;
                    let Some(escape) = self.peek() else {
                        return Err(self.error("escape"));
                    };
                    if let Some(problem) = self.source.unexpected(self.pos) {
                        return Err(problem);
                    }
                    self.pos += 1;
                    let c = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let unit = self.hex4()?;
                            match unit {
                                0xd800..0xdc00 => match self.low_surrogate_at(self.pos) {
                                    Some(low) => {
                                        self.pos += 6;
                                        char::decode_utf16([unit, low])
                                            .next()
                                            .and_then(Result::ok)
                                            .expect("a surrogate pair")
                                    }
                                    None => {
                                        unpaired = true;
                                        '\u{fffd}'
                                    }
                                },
                                0xdc00..0xe000 => {
                                    unpaired = true;
                                    '\u{fffd}'
                                }
                                _ => char::from_u32(u32::from(unit)).expect("a scalar value"),
                            }
                        }
                        _ => return Err(self.error("escape")),
                    };
                    if !unpaired {
                        self.out
                            .reserve_decoded(self.source.bytes().len(), c.len_utf8());
                        self.out.decoded.push(c);
                    }
                    segment = self.pos;
                }
                Some(_) => {
                    let start = self.pos;
                    self.plain();
                    self.source.validate_plain(start..self.pos)?;
                }
            }
        }
        let end = self.pos;
        self.pos += 1;
        let text = if unpaired {
            self.out.decoded.truncate(decoded_start);
            Text::UNPAIRED
        } else {
            self.out.decode(&self.source, segment..end);
            self.out.decoded_ends.push(self.out.decoded.len());
            Text(
                u32::try_from(self.out.decoded_ends.len() - 1)
                    .expect("strings are bounded by the node count"),
            )
        };
        let id = self.push(Kind::String { text, name }, start, parent)?;
        if unpaired {
            self.out.unpaired.push(id);
        }
        Ok(id)
    }
}

/// The UTF-16 units of an admitted string token's body, escapes decoded one
/// unit each, so an unpaired surrogate escape stays one unit.
#[derive(Clone, Debug)]
pub struct Utf16<'a> {
    body: &'a str,
    at: usize,
    low: Option<u16>,
}
impl Iterator for Utf16<'_> {
    type Item = u16;
    fn next(&mut self) -> Option<u16> {
        if let Some(low) = self.low.take() {
            return Some(low);
        }
        let rest = &self.body[self.at..];
        let bytes = rest.as_bytes();
        if bytes.first() == Some(&b'\\') {
            let (unit, width) = match bytes[1] {
                b'"' => (34, 2),
                b'\\' => (92, 2),
                b'/' => (47, 2),
                b'b' => (8, 2),
                b'f' => (12, 2),
                b'n' => (10, 2),
                b'r' => (13, 2),
                b't' => (9, 2),
                _ => (
                    u16::from_str_radix(&rest[2..6], 16).expect("an admitted unicode escape"),
                    6,
                ),
            };
            self.at += width;
            return Some(unit);
        }
        let c = rest.chars().next()?;
        self.at += c.len_utf8();
        let mut units = [0; 2];
        let units = c.encode_utf16(&mut units);
        self.low = units.get(1).copied();
        Some(units[0])
    }
}

// A pointer segment's length once `~` and `/` are escaped.
pub(crate) fn escaped_len(name: &str) -> usize {
    name.len() + name.bytes().filter(|&b| b == b'~' || b == b'/').count()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_utf8_work_depends_on_the_value_not_the_remainder() {
        for prefix in [
            "123456789 ",
            " [\"é\",true] ",
            r#"{"key":"\ud800","k":"café"} "#,
        ] {
            let mut work = Vec::new();
            for suffix_len in [0, 1024 * 1024, 2 * 1024 * 1024] {
                let mut input = prefix.as_bytes().to_vec();
                input.resize(input.len() + suffix_len, b' ');
                input.push(0xff);
                PREFIX_UTF8_BYTES.with(|count| count.set(0));
                let (value, end) = crate::JsonValue::parse_prefix(&input).unwrap();
                work.push(PREFIX_UTF8_BYTES.with(std::cell::Cell::get));
                assert_eq!(end, prefix.trim_end().len());
                assert_eq!(value.text(), prefix.trim());
            }
            assert!(work.iter().all(|&bytes| bytes == work[0]));
            assert!(work[0] <= prefix.len() * 4);
        }
    }
    use std::{fmt::Write, sync::atomic::Ordering};

    fn comparisons(arena: &Arena) -> usize {
        arena.key_comparisons.load(Ordering::Relaxed)
    }
    #[test]
    fn borrowed_name_candidates_keep_threshold_and_exact_units() {
        for count in [15, 16, 33] {
            let mut fields = vec![r#""\ud800":0"#.to_owned(), r#""\ud83d\ude00":1"#.to_owned()];
            fields.extend((2..count).map(|i| format!(r#""k{i}":{i}"#)));
            let text = format!("{{{}}}", fields.join(","));
            {
                let object = crate::JsonValue::parse(&text).unwrap();
                for member in object.view().members().unwrap() {
                    let candidates: Vec<_> =
                        crate::backend::member_candidates(object.view(), member.name, |_| {
                            Ok::<_, ()>(())
                        })
                        .unwrap()
                        .collect();
                    let matches: Vec<_> = candidates
                        .iter()
                        .filter(|candidate| candidate.name.as_string() == member.name.as_string())
                        .collect();
                    assert_eq!(matches.len(), 1);
                    assert_eq!(matches[0].value.text(), member.value.text());
                }
                // A differently escaped token compares as the same key.
                let escaped = crate::JsonValue::parse(r#""k\u0032""#).unwrap();
                assert!(
                    crate::backend::member_candidates(
                        object.view(),
                        escaped.view(),
                        |_| Ok::<_, ()>(())
                    )
                    .unwrap()
                    .any(|m| m.value.text() == "2")
                );
            }
        }
    }

    #[test]
    fn borrowed_name_search_cancellation_never_returns_a_partial_match() {
        for prefix in ["", r"\ud800"] {
            let name = format!("{prefix}{}", "x".repeat(8192));
            let mut fields: Vec<_> = (0..15).map(|i| format!(r#""k{i}":{i}"#)).collect();
            fields.push(format!(r#""{name}":99"#));
            let object = crate::JsonValue::parse(format!("{{{}}}", fields.join(","))).unwrap();
            let name = crate::JsonValue::parse(format!(r#""{name}""#)).unwrap();
            let mut calls = 0;
            let result = crate::backend::member_candidates(object.view(), name.view(), |_| {
                calls += 1;
                if calls == 4 { Err("cancelled") } else { Ok(()) }
            });
            assert_eq!(result.unwrap_err(), "cancelled");
            let found: Vec<_> =
                crate::backend::member_candidates(object.view(), name.view(), |_| Ok::<_, ()>(()))
                    .unwrap()
                    .collect();
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].value.text(), "99");
        }
    }

    #[test]
    fn key_comparisons_and_member_probes_are_n_log_n() {
        // N distinct names, and N names followed by the same N again.
        // Assert exact operation counts rather than scheduler-dependent timing.
        for n in [10_000usize, 20_000, 40_000] {
            for repeated in [false, true] {
                let count = if repeated { 2 * n } else { n };
                let mut text = String::from("{");
                for i in 0..count {
                    if i > 0 {
                        text.push(',');
                    }
                    write!(text, "\"k{}\":{i}", i % n).unwrap();
                }
                text.push('}');
                let value = crate::JsonValue::parse(&text).unwrap();
                assert_eq!(value.view().len(), Some(count));
                for i in 0..n {
                    assert_eq!(value.get(&format!("k{i}")).unwrap().text(), i.to_string());
                    assert!(value.get(&format!("s{i}")).is_none());
                }
                if repeated {
                    let duplicate_keys: Vec<_> = value
                        .view()
                        .members()
                        .unwrap()
                        .skip(n)
                        .map(|m| m.name.id)
                        .collect();
                    assert_eq!(
                        value.owner.duplicates,
                        duplicate_keys
                            .into_iter()
                            .map(|id| (0, id as u32))
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }

    #[test]
    fn many_schema_pointer_references_have_logarithmic_member_probes() {
        // match the reported shape, including reading each actual $ref.
        for n in [10_000usize, 20_000, 40_000] {
            let mut text = String::from("{\"schemas\":{");
            for i in 0..n {
                if i > 0 {
                    text.push(',');
                }
                write!(text, "\"s{i}\":{{\"$ref\":\"#/schemas/s{}\"}}", n - 1).unwrap();
            }
            text.push_str("}}");
            let document = crate::JsonValue::parse(&text).unwrap();
            let schemas = document.get("schemas").unwrap();
            let target = schemas.get(&format!("s{}", n - 1)).unwrap().id;
            document.owner.member_probes.store(0, Ordering::Relaxed);
            for member in schemas.members().unwrap() {
                let reference = member.value.get("$ref").unwrap().as_str().unwrap();
                assert_eq!(
                    document
                        .at(reference.strip_prefix('#').unwrap())
                        .unwrap()
                        .id,
                    target
                );
            }
            // Each reference has two direct probes and a binary search.
        }
    }

    #[test]
    fn sorted_keys_keep_exact_names_order_and_coordinates() {
        // Mixed classes, escaped-equal names and long shared prefixes retain
        // exact equality, first occurrences and source coordinates.
        let prefix = "shared".repeat(1024);
        let mut text = format!(
            r#"{{"a":1,"\u0061":2,"\ud800":3,"\uD800":4,"\udc00":5,"😀":6,"\ud83d\ude00":7,"{prefix}x":8,"{prefix}y":9,"nested":{{"a":10,"\u0061":11}}"#
        );
        for i in 0..16 {
            write!(text, ",\"pad{i}\":{i}").unwrap();
        }
        text.push('}');
        let value = crate::JsonValue::parse(&text).unwrap();
        let a = &value.owner;
        assert!(comparisons(a) <= 4 * 26 * 5 + 1);
        assert_eq!(value.get("a").unwrap().text(), "1");
        assert_eq!(value.get("😀").unwrap().text(), "6");
        assert_eq!(value.get(&format!("{prefix}x")).unwrap().text(), "8");
        assert_eq!(value.get(&format!("{prefix}y")).unwrap().text(), "9");
        assert!(value.get(&format!("{prefix}z")).is_none());
        let offsets: Vec<_> = a
            .duplicates
            .iter()
            .map(|&(_, id)| a.nodes[id as usize].span.start)
            .collect();
        assert_eq!(
            offsets,
            vec![
                text.find(r#""\u0061""#).unwrap(),
                text.find(r#""\uD800""#).unwrap(),
                text.find(r#""\ud83d\ude00""#).unwrap(),
                text.rfind(r#""\u0061""#).unwrap()
            ]
        );
        let members: Vec<_> = value.view().members().unwrap().collect();
        for (i, member) in members.iter().take(9).enumerate() {
            assert_eq!(member.value.text(), (i + 1).to_string());
        }
        assert_eq!(members[2].name.utf16_units(), Some(vec![0xd800]));
        assert_eq!(members[4].name.utf16_units(), Some(vec![0xdc00]));
        // A name with an unpaired unit is still named exactly.
        let exact = members[2].value.location().pointer.unwrap();
        assert_eq!(exact.as_str(), None);
        assert_eq!(exact.utf16_units().collect::<Vec<_>>(), [0x2f, 0xd800]);
        assert_eq!(members[1].name.location().pointer.unwrap(), "/a");
        assert_eq!(members[1].name.location().byte_offset, offsets[0]);
    }

    #[test]
    fn indexed_pointers_and_nested_retained_views_are_thread_safe() {
        // shared, fully initialized indexes survive parent drops, preserve
        // first occurrence and leave RFC 6901 object/array semantics unchanged.
        let mut text = String::from(r#"{"outer":{"a/~":[{"":1,"~1":2}],"a\u002f~":false,"01":3"#);
        for i in 0..20 {
            write!(text, ",\"k{i}\":{i}").unwrap();
        }
        text.push_str("}}");
        let root = crate::JsonValue::parse(&text).unwrap();
        let retained = root.get("outer").unwrap().to_owned();
        drop(root);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let retained = &retained;
                scope.spawn(move || {
                    for _ in 0..32 {
                        assert_eq!(retained.at("/a~1~0/0/").unwrap().text(), "1");
                        assert_eq!(retained.at("/a~1~0/0/~01").unwrap().text(), "2");
                        assert_eq!(retained.at("/01").unwrap().text(), "3");
                        assert_eq!(retained.at("/k19").unwrap().text(), "19");
                        assert_eq!(retained.at("").unwrap().id, retained.id);
                        for pointer in [
                            "/a~1~0/01",
                            "/a~1~0/-",
                            "/a~1~0/+0",
                            "/a~1~0/",
                            "/a~1~0/1",
                            "/a~1~0/9999999999999999999999999",
                            "/a~2",
                            "/a~",
                            "a",
                            "/absent",
                        ] {
                            assert!(retained.at(pointer).is_none(), "{pointer}");
                        }
                    }
                });
            }
        });
        assert_eq!(
            retained
                .at("/a~1~0/0/~01")
                .unwrap()
                .location()
                .pointer
                .unwrap(),
            "/outer/a~1~0/0/~01"
        );
    }

    #[test]
    fn ordered_members_preserve_all_key_classes_across_threshold() {
        for count in [15, 16, 17, 33] {
            for keys in [
                vec![r#""a""#, r#""\u0061""#, r#""b""#],
                vec![r#""\ud800""#, r#""\uD800""#, r#""\udc00""#],
                vec![
                    r#""a""#,
                    r#""\ud800""#,
                    r#""😀""#,
                    r#""\ud83d\ude00""#,
                    r#""\u0061""#,
                ],
                vec![r#""\ud800""#],
                vec![r#""a""#],
            ] {
                let text = format!(
                    "{{{}}}",
                    (0..count)
                        .map(|i| format!("{}:{i}", keys[i % keys.len()]))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let value = crate::JsonValue::parse(&text).unwrap();
                let members: Vec<_> = value.view().members().unwrap().collect();
                let mut seen = std::collections::BTreeMap::new();
                let mut duplicates = Vec::new();
                for (i, member) in members.iter().enumerate() {
                    assert_eq!(member.name.text(), keys[i % keys.len()]);
                    assert_eq!(member.value.text(), i.to_string());
                    let units = member.name.utf16_units().unwrap();
                    let first = *seen.entry(units).or_insert(i);
                    if first != i {
                        duplicates.push((0, member.name.id as u32));
                    }
                    if let Some(scalar) = member.name.as_str() {
                        assert_eq!(value.get(scalar).unwrap().text(), first.to_string());
                    }
                    let mut candidates =
                        crate::backend::member_candidates(value.view(), member.name, |_| {
                            Ok::<_, ()>(())
                        })
                        .unwrap();
                    let found = candidates
                        .find(|m| m.name.as_string() == member.name.as_string())
                        .unwrap();
                    assert_eq!(found.value.text(), first.to_string());
                }
                assert_eq!(value.owner.duplicates, duplicates);
                assert!(value.get("absent").is_none());
                let missing = crate::JsonValue::parse(r#""\udfff""#).unwrap();
                assert!(
                    !crate::backend::member_candidates(
                        value.view(),
                        missing.view(),
                        |_| Ok::<_, ()>(())
                    )
                    .unwrap()
                    .any(|m| m.name.as_string() == missing.view().as_string())
                );
            }
        }
    }

    #[test]
    fn hostile_key_orders_have_n_log_n_parse_and_lookup_comparisons() {
        for n in [256usize, 1024, 4096] {
            for shape in 0..5 {
                let key = |i: usize| match shape {
                    0 => format!("k{:08}", n - i),
                    1 => format!("k{:08}", i.min(n - i)),
                    2 => format!("{}{:08}", "shared".repeat(128), i),
                    3 => format!("\\ud800{:08}", n - i),
                    _ => "equal".to_owned(),
                };
                let text = format!(
                    "{{{}}}",
                    (0..n)
                        .map(|i| format!("\"{}\":{i}", key(i)))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let value = crate::JsonValue::parse(&text).unwrap();
                for member in value.view().members().unwrap() {
                    assert!(
                        crate::backend::member_candidates(
                            value.view(),
                            member.name,
                            |_| Ok::<_, ()>(())
                        )
                        .unwrap()
                        .any(|m| m.name.as_string() == member.name.as_string())
                    );
                    if let Some(key) = member.name.as_str() {
                        assert!(value.get(key).is_some());
                        assert!(value.get(&format!("{key}~absent")).is_none());
                    }
                }
            }
        }
    }

    #[test]
    fn cached_prefix_ties_and_zero_padding_keep_exact_byte_order() {
        let keys = [
            "",
            "a",
            "\0",
            "a\0",
            "ab",
            "ab\0",
            "abcdefghx",
            "abcdefghy",
            "sharedéx",
            "sharedêy",
            "shared😀x",
            "shared😀y",
        ];
        let text = format!(
            "{{{}}}",
            (0..36)
                .map(|i| format!(
                    "{}:{i}",
                    serde_json::to_string(keys[i % keys.len()]).unwrap()
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        let value = crate::JsonValue::parse(text).unwrap();
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(value.get(key).unwrap().text(), i.to_string());
        }
        for key in ["a\0\0", "abcdefghi", "sharedéz", "absent"] {
            assert!(value.get(key).is_none());
        }
        let (slots, _) = value.owner.member_index(0).unwrap();
        let mut expected = keys.to_vec();
        expected.sort_unstable();
        assert_eq!(
            slots
                .iter()
                .map(|&(_, id)| value.owner.string(id as usize).unwrap())
                .collect::<Vec<_>>(),
            expected
        );
    }

    // Independent oracle: the original definition in terms of the entire UTF-8
    // prefix, without using checkpoints or the scanning helper under test.
    fn prefix_position(source: &str, offset: usize) -> Option<(usize, usize)> {
        let prefix = source.get(..offset)?;
        Some((
            prefix.bytes().filter(|&byte| byte == b'\n').count() + 1,
            prefix.rfind('\n').map_or(offset + 1, |last| offset - last),
        ))
    }

    fn position_source() -> String {
        let mut source = String::from("{\"pad\":\"");
        source.push_str(&"x".repeat(POSITION_STRIDE - 1 - source.len()));
        source.push_str("é\","); // UTF-8 scalar crosses the first checkpoint.
        source.push_str(&" ".repeat(2 * POSITION_STRIDE - 1 - source.len()));
        source.push_str("\r\n"); // CRLF crosses the second checkpoint.
        source.push_str(r#""é/~":[{"escaped":"\n\u000a","same":1,"same":2,"\ud800":null},true],"#);
        source.push_str(&" ".repeat(3 * POSITION_STRIDE - 1 - source.len()));
        source.push_str("\n\"tail\":null}\r\n ");
        source
    }

    #[test]
    fn positions_match_prefix_oracle_at_every_utf8_boundary() {
        let source = position_source();
        assert!(!source.is_char_boundary(POSITION_STRIDE));
        assert_eq!(&source.as_bytes()[2 * POSITION_STRIDE - 1..][..2], b"\r\n");
        let arena = Arena::parse(source.as_bytes(), Limits::default()).unwrap();
        let mut requests = Vec::new();
        for offset in 0..=source.len() {
            assert_eq!(
                arena.position(offset),
                prefix_position(&source, offset),
                "offset {offset}"
            );
            if source.is_char_boundary(offset) {
                requests.push(offset);
            }
        }
        assert_eq!(arena.source(), source);

        for length in [POSITION_STRIDE - 1, POSITION_STRIDE, POSITION_STRIDE + 1] {
            let source = format!("{}null", "\n".repeat(length - 4));
            let arena = Arena::parse(source.as_bytes(), Limits::default()).unwrap();
            for offset in [0, length - 1, length] {
                assert_eq!(arena.position(offset), prefix_position(&source, offset));
            }
        }
    }

    #[test]
    fn retained_values_share_positions_across_concurrent_requests() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arena>();
        assert_send_sync::<crate::JsonValue>();
        assert_send_sync::<crate::JsonRef<'static>>();

        let source = position_source();
        let root = crate::JsonValue::parse(&source).unwrap();
        let object = root.at("/é~1~0/0").unwrap();
        let members: Vec<_> = object.members().unwrap().collect();
        let leaves = vec![
            root.get("pad").unwrap().to_owned(),
            members[0].value.to_owned(),
            members[1].value.to_owned(),
            members[2].value.to_owned(),
            members[3].value.to_owned(),
            root.get("tail").unwrap().to_owned(),
        ];
        let expected = [
            (source.find("\"pad\":").unwrap() + 6, Some("/pad")),
            (
                source.find("\"escaped\":").unwrap() + 10,
                Some("/é~1~0/0/escaped"),
            ),
            (
                source.find("\"same\":1").unwrap() + 7,
                Some("/é~1~0/0/same"),
            ),
            (
                source.find("\"same\":2").unwrap() + 7,
                Some("/é~1~0/0/same"),
            ),
            (source.find(r#""\ud800":"#).unwrap() + 9, None),
            (source.find("\"tail\":").unwrap() + 7, Some("/tail")),
        ];
        drop(members);
        drop(root);

        let barrier = std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            for worker in 0..4 {
                let leaves = leaves.clone();
                let expected = &expected;
                let barrier = &barrier;
                let source = &source;
                scope.spawn(move || {
                    barrier.wait();
                    for round in 0..32 {
                        for index in 0..leaves.len() {
                            let index = (index + worker + round) % leaves.len();
                            let leaf = &leaves[index];
                            let (offset, pointer) = expected[index];
                            let (line, column) = prefix_position(source, offset).unwrap();
                            let actual = leaf.location();
                            assert_eq!(actual.byte_offset, offset);
                            assert_eq!((actual.line, actual.byte_column), (line, column));
                            assert_eq!(
                                actual.pointer.as_ref().and_then(crate::Pointer::as_str),
                                pointer
                            );
                            assert_eq!(leaf.original_source(), source.as_bytes());
                        }
                    }
                });
            }
        });
        drop(leaves);
    }

    #[test]
    fn escaped_strings_decode_exactly_as_their_utf16_units() {
        // Independent oracle: each piece's UTF-16 units, then the standard
        // library's verdict on the whole sequence. A surrogate escape pairs only
        // with the unit right after it, whatever spells either unit.
        let pieces: [(&str, &[u16]); 18] = [
            ("a", &[0x61]),
            ("é", &[0xe9]),
            ("😀", &[0xd83d, 0xde00]),
            ("\\n", &[0x0a]),
            ("\\\"", &[0x22]),
            ("\\\\", &[0x5c]),
            ("\\/", &[0x2f]),
            ("\\b", &[0x08]),
            ("\\u0041", &[0x41]),
            ("\\u00E9", &[0xe9]),
            ("\\uFFFF", &[0xffff]),
            ("\\ud83d", &[0xd83d]),
            ("\\uDE00", &[0xde00]),
            ("\\udbff", &[0xdbff]),
            ("\\udfff", &[0xdfff]),
            ("\\ud800", &[0xd800]),
            ("\\udc00", &[0xdc00]),
            ("\\u0000", &[0x00]),
        ];
        let mut state = 0x2545_f491_u64;
        for _ in 0..20_000 {
            let mut body = String::new();
            let mut units = Vec::new();
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            for _ in 0..state % 7 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let (text, piece) = pieces[state as usize % pieces.len()];
                body.push_str(text);
                units.extend_from_slice(piece);
            }
            let text = format!("[\"{body}\",{{\"{body}\":0}}]");
            let arena = Arena::parse(text.as_bytes(), Limits::default()).unwrap();
            for id in [1, 3] {
                match String::from_utf16(&units) {
                    Ok(scalar) => {
                        assert_eq!(arena.string(id), Some(scalar.as_str()), "{body}");
                        assert!(arena.unpaired_units(id).is_none(), "{body}");
                    }
                    Err(_) => {
                        assert_eq!(arena.string(id), None, "{body}");
                        assert_eq!(
                            arena.unpaired_units(id).map(Iterator::collect::<Vec<_>>),
                            Some(units.clone()),
                            "{body}"
                        );
                    }
                }
                assert_eq!(arena.units(id).as_deref(), Some(units.as_slice()), "{body}");
            }
            let unpaired = String::from_utf16(&units).is_err();
            assert_eq!(arena.unpaired, if unpaired { vec![1, 3] } else { vec![] });
        }
    }

    #[test]
    fn invalid_json() {
        for v in [
            "",
            " ",
            "[1,]",
            "{\"a\":1,}",
            "01",
            "-",
            "1.",
            "1e+",
            "true false",
            r#""\q""#,
            r#""\uDDD""#,
            r#"{"a" 1}"#,
            "[}",
        ] {
            assert!(
                Arena::parse(v.as_bytes(), Limits::default()).is_err(),
                "{v}"
            );
        }
    }
    #[test]
    fn depth_and_drop() {
        for depth in [127, 128, 256, 9999, 10000, 10001] {
            let s = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
            let a = Arena::parse(s.as_bytes(), Limits::default());
            assert_eq!(a.is_ok(), depth <= 10000);
            drop(a);
        }
    }
    #[test]
    fn pointers_name_every_node_with_escaped_segments() {
        // Escapes, empty and multibyte names, multi-digit indices, nesting, and a
        // name with an unpaired surrogate that makes its descendants unnamed.
        let source = r#"{"":{"a/b~c":[0,1,2,3,4,5,6,7,8,9,{"é":[[true]],"~~//":null}]},"x":{"\ud800":{"y":[1]},"ok":"\u0041"}}"#;
        let a = Arena::parse(source.as_bytes(), Limits::default()).unwrap();
        let reference = |id: Id| -> Option<String> {
            let mut segments = Vec::new();
            let mut current = id;
            while let Some((parent, edge)) = a.edge(current) {
                segments.push(match edge {
                    Edge::Index(i) => i.to_string(),
                    Edge::Key(k) => a.string(k)?.replace('~', "~0").replace('/', "~1"),
                });
                current = parent;
            }
            segments.reverse();
            Some(segments.iter().map(|s| format!("/{s}")).collect())
        };
        // The exact pointer keeps every unit, including those names.
        let exact = |id: Id| -> Vec<u16> {
            let mut segments = Vec::new();
            let mut current = id;
            while let Some((parent, edge)) = a.edge(current) {
                segments.push(match edge {
                    Edge::Index(i) => i.to_string().encode_utf16().collect(),
                    Edge::Key(k) => a
                        .units(k)
                        .unwrap()
                        .iter()
                        .flat_map(|&unit| match unit {
                            0x7e => vec![0x7e, 0x30],
                            0x2f => vec![0x7e, 0x31],
                            _ => vec![unit],
                        })
                        .collect::<Vec<u16>>(),
                });
                current = parent;
            }
            segments
                .iter()
                .rev()
                .flat_map(|s| std::iter::once(0x2f).chain(s.iter().copied()))
                .collect()
        };
        let mut unnamed = 0;
        for id in 0..a.nodes.len() {
            let expected = reference(id);
            unnamed += usize::from(expected.is_none());
            assert_eq!(a.pointer(id), expected, "node {id}");
            let pointer = a.exact_pointer(id);
            assert_eq!(pointer.as_str(), expected.as_deref(), "node {id}");
            assert_eq!(
                pointer.utf16_units().collect::<Vec<_>>(),
                exact(id),
                "node {id}"
            );
            // Appending writes the same pointer after what the buffer holds.
            let mut out = b"held".to_vec();
            assert_eq!(
                a.pointer_into(id, &mut out),
                expected.is_some(),
                "node {id}"
            );
            assert_eq!(&out[..4], b"held");
            assert_eq!(
                out[4..],
                *expected.unwrap_or_default().as_bytes(),
                "node {id}"
            );
        }
        assert!(unnamed > 0);
        let id = a.at("//a~1b~0c/10/é/0/0").unwrap();
        assert_eq!(a.pointer(id).as_deref(), Some("//a~1b~0c/10/é/0/0"));
        assert_eq!(a.pointer(0).as_deref(), Some(""));
    }
}
