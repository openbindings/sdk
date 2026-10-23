//! OBI-13's compared `$id`s, held as shared spans of the document's own text.
//!
//! [`crate::uri::compared_id`] spells each compared identifier out, and a
//! nested resource's identifier repeats its enclosing one's: a chain of nested
//! relative `$id`s would spell quadratically many bytes. Here an identifier is
//! a handle: its scheme, the directory its hierarchical part ends in (the part
//! up to and including its last `/`), the last segment, the query and the
//! fragment. Directories form a trie whose edges are runs of whole segments,
//! each run a span of one `$id`'s text however many segments it holds. Each
//! declaration adds at most three edges and one leaf, and copies text only
//! when dot segments interrupt the run it writes, then once and never more
//! than its own `$id`. [`Identifiers::duplicates`] finds the handles that share
//! a string and orders them as their strings compare, so callers keep string
//! semantics.
use fluent_uri::UriRef;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet, hash_map::RandomState},
    hash::BuildHasher,
};

const NONE: u32 = u32::MAX;
/// The empty hierarchical part, then `/`, then `//`, below which the first
/// segment is an authority.
const ROOT: u32 = 0;
const SLASH: u32 = 1;
const AUTH: u32 = 2;

/// Text of a directory: a span of one `$id`'s text, or of the text copied out
/// of `$id`s whose removed dot segments separate the segments they write,
/// given by its offsets in all the copied text.
#[derive(Clone, Copy, Debug)]
enum Label<'a> {
    Borrowed(&'a str),
    Copied { start: usize, end: usize },
}
impl<'a> Label<'a> {
    fn sub(self, start: usize, end: usize) -> Self {
        match self {
            Self::Borrowed(text) => Self::Borrowed(&text[start..end]),
            Self::Copied { start: at, .. } => Self::Copied {
                start: at + start,
                end: at + end,
            },
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct Node<'a> {
    parent: u32,
    /// The directory a walk up from here stops at: `ROOT` for a path without
    /// a leading `/`, `SLASH` for one without an authority, and otherwise the
    /// authority's directory, `AUTH`'s child.
    top: u32,
    /// Segments from the root.
    depth: usize,
    /// The keyed hash of the parent and the label's first segment, which
    /// finds this node among its parent's children.
    key: u64,
    /// The segments after the parent's string, `/` between them and implied
    /// after the last.
    label: Label<'a>,
}
#[derive(Clone, Copy, Debug)]
struct Leaf<'a> {
    scheme: u32,
    dir: u32,
    last: &'a str,
    /// Indices into the pieces, or `NONE`.
    query: u32,
    fragment: u32,
    /// For a directory `//`, the directory of the authority that is the last
    /// segment.
    authority: u32,
}
/// The children whose parent and first segment share a hash: one, or a list
/// in `collisions`.
#[derive(Clone, Copy, Debug)]
enum Bucket {
    One(u32),
    Many(u32),
}

#[derive(Debug)]
pub(crate) struct Identifiers<'a> {
    nodes: Vec<Node<'a>>,
    /// The copied labels' text, in blocks: walks write to `copied`, which
    /// begins at offset `copied_from` of all the copied text, and `retired`
    /// holds the earlier blocks, each with its offset. Text a walk has kept
    /// never moves, so growing the text never copies it again or leaves its
    /// old storage behind.
    copied: String,
    copied_from: usize,
    retired: Vec<(usize, Box<str>)>,
    /// At least the length of the identifiers still to come, which bounds
    /// the text they can copy: no walk copies more than its own `$id`.
    budget: usize,
    children: HashMap<u64, Bucket>,
    collisions: Vec<Vec<u32>>,
    hasher: RandomState,
    /// Schemes, interned.
    schemes: Vec<&'a str>,
    scheme_ids: HashMap<&'a str, u32>,
    /// Queries and fragments; a query an identifier takes from its base is
    /// that base's.
    pieces: Vec<&'a str>,
    leaves: Vec<Leaf<'a>>,
    /// The authorities' directories whose authority does not parse as one
    /// when read back.
    invalid_authorities: HashSet<u32>,
}

