//! One URI walk for borrowed validation and owned query storage.
//!
//! `identifier_methods!` specializes field access and span storage at expansion
//! time. Its parameters are the input lifetime and span type, label and leaf
//! types, optional node byte field, and test accounting/spelling configuration.
//! Validation keeps its original concrete fields and containers. Queries supply
//! owned spans and inventory overlays. Neither path dispatches through a vtable.
use fluent_uri::UriRef;
use std::cmp::Ordering;

pub(super) const NONE: u32 = u32::MAX;
pub(super) const ROOT: u32 = 0;
pub(super) const SLASH: u32 = 1;
pub(super) const AUTH: u32 = 2;

/// Where the next own segment hangs: dot-segment removal's output is this
/// position's string, then the own segments a walk has written.
#[derive(Clone, Copy, Debug)]
pub(super) enum Pos {
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
pub(super) struct Walk<'a> {
    pub(super) own: &'a str,
    pub(super) x: Pos,
    /// How many segments `x` may still give up before the output's floor.
    pub(super) limit: usize,
    /// Whether giving up one more empties the path, as for a path without a
    /// leading `/`.
    pub(super) rootless: bool,
    /// For an authority's position, the authority, which an output without
    /// own segments ends in.
    pub(super) authority: &'a str,
    pub(super) count: usize,
    /// The own segments: a span of `own`, or, once removed dot segments
    /// separate them, a copy at the end of the copied labels' current block,
    /// from `base`.
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) copy: String,
    pub(super) base: usize,
    pub(super) copied: bool,
    /// The offset in all the copied text at which `copy` begins.
    pub(super) from: usize,
    /// The block the walk left for want of room, for the shared `place` method
    /// to retire.
    pub(super) retired: Option<String>,
    /// At least the length of this and the identifiers still to come.
    pub(super) budget: usize,
    /// The last own segment written, while it is the output's.
    pub(super) last: Option<&'a str>,
}
impl<'a> Walk<'a> {
    pub(super) fn new(
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
    /// block and the earlier text stays where it is. Routine block growth
    /// doubles up to a 1 MiB target, bounded by the remaining identifier
    /// text budget. Smaller blocks avoid requiring large contiguous heap
    /// holes without moving kept text. A single identifier may exceed the
    /// target; a copy alone in its block grows as a string does.
    pub(super) fn reserve(&mut self, more: usize) {
        if self.base == 0 || self.copy.capacity() - self.copy.len() >= more {
            return;
        }
        debug_assert!(self.retired.is_none(), "a moved copy is alone in its block");
        let written = &self.copy[self.base..];
        const COPIED_BLOCK_TARGET: usize = 1024 * 1024;
        let total = (self.from + self.base)
            .min(self.budget)
            .min(COPIED_BLOCK_TARGET);
        let mut block = String::with_capacity((written.len() + more).max(total));
        block.push_str(written);
        self.copy.truncate(self.base);
        self.retired = Some(std::mem::replace(&mut self.copy, block));
        (self.from, self.base) = (self.from + self.base, 0);
    }
    pub(super) fn offset(&self, piece: &str) -> usize {
        let offset = piece.as_ptr() as usize - self.own.as_ptr() as usize;
        debug_assert!(offset + piece.len() <= self.own.len());
        offset
    }
    pub(super) fn push(&mut self, segment: &'a str) {
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
    pub(super) fn first(&mut self, segment: &'a str) {
        debug_assert!(matches!(self.x, Pos::Node { node: SLASH, up: 0 }) && self.count == 0);
        self.x = Pos::Root;
        self.push(segment);
    }
    // `out.truncate(out.rfind('/').unwrap_or(0))`, which stops at the origin.
    pub(super) fn pop(&mut self) {
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
    pub(super) fn run_len(&self) -> usize {
        if self.copied {
            self.copy.len() - self.base
        } else {
            self.end - self.start
        }
    }
}

/// The byte index of the `/` that ends the first `keep` segments of `label`,
/// before its last `drop`. Scanning from both ends at once costs at most twice
/// the shorter side, which the split that follows makes a label of its own:
/// over all splits, each byte is scanned a logarithmic number of times.
pub(super) fn boundary<const COUNT: bool>(label: &[u8], keep: usize, drop: usize) -> usize {
    let (mut front, mut found) = (0, 0);
    let (mut back, mut lost) = (label.len(), 0);
    let mut chunk = 64;
    loop {
        let end = (front + chunk).min(label.len());
        #[cfg(test)]
        if COUNT {
            crate::resolution_work::text(end - front);
        }
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
        #[cfg(test)]
        if COUNT {
            crate::resolution_work::text(back - start);
        }
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
pub(super) fn text_order<const COUNT: bool>(a: &str, b: &str) -> Ordering {
    if std::ptr::eq(a, b) {
        Ordering::Equal
    } else {
        #[cfg(test)]
        if COUNT {
            crate::resolution_work::text(a.len().min(b.len()));
        }
        a.cmp(b)
    }
}
// The compared form drops an empty fragment.
pub(super) fn fragment<'a>(reference: &UriRef<&'a str>) -> Option<&'a str> {
    reference
        .fragment()
        .map(|f| f.as_str())
        .filter(|f| !f.is_empty())
}

/// Read copied text from its current block or an immutable retired block.
pub(super) fn copied_text<'a>(
    copied: &'a str,
    copied_from: usize,
    retired: &'a [(usize, Box<str>)],
    start: usize,
    end: usize,
) -> &'a str {
    match start.checked_sub(copied_from) {
        Some(at) => &copied[at..end - copied_from],
        None => {
            let block = retired.partition_point(|&(from, _)| from <= start) - 1;
            let (from, text) = &retired[block];
            &text[start - from..end - from]
        }
    }
}

macro_rules! identifier_methods {
    ($q:lifetime, [$($lt:lifetime)?], $input:ty, $label:ty, $leaf:ty,
     [$($bytes:ident)?], $count:literal, [$($spelling:meta)*]) => {
        fn label(&self, node: u32) -> &str {
            self.text(self.nodes[node as usize].label)
        }
        fn segments(&self, node: u32) -> usize {
            let at = &self.nodes[node as usize];
            at.depth - self.nodes[at.parent as usize].depth
        }
        fn segment(&mut self, label: $label) -> u32 {
            #[cfg(test)]
            if $count {
                crate::resolution_work::text(self.text(label).len());
            }
            let text = self.text(label);
            let segment = &text[..text.find('/').unwrap_or(text.len())];
            if let Some(&id) = self.segment_ids.get(segment) {
                return id;
            }
            let segment: Box<str> = segment.into();
            let id =
                u32::try_from(self.segment_ids.len()).expect("segments are bounded by directory nodes");
            self.segment_ids.insert(segment, id);
            id
        }
        /// The child of `parent` whose label begins with the segment `segment`.
        fn child(&self, parent: u32, segment: &str) -> Option<u32> {
            #[cfg(test)]
            if $count {
                crate::resolution_work::text(segment.len());
            }
            let key = self.segment_ids.get(segment)?;
            self.children.get(&(parent, *key)).copied()
        }
        /// A new child of `parent` labelled `label`.
        fn add_node(&mut self, parent: u32, label: $label) -> u32 {
            let id =
                u32::try_from(self.nodes.len()).expect("directories are bounded by the node count");
            let text = self.text(label);
            let segments = 1 + text.bytes().filter(|&c| c == b'/').count();
            let key = self.segment(label);
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
                $($bytes: above.$bytes + self.text(label).len() + 1,)?
                key,
                label,
            });
            self.children.insert((parent, key), id);
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
                $($bytes: above.$bytes + at + 1,)?
                key: old.key,
                label: old.label.sub(0, at),
            });
            // The upper part keeps the parent and first segment, so it takes
            // `node`'s place.
            self.children.insert((old.parent, old.key), id);
            let label = old.label.sub(at + 1, length);
            let key = self.segment(label);
            let lower = &mut self.nodes[node as usize];
            (lower.parent, lower.key, lower.label) = (id, key, label);
            self.children.insert((id, key), node);
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
                    let at = boundary::<$count>(self.label(node).as_bytes(), keep, up);
                    return self.split(node, at, keep);
                }
            }
            node
        }
        /// The directory `path`, whole segments the walk over `own` wrote, ends in
        /// below `at`, adding it where the trie lacks it. The trie keeps an
        /// authority's edge to that one segment.
        fn descend(&mut self, mut at: u32, path: $label, own: &str) -> u32 {
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
                #[cfg(test)]
                if $count {
                    crate::resolution_work::text(self.label(child).len().min(run.len()));
                }
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
        fn authority(&mut self, own: &str, input: $input) -> u32 {
            let label = Self::input_label(input);
            match self.child(AUTH, self.text(label)) {
                Some(child) => child,
                None => self.add_authority(label, own),
            }
        }
        /// A new directory of `AUTH` and the authority `label`, which a walk over
        /// `own` wrote. Read back from an identifier's string, it must parse as an
        /// authority; whether it does is found now, from `own` when that holds it
        /// after `//`.
        fn add_authority(&mut self, label: $label, own: &str) -> u32 {
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
        fn place$(<$lt>)?(&mut self, walk: Walk<$q>, input: $input) -> (u32, $input) {
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
                    Pos::Node { node: SLASH, up: 0 } => (ROOT, Self::empty_input()),
                    Pos::Auth(_) => (AUTH, span(input, own, authority)),
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
                return (at, span(input, own, last));
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
                Self::input_label(span(input, own, &own[start..start + length]))
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
            (dir, span(input, own, last))
        }
        // RFC 3986 §5.2.4 dot-segment removal of `input` onto `walk`, exactly as
        // [`crate::uri`]'s does. Past the last `/.` only segments follow, which are
        // written as one span.
        fn remove_dots<'walk>(walk: &mut Walk<'walk>, mut input: &'walk str) {
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
        fn push_all<'walk>(walk: &mut Walk<'walk>, input: &'walk str) -> bool {
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
        fn remove_dots_after_slash<'walk>(walk: &mut Walk<'walk>, relative: &'walk str) {
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
        fn with_parts$(<$lt>)?(
            &mut self,
            scheme: u32,
            own: &$q str,
            input: $input,
            reference: &UriRef<&$q str>,
        ) -> $leaf {
            let (x, authority) = match reference.authority() {
                Some(authority) => {
                    let text = authority.as_str();
                    (Pos::Auth(self.authority(own, span(input, own, text))), text)
                }
                None => (Pos::Node { node: SLASH, up: 0 }, ""),
            };
            let copy = std::mem::take(&mut self.copied);
            let mut walk = Walk::new(own, x, authority, copy, self.copied_from, self.budget);
            Self::remove_dots(&mut walk, reference.path().as_str());
            let (dir, last) = self.place(walk, input);
            Leaf {
                scheme,
                dir,
                last: self.store_last(last),
                query: self.piece(reference.query().map(|q| span(input, own, q.as_str()))),
                fragment: self.piece(fragment(reference).map(|s| span(input, own, s))),
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
        fn compare$(<$lt>)?(&mut self, parent: Option<u32>, identifier: &$q str, input: $input) -> Option<u32> {
            #[cfg(test)]
            if $count {
                crate::resolution_work::text(identifier.len());
            }
            let own = identifier;
            let reference = UriRef::parse(identifier).ok()?;
            let leaf = if let Some(scheme) = reference.scheme() {
                let scheme = self.scheme(span(input, own, scheme.as_str()));
                self.with_parts(scheme, identifier, input, &reference)
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
                    self.with_parts(base.scheme, identifier, input, &reference)
                } else if path.is_empty() {
                    Leaf {
                        query: match reference.query() {
                            Some(query) => self.piece(Some(span(input, own, query.as_str()))),
                            None => base.query,
                        },
                        fragment: self.piece(fragment(&reference).map(|s| span(input, own, s))),
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
                    let (dir, last) = self.place(walk, input);
                    Leaf {
                        scheme: base.scheme,
                        dir,
                        last: self.store_last(last),
                        query: self.piece(reference.query().map(|q| span(input, own, q.as_str()))),
                        fragment: self.piece(fragment(&reference).map(|s| span(input, own, s))),
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
        pub fn grouping(&self, a: u32, b: u32) -> Ordering {
            let (x, y) = (&self.leaves[a as usize], &self.leaves[b as usize]);
            let piece = |a: u32, b: u32| match (a == b, self.piece_text(a), self.piece_text(b)) {
                (true, _, _) => Ordering::Equal,
                (false, Some(a), Some(b)) if $count => text_order::<$count>(a, b),
                (false, a, b) => a.cmp(&b),
            };
            (x.scheme, x.dir)
                .cmp(&(y.scheme, y.dir))
                .then_with(|| text_order::<$count>(self.last_text(x.last), self.last_text(y.last)))
                .then_with(|| piece(x.query, y.query))
                .then_with(|| piece(x.fragment, y.fragment))
        }
        $(#[$spelling])*
        /// The identifier's string, as [`crate::uri::compared_id`] spells it.
        pub fn spelled(&self, handle: u32) -> String {
            #[cfg(test)]
            if $count {
                crate::resolution_work::text(self.spelling_capacity(handle));
            }
            let leaf = self.leaves[handle as usize];
            let mut dirs = Vec::new();
            let mut dir = leaf.dir;
            while dir != ROOT {
                dirs.push(dir);
                dir = self.nodes[dir as usize].parent;
            }
            let mut out = String::with_capacity(self.spelling_capacity(handle));
            out.push_str(self.scheme_text(leaf.scheme));
            out.push(':');
            for &dir in dirs.iter().rev() {
                out.push_str(self.label(dir));
                out.push('/');
            }
            out.push_str(self.last_text(leaf.last));
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
    };
}
pub(super) use identifier_methods;

// Span storage specializes without enlarging validation's enum. The owned
// block coordinates exist only in the query instantiation.
macro_rules! identifier_label {
    ($name:ident $(<$life:lifetime>)?, $borrowed:ty, [$($owned:ident)?]) => {
        #[derive(Clone, Copy, Debug)]
        /// Directory text stored as a source span or offsets in copied blocks.
        enum $name $(<$life>)? {
            Borrowed($borrowed),
            Copied { start: usize, end: usize },
            $($owned { block: usize, start: usize, end: usize },)?
        }
        impl $(<$life>)? $name $(<$life>)? {
            fn sub(self, start: usize, end: usize) -> Self {
                match self {
                    Self::Borrowed(text) => Self::Borrowed(&text[start..end]),
                    Self::Copied { start: at, .. } => Self::Copied {
                        start: at + start,
                        end: at + end,
                    },
                    $(Self::$owned { block, start: at, .. } => Self::$owned {
                        block,
                        start: at + start,
                        end: at + end,
                    },)?
                }
            }
        }
    };
}
pub(super) use identifier_label;
