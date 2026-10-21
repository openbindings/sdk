//! OBI-13's compared `$id`s, held as shared pieces of the document's own text.
//!
//! [`crate::uri::compared_id`] spells each compared identifier out, and a
//! nested resource's identifier repeats its enclosing one's: a chain of nested
//! relative `$id`s would spell quadratically many bytes. Here an identifier is
//! a handle into a trie of the pieces RFC 3986 resolution moves: its scheme,
//! the directory segments of its hierarchical part (each with the `/` after
//! it, which dot-segment removal pops whole), the last segment, the query and
//! the fragment. Every piece is a slice of some `$id` string. Each compared
//! declaration gets a handle; [`Identifiers::duplicates`] finds the handles
//! that share a string and orders them as their strings compare, so callers
//! keep string semantics.
use fluent_uri::UriRef;
use std::{cmp::Ordering, collections::HashMap};

const NONE: u32 = u32::MAX;
const ROOT: u32 = 0;
const EMPTY: u32 = 0;

#[derive(Clone, Copy, Debug)]
struct Dir {
    parent: u32,
    /// This directory's segment; its string is the parent's, the segment and `/`.
    segment: u32,
    /// Whether the hierarchical part so far begins `//`: an authority.
    authority: bool,
    /// For a directory below the authority, the one whose segment it is.
    authority_dir: u32,
}
/// The output of dot-segment removal so far: a directory and a last segment,
/// with the last segment's interned text once known.
#[derive(Clone, Copy, Debug)]
struct Out<'a> {
    dir: u32,
    last: &'a str,
    last_id: u32,
}
#[derive(Clone, Copy, Debug)]
struct Leaf<'a> {
    scheme: u32,
    /// The hierarchical part is the directory's string and then the last segment.
    out: Out<'a>,
    /// Indices into the query and fragment pieces, or `NONE`.
    query: u32,
    fragment: u32,
}

