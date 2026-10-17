//! Private flat exact-JSON storage; iterative parsing and destruction.
use std::{
    collections::{HashMap, hash_map::RandomState},
    hash::{BuildHasher, Hasher},
    ops::Range,
    sync::{Arc, Mutex},
};

pub type Id = usize;

const INDEX_MIN_MEMBERS: usize = 16;

// One slot per distinct decoded key. Equal repeats reuse their first slot;
// colliding distinct keys keep separate slots, in original member order.
// Inline singletons avoid one heap allocation per ordinary key.
#[derive(Debug)]
enum KeyBucket {
    One(usize),
    Many(Vec<usize>),
}
impl KeyBucket {
    fn members(&self) -> &[usize] {
        match self {
            Self::One(index) => std::slice::from_ref(index),
            Self::Many(indices) => indices,
        }
    }
    fn push(&mut self, index: usize) {
        match self {
            Self::One(first) => *self = Self::Many(vec![*first, index]),
            Self::Many(indices) => indices.push(index),
        }
    }
}
type MemberIndex = HashMap<u64, KeyBucket>;

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
#[derive(Debug, Clone)]
pub enum Kind {
    Null,
    Bool(bool),
    Number,
    String {
        decoded: Option<Box<str>>,
        unpaired: Option<Box<[u16]>>,
    },
    Array(Vec<Id>),
    Object(Vec<Member>),
}
#[derive(Debug, Clone, Copy)]
pub struct Member {
    pub key: Id,
    pub value: Id,
}
#[derive(Debug, Clone, Copy)]
pub enum Edge {
    Key(Id),
    Index(usize),
}
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    pub span: Range<usize>,
    pub parent: Option<(Id, Edge)>,
}
#[derive(Debug, Clone)]
pub struct Problem {
    pub code: &'static str,
    pub offset: usize,
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
    source: Arc<str>,
    positions: Mutex<PositionIndex>,
    // Completed objects publish the parser's existing buckets. The immutable
    // index is shared by retained views and threads, without lookup-time locks.
    member_indexes: HashMap<Id, MemberIndex>,
    key_hasher: KeyHasher,
    #[cfg(test)]
    key_comparisons: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    member_probes: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    position_scanned: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub duplicate_query_probes: std::sync::atomic::AtomicUsize,
    pub nodes: Vec<Node>,
    pub duplicates: Vec<(Id, Id)>,
    pub unpaired: Vec<Id>,
}
impl Arena {
    pub fn parse(bytes: &[u8], limits: Limits) -> Result<Self, Problem> {
        Self::parse_with_hasher(bytes, limits, KeyHasher::default())
    }
    fn parse_with_hasher(
        bytes: &[u8],
        limits: Limits,
        key_hasher: KeyHasher,
    ) -> Result<Self, Problem> {
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(Problem {
                code: "bom",
                offset: 0,
            });
        }
        let text = std::str::from_utf8(bytes).map_err(|e| Problem {
            code: "utf8",
            offset: e.valid_up_to(),
        })?;
        let mut parser = Parser {
            out: Self {
                _lifetime: Lifetime::new(),
                source: Arc::from(text),
                positions: Mutex::default(),
                member_indexes: HashMap::new(),
                key_hasher,
                #[cfg(test)]
                key_comparisons: std::sync::atomic::AtomicUsize::new(0),
                #[cfg(test)]
                member_probes: std::sync::atomic::AtomicUsize::new(0),
                #[cfg(test)]
                position_scanned: std::sync::atomic::AtomicUsize::new(0),
                #[cfg(test)]
                duplicate_query_probes: std::sync::atomic::AtomicUsize::new(0),
                nodes: Vec::new(),
                duplicates: Vec::new(),
                unpaired: Vec::new(),
            },
            pos: 0,
            stack: Vec::new(),
            limits,
        };
        parser.value(None)?;
        while let Some(frame) = parser.stack.last() {
            let id = frame.id;
            let state = frame.state;
            let object = matches!(parser.out.nodes[id].kind, Kind::Object(_));
            parser.space();
            let next = parser.peek();
            let close = if object { b'}' } else { b']' };
            if next == Some(close) && state != 2 {
                parser.pos += 1;
                parser.out.nodes[id].span.end = parser.pos;
                let frame = parser.stack.pop().unwrap();
                if let Kind::Object(members) = &parser.out.nodes[id].kind
                    && members.len() >= INDEX_MIN_MEMBERS
                {
                    parser.out.member_indexes.insert(id, frame.keys);
                }
                continue;
            }
            if state == 1 {
                if next != Some(b',') {
                    return Err(parser.error("comma-or-close"));
                }
                parser.pos += 1;
                parser.stack.last_mut().unwrap().state = 2;
                continue;
            }
            if object {
                if next != Some(b'"') {
                    return Err(parser.error("object-key"));
                }
                let key = parser.string(None)?;
                let hash = parser.out.hash_key(key);
                let Kind::Object(members) = &parser.out.nodes[id].kind else {
                    unreachable!()
                };
                // compare only distinct names sharing this keyed hash.
                // Expected work is linear in decoded key units plus matching
                // repeats. A bucket of b adversarially colliding distinct names
                // still takes up to b exact comparisons per insertion/lookup,
                // each bounded by the compared key lengths. RandomState makes
                // deliberate collisions difficult, not impossible. Input byte
                // and node limits bound those lengths and b (at most half the
                // admitted nodes for member-name/value pairs), not constant work.
                match parser.stack.last_mut().unwrap().keys.entry(hash) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(KeyBucket::One(members.len()));
                    }
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        if entry
                            .get()
                            .members()
                            .iter()
                            .any(|&index| parser.out.equal_keys(members[index].key, key))
                        {
                            parser.out.duplicates.push((id, key));
                        } else {
                            entry.get_mut().push(members.len());
                        }
                    }
                }
                parser.out.nodes[key].parent = Some((id, Edge::Key(key)));
                parser.space();
                if parser.peek() != Some(b':') {
                    return Err(parser.error("colon"));
                }
                parser.pos += 1;
                parser.stack.last_mut().unwrap().state = 1;
                let value = parser.value(Some((id, Edge::Key(key))))?;
                let Kind::Object(members) = &mut parser.out.nodes[id].kind else {
                    unreachable!()
                };
                members.push(Member { key, value });
            } else {
                let Kind::Array(children) = &parser.out.nodes[id].kind else {
                    unreachable!()
                };
                let index = children.len();
                parser.stack.last_mut().unwrap().state = 1;
                let value = parser.value(Some((id, Edge::Index(index))))?;
                let Kind::Array(children) = &mut parser.out.nodes[id].kind else {
                    unreachable!()
                };
                children.push(value);
            }
        }
        parser.space();
        if parser.pos != parser.out.source.len() {
            return Err(parser.error("trailing-token"));
        }
        Ok(parser.out)
    }
    fn equal_keys(&self, left: Id, right: Id) -> bool {
        #[cfg(test)]
        self.key_comparisons
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match (self.string(left), self.string(right)) {
            (Some(a), Some(b)) => a == b,
            _ => self.units(left) == self.units(right),
        }
    }
    fn hash_key(&self, id: Id) -> u64 {
        if let Some(s) = self.string(id) {
            self.key_hasher.hash(s.encode_utf16())
        } else if let Kind::String {
            unpaired: Some(units),
            ..
        } = &self.nodes[id].kind
        {
            self.key_hasher.hash(units.iter().copied())
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
    pub fn has_duplicate_names(&self, id: Id) -> bool {
        let node = &self.nodes[id];
        if !matches!(node.kind, Kind::Array(_) | Kind::Object(_)) {
            return false;
        }
        // Parsing records duplicate key tokens in source order. Object IDs are
        // not ordered here: a nested duplicate can precede its parent's next
        // duplicate. A key inside a container belongs to it or a descendant.
        let first = self.duplicates.partition_point(|&(_, key)| {
            #[cfg(test)]
            self.duplicate_query_probes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.nodes[key].span.start < node.span.start
        });
        self.duplicates
            .get(first)
            .is_some_and(|&(_, key)| self.nodes[key].span.start < node.span.end)
    }
    pub fn string(&self, id: Id) -> Option<&str> {
        match &self.nodes[id].kind {
            Kind::String {
                decoded: Some(s), ..
            } => Some(s),
            Kind::String {
                decoded: None,
                unpaired: None,
            } => Some(&self.source[self.nodes[id].span.start + 1..self.nodes[id].span.end - 1]),
            _ => None,
        }
    }
    pub fn units(&self, id: Id) -> Option<Vec<u16>> {
        match &self.nodes[id].kind {
            Kind::String {
                unpaired: Some(s), ..
            } => Some(s.to_vec()),
            _ => self.string(id).map(|s| s.encode_utf16().collect()),
        }
    }
    pub fn get(&self, id: Id, key: &str) -> Option<Id> {
        self.get_with_probe(id, key, || true)
    }
    pub(crate) fn get_with_probe(
        &self,
        id: Id,
        key: &str,
        mut probe: impl FnMut() -> bool,
    ) -> Option<Id> {
        let Kind::Object(members) = &self.nodes[id].kind else {
            return None;
        };
        let matches = |member: &Member, admitted: bool| {
            if !admitted {
                return None;
            }
            #[cfg(test)]
            self.member_probes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Some(self.string(member.key) == Some(key))
        };
        if let Some(index) = self.member_indexes.get(&id) {
            // hash the token once, then inspect only its exact-name bucket.
            // Charge the table lookup as well as each collision candidate when
            // called by the evaluator. Small objects scan at most 15 members.
            if !probe() {
                return None;
            }
            let hash = self.key_hasher.hash(key.encode_utf16());
            for &slot in index.get(&hash)?.members() {
                if matches(&members[slot], probe())? {
                    return Some(members[slot].value);
                }
            }
        } else {
            for member in members {
                if matches(member, probe())? {
                    return Some(member.value);
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
            current = match &self.nodes[current].kind {
                Kind::Object(_) => self.get(current, name)?,
                Kind::Array(values) => {
                    if name.len() > 1 && name.starts_with('0') {
                        return None;
                    }
                    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
                        return None;
                    }
                    *values.get(name.parse::<usize>().ok()?)?
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
        let escaped =
            |name: &str| name.len() + name.bytes().filter(|&b| b == b'~' || b == b'/').count();
        let edges = |mut current: Id| {
            std::iter::from_fn(move || {
                (current != root).then(|| {
                    let (parent, edge) =
                        self.nodes[current].parent.expect("an ancestor of the node");
                    current = parent;
                    edge
                })
            })
        };
        if id == root {
            return Some(String::new());
        }
        let mut size = 0usize;
        for edge in edges(id) {
            size += 1 + match edge {
                Edge::Index(index) => index
                    .checked_ilog10()
                    .map_or(1, |digits| digits as usize + 1),
                Edge::Key(key) => escaped(self.string(key)?),
            };
        }
        let mut bytes = vec![0u8; size];
        let mut end = size;
        for edge in edges(id) {
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
                    let start = end - escaped(name);
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
        Some(String::from_utf8(bytes).expect("segments are UTF-8"))
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
        let mut segments = Vec::new();
        let mut current = id;
        while current != root {
            let (parent, edge) = self.nodes[current].parent.expect("an ancestor of the node");
            segments.push(match edge {
                Edge::Index(index) => index.to_string().encode_utf16().collect(),
                Edge::Key(key) => self.units(key).expect("member name is a string"),
            });
            current = parent;
        }
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
struct Frame {
    id: Id,
    state: u8,
    keys: MemberIndex,
}
struct Parser {
    out: Arena,
    pos: usize,
    stack: Vec<Frame>,
    limits: Limits,
}
impl Parser {
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
    fn push(
        &mut self,
        kind: Kind,
        start: usize,
        parent: Option<(Id, Edge)>,
    ) -> Result<Id, Problem> {
        if self.out.nodes.len() >= self.limits.max_nodes {
            return Err(self.error("node-limit"));
        }
        let id = self.out.nodes.len();
        self.out.nodes.push(Node {
            kind,
            span: start..self.pos,
            parent,
        });
        Ok(id)
    }
    fn value(&mut self, parent: Option<(Id, Edge)>) -> Result<Id, Problem> {
        self.space();
        let start = self.pos;
        match self.peek() {
            Some(b'{') | Some(b'[') => {
                if self.stack.len() >= self.limits.max_depth {
                    return Err(self.error("depth-limit"));
                }
                let kind = if self.peek() == Some(b'{') {
                    Kind::Object(Vec::new())
                } else {
                    Kind::Array(Vec::new())
                };
                self.pos += 1;
                let id = self.push(kind, start, parent)?;
                self.stack.push(Frame {
                    id,
                    state: 0,
                    keys: HashMap::new(),
                });
                Ok(id)
            }
            Some(b'"') => self.string(parent),
            Some(b'n') => self.literal("null", Kind::Null, parent),
            Some(b't') => self.literal("true", Kind::Bool(true), parent),
            Some(b'f') => self.literal("false", Kind::Bool(false), parent),
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
                self.push(Kind::Number, start, parent)
            }
            _ => Err(self.error("value")),
        }
    }
    fn digits(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
    }
    fn literal(
        &mut self,
        text: &str,
        kind: Kind,
        parent: Option<(Id, Edge)>,
    ) -> Result<Id, Problem> {
        let start = self.pos;
        if !self.out.source[self.pos..].starts_with(text) {
            return Err(self.error("literal"));
        }
        self.pos += text.len();
        self.push(kind, start, parent)
    }
    fn string(&mut self, parent: Option<(Id, Edge)>) -> Result<Id, Problem> {
        let start = self.pos;
        self.pos += 1;
        let mut segment = self.pos;
        let mut units: Option<Vec<u16>> = None;
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated-string")),
                Some(0..=0x1f) => return Err(self.error("string-control")),
                Some(b'"') => {
                    if let Some(u) = units.as_mut() {
                        u.extend(self.out.source[segment..self.pos].encode_utf16());
                    }
                    self.pos += 1;
                    let (decoded, unpaired) = match units {
                        None => (None, None),
                        Some(u) => match String::from_utf16(&u) {
                            Ok(s) => (Some(s.into_boxed_str()), None),
                            Err(_) => (None, Some(u.into_boxed_slice())),
                        },
                    };
                    let lone = unpaired.is_some();
                    let id = self.push(Kind::String { decoded, unpaired }, start, parent)?;
                    if lone {
                        self.out.unpaired.push(id);
                    }
                    return Ok(id);
                }
                Some(b'\\') => {
                    let u = units.get_or_insert_with(Vec::new);
                    u.extend(self.out.source[segment..self.pos].encode_utf16());
                    self.pos += 1;
                    let Some(escape) = self.peek() else {
                        return Err(self.error("escape"));
                    };
                    self.pos += 1;
                    let unit = match escape {
                        b'"' => 34,
                        b'\\' => 92,
                        b'/' => 47,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'u' => {
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
                            n
                        }
                        _ => return Err(self.error("escape")),
                    };
                    units.as_mut().unwrap().push(unit);
                    segment = self.pos;
                }
                Some(_) => {
                    self.pos += 1;
                }
            }
        }
    }
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
            owner: Arc::new(
                Arena::parse_with_hasher(
                    text.as_bytes(),
                    Limits::default(),
                    KeyHasher {
                        injected: Some(hash),
                        ..KeyHasher::default()
                    },
                )
                .unwrap(),
            ),
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
                            .map(|id| (0, id))
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
            .map(|&(_, id)| a.nodes[id].span.start)
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
        assert!(!retained.owner.member_indexes.contains_key(&small.id));
        assert!(retained.owner.member_indexes.contains_key(&retained.id));
    }

    #[test]
    fn evaluator_index_preserves_collision_work_accounting() {
        // the evaluator charges the table access and every exact candidate,
        // and refuses exhausted budgets without returning a partial match.
        use jsonschema::json::{JsonNumber as _, Node as _, Object as _};
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
            let result = jsonschema::ob_work::bounded(required, 10, || {
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
                jsonschema::ob_work::bounded(required - 1, 10, || object.get(&key).is_some()),
                Err(jsonschema::ob_work::Stop::Work)
            );
            assert_eq!(
                jsonschema::ob_work::bounded(0, 10, || object
                    .get(&String::from("absent"))
                    .is_some()),
                Err(jsonschema::ob_work::Stop::Work)
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
                value.owner.member_indexes.contains_key(&0),
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
            while let Some((parent, edge)) = a.nodes[current].parent {
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
            while let Some((parent, edge)) = a.nodes[current].parent {
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
        let Kind::Array(v) = &a.nodes[id].kind else {
            panic!()
        };
        assert_eq!(a.pointer(v[0]).as_deref(), Some("/é~1~0/0"));
        assert_eq!(a.position(a.nodes[v[0]].span.start), Some((2, 10)));
    }
}
