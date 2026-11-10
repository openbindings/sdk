//! Compact owned queries, separate from validation's borrowed identifiers.
//! Queries own their input spans in a request-local overlay. Inherited
//! directories and components stay shared.
//! Canonical aliases attach normalized handles to these same components.
//!
//! [`crate::uri::compared_id`] spells each compared identifier out, and a
//! nested resource's identifier repeats its enclosing one's: a chain of nested
//! relative `$id`s would spell quadratically many bytes. Here an identifier is
//! a handle: its scheme, the directory its hierarchical part ends in (the part
//! up to and including its last `/`), the last segment, the query and the
//! fragment. Directories form a trie whose edges are runs of whole segments,
//! each run a span of one `$id`'s text however many segments it holds. Each
//! declaration adds at most three edges and one leaf. Directory runs copy text
//! only when dot segments interrupt the run, never more than the own `$id`.
//! Ordered child lookup interns each distinct first segment once.
//! Grouping preserves exact spelling equality without expanding directories.
//! The walk lives in `identifier_walk`; this module supplies owned storage
//! and query-only canonicalization.
use super::identifier_walk::*;
use fluent_uri::UriRef;
use std::{cmp::Ordering, collections::HashSet};

use super::query_storage::{Table, Vector};

type TextKey = (u8, usize, usize, usize);
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
enum Normalization {
    Scheme,
    Path,
    Query,
}