#[derive(Debug)]
pub(crate) struct Identifiers<'a> {
    texts: Vec<&'a str>,
    text_ids: HashMap<&'a str, u32>,
    dirs: Vec<Dir>,
    dir_ids: HashMap<(u32, u32), u32>,
    /// The directory of a hierarchical part `//`, whose last segment is then
    /// the authority.
    authority_root: u32,
    leaves: Vec<Leaf<'a>>,
    /// Queries and fragments, which only references write.
    pieces: Vec<&'a str>,
    authority_valid: HashMap<u32, bool>,
}
impl<'a> Identifiers<'a> {
    pub fn new() -> Self {
        let mut ids = Self {
            texts: Vec::new(),
            text_ids: HashMap::new(),
            dirs: vec![Dir {
                parent: NONE,
                segment: EMPTY,
                authority: false,
                authority_dir: NONE,
            }],
            dir_ids: HashMap::new(),
            authority_root: NONE,
            leaves: Vec::new(),
            pieces: Vec::new(),
            authority_valid: HashMap::new(),
        };
        assert_eq!(ids.text(""), EMPTY);
        let slash = ids.dir(ROOT, EMPTY);
        ids.authority_root = ids.dir(slash, EMPTY);
        ids
    }
    fn text(&mut self, text: &'a str) -> u32 {
        *self.text_ids.entry(text).or_insert_with(|| {
            self.texts.push(text);
            u32::try_from(self.texts.len() - 1).expect("pieces are bounded by the source")
        })
    }
    fn dir(&mut self, parent: u32, segment: u32) -> u32 {
        if let Some(&dir) = self.dir_ids.get(&(parent, segment)) {
            return dir;
        }
        let id = u32::try_from(self.dirs.len()).expect("pieces are bounded by the source");
        let above = self.dirs[parent as usize];
        let (authority, authority_dir) = if parent == self.authority_root {
            (true, id)
        } else {
            (above.authority, above.authority_dir)
        };
        self.dirs.push(Dir {
            parent,
            segment,
            authority,
            authority_dir,
        });
        self.dir_ids.insert((parent, segment), id);
        id
    }
    fn out(&self, dir: u32, segment: u32) -> Out<'a> {
        Out {
            dir,
            last: self.texts[segment as usize],
            last_id: segment,
        }
    }
    fn last_id(&mut self, out: Out<'a>) -> u32 {
        if out.last_id == NONE {
            self.text(out.last)
        } else {
            out.last_id
        }
    }
    /// Whether the hierarchical part begins `//`, so that the identifier, read
    /// back as a base, has an authority.
    fn has_authority(&self, out: Out<'a>) -> bool {
        out.dir == self.authority_root || self.dirs[out.dir as usize].authority
    }
    /// The authority a base read back from this hierarchical part has.
    fn authority_of(&mut self, out: Out<'a>) -> u32 {
        if out.dir == self.authority_root {
            self.last_id(out)
        } else {
            self.dirs[self.dirs[out.dir as usize].authority_dir as usize].segment
        }
    }
    /// The state for an empty path after the hierarchical part's authority, if
    /// any.
    fn origin(&self, authority: Option<u32>) -> Out<'a> {
        match authority {
            Some(authority) => self.out(self.authority_root, authority),
            None => self.out(ROOT, EMPTY),
        }
    }
    fn push(&mut self, out: Out<'a>, segment: &'a str) -> Out<'a> {
        let last = self.last_id(out);
        Out {
            dir: self.dir(out.dir, last),
            last: segment,
            last_id: NONE,
        }
    }
    // `out.truncate(out.rfind('/').unwrap_or(0))` on the path after `origin`.
    fn pop(&self, out: Out<'a>, origin: Out<'a>) -> Out<'a> {
        if out.dir == origin.dir {
            return origin;
        }
        let dir = self.dirs[out.dir as usize];
        self.out(dir.parent, dir.segment)
    }
    /// RFC 3986 §5.2.4 dot-segment removal of `input`, written onto `out`,
    /// exactly as [`crate::uri`]'s does; `origin` is the empty output.
    fn remove_dots(&mut self, mut out: Out<'a>, origin: Out<'a>, mut input: &'a str) -> Out<'a> {
        while !input.is_empty() {
            if let Some(rest) = input.strip_prefix("../") {
                input = rest;
            } else if let Some(rest) = input.strip_prefix("./") {
                input = rest;
            } else if input.starts_with("/./") {
                input = &input[2..];
            } else if input == "/." {
                input = "/";
            } else if input.starts_with("/../") || input == "/.." {
                input = if input == "/.." { "/" } else { &input[3..] };
                out = self.pop(out, origin);
            } else if input == "." || input == ".." {
                input = "";
            } else if let Some(rest) = input.strip_prefix('/') {
                let end = rest.find('/').unwrap_or(rest.len());
                out = self.push(out, &rest[..end]);
                input = &rest[end..];
            } else {
                // Only the first piece of an output that is still empty lacks
                // a leading slash, and only without an authority.
                debug_assert!(out.dir == ROOT && out.last.is_empty());
                let end = input[1..].find('/').map_or(input.len(), |i| i + 1);
                out.last = &input[..end];
                out.last_id = NONE;
                input = &input[end..];
            }
        }
        out
    }
    /// Dot-segment removal of `/` and then `relative`, which has no leading
    /// slash, onto `out`.
    fn remove_dots_after_slash(
        &mut self,
        out: Out<'a>,
        origin: Out<'a>,
        relative: &'a str,
    ) -> Out<'a> {
        if let Some(rest) = relative.strip_prefix("..")
            && (rest.is_empty() || rest.starts_with('/'))
        {
            // "/../": pop, then the rest; "/..": pop, then "/".
            let out = self.pop(out, origin);
            return if rest.is_empty() {
                self.push(out, "")
            } else {
                self.remove_dots(out, origin, rest)
            };
        }
        if let Some(rest) = relative.strip_prefix('.') {
            if rest.is_empty() {
                return self.push(out, "");
            }
            if rest.starts_with('/') {
                return self.remove_dots(out, origin, rest);
            }
        }
        let end = relative.find('/').unwrap_or(relative.len());
        let out = self.push(out, &relative[..end]);
        self.remove_dots(out, origin, &relative[end..])
    }
    fn piece(&mut self, piece: Option<&'a str>) -> u32 {
        piece.map_or(NONE, |piece| {
            self.pieces.push(piece);
            u32::try_from(self.pieces.len() - 1).expect("pieces are bounded by the source")
        })
    }
    fn piece_text(&self, piece: u32) -> Option<&'a str> {
        (piece != NONE).then(|| self.pieces[piece as usize])
    }
    fn with_parts(&mut self, scheme: u32, reference: &UriRef<&'a str>) -> Leaf<'a> {
        let authority = reference.authority().map(|a| self.text(a.as_str()));
        let origin = self.origin(authority);
        Leaf {
            scheme,
            out: self.remove_dots(origin, origin, reference.path().as_str()),
            query: self.piece(reference.query().map(|q| q.as_str())),
            fragment: self.piece(fragment(reference)),
        }
    }
    fn authority_parses(&mut self, authority: u32) -> bool {
        if let Some(&valid) = self.authority_valid.get(&authority) {
            return valid;
        }
        let text = self.texts[authority as usize];
        let valid = UriRef::parse(format!("//{text}").as_str()).is_ok_and(|r| {
            r.authority().is_some_and(|a| a.as_str() == text) && r.path().is_empty()
        });
        self.authority_valid.insert(authority, valid);
        valid
    }
    /// The compared identifier of an `$id` written `identifier`, resolved
    /// against `parent`, the compared identifier of the nearest enclosing
    /// schema that declares one. `None` when [`crate::uri::compared_id`] gives
    /// none: an identifier that is no URI-reference, or a relative one without
    /// a usable base. Each compared declaration's handle is the next.
    pub fn compared(&mut self, parent: Option<u32>, identifier: &'a str) -> Option<u32> {
        let reference = UriRef::parse(identifier).ok()?;
        let leaf = if let Some(scheme) = reference.scheme() {
            let scheme = self.text(scheme.as_str());
            self.with_parts(scheme, &reference)
        } else {
            let parent = parent? as usize;
            let base = self.leaves[parent];
            // The base is read back from its string; an authority found then
            // must parse as one.
            let base_authority = if self.has_authority(base.out) {
                let authority = self.authority_of(base.out);
                if base.out.dir == self.authority_root {
                    // Keep the interned last segment for the next reader.
                    self.leaves[parent].out.last_id = authority;
                }
                Some(authority)
            } else {
                None
            };
            if base_authority.is_some_and(|authority| !self.authority_parses(authority)) {
                return None;
            }
            if reference.authority().is_some() {
                self.with_parts(base.scheme, &reference)
            } else {
                let base_out = self.leaves[parent].out;
                let path = reference.path().as_str();
                let origin = self.origin(base_authority);
                let (out, query) = if path.is_empty() {
                    match reference.query() {
                        Some(query) => (base_out, self.piece(Some(query.as_str()))),
                        None => (base_out, base.query),
                    }
                } else {
                    let out = if path.starts_with('/') {
                        self.remove_dots(origin, origin, path)
                    } else if base_authority.is_some() && base.out.dir == self.authority_root {
                        // An authority and an empty path: the prefix is "/".
                        self.remove_dots_after_slash(origin, origin, path)
                    } else if base.out.dir != ROOT {
                        // The base path up to its last slash, then the path.
                        let dir = self.dirs[base.out.dir as usize];
                        let out = self.out(dir.parent, dir.segment);
                        self.remove_dots_after_slash(out, origin, path)
                    } else {
                        // No slash in the base path: the path alone.
                        self.remove_dots(origin, origin, path)
                    };
                    (out, self.piece(reference.query().map(|q| q.as_str())))
                };
                Leaf {
                    scheme: base.scheme,
                    out,
                    query,
                    fragment: self.piece(fragment(&reference)),
                }
            }
        };
        self.leaves.push(leaf);
        Some(u32::try_from(self.leaves.len() - 1).expect("identifiers are bounded by the source"))
    }
    fn key(&self, handle: u32) -> (u32, u32, &'a str, Option<&'a str>, Option<&'a str>) {
        let leaf = &self.leaves[handle as usize];
        (
            leaf.scheme,
            leaf.out.dir,
            leaf.out.last,
            self.piece_text(leaf.query),
            self.piece_text(leaf.fragment),
        )
    }
    /// The handles whose identifier string another handle's is too, each
    /// with its string's rank among those strings: by string, then handle.
    pub fn duplicates(&self) -> Vec<(u32, u32)> {
        // A stable sort keeps one string's handles in order.
        let mut by_leaf: Vec<u32> = (0..self.leaves.len() as u32).collect();
        by_leaf.sort_by_key(|&handle| self.key(handle));
        let runs: Vec<&[u32]> = by_leaf
            .chunk_by(|&a, &b| self.key(a) == self.key(b))
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
            (leaf.out.dir, leaf.out.last, after)
        };
        let mut groups: Vec<(u32, &str, After)> = handles.iter().map(|&h| group_of(h)).collect();
        groups.sort_unstable();
        groups.dedup();
        // A directory's items, each keyed by the text that begins it: child
        // directories by segment and '/', groups by last segment and the
        // character after it. Neither key's continuation can reorder them: a
        // segment holds no '/', '?' or '#'.
        #[derive(Clone, Copy)]
        enum Item {
            Dir(u32),
            Group(u32),
        }
        let mut entries: Vec<(u32, Item)> = (1..self.dirs.len() as u32)
            .map(|dir| (self.dirs[dir as usize].parent, Item::Dir(dir)))
            .chain(
                groups
                    .iter()
                    .enumerate()
                    .map(|(g, &(dir, _, _))| (dir, Item::Group(g as u32))),
            )
            .collect();
        let key = |item: &Item| match *item {
            Item::Dir(dir) => (
                self.texts[self.dirs[dir as usize].segment as usize],
                After::Slash,
            ),
            Item::Group(g) => {
                let (_, last, after) = groups[g as usize];
                (last, after)
            }
        };
        entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| keyed(key(&a.1), key(&b.1))));
        // Depth first over directories, in key order, numbering the groups.
        let mut first = vec![0u32; self.dirs.len() + 1];
        for &(parent, _) in &entries {
            first[parent as usize + 1] += 1;
        }
        for dir in 0..self.dirs.len() {
            first[dir + 1] += first[dir];
        }
        let mut position = vec![0u32; groups.len()];
        let mut next = 0;
        let mut stack = vec![(ROOT, first[ROOT as usize])];
        while let Some((dir, at)) = stack.pop() {
            if at == first[dir as usize + 1] {
                continue;
            }
            stack.push((dir, at + 1));
            match entries[at as usize].1 {
                Item::Dir(child) => stack.push((child, first[child as usize])),
                Item::Group(g) => {
                    position[g as usize] = next;
                    next += 1;
                }
            }
        }
        let position_of = |handle: u32| {
            position[groups
                .binary_search(&group_of(handle))
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
                (self.texts[a as usize], After::Colon),
                (self.texts[b as usize], After::Colon),
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
        let mut dir = leaf.out.dir;
        while dir != ROOT {
            dirs.push(dir);
            dir = self.dirs[dir as usize].parent;
        }
        let mut out = format!("{}:", self.texts[leaf.scheme as usize]);
        for &dir in dirs.iter().rev() {
            out.push_str(self.texts[self.dirs[dir as usize].segment as usize]);
            out.push('/');
        }
        out.push_str(leaf.out.last);
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
            let mut ids = Identifiers::new();
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

    #[test]
    fn nested_relative_identifiers_take_linear_pieces() {
        // Each level adds one directory: the pieces held grow with the
        // declarations, not with the length of the identifiers they spell.
        let levels = 20_000;
        let mut ids = Identifiers::new();
        let mut parent = ids.compared(None, "https://e.test/");
        for _ in 0..levels {
            parent = ids.compared(parent, "a/");
        }
        let leaf = parent.unwrap();
        assert_eq!(
            ids.spelled(leaf).len(),
            "https://e.test/".len() + 2 * levels
        );
        assert!(ids.dirs.len() <= levels + 8);
        assert!(ids.texts.len() <= 8);
        assert_eq!(ids.leaves.len(), levels + 1);
        assert!(ids.duplicates().is_empty());
    }
}
