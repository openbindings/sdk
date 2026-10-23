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
    collections::{HashMap, hash_map::RandomState},
    hash::{BuildHasher, Hasher},
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

// One slot per distinct decoded key: the member position of its first
// occurrence. Equal repeats reuse that slot; distinct keys whose hashes collide
// share a list, in original member order.
#[derive(Debug, Clone, Copy)]
enum KeyBucket {
    One(u32),
    Many(u32),
}
#[derive(Debug, Default)]
struct MemberIndex {
    slots: HashMap<u64, KeyBucket>,
    collisions: Vec<Vec<u32>>,
}
impl MemberIndex {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: HashMap::with_capacity(capacity),
            collisions: Vec::new(),
        }
    }
    fn members(&self, hash: u64) -> &[u32] {
        match self.slots.get(&hash) {
            None => &[],
            Some(KeyBucket::One(slot)) => std::slice::from_ref(slot),
            Some(&KeyBucket::Many(list)) => &self.collisions[list as usize],
        }
    }
}

#[derive(Debug, Default)]
struct KeyHasher {
    state: RandomState,
    #[cfg(test)]
    injected: Option<fn(&[u16]) -> u64>,
}
impl KeyHasher {
    fn hash(&self, units: impl Iterator<Item = u16>) -> u64 {
        #[cfg(test)]
        if let Some(hash) = self.injected {
            return hash(&units.collect::<Vec<_>>());
        }
        let mut hasher = self.state.build_hasher();
        for unit in units {
            hasher.write_u16(unit);
        }
        hasher.finish()
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
    // Completed objects of at least INDEX_MIN_MEMBERS members publish an
    // index built once, at their exact size, when they close; listed by object
    // once parsing ends. The immutable index is shared by retained views and
    // threads, without lookup-time locks.
    member_indexes: Vec<(u32, MemberIndex)>,
    key_hasher: KeyHasher,
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
    /// Parse into a shared owner, written where it will stay.
    pub fn parse(bytes: &[u8], limits: Limits) -> Result<Arc<Self>, Problem> {
        Self::parse_with_hasher(bytes, limits, KeyHasher::default())
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
        Self::parse_source(text.into_boxed_str(), limits, KeyHasher::default())
            .map_err(|(problem, source)| (problem, source.into_string().into_bytes()))
    }
    fn parse_with_hasher(
        bytes: &[u8],
        limits: Limits,
        key_hasher: KeyHasher,
    ) -> Result<Arc<Self>, Problem> {
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(bom());
        }
        let text = std::str::from_utf8(bytes).map_err(|e| utf8(e.valid_up_to()))?;
        Self::parse_source(Box::from(text), limits, key_hasher).map_err(|(problem, _)| problem)
    }
    // Parse `source` into a new owner; a refusal hands the source back.
    fn parse_source(
        source: Box<str>,
        limits: Limits,
        key_hasher: KeyHasher,
    ) -> Result<Arc<Self>, (Problem, Box<str>)> {
        let mut owner = Arc::new(Self {
            _lifetime: Lifetime::new(),
            source,
            positions: Mutex::default(),
            member_indexes: Vec::new(),
            key_hasher,
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
        });
        let parsed = Parser {
            out: Arc::get_mut(&mut owner).expect("a new owner"),
            pos: 0,
            open: NO_PARENT,
            state: 0,
            depth: 0,
            pending: Vec::new(),
            distinct: Vec::new(),
            limits,
        }
        .document();
        match parsed {
            Ok(()) => Ok(owner),
            Err(problem) => {
                let arena = Arc::into_inner(owner).expect("a new owner");
                Err((problem, arena.source))
            }
        }
    }
    fn equal_keys(&self, left: Id, right: Id) -> bool {
        #[cfg(test)]
        self.key_comparisons
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match (self.string(left), self.string(right)) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.utf16(left).eq(self.utf16(right)),
            _ => false,
        }
    }
    fn hash_key(&self, id: Id) -> u64 {
        if let Some(s) = self.string(id) {
            self.key_hasher.hash(s.encode_utf16())
        } else if let Some(units) = self.unpaired_units(id) {
            self.key_hasher.hash(units)
        } else {
            unreachable!("only string tokens are member names")
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
    fn decode(&mut self, range: Range<usize>) {
        self.reserve_decoded(range.len());
        let Self {
            decoded, source, ..
        } = self;
        decoded.push_str(&source[range]);
    }
    // Grow the decoded buffer geometrically, but never past the source's
    // length, which bounds all decoded text.
    fn reserve_decoded(&mut self, additional: usize) {
        let (len, capacity) = (self.decoded.len(), self.decoded.capacity());
        if capacity - len >= additional {
            return;
        }
        let target = (len + additional).max(capacity.saturating_mul(2).min(self.source.len()));
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
        let node = &self.nodes[id];
        match node.kind {
            Kind::String { text, .. } => match text.decoded() {
                Some(index) => Some(self.decoded_text(index)),
                None if text == Text::SOURCE => {
                    Some(&self.source[node.span.start + 1..node.span.end - 1])
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
        let span = &self.nodes[id].span;
        Utf16 {
            body: &self.source[span.start + 1..span.end - 1],
            at: 0,
            low: None,
        }
    }
    /// Separately held decoded bytes of an escaped scalar string; zero for one
    /// that borrows its source.
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
    fn member_index(&self, object: Id) -> Option<&MemberIndex> {
        let object = object as u32;
        let at = self
            .member_indexes
            .binary_search_by_key(&object, |&(id, _)| id)
            .ok()?;
        Some(&self.member_indexes[at].1)
    }
    pub(crate) fn get_with_probe(
        &self,
        id: Id,
        key: &str,
        mut probe: impl FnMut() -> bool,
    ) -> Option<Id> {
        let names = self.member_names(id)?;
        let matches = |name: u32, admitted: bool| {
            if !admitted {
                return None;
            }
            #[cfg(test)]
            self.member_probes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Some(self.name_is(name as usize, key))
        };
        if let Some(index) = self.member_index(id) {
            // hash the token once, then inspect only its exact-name bucket.
            // Charge the table lookup as well as each collision candidate when
            // called by the evaluator. Small objects scan at most 15 members.
            if !probe() {
                return None;
            }
            let hash = self.key_hasher.hash(key.encode_utf16());
            for &slot in index.members(hash) {
                let name = names[slot as usize];
                if matches(name, probe())? {
                    return Some(name as usize + 1);
                }
            }
        } else {
            for &name in names {
                if matches(name, probe())? {
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
        // An ancestor's name holds an unpaired unit: write escaped units.
        let segments: Vec<Vec<u16>> = self
            .edges(id, root)
            .map(|edge| match edge {
                Edge::Index(index) => index.to_string().encode_utf16().collect(),
                Edge::Key(key) => self.units(key).expect("member name is a string"),
            })
            .collect();
        let escaped: usize = segments
            .iter()
            .map(|segment| {
                1 + segment.len()
                    + segment
                        .iter()
                        .filter(|&&unit| unit == u16::from(b'~') || unit == u16::from(b'/'))
                        .count()
            })
            .sum();
        let mut units = Vec::with_capacity(escaped);
        for segment in segments.iter().rev() {
            units.push(u16::from(b'/'));
            for &unit in segment {
                match u8::try_from(unit) {
                    Ok(b'~') => units.extend([u16::from(b'~'), u16::from(b'0')]),
                    Ok(b'/') => units.extend([u16::from(b'~'), u16::from(b'1')]),
                    _ => units.push(unit),
                }
            }
        }
        crate::Pointer::from_escaped_units(units)
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
struct Parser<'a> {
    out: &'a mut Arena,
    pos: usize,
    /// The innermost unclosed container, or `NO_PARENT`.
    open: u32,
    state: u8,
    depth: usize,
    /// Items of the unclosed containers, outermost first. While open, a
    /// container's `first` is where its items begin here.
    pending: Vec<u32>,
    /// A small object's distinct names so far, with their hashes.
    distinct: Vec<(u64, u32)>,
    limits: Limits,
}
impl Parser<'_> {
    /// Parse the whole source: one value, then only whitespace.
    fn document(&mut self) -> Result<(), Problem> {
        self.value(NO_PARENT, false)?;
        // `open` is the innermost unclosed container; its state is 0 just
        // after it opens, 1 after an item and 2 after a separating comma.
        while self.open != NO_PARENT {
            let id = self.open as usize;
            let state = self.state;
            let object = matches!(self.out.nodes[id].kind, Kind::Object { .. });
            self.space();
            let next = self.peek();
            let close = if object { b'}' } else { b']' };
            if next == Some(close) && state != 2 {
                self.pos += 1;
                self.close(id);
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
                self.space();
                if self.peek() != Some(b':') {
                    return Err(self.error("colon"));
                }
                self.pos += 1;
                self.state = 1;
                self.value(id as u32, false)?;
            } else {
                self.state = 1;
                self.value(id as u32, true)?;
            }
        }
        self.space();
        if self.pos != self.out.source.len() {
            return Err(self.error("trailing-token"));
        }
        // Objects record their repeated names and indexes as they close,
        // innermost first; token ids follow the source, so this restores
        // source order.
        self.out.duplicates.sort_unstable_by_key(|&(_, key)| key);
        self.out
            .member_indexes
            .sort_unstable_by_key(|&(object, _)| object);
        Ok(())
    }
    fn error(&self, code: &'static str) -> Problem {
        Problem {
            code,
            offset: self.pos,
        }
    }
    fn peek(&self) -> Option<u8> {
        self.out.source.as_bytes().get(self.pos).copied()
    }
    fn space(&mut self) {
        while self
            .peek()
            .is_some_and(|c| matches!(c, b' ' | b'\n' | b'\r' | b'\t'))
        {
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
    fn close(&mut self, id: Id) {
        let node = &mut self.out.nodes[id];
        node.span.end = self.pos;
        let parent = node.parent;
        let object = matches!(node.kind, Kind::Object { .. });
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
    // Compare only distinct names sharing a keyed hash, in member order.
    // Expected work is linear in decoded key units plus matching repeats. A
    // bucket of b adversarially colliding distinct names still takes up to b
    // exact comparisons per insertion/lookup, each bounded by the compared key
    // lengths. RandomState makes deliberate collisions difficult, not
    // impossible. Input byte and node limits bound those lengths and b (at
    // most half the admitted nodes for member-name/value pairs), not constant
    // work.
    fn members(&mut self, object: Id, start: usize, count: usize) {
        let out = &mut *self.out;
        let names = &out.items[start..start + count];
        if count < INDEX_MIN_MEMBERS {
            self.distinct.clear();
            for &name in names {
                let hash = out.hash_key(name as usize);
                let repeated = self.distinct.iter().any(|&(seen, first)| {
                    seen == hash && out.equal_keys(first as usize, name as usize)
                });
                if repeated {
                    out.duplicates.push((object as u32, name));
                } else {
                    self.distinct.push((hash, name));
                }
            }
            return;
        }
        let mut index = MemberIndex::with_capacity(count);
        for (slot, &name) in names.iter().enumerate() {
            let slot = slot as u32;
            let hash = out.hash_key(name as usize);
            let Some(&bucket) = index.slots.get(&hash) else {
                index.slots.insert(hash, KeyBucket::One(slot));
                continue;
            };
            let first;
            let candidates = match bucket {
                KeyBucket::One(slot) => {
                    first = slot;
                    std::slice::from_ref(&first)
                }
                KeyBucket::Many(list) => index.collisions[list as usize].as_slice(),
            };
            if candidates
                .iter()
                .any(|&prior| out.equal_keys(names[prior as usize] as usize, name as usize))
            {
                out.duplicates.push((object as u32, name));
                continue;
            }
            match bucket {
                KeyBucket::One(first) => {
                    let list = u32::try_from(index.collisions.len())
                        .expect("collisions are bounded by the node count");
                    index.collisions.push(vec![first, slot]);
                    index.slots.insert(hash, KeyBucket::Many(list));
                }
                KeyBucket::Many(list) => index.collisions[list as usize].push(slot),
            }
        }
        out.member_indexes.push((object as u32, index));
    }
    /// Parse one value; an `element` is listed as an item of its array parent.
    /// A container is left open, its items to follow.
    fn value(&mut self, parent: u32, element: bool) -> Result<(), Problem> {
        self.space();
        let start = self.pos;
        let id = match self.peek() {
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
            Some(b'-' | b'0'..=b'9') => {
                if self.peek() == Some(b'-') {
                    self.pos += 1;
                }
                match self.peek() {
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
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
    }
    fn literal(&mut self, text: &str, kind: Kind, parent: u32) -> Result<u32, Problem> {
        let start = self.pos;
        if !self.out.source[self.pos..].starts_with(text) {
            return Err(self.error("literal"));
        }
        self.pos += text.len();
        self.push(kind, start, parent)
    }
    // Advance to the next quote, backslash or control byte, or the end.
    fn plain(&mut self) {
        let rest = &self.out.source.as_bytes()[self.pos..];
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
        let escape = self.out.source.as_bytes().get(at..at + 6)?;
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
                        self.out.decode(segment..self.pos);
                    }
                    self.pos += 1;
                    let Some(escape) = self.peek() else {
                        return Err(self.error("escape"));
                    };
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
                        self.out.reserve_decoded(c.len_utf8());
                        self.out.decoded.push(c);
                    }
                    segment = self.pos;
                }
                Some(_) => self.plain(),
            }
        }
        let end = self.pos;
        self.pos += 1;
        let text = if unpaired {
            self.out.decoded.truncate(decoded_start);
            Text::UNPAIRED
        } else {
            self.out.decode(segment..end);
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
    use std::{fmt::Write, sync::atomic::Ordering};

    fn comparisons(arena: &Arena) -> usize {
        arena.key_comparisons.load(Ordering::Relaxed)
    }
    fn probes(arena: &Arena) -> usize {
        arena.member_probes.load(Ordering::Relaxed)
    }
    fn with_hash(text: &str, hash: fn(&[u16]) -> u64) -> crate::JsonValue {
        crate::JsonValue {
            owner: Arena::parse_with_hasher(
                text.as_bytes(),
                Limits::default(),
                KeyHasher {
                    injected: Some(hash),
                    ..KeyHasher::default()
                },
            )
            .unwrap(),
            id: 0,
        }
    }
    fn distinct_hash(units: &[u16]) -> u64 {
        // Base 257 is injective for the short ASCII kN/sN names in the scaling
        // fixtures. This removes randomness from the asserted work counts.
        units.iter().fold(0_u64, |n, &u| {
            n.wrapping_mul(257).wrapping_add(u64::from(u))
        })
    }

    #[test]
    fn key_comparisons_and_member_probes_are_linear() {
        // N distinct names, and N names followed by the same N again.
        // Assert exact operation counts rather than scheduler-dependent timing.
        for n in [10_000, 20_000, 40_000] {
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
                let value = with_hash(&text, distinct_hash);
                assert_eq!(comparisons(&value.owner), if repeated { n } else { 0 });
                assert_eq!(value.owner.duplicates.len(), if repeated { n } else { 0 });
                assert_eq!(value.view().len(), Some(count));
                for i in 0..n {
                    assert_eq!(value.get(&format!("k{i}")).unwrap().text(), i.to_string());
                    assert!(value.get(&format!("s{i}")).is_none());
                }
                assert_eq!(probes(&value.owner), n);
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
    fn many_schema_pointer_references_have_linear_member_probes() {
        // match the reported shape, including reading each actual $ref.
        for n in [10_000, 20_000, 40_000] {
            let mut text = String::from("{\"schemas\":{");
            for i in 0..n {
                if i > 0 {
                    text.push(',');
                }
                write!(text, "\"s{i}\":{{\"$ref\":\"#/schemas/s{}\"}}", n - 1).unwrap();
            }
            text.push_str("}}");
            let document = with_hash(&text, distinct_hash);
            assert_eq!(comparisons(&document.owner), 0);
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
            // One $ref probe, one root probe and one indexed schemas probe.
            assert_eq!(probes(&document.owner), 3 * n);
        }
    }

    #[test]
    fn forced_collisions_keep_exact_names_order_and_coordinates() {
        // all hashes collide, including unpaired and escaped-equal
        // UTF-16 names. A hash is never authority for name equality.
        let prefix = "shared".repeat(1024);
        let mut text = format!(
            r#"{{"a":1,"\u0061":2,"\ud800":3,"\uD800":4,"\udc00":5,"😀":6,"\ud83d\ude00":7,"{prefix}x":8,"{prefix}y":9,"nested":{{"a":10,"\u0061":11}}"#
        );
        for i in 0..16 {
            write!(text, ",\"pad{i}\":{i}").unwrap();
        }
        text.push('}');
        let value = with_hash(&text, |_| 0);
        let a = &value.owner;
        assert_eq!(comparisons(a), 23 * 22 / 2 + 1 + 2 + 4 + 1);
        assert_eq!(value.get("a").unwrap().text(), "1");
        assert_eq!(value.get("😀").unwrap().text(), "6");
        assert_eq!(value.get(&format!("{prefix}x")).unwrap().text(), "8");
        assert_eq!(value.get(&format!("{prefix}y")).unwrap().text(), "9");
        let before = probes(a);
        assert!(value.get(&format!("{prefix}z")).is_none());
        // Every distinct first name is tried, once. Repeats have no index slot.
        assert_eq!(probes(a) - before, 23);
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
        // Force collision fallback in pointer lookup too.
        let root = with_hash(&text, |_| 0);
        let retained = root.get("outer").unwrap().to_owned();
        drop(root);
        let count = probes(&retained.owner);
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
        assert!(probes(&retained.owner) > count);
        assert_eq!(
            retained
                .at("/a~1~0/0/~01")
                .unwrap()
                .location()
                .pointer
                .unwrap(),
            "/outer/a~1~0/0/~01"
        );
        // Small objects intentionally have no index.
        let small = retained.at("/a~1~0/0").unwrap();
        assert!(retained.owner.member_index(small.id).is_none());
        assert!(retained.owner.member_index(retained.id).is_some());
    }

    #[cfg(feature = "evaluator-work")]
    #[test]
    fn evaluator_index_preserves_collision_work_accounting() {
        // the evaluator charges the table access and every exact candidate,
        // and refuses exhausted budgets without returning a partial match.
        use crate::work;
        use jsonschema_value::{JsonNumber as _, Node as _, Object as _};
        let mut text = String::from("{");
        for i in 0..20 {
            if i > 0 {
                text.push(',');
            }
            write!(text, "\"k{i}\":{i}").unwrap();
        }
        text.push('}');
        for (hash, required) in [
            (distinct_hash as fn(&[u16]) -> u64, 2),
            ((|_| 0) as fn(&[u16]) -> u64, 21),
        ] {
            let value = with_hash(&text, hash);
            let view = crate::backend::View::Stored(&value.owner, 0);
            let object = view.as_object().unwrap();
            let key = String::from("k19");
            let result = work::scope(required, || {
                object
                    .get(&key)
                    .unwrap()
                    .as_number()
                    .unwrap()
                    .as_str()
                    .into_owned()
            });
            assert_eq!(result, Ok(String::from("19")));
            assert_eq!(
                work::scope(required - 1, || object.get(&key).is_some()),
                Err(work::Spent)
            );
            assert_eq!(
                work::scope(0, || object.get(&String::from("absent")).is_some()),
                Err(work::Spent)
            );
        }
    }

    #[test]
    fn member_index_threshold_keeps_first_match_and_absence() {
        // exercise both sides of the small-object scan threshold.
        for n in [INDEX_MIN_MEMBERS - 1, INDEX_MIN_MEMBERS] {
            let mut text = String::from(r#"{"a":1,"\u0061":2"#);
            for i in 2..n {
                write!(text, ",\"k{i}\":{i}").unwrap();
            }
            text.push('}');
            let value = with_hash(&text, distinct_hash);
            assert_eq!(
                value.owner.member_index(0).is_some(),
                n >= INDEX_MIN_MEMBERS
            );
            assert_eq!(value.get("a").unwrap().text(), "1");
            for i in 2..n {
                assert_eq!(value.get(&format!("k{i}")).unwrap().text(), i.to_string());
            }
            assert!(value.get("absent").is_none());
            assert_eq!(value.owner.duplicates.len(), 1);
        }
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

    fn scanned(arena: &Arena) -> usize {
        arena
            .position_scanned
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn assert_position_bound(arena: &Arena, requests: &[usize]) {
        let maximum = requests.iter().copied().max().unwrap_or(0);
        let blocks = maximum / POSITION_STRIDE;
        let tail_bytes: usize = requests.iter().map(|offset| offset % POSITION_STRIDE).sum();
        assert_eq!(scanned(arena), blocks * POSITION_STRIDE + tail_bytes);
        let index = arena.positions.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(index.populated, blocks);
        assert!(index.checkpoints.len() <= blocks * 2);
        assert!(index.checkpoints.len() <= arena.source.len() / POSITION_STRIDE);
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
        assert_position_bound(&arena, &requests);

        for length in [POSITION_STRIDE - 1, POSITION_STRIDE, POSITION_STRIDE + 1] {
            let source = format!("{}null", "\n".repeat(length - 4));
            let arena = Arena::parse(source.as_bytes(), Limits::default()).unwrap();
            for offset in [0, length - 1, length] {
                assert_eq!(arena.position(offset), prefix_position(&source, offset));
            }
            assert_position_bound(&arena, &[0, length - 1, length]);
        }
    }

    #[test]
    fn positions_are_lazy_bounded_and_reused_in_any_order() {
        let source = position_source();
        let arena = Arena::parse(source.as_bytes(), Limits::default()).unwrap();
        assert_position_bound(&arena, &[]);
        for invalid in [POSITION_STRIDE, source.len() + 1, usize::MAX] {
            assert_eq!(arena.position(invalid), None);
        }
        assert_position_bound(&arena, &[]);
        let mut requests = vec![0, 1, 5];
        for &offset in &requests {
            assert_eq!(arena.position(offset), prefix_position(&source, offset));
        }
        assert_position_bound(&arena, &requests); // No allocation or full-source scan.
        let mut order = vec![source.len(), POSITION_STRIDE - 1, 2 * POSITION_STRIDE, 0];
        let mut state = 0x5eed_u64;
        for _ in 0..256 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let offset = state as usize % (source.len() + 1);
            if source.is_char_boundary(offset) {
                order.push(offset);
            }
        }
        order.extend(order.clone().into_iter().rev());
        order.extend([source.len(); 32]);
        for offset in order {
            assert_eq!(arena.position(offset), prefix_position(&source, offset));
            requests.push(offset);
        }
        for invalid in [POSITION_STRIDE, source.len() + 1, usize::MAX] {
            assert_eq!(arena.position(invalid), None);
        }
        assert_position_bound(&arena, &requests);
    }

    #[test]
    fn checkpoint_storage_grows_geometrically_with_a_source_cap() {
        let source = format!("{}null", " \n\r\t".repeat(9 * POSITION_STRIDE / 4));
        let arena = Arena::parse(source.as_bytes(), Limits::default()).unwrap();
        let mut requests = Vec::new();
        for (block, slots) in [(1, 1), (2, 2), (3, 4), (4, 4), (5, 8), (8, 8), (9, 9)] {
            let offset = block * POSITION_STRIDE;
            assert_eq!(arena.position(offset), prefix_position(&source, offset));
            requests.push(offset);
            assert_eq!(arena.positions.lock().unwrap().checkpoints.len(), slots);
            assert_position_bound(&arena, &requests);
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
        assert!(
            leaves
                .iter()
                .all(|leaf| Arc::ptr_eq(&root.owner, &leaf.owner))
        );
        let weak = Arc::downgrade(&root.owner);
        drop(members);
        drop(root);
        assert!(weak.upgrade().is_some());

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
        let requests: Vec<_> = (0..4 * 32)
            .flat_map(|_| expected.iter().map(|&(offset, _)| offset))
            .collect();
        assert_position_bound(&leaves[0].owner, &requests);
        drop(leaves);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn position_index_recovers_complete_prefix_after_mutex_poison() {
        let source = position_source();
        let arena = Arena::parse(source.as_bytes(), Limits::default()).unwrap();
        let early = 2 * POSITION_STRIDE;
        assert_eq!(arena.position(early), prefix_position(&source, early));
        let result = std::panic::catch_unwind(|| {
            let _guard = arena.positions.lock().unwrap();
            panic!("intentional private lock poison");
        });
        assert!(result.is_err());
        assert!(arena.positions.is_poisoned());
        assert_eq!(
            arena.position(source.len()),
            prefix_position(&source, source.len())
        );
        assert_eq!(arena.position(early), prefix_position(&source, early));
        assert_position_bound(&arena, &[early, source.len(), early]);
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
            let escaped = body.contains('\\');
            for id in [1, 3] {
                match String::from_utf16(&units) {
                    Ok(scalar) => {
                        assert_eq!(arena.string(id), Some(scalar.as_str()), "{body}");
                        assert!(arena.unpaired_units(id).is_none(), "{body}");
                        let held = if escaped { scalar.len() } else { 0 };
                        assert_eq!(arena.decoded_len(id), held, "{body}");
                    }
                    Err(_) => {
                        assert_eq!(arena.string(id), None, "{body}");
                        assert_eq!(
                            arena.unpaired_units(id).map(Iterator::collect::<Vec<_>>),
                            Some(units.clone()),
                            "{body}"
                        );
                        assert_eq!(arena.decoded_len(id), 0, "{body}");
                    }
                }
                assert_eq!(arena.units(id).as_deref(), Some(units.as_slice()), "{body}");
            }
            let unpaired = String::from_utf16(&units).is_err();
            assert_eq!(arena.unpaired, if unpaired { vec![1, 3] } else { vec![] });
        }
    }
    #[test]
    fn nodes_are_fixed_records_and_containers_own_no_allocation() {
        // Small, deterministic representation check, not a peak-memory benchmark.
        assert_eq!(std::mem::size_of::<Node>(), 32);
        let depth = 1000;
        let text = format!("{}0{}", "[{\"a\":".repeat(depth), "}]".repeat(depth));
        let arena = Arena::parse(text.as_bytes(), Limits::default()).unwrap();
        assert_eq!(arena.nodes.len(), 3 * depth + 1);
        // One item per element and per member name, all in one list.
        assert_eq!(arena.items.len(), 2 * depth);
        assert!(arena.decoded.is_empty());
        let leaf = arena.nodes.len() - 1;
        let pointer = "/0/a".repeat(depth);
        assert_eq!(arena.pointer(leaf).as_deref(), Some(pointer.as_str()));
        assert_eq!(arena.at(&pointer), Some(leaf));
    }
    #[test]
    fn exact_and_duplicate() {
        let a = Arena::parse(
            br#"{"a":9007199254740993,"\u0061":-0.00e+004,"s":"\ud83d\ude00"}"#,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(a.duplicates.len(), 1);
        assert_eq!(a.raw(a.get(0, "a").unwrap()), "9007199254740993");
        assert_eq!(a.string(a.get(0, "s").unwrap()), Some("😀"));
    }
    #[test]
    fn valid_unpaired_is_not_json_error() {
        let a = Arena::parse(
            br#"{"\ud800":1,"\uD800":2,"v":"\udc00"}"#,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(a.unpaired.len(), 3);
        assert_eq!(a.duplicates.len(), 1);
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
    #[test]
    fn source_locations() {
        let a = Arena::parse("{\n\"é/~\": [true]}".as_bytes(), Limits::default()).unwrap();
        let id = a.get(0, "é/~").unwrap();
        let v = a.elements(id).unwrap();
        assert_eq!(a.pointer(v[0] as usize).as_deref(), Some("/é~1~0/0"));
        assert_eq!(a.position(a.nodes[v[0] as usize].span.start), Some((2, 10)));
    }
}