identifier_label!(Label, &'static str, [Owned]);
impl Label {
    fn key(self) -> TextKey {
        match self {
            Self::Borrowed(s) => (0, s.as_ptr() as usize, 0, s.len()),
            Self::Copied { start, end } => (1, 0, start, end),
            Self::Owned { block, start, end } => (2, block, start, end),
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct Node {
    parent: u32,
    /// The directory a walk up from here stops at: `ROOT` for a path without
    /// a leading `/`, `SLASH` for one without an authority, and otherwise the
    /// authority's directory, `AUTH`'s child.
    top: u32,
    /// Segments from the root.
    depth: usize,
    bytes: usize,
    /// Interned first segment, reused when an edge is split.
    key: u32,
    /// The segments after the parent's string, `/` between them and implied
    /// after the last.
    label: Label,
}
#[derive(Clone, Copy, Debug)]
struct Leaf {
    scheme: u32,
    dir: u32,
    last: u32,
    /// Indices into the pieces, or `NONE`.
    query: u32,
    fragment: u32,
    /// For a directory `//`, the directory of the authority that is the last
    /// segment.
    authority: u32,
}
#[derive(Debug)]
pub(crate) struct Identifiers<'a> {
    nodes: Vector<'a, Node>,
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
    children: Table<'a, (u32, u32), u32>,
    segment_ids: Table<'a, Box<str>, u32>,
    /// Schemes, interned.
    schemes: Vector<'a, Label>,
    scheme_ids: Table<'a, Box<str>, u32>,
    owned: Vector<'a, Box<str>>,
    text_ids: Table<'a, Box<str>, Label>,
    normal_text: Table<'a, (TextKey, Normalization), Label>,
    normal_dirs: Table<'a, (u32, u32), u32>,
    escaped_dirs: Table<'a, u32, u32>,
    normal_authorities: Table<'a, (TextKey, u32), Label>,
    normal_schemes: Table<'a, u32, u32>,
    normal_pieces: Table<'a, u32, u32>,
    /// Queries and fragments; a query an identifier takes from its base is
    /// that base's.
    pieces: Vector<'a, Label>,
    piece_ids: Table<'a, Box<str>, u32>,
    lasts: Vector<'a, Label>,
    last_ids: Table<'a, Box<str>, u32>,
    normal_lasts: Table<'a, (u32, u32), u32>,
    leaves: Vector<'a, Leaf>,
    /// The authorities' directories whose authority does not parse as one
    /// when read back.
    invalid_authorities: HashSet<u32>,
    inventory: Option<&'a Identifiers<'a>>,
}

impl<'a> Identifiers<'a> {
    pub(crate) fn retained_bytes(&self) -> usize {
        use openbindings_internal_json::backend::vec_bytes;
        self.nodes.retained_bytes() + self.copied.capacity() + vec_bytes(&self.retired)
            + self.retired.iter().map(|(_, text)| text.len()).sum::<usize>()
            + self.children.retained_bytes() + self.segment_ids.retained_bytes()
            + self.segment_ids.owned_keys().map(|s| s.len()).sum::<usize>()
            + self.schemes.retained_bytes() + self.scheme_ids.retained_bytes()
            + self.scheme_ids.owned_keys().map(|s| s.len()).sum::<usize>()
            + self.owned.retained_bytes() + self.owned[0..].iter().map(|s| s.len()).sum::<usize>()
            + self.text_ids.retained_bytes() + self.text_ids.owned_keys().map(|s| s.len()).sum::<usize>()
            + self.normal_text.retained_bytes() + self.normal_dirs.retained_bytes()
            + self.escaped_dirs.retained_bytes() + self.normal_authorities.retained_bytes()
            + self.normal_schemes.retained_bytes() + self.normal_pieces.retained_bytes()
            + self.pieces.retained_bytes() + self.piece_ids.retained_bytes()
            + self.piece_ids.owned_keys().map(|s| s.len()).sum::<usize>()
            + self.lasts.retained_bytes() + self.last_ids.retained_bytes()
            + self.last_ids.owned_keys().map(|s| s.len()).sum::<usize>()
            + self.normal_lasts.retained_bytes() + self.leaves.retained_bytes()
            // At most one u32 key plus a control byte per hash bucket.
            + if self.invalid_authorities.capacity() == 0 { 0 } else {
                (self.invalid_authorities.capacity() * 8 / 7).next_power_of_two() * 5 + 16
            }
    }

    identifier_methods!('q, ['q], Label, Label, Leaf, [bytes], true, []);

    fn empty_input() -> Label {
        Label::Borrowed("")
    }
    fn input_label(input: Label) -> Label {
        input
    }
    fn store_last(&mut self, last: Label) -> u32 {
        self.last(last)
    }
    fn scheme_text(&self, scheme: u32) -> &str {
        self.text(self.schemes[scheme as usize])
    }
    fn spelling_capacity(&self, handle: u32) -> usize {
        self.length(handle)
    }

    /// Identifiers whose texts total at most `budget` bytes, such as the
    /// length of the document that holds them.
    pub fn new(budget: usize) -> Self {
        let mut ids = Self {
            nodes: Vector::from(vec![Node {
                parent: NONE,
                top: ROOT,
                depth: 0,
                bytes: 0,
                key: 0,
                label: Label::Borrowed(""),
            }]),
            copied: String::new(),
            copied_from: 0,
            retired: Vec::new(),
            budget,
            children: Table::new(),
            segment_ids: Table::new(),
            schemes: Vector::new(),
            owned: Vector::new(),
            text_ids: Table::new(),
            normal_text: Table::new(),
            normal_dirs: Table::new(),
            escaped_dirs: Table::new(),
            normal_authorities: Table::new(),
            normal_schemes: Table::new(),
            normal_pieces: Table::new(),
            scheme_ids: Table::new(),
            pieces: Vector::new(),
            piece_ids: Table::new(),
            lasts: Vector::new(),
            last_ids: Table::new(),
            normal_lasts: Table::new(),
            leaves: Vector::new(),
            invalid_authorities: HashSet::new(),
            inventory: None,
        };
        assert_eq!(ids.add_node(ROOT, Label::Borrowed("")), SLASH);
        assert_eq!(ids.add_node(SLASH, Label::Borrowed("")), AUTH);
        ids
    }
    /// Borrow immutable environment names. No inventory text or entries are copied.
    pub fn overlay(&self) -> Identifiers<'_> {
        Identifiers {
            nodes: self.nodes.overlay(),
            schemes: self.schemes.overlay(),
            owned: self.owned.overlay(),
            pieces: self.pieces.overlay(),
            lasts: self.lasts.overlay(),
            leaves: self.leaves.overlay(),
            children: self.children.overlay(),
            segment_ids: self.segment_ids.overlay(),
            scheme_ids: self.scheme_ids.overlay(),
            text_ids: self.text_ids.overlay(),
            normal_text: self.normal_text.overlay(),
            normal_dirs: self.normal_dirs.overlay(),
            escaped_dirs: self.escaped_dirs.overlay(),
            normal_authorities: self.normal_authorities.overlay(),
            normal_schemes: self.normal_schemes.overlay(),
            normal_pieces: self.normal_pieces.overlay(),
            piece_ids: self.piece_ids.overlay(),
            last_ids: self.last_ids.overlay(),
            normal_lasts: self.normal_lasts.overlay(),
            copied: String::new(),
            copied_from: self.copied_from + self.copied.len(),
            retired: Vec::new(),
            budget: usize::MAX,
            invalid_authorities: HashSet::new(),
            inventory: Some(self),
        }
    }
    fn text(&self, label: Label) -> &str {
        match label {
            Label::Borrowed(text) => text,
            Label::Owned { block, start, end } => &self.owned[block][start..end],
            Label::Copied { start, end } => {
                if let Some(base) = self.inventory
                    && start < base.copied_from + base.copied.len()
                {
                    return base.text(label);
                }
                copied_text(&self.copied, self.copied_from, &self.retired, start, end)
            }
        }
    }

    fn own(&mut self, text: &str) -> Label {
        crate::resolution_work::text(text.len());
        let block = self.owned.len();
        self.owned.push(text.into());
        Label::Owned {
            block,
            start: 0,
            end: text.len(),
        }
    }
    fn intern(&mut self, label: Label) -> Label {
        if matches!(label, Label::Borrowed(_)) {
            return label;
        }
        crate::resolution_work::text(self.text(label).len());
        if let Some(&existing) = self.text_ids.get(self.text(label)) {
            return existing;
        }
        let text = self.text(label).into();
        self.text_ids.insert(text, label);
        label
    }
    fn scheme(&mut self, scheme: Label) -> u32 {
        crate::resolution_work::text(self.text(scheme).len());
        if let Some(&id) = self.scheme_ids.get(self.text(scheme)) {
            return id;
        }
        let text = self.text(scheme).into();
        let id = self.schemes.len() as u32;
        self.schemes.push(scheme);
        self.scheme_ids.insert(text, id);
        id
    }
    fn last(&mut self, label: Label) -> u32 {
        let text = self.text(label);
        if text.is_empty() {
            return NONE;
        }
        crate::resolution_work::text(text.len());
        if let Some(&id) = self.last_ids.get(text) {
            return id;
        }
        let text = text.into();
        let id = self.lasts.len() as u32;
        self.lasts.push(label);
        self.last_ids.insert(text, id);
        id
    }
    fn last_label(&self, last: u32) -> Label {
        if last == NONE {
            Label::Borrowed("")
        } else {
            self.lasts[last as usize]
        }
    }
    fn last_text(&self, last: u32) -> &str {
        self.text(self.last_label(last))
    }
    fn piece(&mut self, piece: Option<Label>) -> u32 {
        piece.map_or(NONE, |piece| {
            let text = self.text(piece);
            crate::resolution_work::text(text.len());
            if let Some(&id) = self.piece_ids.get(text) {
                return id;
            }
            let text = text.into();
            let id = self.pieces.len() as u32;
            self.pieces.push(piece);
            self.piece_ids.insert(text, id);
            id
        })
    }
    fn piece_text(&self, piece: u32) -> Option<&str> {
        (piece != NONE).then(|| self.text(self.pieces[piece as usize]))
    }

    fn authority_parses(&self, authority: u32) -> bool {
        !self.invalid_authorities.contains(&authority)
            && self
                .inventory
                .is_none_or(|base| base.authority_parses(authority))
    }
    /// Own only the input text, sharing every inherited directory and component.
    pub fn compared_owned(&mut self, parent: Option<u32>, identifier: &str) -> Option<u32> {
        let input = self.own(identifier);
        self.compare(parent, identifier, input)
    }

    /// An already resolved spelling, without another dot-removal pass.
    pub fn absolute_owned(&mut self, text: &str) -> Option<u32> {
        crate::resolution_work::text(text.len());
        let r = UriRef::parse(text).ok()?;
        let input = self.own(text);
        let scheme = self.scheme(span(input, text, r.scheme()?.as_str()));
        let path = r.path().as_str();
        let origin = r
            .authority()
            .map(|a| self.authority(text, span(input, text, a.as_str())));
        let (dir, last) = if path.is_empty() {
            match r.authority() {
                Some(a) => (AUTH, span(input, text, a.as_str())),
                None => (ROOT, Label::Borrowed("")),
            }
        } else {
            let (at, path) = if let Some(origin) = origin {
                (origin, &path[1..])
            } else if let Some(path) = path.strip_prefix('/') {
                (SLASH, path)
            } else {
                (ROOT, path)
            };
            match path.rsplit_once('/') {
                Some((dirs, last)) => (
                    self.descend(at, span(input, text, dirs), text),
                    span(input, text, last),
                ),
                None => (at, span(input, text, path)),
            }
        };
        let leaf = Leaf {
            scheme,
            dir,
            last: self.last(last),
            query: self.piece(r.query().map(|q| span(input, text, q.as_str()))),
            fragment: self.piece(fragment(&r).map(|s| span(input, text, s))),
            authority: if dir == AUTH { origin.unwrap() } else { NONE },
        };
        self.leaves.push(leaf);
        Some((self.leaves.len() - 1) as u32)
    }
    pub fn valid_absolute(&self, handle: u32) -> bool {
        self.base_authority(handle as usize)
            .is_none_or(|a| self.authority_parses(a))
    }
    pub fn length(&self, handle: u32) -> usize {
        let leaf = self.leaves[handle as usize];
        self.text(self.schemes[leaf.scheme as usize]).len()
            + 1
            + self.nodes[leaf.dir as usize].bytes
            + self.last_text(leaf.last).len()
            + self.piece_text(leaf.query).map_or(0, |s| s.len() + 1)
            + self.piece_text(leaf.fragment).map_or(0, |s| s.len() + 1)
    }
    // Normalization is attached to the same interned components and directory
    // handles. An inherited component is normalized once, never per query.
    fn normalized_text(&mut self, label: Label, kind: Normalization) -> Label {
        let key = (label.key(), kind);
        if let Some(&result) = self.normal_text.get(&key) {
            return result;
        }
        let text = self.text(label);
        crate::resolution_work::text(text.len());
        let normalized = match kind {
            Normalization::Scheme => text.to_ascii_lowercase(),
            Normalization::Path => UriRef::parse(format!("x:a/{text}"))
                .unwrap()
                .normalize()
                .as_str()[4..]
                .to_owned(),
            Normalization::Query => UriRef::parse(format!("?{text}"))
                .unwrap()
                .normalize()
                .as_str()[1..]
                .to_owned(),
        };
        let result = if normalized == text {
            label
        } else {
            self.own(&normalized)
        };
        let result = self.intern(result);
        self.normal_text.insert(key, result);
        result
    }
    fn normalized_authority(&mut self, label: Label, scheme: u32) -> Label {
        let key = (label.key(), scheme);
        if let Some(&label) = self.normal_authorities.get(&key) {
            return label;
        }
        crate::resolution_work::text(
            self.text(label).len() + self.text(self.schemes[scheme as usize]).len(),
        );
        let own = format!(
            "{}://{}",
            self.text(self.schemes[scheme as usize]),
            self.text(label)
        );
        let normalized = UriRef::parse(own.as_str()).unwrap().normalize();
        let text = normalized.authority().unwrap().as_str();
        let result = if text == self.text(label) {
            label
        } else {
            self.own(text)
        };
        let result = self.intern(result);
        self.normal_authorities.insert(key, result);
        result
    }
    // Walk a normalized absolute path without interpreting a newly exposed
    // leading // as an authority. URI normalization escapes that case later.
    fn normalized_path(&mut self, parent: u32, text: &str, rooted: bool) -> (u32, Label) {
        let own = format!("./{text}");
        let input = self.own(&own);
        let copy = std::mem::take(&mut self.copied);
        let mut walk = Walk::new(
            &own,
            Pos::Node {
                node: parent,
                up: 0,
            },
            "",
            copy,
            self.copied_from,
            self.budget,
        );
        let top = self.nodes[parent as usize].top;
        let floor = if rooted || top == ROOT || top == SLASH {
            1
        } else {
            self.nodes[top as usize].depth
        };
        walk.limit = self.nodes[parent as usize].depth - floor;
        Self::remove_dots_after_slash(&mut walk, &own);
        self.place(walk, input)
    }
    fn normalized_directory(&mut self, dir: u32, scheme: u32) -> u32 {
        let mut pending = Vec::new();
        let mut at = dir;
        while at > AUTH && !self.normal_dirs.contains_key(&(at, scheme)) {
            pending.push(at);
            at = self.nodes[at as usize].parent;
        }
        for id in pending.into_iter().rev() {
            let node = self.nodes[id as usize];
            let parent = self
                .normal_dirs
                .get(&(node.parent, scheme))
                .copied()
                .unwrap_or(node.parent);
            let normalized = if node.parent == AUTH {
                self.normalized_authority(node.label, scheme)
            } else {
                self.normalized_text(node.label, Normalization::Path)
            };
            let mapped = if normalized.key() == node.label.key() && parent == node.parent {
                id
            } else if node.parent == AUTH {
                let text = self.text(normalized).to_owned();
                self.authority(&format!("//{text}"), normalized)
            } else if node.top == ROOT {
                self.descend(parent, normalized, "")
            } else {
                let text = format!("{}/", self.text(normalized));
                self.normalized_path(parent, &text, node.top == SLASH).0
            };
            self.normal_dirs.insert((id, scheme), mapped);
        }
        self.normal_dirs.get(&(dir, scheme)).copied().unwrap_or(dir)
    }
    // A path that normalization made start with // needs the URI library's
    // /.-prefix. Reattach its shared directory runs, without spelling them out.
    fn escaped_directory(&mut self, dir: u32) -> u32 {
        let mut pending = Vec::new();
        let mut at = dir;
        while at != AUTH && !self.escaped_dirs.contains_key(&at) {
            pending.push(at);
            at = self.nodes[at as usize].parent;
        }
        if !self.escaped_dirs.contains_key(&AUTH) {
            let label = self.own("./");
            let escaped = self.descend(SLASH, label, "");
            self.escaped_dirs.insert(AUTH, escaped);
        }
        for id in pending.into_iter().rev() {
            let node = self.nodes[id as usize];
            let parent = self.escaped_dirs[&node.parent];
            let mapped = self.descend(parent, node.label, "");
            self.escaped_dirs.insert(id, mapped);
        }
        self.escaped_dirs[&dir]
    }
    /// The URI library's canonical resource name, still in this identifier trie.
    pub fn canonical(&mut self, handle: u32) -> Option<u32> {
        if !self.valid_absolute(handle) {
            return None;
        }
        let leaf = self.leaves[handle as usize];
        let scheme = match self.normal_schemes.get(&leaf.scheme) {
            Some(&scheme) => scheme,
            None => {
                let label =
                    self.normalized_text(self.schemes[leaf.scheme as usize], Normalization::Scheme);
                let scheme = self.scheme(label);
                self.normal_schemes.insert(leaf.scheme, scheme);
                scheme
            }
        };
        let mut dir = self.normalized_directory(leaf.dir, scheme);
        let last_key = (leaf.last, if leaf.dir == AUTH { scheme } else { NONE });
        let mut last = match self.normal_lasts.get(&last_key) {
            Some(&last) => last,
            None => {
                let label = self.last_label(leaf.last);
                let normalized = if leaf.dir == AUTH {
                    self.normalized_authority(label, scheme)
                } else {
                    self.normalized_text(label, Normalization::Path)
                };
                let last = self.last(normalized);
                self.normal_lasts.insert(last_key, last);
                last
            }
        };
        let top = self.nodes[leaf.dir as usize].top;
        if top != ROOT && leaf.dir != AUTH && matches!(self.last_text(last), "." | "..") {
            let text = self.last_text(last).to_owned();
            let (normalized_dir, label) = self.normalized_path(dir, &text, top == SLASH);
            dir = normalized_dir;
            last = self.last(label);
        }
        if self.base_authority(handle as usize).is_none()
            && top == SLASH
            && dir >= AUTH
            && (dir == AUTH || self.nodes[dir as usize].top != SLASH)
        {
            dir = self.escaped_directory(dir);
        }
        let query = if leaf.query == NONE {
            NONE
        } else if let Some(&query) = self.normal_pieces.get(&leaf.query) {
            query
        } else {
            let label =
                self.normalized_text(self.pieces[leaf.query as usize], Normalization::Query);
            let query = self.piece(Some(label));
            self.normal_pieces.insert(leaf.query, query);
            query
        };
        let authority = if dir == AUTH {
            self.normalized_directory(leaf.authority, scheme)
        } else {
            NONE
        };
        self.leaves.push(Leaf {
            scheme,
            dir,
            last,
            query,
            fragment: NONE,
            authority,
        });
        Some((self.leaves.len() - 1) as u32)
    }
}
// A component is a span of the own input in either borrowed or owned storage.
fn span(input: Label, own: &str, part: &str) -> Label {
    if part.is_empty() {
        return Label::Borrowed("");
    }
    let start = (part.as_ptr() as usize) - (own.as_ptr() as usize);
    input.sub(start, start + part.len())
}
#[cfg(test)]
mod query_tests {
    use super::*;

    #[test]
    fn query_overlays_split_and_normalize_without_changing_the_inventory() {
        let mut inventory = Identifiers::new(usize::MAX);
        let names = [
            "https://EXAMPLE.test:443/a/b/c/d/leaf?x=%7a",
            "s:/a/%2e%2e//b/leaf",
            "s:a/b/c/leaf",
            "s://host/a/b/leaf",
        ];
        let handles: Vec<_> = names
            .iter()
            .map(|s| inventory.absolute_owned(s).unwrap())
            .collect();
        for _ in 0..3 {
            let mut query = inventory.overlay();
            for (&base, name) in handles.iter().zip(names) {
                for reference in [
                    "../other",
                    "../../c/leaf",
                    "x/../y/z",
                    "?x=%7a",
                    "#a",
                    "../../../%2e%2e//x",
                ] {
                    let expected = crate::uri::compared_id(Some(name), reference).unwrap();
                    let handle = query.compared_owned(Some(base), reference).unwrap();
                    assert_eq!(query.spelled(handle), expected);
                    assert_eq!(query.length(handle), expected.len());
                    assert_eq!(
                        query.canonical(handle).map(|h| query.spelled(h)),
                        crate::input_policy::canonical_uri(expected.split('#').next().unwrap())
                    );
                }
            }
            drop(query);
            for (&handle, name) in handles.iter().zip(names) {
                assert_eq!(inventory.spelled(handle), name);
            }
        }
    }

    #[test]
    fn owned_queries_and_canonical_components_match_the_string_resolvers() {
        let bases = [
            "https://EXAMPLE.test:443/a/b?x=%7a",
            "s:a/b",
            "s:/a/b",
            "s://host/a/b",
            "s:/a/%2e/%2e%2e/b/",
            "s:a/%2e%2e/b/",
            "s:/a/%2e%2e//b/",
            "s:/%2e//host/a/",
            "s://[2001:0db8::1]:/a/",
            "s://u%73er@HOST:/a/",
            "s://",
            "s:/",
            "s:",
            "s://host",
            "s:a//b/",
            "s:/a//b/",
        ];
        let references = [
            "",
            "#",
            "#/$defs/s0",
            "?",
            "?x=%7a",
            "x",
            "x/",
            "../x",
            "../../x",
            "../../../x",
            "%2e",
            "%2e%2e",
            "%2e/",
            "%2e%2e/",
            "%2e//x",
            "a/%2e%2e/x",
            "/a/%2e%2e//b",
            "/.//host",
            "//HOST:443/a/%7a",
            "s:a/%2e%2e/b",
            "s:/a/%2e%2e//b",
            "s:/%2e//host/x",
            "s:/.//host:word",
        ];
        let mut ids = Identifiers::new(usize::MAX);
        for base in bases {
            let base = crate::uri::compared_id(None, base).unwrap();
            let parent = ids.absolute_owned(&base).unwrap();
            assert_eq!(ids.spelled(parent), base);
            for reference in references {
                let expected = crate::uri::compared_id(Some(&base), reference).unwrap();
                let handle = ids.compared_owned(Some(parent), reference).unwrap();
                assert_eq!(ids.spelled(handle), expected, "{base} + {reference}");
                assert_eq!(ids.length(handle), expected.len());
                let resource = expected.split('#').next().unwrap();
                let normalized = ids.canonical(handle);
                let canonical = normalized.map(|h| ids.spelled(h));
                if let Some(ref text) = canonical {
                    let name = ids.absolute_owned(text).unwrap();
                    assert!(ids.grouping(normalized.unwrap(), name).is_eq());
                }
                assert_eq!(
                    canonical,
                    crate::input_policy::canonical_uri(resource),
                    "{base} + {reference} = {resource}"
                );
            }
        }
    }
    #[test]
    fn normalized_directory_runs_match_the_uri_library() {
        let mut ids = Identifiers::new(usize::MAX);
        for prefix in ["s:", "s:/", "HTTPS://HOST:443/"] {
            for a in ["a", "%2e", "%2e%2e", "", "%2F", "%7e"] {
                for b in ["b", "%2e", "%2e%2e", "", "%2F", "%7e"] {
                    for c in ["c", "%2e", "%2e%2e", "", "%2F", "%7e"] {
                        let text = format!("{prefix}{a}/{b}/{c}");
                        let expected = crate::uri::compared_id(None, &text).unwrap();
                        let handle = ids.compared_owned(None, &text).unwrap();
                        assert_eq!(ids.spelled(handle), expected);
                        let normalized = ids.canonical(handle).map(|h| ids.spelled(h));
                        assert_eq!(
                            normalized,
                            crate::input_policy::canonical_uri(&expected),
                            "{text} -> {expected}"
                        );
                    }
                }
            }
        }
    }
}