/// Where the next own segment hangs: dot-segment removal's output is this
/// position's string, then the own segments a walk has written.
#[derive(Clone, Copy, Debug)]
enum Pos {
    /// `node`'s string without its last `up` segments.
    Node { node: u32, up: usize },
    /// The empty hierarchical part: the own segments begin it, with no `/`
    /// before the first.
    Root,
    /// `//` and an authority, whose directory this is.
    Auth(u32),
}
/// RFC 3986 §5.2.4 dot-segment removal in progress, as [`crate::uri`] does
/// it: the output is `x`'s string and then `count` own segments, `/` between
/// them. With no own segments it is `x`'s string without its final `/`.
struct Walk<'a> {
    own: &'a str,
    x: Pos,
    /// How many segments `x` may still give up before the output's floor.
    limit: usize,
    /// Whether giving up one more empties the path, as for a path without a
    /// leading `/`.
    rootless: bool,
    /// For an authority's position, the authority, which an output without
    /// own segments ends in.
    authority: &'a str,
    count: usize,
    /// The own segments: a span of `own`, or, once removed dot segments
    /// separate them, a copy at the end of the copied labels' current block,
    /// from `base`.
    start: usize,
    end: usize,
    copy: String,
    base: usize,
    copied: bool,
    /// The offset in all the copied text at which `copy` begins.
    from: usize,
    /// The block the walk left for want of room, for [`Identifiers::place`]
    /// to retire.
    retired: Option<String>,
    /// At least the length of this and the identifiers still to come.
    budget: usize,
    /// The last own segment written, while it is the output's.
    last: Option<&'a str>,
}
impl<'a> Walk<'a> {
    fn new(
        own: &'a str,
        x: Pos,
        authority: &'a str,
        copy: String,
        from: usize,
        budget: usize,
    ) -> Self {
        Self {
            own,
            x,
            limit: 0,
            rootless: false,
            authority,
            count: 0,
            start: 0,
            end: 0,
            base: copy.len(),
            copy,
            copied: false,
            from,
            retired: None,
            budget,
            last: None,
        }
    }
    /// Room for `more` bytes of this walk's copy. When the block lacks it
    /// and holds text earlier walks kept, the copy so far moves to a new
    /// block and the earlier text stays where it is. The new block is as
    /// large as all the copied text before it, so the blocks double in
    /// total without kept text moving, but never larger than the text the
    /// identifiers still to come can copy. A copy alone in its block grows
    /// as a string does.
    fn reserve(&mut self, more: usize) {
        if self.base == 0 || self.copy.capacity() - self.copy.len() >= more {
            return;
        }
        debug_assert!(self.retired.is_none(), "a moved copy is alone in its block");
        let written = &self.copy[self.base..];
        let total = (self.from + self.base).min(self.budget);
        let mut block = String::with_capacity((written.len() + more).max(total));
        block.push_str(written);
        self.copy.truncate(self.base);
        self.retired = Some(std::mem::replace(&mut self.copy, block));
        (self.from, self.base) = (self.from + self.base, 0);
    }
    fn offset(&self, piece: &str) -> usize {
        let offset = piece.as_ptr() as usize - self.own.as_ptr() as usize;
        debug_assert!(offset + piece.len() <= self.own.len());
        offset
    }
    fn push(&mut self, segment: &'a str) {
        if self.count == 0 {
            let at = self.offset(segment);
            (self.start, self.end, self.copied) = (at, at + segment.len(), false);
            self.copy.truncate(self.base);
        } else if self.copied {
            self.reserve(1 + segment.len());
            self.copy.push('/');
            self.copy.push_str(segment);
        } else if self.own.as_bytes().get(self.end) == Some(&b'/')
            && (segment.is_empty() || self.offset(segment) == self.end + 1)
        {
            self.end += 1 + segment.len();
        } else {
            self.reserve(self.end - self.start + 1 + segment.len());
            self.copy.push_str(&self.own[self.start..self.end]);
            self.copy.push('/');
            self.copy.push_str(segment);
            self.copied = true;
        }
        self.count += 1;
        self.last = Some(segment);
    }
    /// The first segment of a path without a leading `/`, written to an empty
    /// output.
    fn first(&mut self, segment: &'a str) {
        debug_assert!(matches!(self.x, Pos::Node { node: SLASH, up: 0 }) && self.count == 0);
        self.x = Pos::Root;
        self.push(segment);
    }
    // `out.truncate(out.rfind('/').unwrap_or(0))`, which stops at the origin.
    fn pop(&mut self) {
        self.last = None;
        match self.count {
            0 => match self.x {
                Pos::Node { node, up } if up < self.limit => {
                    self.x = Pos::Node { node, up: up + 1 };
                }
                Pos::Node { .. } if self.rootless => {
                    // The path is empty: what follows begins with `/`.
                    (self.x, self.limit, self.rootless) =
                        (Pos::Node { node: SLASH, up: 0 }, 0, false);
                }
                Pos::Node { .. } | Pos::Auth(_) => {}
                Pos::Root => unreachable!("the root holds own segments"),
            },
            1 => {
                self.count = 0;
                if matches!(self.x, Pos::Root) {
                    self.x = Pos::Node { node: SLASH, up: 0 };
                }
            }
            _ => {
                self.count -= 1;
                if self.copied {
                    let at = self.copy[self.base..].rfind('/').expect("two segments");
                    self.copy.truncate(self.base + at);
                } else {
                    let run = &self.own[self.start..self.end];
                    self.end = self.start + run.rfind('/').expect("two segments");
                }
            }
        }
    }
    /// The length of the own segments' text.
    fn run_len(&self) -> usize {
        if self.copied {
            self.copy.len() - self.base
        } else {
            self.end - self.start
        }
    }
}

