//! OBI-13's compared `$id`s, held as shared spans of the document's own text.
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
//! [`Identifiers::duplicates`] finds handles that share a string and orders
//! them as their strings compare, so callers keep string semantics.
use super::identifier_walk::*;
use fluent_uri::UriRef;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, HashSet},
};

identifier_label!(Label<'a>, &'a str, []);
#[derive(Clone, Copy, Debug)]
struct Node<'a> {
    parent: u32,
    /// The directory a walk up from here stops at: `ROOT` for a path without
    /// a leading `/`, `SLASH` for one without an authority, and otherwise the
    /// authority's directory, `AUTH`'s child.
    top: u32,
    /// Segments from the root.
    depth: usize,
    /// Interned first segment, reused when an edge is split.
    key: u32,
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
    children: BTreeMap<(u32, u32), u32>,
    segment_ids: BTreeMap<Box<str>, u32>,
    /// Schemes, interned.
    schemes: Vec<&'a str>,
    scheme_ids: BTreeMap<&'a str, u32>,
    /// Queries and fragments; a query an identifier takes from its base is
    /// that base's.
    pieces: Vec<&'a str>,
    leaves: Vec<Leaf<'a>>,
    /// The authorities' directories whose authority does not parse as one
    /// when read back.
    invalid_authorities: HashSet<u32>,
}

impl<'a> Identifiers<'a> {
    identifier_methods!('a, [], &'a str, Label<'a>, Leaf<'a>, [], false, [cfg(test)]);

    pub fn compared(&mut self, parent: Option<u32>, identifier: &'a str) -> Option<u32> {
        self.compare(parent, identifier, identifier)
    }
    fn empty_input() -> &'a str {
        ""
    }
    fn input_label(input: &'a str) -> Label<'a> {
        Label::Borrowed(input)
    }
    fn store_last(&mut self, last: &'a str) -> &'a str {
        last
    }
    fn last_text(&self, last: &'a str) -> &'a str {
        last
    }
    #[cfg(test)]
    fn scheme_text(&self, scheme: u32) -> &str {
        self.schemes[scheme as usize]
    }
    #[cfg(test)]
    fn spelling_capacity(&self, _handle: u32) -> usize {
        0
    }

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
            children: BTreeMap::new(),
            segment_ids: BTreeMap::new(),
            schemes: Vec::new(),
            scheme_ids: BTreeMap::new(),
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
            Label::Copied { start, end } => {
                copied_text(&self.copied, self.copied_from, &self.retired, start, end)
            }
        }
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

    fn authority_parses(&self, authority: u32) -> bool {
        !self.invalid_authorities.contains(&authority)
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
                .then_with(|| text_order::<false>(a.1, b.1))
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

    #[test]
    fn validation_layout_matches_727dcd0() {
        use std::mem::size_of;
        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            (
                size_of::<Leaf<'_>>(),
                size_of::<Node<'_>>(),
                size_of::<Label<'_>>()
            ),
            (40, 48, 24)
        );
        #[cfg(target_pointer_width = "32")]
        assert_eq!(
            (
                size_of::<Leaf<'_>>(),
                size_of::<Node<'_>>(),
                size_of::<Label<'_>>()
            ),
            (28, 28, 12)
        );
        let ids = Identifiers::new(0);
        let _: &Vec<Node<'_>> = &ids.nodes;
        let _: &Vec<Leaf<'_>> = &ids.leaves;
        let _: &Vec<&str> = &ids.schemes;
        let _: &Vec<&str> = &ids.pieces;
        let _: &BTreeMap<(u32, u32), u32> = &ids.children;
        let _: &BTreeMap<Box<str>, u32> = &ids.segment_ids;
        let _: &BTreeMap<&str, u32> = &ids.scheme_ids;
    }

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
    fn splitting_shared_long_segments_interns_their_text_once() {
        let long = "z".repeat(16_384);
        let text = format!("https://example.test/{long}/{}", "x/".repeat(256));
        let mut ids = Identifiers::new(text.len());
        let handle = ids.compared(None, &text).unwrap();
        let mut directory = ids.leaves[handle as usize].dir;
        for _ in 0..256 {
            directory = ids.up(directory, 1);
        }
        assert_eq!(ids.spelled(handle), text);
        assert_eq!(
            ids.segment_ids
                .keys()
                .filter(|s| s.as_ref() == long)
                .count(),
            1
        );
        assert!(ids.segment_ids.keys().map(|s| s.len()).sum::<usize>() <= long.len() + 32);
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

#[cfg(test)]
mod bounded_block_tests {
    use super::*;
    #[test]
    fn capped_blocks_preserve_every_original_identifier_and_duplicate_handle() {
        let texts: Vec<String> = (0..50_000)
            .map(|i| format!("s://{i:x}/q/a/../b/{}/x?z#", "c".repeat(100 + i % 31)))
            .collect();
        let mut ids = Identifiers::new(texts.iter().map(String::len).sum());
        let handles: Vec<u32> = texts
            .iter()
            .map(|t| ids.compared(None, t).unwrap())
            .collect();
        assert!(ids.retired.len() > 5);
        for (text, h) in texts.iter().zip(handles) {
            assert_eq!(Some(ids.spelled(h)), crate::uri::compared_id(None, text));
        }
        assert!(ids.retired.iter().all(|(_, s)| s.len() <= 1024 * 1024));
        assert!(ids.duplicates().is_empty());
        for i in (0..50_000).step_by(1000) {
            ids.compared(None, &texts[i]).unwrap();
        }
        let d = ids.duplicates();
        assert_eq!(d.len(), 100);
        assert!(d.chunk_by(|a, b| a.0 == b.0).all(|g| g.len() == 2));
    }
    #[test]
    fn a_single_large_identifier_can_exceed_chunk_target_without_losing_text() {
        let first = String::from("s://first/q/a/../b/short/x");
        let big = format!("s://big/q/a/../b/{}/x", "long/".repeat(300_000));
        let tail = String::from("s://tail/q/a/../b/last/x");
        let texts = [first, big, tail];
        let mut ids = Identifiers::new(texts.iter().map(String::len).sum());
        let h: Vec<_> = texts
            .iter()
            .map(|t| ids.compared(None, t).unwrap())
            .collect();
        for (t, h) in texts.iter().zip(h) {
            assert_eq!(Some(ids.spelled(h)), crate::uri::compared_id(None, t));
        }
        assert!(ids.duplicates().is_empty());
    }
}

// Borrowed components already carry their source lifetime; no offset or copy is needed.
fn span<'a>(_input: &'a str, _own: &str, part: &'a str) -> &'a str {
    part
}