impl<'a> Identifiers<'a> {
    /// Identifiers whose texts total at most `budget` bytes, such as the
    /// length of the document that holds them.
    pub fn new(budget: usize) -> Self {
        let mut ids = Self {
            nodes: vec![Node {
                parent: NONE,
                top: ROOT,
                depth: 0,
                key: 0,
                label: Label::Borrowed(""),
            }],
            copied: String::new(),
            copied_from: 0,
            retired: Vec::new(),
            budget,
            children: HashMap::new(),
            collisions: Vec::new(),
            hasher: RandomState::new(),
            schemes: Vec::new(),
            scheme_ids: HashMap::new(),
            pieces: Vec::new(),
            leaves: Vec::new(),
            invalid_authorities: HashSet::new(),
        };
        assert_eq!(ids.add_node(ROOT, Label::Borrowed("")), SLASH);
        assert_eq!(ids.add_node(SLASH, Label::Borrowed("")), AUTH);
        ids
    }
    fn text(&self, label: Label<'a>) -> &str {
        match label {
            Label::Borrowed(text) => text,
            Label::Copied { start, end } => match start.checked_sub(self.copied_from) {
                Some(at) => &self.copied[at..end - self.copied_from],
                None => {
                    let block = self.retired.partition_point(|&(from, _)| from <= start) - 1;
                    let (from, text) = &self.retired[block];
                    &text[start - from..end - from]
                }
            },
        }
    }
    fn label(&self, node: u32) -> &str {
        self.text(self.nodes[node as usize].label)
    }
    fn segments(&self, node: u32) -> usize {
        let at = &self.nodes[node as usize];
        at.depth - self.nodes[at.parent as usize].depth
    }
    fn scheme(&mut self, scheme: &'a str) -> u32 {
        *self.scheme_ids.entry(scheme).or_insert_with(|| {
            self.schemes.push(scheme);
            u32::try_from(self.schemes.len() - 1).expect("schemes are bounded by the node count")
        })
    }
    fn piece(&mut self, piece: Option<&'a str>) -> u32 {
        piece.map_or(NONE, |piece| {
            self.pieces.push(piece);
            u32::try_from(self.pieces.len() - 1).expect("pieces are bounded by the node count")
        })
    }
    fn piece_text(&self, piece: u32) -> Option<&'a str> {
        (piece != NONE).then(|| self.pieces[piece as usize])
    }
    fn candidates(&self, key: u64) -> &[u32] {
        match self.children.get(&key) {
            None => &[],
            Some(Bucket::One(child)) => std::slice::from_ref(child),
            Some(&Bucket::Many(list)) => &self.collisions[list as usize],
        }
    }
    fn key(&self, parent: u32, segment: &str) -> u64 {
        self.hasher.hash_one((parent, segment))
    }
    /// The child of `parent` whose label begins with the segment `segment`.
    fn child(&self, parent: u32, segment: &str) -> Option<u32> {
        let key = self.key(parent, segment);
        self.candidates(key).iter().copied().find(|&child| {
            let label = self.label(child).as_bytes();
            self.nodes[child as usize].parent == parent
                && label.starts_with(segment.as_bytes())
                && label.get(segment.len()).is_none_or(|&c| c == b'/')
        })
    }
    fn insert_child(&mut self, key: u64, child: u32) {
        let bucket = match self.children.get(&key) {
            None => Bucket::One(child),
            Some(&Bucket::One(first)) => {
                let list = u32::try_from(self.collisions.len())
                    .expect("collisions are bounded by the node count");
                self.collisions.push(vec![first, child]);
                Bucket::Many(list)
            }
            Some(&Bucket::Many(list)) => {
                self.collisions[list as usize].push(child);
                return;
            }
        };
        self.children.insert(key, bucket);
    }
    /// A new child of `parent` labelled `label`.
    fn add_node(&mut self, parent: u32, label: Label<'a>) -> u32 {
        let id =
            u32::try_from(self.nodes.len()).expect("directories are bounded by the node count");
        let text = self.text(label);
        let key = self.key(parent, &text[..text.find('/').unwrap_or(text.len())]);
        let segments = 1 + text.bytes().filter(|&c| c == b'/').count();
        let above = self.nodes[parent as usize];
        let top = match parent {
            AUTH => id,
            ROOT if id == SLASH => SLASH,
            _ => above.top,
        };
        let depth = above.depth + segments;
        self.nodes.push(Node {
            parent,
            top,
            depth,
            key,
            label,
        });
        self.insert_child(key, id);
        id
    }
    /// Split `node`'s label at the `/` at byte `at`, which ends its first
    /// `keep` segments: a new directory takes the part before, and is returned.
    fn split(&mut self, node: u32, at: usize, keep: usize) -> u32 {
        let id =
            u32::try_from(self.nodes.len()).expect("directories are bounded by the node count");
        let old = self.nodes[node as usize];
        debug_assert!(old.parent != AUTH, "an authority's edge is one segment");
        let above = self.nodes[old.parent as usize];
        let length = self.text(old.label).len();
        self.nodes.push(Node {
            parent: old.parent,
            top: if old.parent == ROOT { ROOT } else { above.top },
            depth: above.depth + keep,
            key: old.key,
            label: old.label.sub(0, at),
        });
        // The upper part keeps the parent and first segment, so it takes
        // `node`'s place.
        match self.children.get_mut(&old.key) {
            Some(Bucket::One(child)) => *child = id,
            Some(&mut Bucket::Many(list)) => {
                let list = &mut self.collisions[list as usize];
                let at = list
                    .iter()
                    .position(|&c| c == node)
                    .expect("a listed child");
                list[at] = id;
            }
            None => unreachable!("a child is listed"),
        }
        let label = old.label.sub(at + 1, length);
        let text = self.text(label);
        let key = self.key(id, &text[..text.find('/').unwrap_or(text.len())]);
        let lower = &mut self.nodes[node as usize];
        (lower.parent, lower.key, lower.label) = (id, key, label);
        self.insert_child(key, node);
        id
    }
    /// The directory `node`'s string is without its last `up` segments.
    fn up(&mut self, mut node: u32, mut up: usize) -> u32 {
        while up > 0 {
            let segments = self.segments(node);
            if up >= segments {
                up -= segments;
                node = self.nodes[node as usize].parent;
            } else {
                let keep = segments - up;
                let at = boundary(self.label(node).as_bytes(), keep, up);
                return self.split(node, at, keep);
            }
        }
        node
    }
    /// The directory `path`, whole segments the walk over `own` wrote, ends in
    /// below `at`, adding it where the trie lacks it. The trie keeps an
    /// authority's edge to that one segment.
    fn descend(&mut self, mut at: u32, path: Label<'a>, own: &'a str) -> u32 {
        let end = self.text(path).len();
        let mut start = 0;
        loop {
            let run = &self.text(path)[start..];
            let first = run.find('/').unwrap_or(run.len());
            let Some(child) = self.child(at, &run[..first]) else {
                if at == AUTH {
                    let authority = self.add_authority(path.sub(start, start + first), own);
                    if start + first == end {
                        return authority;
                    }
                    at = authority;
                    start += first + 1;
                    continue;
                }
                return self.add_node(at, path.sub(start, end));
            };
            let label = self.label(child).as_bytes();
            let run = run.as_bytes();
            let common = label
                .iter()
                .zip(run)
                .position(|(a, b)| a != b)
                .unwrap_or(label.len().min(run.len()));
            if common == label.len() && run.get(common).is_none_or(|&c| c == b'/') {
                at = child;
                if common == run.len() {
                    return at;
                }
                start += common + 1;
            } else if common == run.len() && label[common] == b'/' {
                // The run ends inside the label, at a segment's end.
                let keep = 1 + label[..common].iter().filter(|&&c| c == b'/').count();
                return self.split(child, common, keep);
            } else {
                // They part inside a segment, after the first: split after the
                // last segment they share.
                let shared = label[..common]
                    .iter()
                    .rposition(|&c| c == b'/')
                    .expect("the first segment matched");
                let keep = 1 + label[..shared].iter().filter(|&&c| c == b'/').count();
                let parent = self.split(child, shared, keep);
                return self.add_node(parent, path.sub(start + shared + 1, end));
            }
        }
    }
    /// The directory of `AUTH` and the authority `text`, a span of `own`.
    fn authority(&mut self, own: &'a str, text: &'a str) -> u32 {
        match self.child(AUTH, text) {
            Some(child) => child,
            None => self.add_authority(Label::Borrowed(text), own),
        }
    }
    /// A new directory of `AUTH` and the authority `label`, which a walk over
    /// `own` wrote. Read back from an identifier's string, it must parse as an
    /// authority; whether it does is found now, from `own` when that holds it
    /// after `//`.
    fn add_authority(&mut self, label: Label<'a>, own: &'a str) -> u32 {
        let node = self.add_node(AUTH, label);
        let text = self.text(label);
        let parses = |spelled: &str| {
            UriRef::parse(spelled).is_ok_and(|r| {
                r.authority().is_some_and(|a| a.as_str() == text) && r.path().is_empty()
            })
        };
        let start = (text.as_ptr() as usize).wrapping_sub(own.as_ptr() as usize);
        let valid = match own.get(start.wrapping_sub(2)..start.wrapping_add(text.len())) {
            Some(spelled) if spelled.starts_with("//") => parses(spelled),
            _ => parses(&format!("//{text}")),
        };
        if !valid {
            self.invalid_authorities.insert(node);
        }
        node
    }
    /// The directory and last segment of a finished walk.
    fn place(&mut self, walk: Walk<'a>) -> (u32, &'a str) {
        let run = walk.run_len();
        let Walk {
            own,
            x,
            authority,
            count,
            start,
            mut copy,
            base,
            copied,
            from,
            retired,
            last,
            ..
        } = walk;
        if let Some(block) = retired {
            if !block.is_empty() {
                self.retired
                    .push((self.copied_from, block.into_boxed_str()));
            }
            self.copied_from = from;
        }
        if !copied || count < 2 {
            copy.truncate(base);
        }
        self.copied = copy;
        if count == 0 {
            return match x {
                Pos::Node { node: SLASH, up: 0 } => (ROOT, ""),
                Pos::Auth(_) => (AUTH, authority),
                // Every walk from a base's directory ends writing a segment.
                _ => unreachable!("an output without own segments is an origin"),
            };
        }
        let last = last.expect("the walk ended writing its last segment");
        let at = match x {
            Pos::Node { node, up } => self.up(node, up),
            Pos::Root => ROOT,
            Pos::Auth(node) => node,
        };
        if count == 1 {
            return (at, last);
        }
        // The own segments before the last name directories below `at`.
        let length = run - last.len() - 1;
        // The walk's copy, by its offset in all the copied text.
        let begin = from + base;
        let path = if copied {
            self.copied.truncate(base + length);
            Label::Copied {
                start: begin,
                end: begin + length,
            }
        } else {
            Label::Borrowed(&own[start..start + length])
        };
        let nodes = self.nodes.len();
        let dir = self.descend(at, path, own);
        if copied {
            // Keep only the copied text new labels use.
            let used = self.nodes[nodes..]
                .iter()
                .filter_map(|node| match node.label {
                    Label::Copied { start, .. } if start >= begin => Some(start),
                    _ => None,
                })
                .min();
            match used {
                None => self.copied.truncate(base),
                Some(used) => {
                    let unused = used - begin;
                    self.copied.replace_range(base..base + unused, "");
                    for node in &mut self.nodes[nodes..] {
                        if let Label::Copied { start, end } = &mut node.label
                            && *start >= begin
                        {
                            (*start, *end) = (*start - unused, *end - unused);
                        }
                    }
                }
            }
        }
        (dir, last)
    }
    // RFC 3986 §5.2.4 dot-segment removal of `input` onto `walk`, exactly as
    // [`crate::uri`]'s does. Past the last `/.` only segments follow, which are
    // written as one span.
    fn remove_dots(walk: &mut Walk<'a>, mut input: &'a str) {
        let plain = walk.offset(input) + input.rfind("/.").map_or(0, |at| at + 1);
        while !input.is_empty() {
            if let Some(rest) = input.strip_prefix("../") {
                input = rest;
            } else if let Some(rest) = input.strip_prefix("./") {
                input = rest;
            } else if input.starts_with("/./") {
                input = &input[2..];
            } else if input == "/." {
                input = &input[..1];
            } else if input.starts_with("/../") || input == "/.." {
                input = if input == "/.." {
                    &input[..1]
                } else {
                    &input[3..]
                };
                walk.pop();
            } else if input == "." || input == ".." {
                input = "";
            } else if input.starts_with('/') {
                if walk.offset(input) >= plain && Self::push_all(walk, input) {
                    return;
                }
                let rest = &input[1..];
                let end = rest.find('/').unwrap_or(rest.len());
                walk.push(&rest[..end]);
                input = &rest[end..];
            } else {
                // Only the first piece of an output that is still empty lacks
                // a leading slash, and only without an authority.
                let end = input[1..].find('/').map_or(input.len(), |i| i + 1);
                walk.first(&input[..end]);
                input = &input[end..];
            }
        }
    }
    /// Write every segment of `input`, `/` and segments holding no dot
    /// segment, at once when they continue the walk's span.
    fn push_all(walk: &mut Walk<'a>, input: &'a str) -> bool {
        let last = &input[input.rfind('/').expect("a leading slash") + 1..];
        let at = walk.offset(input);
        let segments = input.bytes().filter(|&c| c == b'/').count();
        if walk.count == 0 {
            (walk.start, walk.end, walk.copied) = (at + 1, at + input.len(), false);
        } else if !walk.copied && walk.end == at {
            walk.end = at + input.len();
        } else if walk.copied {
            walk.reserve(input.len());
            walk.copy.push_str(input);
        } else {
            return false;
        }
        walk.count += segments;
        walk.last = Some(last);
        true
    }
    /// Dot-segment removal of `/` and then `relative`, which has no leading
    /// slash, onto `walk`.
    fn remove_dots_after_slash(walk: &mut Walk<'a>, relative: &'a str) {
        let end = &relative[relative.len()..];
        if let Some(rest) = relative.strip_prefix("..")
            && (rest.is_empty() || rest.starts_with('/'))
        {
            // "/../": pop, then the rest; "/..": pop, then "/".
            walk.pop();
            if rest.is_empty() {
                walk.push(end);
            } else {
                Self::remove_dots(walk, rest);
            }
            return;
        }
        if let Some(rest) = relative.strip_prefix('.') {
            if rest.is_empty() {
                walk.push(end);
                return;
            }
            if rest.starts_with('/') {
                Self::remove_dots(walk, rest);
                return;
            }
        }
        let first = relative.find('/').unwrap_or(relative.len());
        walk.push(&relative[..first]);
        Self::remove_dots(walk, &relative[first..]);
    }
    fn with_parts(&mut self, scheme: u32, own: &'a str, reference: &UriRef<&'a str>) -> Leaf<'a> {
        let (x, authority) = match reference.authority() {
            Some(authority) => {
                let text = authority.as_str();
                (Pos::Auth(self.authority(own, text)), text)
            }
            None => (Pos::Node { node: SLASH, up: 0 }, ""),
        };
        let copy = std::mem::take(&mut self.copied);
        let mut walk = Walk::new(own, x, authority, copy, self.copied_from, self.budget);
        Self::remove_dots(&mut walk, reference.path().as_str());
        let (dir, last) = self.place(walk);
        Leaf {
            scheme,
            dir,
            last,
            query: self.piece(reference.query().map(|q| q.as_str())),
            fragment: self.piece(fragment(reference)),
            authority: match x {
                Pos::Auth(node) if dir == AUTH => node,
                _ if dir == AUTH => self.authority(own, last),
                _ => NONE,
            },
        }
    }
    /// The authority a base read back from its string has: its directory.
    fn base_authority(&self, base: usize) -> Option<u32> {
        let leaf = self.leaves[base];
        if leaf.dir == AUTH {
            return Some(leaf.authority);
        }
        let top = self.nodes[leaf.dir as usize].top;
        (top != ROOT && top != SLASH).then_some(top)
    }
    fn authority_parses(&self, authority: u32) -> bool {
        !self.invalid_authorities.contains(&authority)
    }
    /// The compared identifier of an `$id` written `identifier`, resolved
    /// against `parent`, the compared identifier of the nearest enclosing
    /// schema that declares one. `None` when [`crate::uri::compared_id`] gives
    /// none: an identifier that is no URI-reference, or a relative one without
    /// a usable base. Each compared declaration's handle is the next.
    pub fn compared(&mut self, parent: Option<u32>, identifier: &'a str) -> Option<u32> {
        let reference = UriRef::parse(identifier).ok()?;
        let leaf = if let Some(scheme) = reference.scheme() {
            let scheme = self.scheme(scheme.as_str());
            self.with_parts(scheme, identifier, &reference)
        } else {
            let parent = parent? as usize;
            let base = self.leaves[parent];
            // The base is read back from its string; an authority found then
            // must parse as one.
            let base_authority = self.base_authority(parent);
            if base_authority.is_some_and(|authority| !self.authority_parses(authority)) {
                return None;
            }
            let path = reference.path().as_str();
            if reference.authority().is_some() {
                self.with_parts(base.scheme, identifier, &reference)
            } else if path.is_empty() {
                Leaf {
                    query: match reference.query() {
                        Some(query) => self.piece(Some(query.as_str())),
                        None => base.query,
                    },
                    fragment: self.piece(fragment(&reference)),
                    ..self.leaves[parent]
                }
            } else {
                let origin = match base_authority {
                    Some(authority) => Pos::Auth(authority),
                    None => Pos::Node { node: SLASH, up: 0 },
                };
                let copy = std::mem::take(&mut self.copied);
                let mut walk =
                    Walk::new(identifier, origin, "", copy, self.copied_from, self.budget);
                if path.starts_with('/') {
                    Self::remove_dots(&mut walk, path);
                } else if base_authority.is_some() && base.dir == AUTH {
                    // An authority and an empty path: the prefix is "/".
                    Self::remove_dots_after_slash(&mut walk, path);
                } else if base.dir != ROOT {
                    // The base path up to its last slash, then the path; dot
                    // segments may give up the base's segments down to its
                    // floor.
                    let dir = &self.nodes[base.dir as usize];
                    let (floor, rootless) = match dir.top {
                        ROOT => (1, true),
                        SLASH => (1, false),
                        authority => (self.nodes[authority as usize].depth, false),
                    };
                    walk.x = Pos::Node {
                        node: base.dir,
                        up: 0,
                    };
                    (walk.limit, walk.rootless) = (dir.depth - floor, rootless);
                    Self::remove_dots_after_slash(&mut walk, path);
                } else {
                    // No slash in the base path: the path alone.
                    Self::remove_dots(&mut walk, path);
                }
                let (dir, last) = self.place(walk);
                Leaf {
                    scheme: base.scheme,
                    dir,
                    last,
                    query: self.piece(reference.query().map(|q| q.as_str())),
                    fragment: self.piece(fragment(&reference)),
                    authority: if dir == AUTH {
                        self.authority(identifier, last)
                    } else {
                        NONE
                    },
                }
            }
        };
        self.budget = self.budget.saturating_sub(identifier.len());
        self.leaves.push(leaf);
        Some(u32::try_from(self.leaves.len() - 1).expect("identifiers are bounded by the source"))
    }
    /// Order handles so that equal identifiers neighbour: by scheme and
    /// directory, then by the texts of the last segment, query and fragment.
    /// Texts one identifier took from another are the same text and compare
    /// at once.
    fn grouping(&self, a: u32, b: u32) -> Ordering {
        let (x, y) = (&self.leaves[a as usize], &self.leaves[b as usize]);
        let piece = |a: u32, b: u32| match (a == b, self.piece_text(a), self.piece_text(b)) {
            (true, _, _) => Ordering::Equal,
            (false, a, b) => a.cmp(&b),
        };
        (x.scheme, x.dir)
            .cmp(&(y.scheme, y.dir))
            .then_with(|| text_order(x.last, y.last))
            .then_with(|| piece(x.query, y.query))
            .then_with(|| piece(x.fragment, y.fragment))
    }
    /// The handles whose identifier string another handle's is too, each
    /// with its string's rank among those strings: by string, then handle.
    pub fn duplicates(&self) -> Vec<(u32, u32)> {
        let mut by_leaf: Vec<u32> = (0..self.leaves.len() as u32).collect();
        by_leaf.sort_unstable_by(|&a, &b| self.grouping(a, b).then(a.cmp(&b)));
        let runs: Vec<&[u32]> = by_leaf
            .chunk_by(|&a, &b| self.grouping(a, b) == Ordering::Equal)
            .filter(|run| run.len() > 1)
            .collect();
        let order = self.order(runs.iter().map(|run| run[0]).collect());
        let mut run_of: Vec<(u32, usize)> = runs
            .iter()
            .enumerate()
            .map(|(at, run)| (run[0], at))
            .collect();
        run_of.sort_unstable();
        let mut out = Vec::new();
        for (rank, first) in order.into_iter().enumerate() {
            let at = run_of[run_of.partition_point(|&(handle, _)| handle < first)].1;
            out.extend(runs[at].iter().map(|&handle| (rank as u32, handle)));
        }
        out
    }
    /// Handles of distinct identifier strings, sorted as the strings compare.
    fn order(&self, handles: Vec<u32>) -> Vec<u32> {
        if handles.len() < 2 {
            return handles;
        }
        // Identifiers ending in one directory with one last segment and one
        // character after it form a group, ordered within by what follows.
        let group_of = |handle: u32| {
            let leaf = &self.leaves[handle as usize];
            let after = if leaf.query != NONE {
                After::Query
            } else if leaf.fragment != NONE {
                After::Fragment
            } else {
                After::End
            };
            (leaf.dir, leaf.last, after)
        };
        let group_order = |a: &(u32, &str, After), b: &(u32, &str, After)| {
            a.0.cmp(&b.0)
                .then_with(|| text_order(a.1, b.1))
                .then(a.2.cmp(&b.2))
        };
        let mut groups: Vec<(u32, &str, After)> = handles.iter().map(|&h| group_of(h)).collect();
        groups.sort_unstable_by(group_order);
        groups.dedup_by(|a, b| group_order(a, b) == Ordering::Equal);
        // The directories on the way to the groups' directories, each once.
        let mut seen = HashSet::new();
        let mut dirs = Vec::new();
        for &(dir, _, _) in &groups {
            let mut dir = dir;
            while dir != ROOT && seen.insert(dir) {
                dirs.push(dir);
                dir = self.nodes[dir as usize].parent;
            }
        }
        drop(seen);
        // A directory's items, each keyed by the text that begins it: child
        // directories by first segment and '/', groups by last segment and
        // the character after it. Neither key's continuation can reorder them:
        // a segment holds no '/', '?' or '#', and children's first segments
        // differ.
        #[derive(Clone, Copy)]
        enum Item {
            Dir(u32),
            Group(u32),
        }
        let mut entries: Vec<(u32, Item, &str, After)> = dirs
            .iter()
            .map(|&dir| {
                let label = self.label(dir);
                let first = &label[..label.find('/').unwrap_or(label.len())];
                (
                    self.nodes[dir as usize].parent,
                    Item::Dir(dir),
                    first,
                    After::Slash,
                )
            })
            .chain(
                groups
                    .iter()
                    .enumerate()
                    .map(|(g, &(dir, last, after))| (dir, Item::Group(g as u32), last, after)),
            )
            .collect();
        drop(dirs);
        entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| keyed((a.2, a.3), (b.2, b.3))));
        let items = |dir: u32| {
            let start = entries.partition_point(|entry| entry.0 < dir);
            (
                start,
                start + entries[start..].partition_point(|entry| entry.0 == dir),
            )
        };
        // Depth first over directories, in key order, numbering the groups.
        let mut position = vec![0u32; groups.len()];
        let mut next = 0;
        let mut stack = vec![items(ROOT)];
        while let Some((at, end)) = stack.pop() {
            if at == end {
                continue;
            }
            stack.push((at + 1, end));
            match entries[at].1 {
                Item::Dir(child) => stack.push(items(child)),
                Item::Group(g) => {
                    position[g as usize] = next;
                    next += 1;
                }
            }
        }
        let position_of = |handle: u32| {
            position[groups
                .binary_search_by(|group| group_order(group, &group_of(handle)))
                .expect("a listed group")]
        };
        // Schemes decide first, as each scheme and its ':' does.
        let mut schemes: Vec<u32> = handles
            .iter()
            .map(|&h| self.leaves[h as usize].scheme)
            .collect();
        schemes.sort_unstable();
        schemes.dedup();
        schemes.sort_by(|&a, &b| {
            keyed(
                (self.schemes[a as usize], After::Colon),
                (self.schemes[b as usize], After::Colon),
            )
        });
        let scheme_rank: HashMap<u32, u32> = schemes
            .iter()
            .enumerate()
            .map(|(rank, &scheme)| (scheme, rank as u32))
            .collect();
        let mut order = handles;
        order.sort_by(|&a, &b| {
            let (x, y) = (&self.leaves[a as usize], &self.leaves[b as usize]);
            scheme_rank[&x.scheme]
                .cmp(&scheme_rank[&y.scheme])
                .then_with(|| position_of(a).cmp(&position_of(b)))
                .then_with(|| self.after_key(x, y))
        });
        order
    }
    // Two identifiers of one group: equal up to the character after the last
    // segment, so their query, then fragment, decide. Schemes are not
    // compared here.
    fn after_key(&self, a: &Leaf<'a>, b: &Leaf<'a>) -> Ordering {
        let fragments = || {
            self.piece_text(a.fragment)
                .cmp(&self.piece_text(b.fragment))
        };
        let (Some(qa), Some(qb)) = (self.piece_text(a.query), self.piece_text(b.query)) else {
            // Both "#fragment", or both nothing: a fragment holds no '#'.
            return fragments();
        };
        let ends = |leaf: &Leaf<'a>| {
            if leaf.fragment == NONE {
                After::End
            } else {
                After::Fragment
            }
        };
        match keyed((qa, ends(a)), (qb, ends(b))) {
            Ordering::Equal => fragments(),
            order => order,
        }
    }
    /// The identifier's string, as [`crate::uri::compared_id`] spells it.
    #[cfg(test)]
    pub fn spelled(&self, handle: u32) -> String {
        let leaf = self.leaves[handle as usize];
        let mut dirs = Vec::new();
        let mut dir = leaf.dir;
        while dir != ROOT {
            dirs.push(dir);
            dir = self.nodes[dir as usize].parent;
        }
        let mut out = format!("{}:", self.schemes[leaf.scheme as usize]);
        for &dir in dirs.iter().rev() {
            out.push_str(self.label(dir));
            out.push('/');
        }
        out.push_str(leaf.last);
        if let Some(query) = self.piece_text(leaf.query) {
            out.push('?');
            out.push_str(query);
        }
        if let Some(fragment) = self.piece_text(leaf.fragment) {
            out.push('#');
            out.push_str(fragment);
        }
        out
    }
}
/// The byte index of the `/` that ends the first `keep` segments of `label`,
/// before its last `drop`. Scanning from both ends at once costs at most twice
/// the shorter side, which the split that follows makes a label of its own:
/// over all splits, each byte is scanned a logarithmic number of times.
fn boundary(label: &[u8], keep: usize, drop: usize) -> usize {
    let (mut front, mut found) = (0, 0);
    let (mut back, mut lost) = (label.len(), 0);
    let mut chunk = 64;
    loop {
        let end = (front + chunk).min(label.len());
        for (at, &c) in label[front..end].iter().enumerate() {
            if c == b'/' {
                found += 1;
                if found == keep {
                    return front + at;
                }
            }
        }
        front = end;
        let start = back.saturating_sub(chunk);
        for at in (start..back).rev() {
            if label[at] == b'/' {
                lost += 1;
                if lost == drop {
                    return at;
                }
            }
        }
        back = start;
        chunk *= 2;
    }
}
/// Texts in byte order; one text compares equal to itself at once.
fn text_order(a: &str, b: &str) -> Ordering {
    if std::ptr::eq(a, b) {
        Ordering::Equal
    } else {
        a.cmp(b)
    }
}
// The compared form drops an empty fragment.
fn fragment<'a>(reference: &UriRef<&'a str>) -> Option<&'a str> {
    reference
        .fragment()
        .map(|f| f.as_str())
        .filter(|f| !f.is_empty())
}
/// The character a key's continuation begins with, ordered as the
/// characters compare; `End` when nothing follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum After {
    End,
    Fragment,
    Slash,
    Colon,
    Query,
}
impl After {
    fn byte(self) -> Option<u8> {
        match self {
            Self::End => None,
            Self::Fragment => Some(b'#'),
            Self::Slash => Some(b'/'),
            Self::Colon => Some(b':'),
            Self::Query => Some(b'?'),
        }
    }
}
// Compare `text` followed by its continuation's first character. The
// continuation character never occurs in the other key's text where it is
// compared, so the comparison is decided here.
fn keyed((a, a_after): (&str, After), (b, b_after): (&str, After)) -> Ordering {
    let common = a.len().min(b.len());
    match a.as_bytes()[..common].cmp(&b.as_bytes()[..common]) {
        Ordering::Equal => {}
        order => return order,
    }
    let next = |text: &str, after: After| text.as_bytes().get(common).copied().or(after.byte());
    match (next(a, a_after), next(b, b_after)) {
        (Some(x), Some(y)) if a.len() != b.len() => x.cmp(&y),
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        _ => a_after.cmp(&b_after),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uri::compared_id;

    const IDS: &[&str] = &[
        "https://e.test/a/b",
        "https://e.test",
        "https://e.test/",
        "https://e.test/a/../b",
        "https://e.test/./a/",
        "https://e.test/a#f",
        "https://e.test/a#",
        "https://e.test/a?q#f",
        "https://e.test/a?",
        "https://[::1]:8080/p",
        "https://e.test/a!/b",
        "https://e.test/a/",
        "s:a:b",
        "s:/.//x",
        "s:/.//a:b/c",
        "s:/.//a@b@c/d",
        "s://x/y",
        "s:",
        "s:?q",
        "s1:a",
        "s+:a",
        "S:/a",
        "urn:x:y",
        "a:b/c",
        "",
        "#",
        "#f",
        "#g",
        "?q",
        "?q#f",
        "?q/r",
        "?q!",
        "?",
        "a",
        "a!",
        "a/",
        "b/c",
        "./",
        "../",
        "..",
        ".",
        "../..",
        "../../x",
        "/x",
        "/x/y/",
        "//h",
        "//h/p",
        "//h:1/p/./q",
        "//a@b",
        "//a:b",
        "//",
        "///x",
        ".//x",
        ".//y/z",
        "x/../y",
        "..a",
        ".a/",
        "a/./b/../c",
        "%41",
        "?a/b#c/d",
        "/./",
        "/../",
        "...",
        "a?q",
        "a#f",
        "a#",
        ":",
        "a b",
        "//x/../y",
        "../a/../..",
        "./../",
        "a/..",
        "a/.",
        "?q#",
        "#?",
        "/..",
        "/.",
        "//[::1]/p",
        "//h/",
        "s:a/../b",
        "s:./a",
        "s:../a",
        "s:a/./b",
        "s:.",
        "s:..",
        "s:/..",
        "s://",
        "s:///",
        "s:////x",
        "s:/.//",
        "s:/.///",
        "s:/.//:1/x",
        "s:/.//1:/x",
        "s:/.//@/x",
        "x:",
        "x:y?",
        ".//",
        "..//",
        "./a:b",
    ];

    // A tree of nested `$id` declarations: each entry's parent precedes it.
    fn tree(state: &mut u64, size: usize) -> Vec<(Option<usize>, &'static str)> {
        let mut next = || {
            *state ^= *state << 13;
            *state ^= *state >> 7;
            *state ^= *state << 17;
            *state as usize
        };
        let mut out = Vec::new();
        for i in 0..size {
            let parent = match next() % 6 {
                _ if i == 0 => None,
                0 => None,
                1..=3 => Some(i - 1),
                _ => Some(next() % i),
            };
            out.push((parent, IDS[next() % IDS.len()]));
        }
        out
    }

    #[test]
    fn identifiers_match_compared_strings_and_their_order() {
        let mut state = 0x9e37_79b9_u64;
        for _ in 0..6000 {
            let declarations = tree(&mut state, 32);
            // Exactly the identifiers' length, the tightest budget.
            let mut ids = Identifiers::new(declarations.iter().map(|(_, t)| t.len()).sum());
            let mut strings: Vec<Option<String>> = Vec::new();
            let mut handles: Vec<Option<u32>> = Vec::new();
            for &(parent, text) in &declarations {
                let parent_string = parent.and_then(|p| strings[p].clone());
                let parent_handle = parent.and_then(|p| handles[p]);
                let expected = compared_id(parent_string.as_deref(), text);
                let actual = ids.compared(parent_handle, text);
                assert_eq!(
                    actual.map(|h| ids.spelled(h)),
                    expected,
                    "{text} in {parent_string:?}"
                );
                strings.push(expected);
                handles.push(actual);
            }
            // Duplicates: exactly the handles sharing a string, ranked as
            // those strings compare.
            let spelled: Vec<(u32, &String)> = handles
                .iter()
                .zip(&strings)
                .filter_map(|(h, s)| Some(((*h)?, s.as_ref()?)))
                .collect();
            let mut expected: Vec<(&String, u32)> = spelled
                .iter()
                .filter(|(_, s)| spelled.iter().filter(|(_, t)| t == s).count() > 1)
                .map(|&(h, s)| (s, h))
                .collect();
            expected.sort();
            let duplicates = ids.duplicates();
            assert_eq!(
                duplicates.iter().map(|&(_, h)| h).collect::<Vec<_>>(),
                expected.iter().map(|&(_, h)| h).collect::<Vec<_>>()
            );
            for pair in duplicates.windows(2).zip(expected.windows(2)) {
                assert_eq!(pair.0[0].0 == pair.0[1].0, pair.1[0].0 == pair.1[1].0);
            }
            // Order: one handle per distinct string, sorted as the strings.
            let mut distinct: Vec<(u32, &String)> = Vec::new();
            for &(h, s) in &spelled {
                if !distinct.iter().any(|(_, t)| *t == s) {
                    distinct.push((h, s));
                }
            }
            let ordered = ids.order(distinct.iter().map(|&(h, _)| h).collect());
            let mut sorted = distinct.clone();
            sorted.sort_by(|a, b| a.1.cmp(b.1));
            assert_eq!(ordered, sorted.iter().map(|&(h, _)| h).collect::<Vec<_>>());
        }
    }

    // Paths of many segments, with dot segments that give up parts of the
    // runs earlier declarations wrote: the splits a walk makes inside a
    // run of segments.
    fn segment_tree(state: &mut u64, size: usize) -> Vec<(Option<usize>, String)> {
        const SEGMENTS: &[&str] = &["a", "b", "ab", "", ".", "..", "a.", "%2e"];
        const STARTS: &[&str] = &["https://e.test/", "s:/", "s:", "s://h", "", "/", "//h/"];
        let mut next = || {
            *state ^= *state << 13;
            *state ^= *state >> 7;
            *state ^= *state << 17;
            *state as usize
        };
        let mut out = Vec::new();
        for i in 0..size {
            let parent = match next() % 4 {
                _ if i == 0 => None,
                0 => None,
                1 | 2 => Some(i - 1),
                _ => Some(next() % i),
            };
            let mut text = String::from(STARTS[next() % STARTS.len()]);
            for at in 0..next() % 12 {
                if at > 0 || !text.is_empty() && !text.ends_with('/') {
                    text.push('/');
                }
                text.push_str(SEGMENTS[next() % SEGMENTS.len()]);
            }
            match next() % 8 {
                0 => text.push_str("?q"),
                1 => text.push_str("#f"),
                _ => {}
            }
            out.push((parent, text));
        }
        out
    }

    #[test]
    fn long_paths_and_dot_segments_match_compared_strings_and_their_order() {
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        for _ in 0..4000 {
            let declarations = segment_tree(&mut state, 24);
            let mut ids = Identifiers::new(declarations.iter().map(|(_, t)| t.len()).sum());
            let mut strings: Vec<Option<String>> = Vec::new();
            let mut handles: Vec<Option<u32>> = Vec::new();
            for (parent, text) in &declarations {
                let parent_string = parent.and_then(|p| strings[p].clone());
                let parent_handle = parent.and_then(|p| handles[p]);
                let expected = compared_id(parent_string.as_deref(), text);
                let actual = ids.compared(parent_handle, text);
                assert_eq!(
                    actual.map(|h| ids.spelled(h)),
                    expected,
                    "{text} in {parent_string:?}"
                );
                strings.push(expected);
                handles.push(actual);
            }
            let spelled: Vec<(u32, &String)> = handles
                .iter()
                .zip(&strings)
                .filter_map(|(h, s)| Some(((*h)?, s.as_ref()?)))
                .collect();
            let mut expected: Vec<(&String, u32)> = spelled
                .iter()
                .filter(|(_, s)| spelled.iter().filter(|(_, t)| t == s).count() > 1)
                .map(|&(h, s)| (s, h))
                .collect();
            expected.sort();
            let duplicates = ids.duplicates();
            assert_eq!(
                duplicates.iter().map(|&(_, h)| h).collect::<Vec<_>>(),
                expected.iter().map(|&(_, h)| h).collect::<Vec<_>>()
            );
            for pair in duplicates.windows(2).zip(expected.windows(2)) {
                assert_eq!(pair.0[0].0 == pair.0[1].0, pair.1[0].0 == pair.1[1].0);
            }
        }
    }

    #[test]
    fn nested_relative_identifiers_take_linear_pieces() {
        // Each level adds one directory: the pieces held grow with the
        // declarations, not with the length of the identifiers they spell.
        let levels = 20_000;
        let mut ids = Identifiers::new(usize::MAX);
        let mut parent = ids.compared(None, "https://e.test/");
        for _ in 0..levels {
            parent = ids.compared(parent, "a/");
        }
        let leaf = parent.unwrap();
        assert_eq!(
            ids.spelled(leaf).len(),
            "https://e.test/".len() + 2 * levels
        );
        assert!(ids.nodes.len() <= levels + 8);
        assert!(ids.schemes.len() == 1 && ids.pieces.is_empty());
        assert_eq!(ids.leaves.len(), levels + 1);
        assert!(ids.duplicates().is_empty());
    }

    #[test]
    fn copied_labels_read_back_from_every_block() {
        // A dot segment makes each path a copy, and an authority of its own
        // keeps each copy: the copied text fills block after block, and
        // every identifier still spells out once all are declared.
        let texts: Vec<String> = (0..5000)
            .map(|i| format!("s://{i:x}/q/a/../b/{}/x", "c".repeat(i % 50)))
            .collect();
        let mut ids = Identifiers::new(texts.iter().map(String::len).sum());
        let handles: Vec<u32> = texts
            .iter()
            .map(|text| ids.compared(None, text).unwrap())
            .collect();
        for (text, &handle) in texts.iter().zip(&handles) {
            assert_eq!(Some(ids.spelled(handle)), compared_id(None, text));
        }
        assert!(ids.retired.len() > 1);
        let mut from = 0;
        for (start, block) in &ids.retired {
            assert_eq!(*start, from);
            from += block.len();
        }
        assert_eq!(ids.copied_from, from);
    }

    #[test]
    fn identifiers_hold_runs_of_segments_not_segments() {
        // One identifier of many segments is one directory, and identifiers
        // giving up and replacing some of its segments add a few each,
        // however many segments their paths hold.
        let segments = 100_000;
        let long = format!("https://e.test/{}", "a/".repeat(segments));
        let mut ids = Identifiers::new(usize::MAX);
        let base = ids.compared(None, &long);
        let children: Vec<String> = (1..200)
            .map(|up| format!("{}x{up}/y", "../".repeat(up * 37)))
            .collect();
        let dotted = format!("https://e.test/{}", "a/./".repeat(segments));
        for child in &children {
            let handle = ids.compared(base, child).unwrap();
            let expected = compared_id(Some(&ids.spelled(base.unwrap())), child).unwrap();
            assert_eq!(ids.spelled(handle), expected);
        }
        let again = ids.compared(None, &dotted).unwrap();
        assert_eq!(ids.spelled(again), long);
        assert!(ids.nodes.len() <= 3 + 1 + 3 * children.len() + 3);
        // The run interrupted by dot segments is copied once and not kept,
        // since its directories exist.
        assert!(ids.copied.is_empty() && ids.retired.is_empty());
        let duplicates = ids.duplicates();
        assert_eq!(
            duplicates.iter().map(|&(_, h)| h).collect::<Vec<_>>(),
            [base.unwrap(), again]
        );
    }
}
