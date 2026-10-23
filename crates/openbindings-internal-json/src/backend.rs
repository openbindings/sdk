//! Shared internal adapter: the schema engine reads immutable flat JSON without re-parsing.
use crate::decimal::Decimal;
use crate::raw::{Arena, Id, Kind, NO_PARENT, Text};
use jsonschema_value::types::JsonType;
use jsonschema_value::{Array, Json, JsonNumber, Node, NodeIdentity, Object};
use serde_json::Value;
use std::borrow::Cow;

/// Charge `units` of this adapter's reads to the default evaluator's current
/// evaluation work allowance; `false` once it is spent. Outside an allowance,
/// and in builds without the evaluator, nothing is counted and reads continue:
/// core's fixed checks, whose work is linear in the admitted document, have no
/// budget.
#[inline]
fn charge(units: usize) -> bool {
    #[cfg(feature = "evaluator-work")]
    {
        crate::work::charge(units)
    }
    #[cfg(not(feature = "evaluator-work"))]
    {
        let _ = units;
        true
    }
}
/// Whether this thread's reads are charged: its pass has an evaluation
/// allowance or a stack guard. Outside both a charge counts nothing and always
/// succeeds. A pass's state is fixed while it runs, and each lookup and
/// uniqueness check runs within one pass, so each reads this once and charges
/// only when it holds, instead of reaching the thread's state for every probe.
/// The member and element iterators charge each read themselves: the library
/// holds one in every frame it recurses through, and one that also held the
/// state would take more stack at every level of the value.
#[inline]
fn counting() -> bool {
    #[cfg(feature = "evaluator-work")]
    {
        crate::work::counting()
    }
    #[cfg(not(feature = "evaluator-work"))]
    {
        false
    }
}

/// Borrow one object member by source position, without visiting preceding members.
/// This internal bridge helper preserves duplicate names and exact name tokens.
pub fn member_at(value: crate::JsonRef<'_>, index: usize) -> Option<crate::JsonMember<'_>> {
    let name = *value.owner.member_names(value.id)?.get(index)? as usize;
    Some(crate::JsonMember {
        name: crate::JsonRef {
            owner: value.owner,
            id: name,
        },
        value: crate::JsonRef {
            owner: value.owner,
            id: name + 1,
        },
    })
}

// Fixed schemas only test ASCII patterns, ASCII constants and nonempty strings.
// Escape the marker itself and encode unpaired units, retaining every UTF-16 unit.
// This presentation is injective; it is not a Unicode replacement operation.
const STRING_MARKER: char = '\u{e000}';
#[derive(Clone, Copy, Default)]
struct StringAccess {
    fixed: bool,
    unsupported: bool,
}
thread_local! {
    static STRING_ACCESS: std::cell::Cell<StringAccess> = const {
        std::cell::Cell::new(StringAccess { fixed: false, unsupported: false })
    };
}
struct StringScope(StringAccess);
impl Drop for StringScope {
    fn drop(&mut self) {
        STRING_ACCESS.set(self.0);
    }
}
/// Run one runtime evaluation, recording only strings actually interpreted by it.
/// Nested calls restore their caller's state, including on unwinding.
pub fn string_scope<T>(evaluate: impl FnOnce() -> T) -> Result<T, UnsupportedString> {
    let _restore = StringScope(STRING_ACCESS.replace(StringAccess::default()));
    let result = evaluate();
    if STRING_ACCESS.get().unsupported {
        Err(UnsupportedString)
    } else {
        Ok(result)
    }
}
#[derive(Debug)]
pub struct UnsupportedString;
/// Use the lossless presentation only for the trusted fixed-schema constraints.
pub fn fixed_strings<T>(evaluate: impl FnOnce() -> T) -> T {
    let _restore = StringScope(STRING_ACCESS.replace(StringAccess {
        fixed: true,
        unsupported: false,
    }));
    evaluate()
}
fn interpreted_unpaired() {
    STRING_ACCESS.with(|cell| {
        let mut state = cell.get();
        if !state.fixed {
            state.unsupported = true;
        }
        cell.set(state);
    });
}
fn present_string(arena: &Arena, id: Id) -> Option<Cow<'_, str>> {
    if let Some(text) = arena.string(id) {
        if !text.contains(STRING_MARKER) {
            return Some(Cow::Borrowed(text));
        }
        let markers = text.matches(STRING_MARKER).count();
        let mut doubled = String::with_capacity(text.len() + markers * STRING_MARKER.len_utf8());
        for (at, piece) in text.split(STRING_MARKER).enumerate() {
            if at > 0 {
                doubled.push(STRING_MARKER);
                doubled.push(STRING_MARKER);
            }
            doubled.push_str(piece);
        }
        return Some(Cow::Owned(doubled));
    }
    let units = arena.unpaired_units(id)?;
    // Sized first, so the presentation is written once.
    let size = char::decode_utf16(units.clone())
        .map(|unit| match unit {
            Ok(STRING_MARKER) => 2 * STRING_MARKER.len_utf8(),
            Ok(c) => c.len_utf8(),
            Err(_) => STRING_MARKER.len_utf8() + 5,
        })
        .sum();
    let mut text = String::with_capacity(size);
    for unit in char::decode_utf16(units) {
        match unit {
            Ok(c) => {
                text.push(c);
                if c == STRING_MARKER {
                    text.push(c);
                }
            }
            Err(unit) => {
                use std::fmt::Write;
                let _ = write!(text, "{STRING_MARKER}{:04x};", unit.unpaired_surrogate());
            }
        }
    }
    Some(Cow::Owned(text))
}
// How the fixed checks read a string. An arena holding no unpaired unit encodes
// none, so each of its strings is read as itself, which is injective already. A
// fixed check compares one arena's strings only with each other and with the
// fixed schemas' ASCII text. The schema space keeps the presentation for every
// arena, since references name its members by it.
fn fixed_presentation(arena: &Arena, id: Id) -> Option<Cow<'_, str>> {
    match arena.string(id) {
        Some(text) if arena.unpaired.is_empty() => Some(Cow::Borrowed(text)),
        _ => present_string(arena, id),
    }
}
#[inline]
fn read_string(arena: &Arena, id: Id) -> Option<Cow<'_, str>> {
    // Without an unpaired unit every string is read as itself, in either
    // presentation, so the pass's string state need not be read.
    if arena.unpaired.is_empty() {
        return arena.string(id).map(Cow::Borrowed);
    }
    presented_string(arena, id)
}
// A string of an arena holding an unpaired unit, as the pass presents it;
// kept out of line so that the readers above inline.
#[cold]
#[inline(never)]
fn presented_string(arena: &Arena, id: Id) -> Option<Cow<'_, str>> {
    if STRING_ACCESS.get().fixed {
        return fixed_presentation(arena, id);
    }
    if let Some(text) = arena.string(id) {
        return Some(Cow::Borrowed(text));
    }
    if matches!(arena.nodes[id].kind, Kind::String { .. }) {
        interpreted_unpaired();
    }
    present_string(arena, id)
}
/// The injective spelling of a string: the marker doubled and unpaired units
/// encoded with it. The schema space names members by it.
pub fn fixed_string(value: crate::JsonRef<'_>) -> Option<Cow<'_, str>> {
    present_string(value.owner, value.id)
}

// The schema libraries call these accessors for every node they visit. Each is
// a few instructions, and both core's library and the evaluator's call them, so
// they carry inline hints: without them, a second set of callers can tip the
// optimizer into calling instead of inlining them on the evaluator's hot paths.
#[derive(Debug)]
pub struct FlatJson;
#[derive(Clone, Copy)]
pub enum View<'a> {
    Stored(&'a Arena, Id),
    Text(&'a str),
}
impl std::fmt::Debug for View<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

impl Json for FlatJson {
    type Node<'a> = View<'a>;
    type PreparedKey = String;
    // A property name is a plain string node over the name's own text.
    type StringBuffer = ();
    const KEYS_PER_LOOKUP: usize = 1;
    fn prepare_key(key: &str) -> String {
        key.to_owned()
    }
    fn with_string_node<T>((): &mut (), text: &str, f: impl FnOnce(View<'_>) -> T) -> T {
        f(View::Text(text))
    }
}
pub struct Number<'a>(&'a str);
impl std::fmt::Debug for Number<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Number").finish_non_exhaustive()
    }
}

impl JsonNumber for Number<'_> {
    #[inline]
    fn as_u64(&self) -> Option<u64> {
        self.0.parse().ok()
    }
    #[inline]
    fn as_i64(&self) -> Option<i64> {
        self.0.parse().ok()
    }
    #[inline]
    fn as_f64(&self) -> Option<f64> {
        self.0.parse::<f64>().ok().filter(|v| v.is_finite())
    }
    #[inline]
    fn as_str(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.0)
    }
    // Opaque cold-path payload, never an authoritative number or SDK report.
    fn to_number(&self) -> Cow<'_, serde_json::Number> {
        Cow::Owned(0.into())
    }
    #[inline]
    fn is_integer(&self) -> bool {
        // The arena has already admitted JSON number syntax. Without a decimal
        // point or exponent, even -0 and tokens longer than i64 are integers.
        if !self.0.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
            return true;
        }
        Decimal::parse(self.0).is_integer()
    }
}
impl<'a> Node<'a, FlatJson> for View<'a> {
    type Object = ObjectView<'a>;
    type Array = ArrayView<'a>;
    type Number = Number<'a>;
    #[inline]
    fn as_object(&self) -> Option<ObjectView<'a>> {
        match self {
            Self::Stored(a, id) => a.member_names(*id).map(|names| ObjectView(a, *id, names)),
            _ => None,
        }
    }
    #[inline]
    fn as_array(&self) -> Option<ArrayView<'a>> {
        match self {
            Self::Stored(a, id) => a.elements(*id).map(|items| ArrayView(a, items)),
            _ => None,
        }
    }
    #[inline]
    fn as_string(&self) -> Option<Cow<'a, str>> {
        match self {
            Self::Text(s) => Some(Cow::Borrowed(s)),
            Self::Stored(a, id) => read_string(a, *id),
        }
    }
    #[inline]
    fn as_number(&self) -> Option<Number<'a>> {
        match self {
            Self::Stored(a, id) if matches!(a.nodes[*id].kind, Kind::Number) => {
                Some(Number(a.raw(*id)))
            }
            _ => None,
        }
    }
    #[inline]
    fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Stored(a, id) => match a.nodes[*id].kind {
                Kind::Bool(b) => Some(b),
                _ => None,
            },
            _ => None,
        }
    }
    #[inline]
    fn is_null(&self) -> bool {
        matches!(self,Self::Stored(a,id) if matches!(a.nodes[*id].kind,Kind::Null))
    }
    // `as_number().is_some()`, without borrowing the token.
    #[inline]
    fn is_number(&self) -> bool {
        matches!(self, Self::Stored(a, id) if matches!(a.nodes[*id].kind, Kind::Number))
    }
    #[inline]
    fn json_type(&self) -> JsonType {
        match self {
            Self::Text(_) => JsonType::String,
            Self::Stored(a, id) => match a.nodes[*id].kind {
                Kind::Null => JsonType::Null,
                Kind::Bool(_) => JsonType::Boolean,
                Kind::Number => JsonType::Number,
                Kind::String { .. } => JsonType::String,
                Kind::Array { .. } => JsonType::Array,
                Kind::Object { .. } => JsonType::Object,
            },
        }
    }
    fn equals_value(&self, expected: &Value) -> bool {
        // Pairs still to compare; comparing scalars allocates nothing.
        let mut stack = Vec::new();
        let mut pair = Some((*self, expected));
        let counting = counting();
        while let Some((left, right)) = pair.take().or_else(|| stack.pop()) {
            if counting && !charge(1) {
                return false;
            }
            match right {
                Value::Null => {
                    if !left.is_null() {
                        return false;
                    }
                }
                Value::Bool(b) => {
                    if left.as_boolean() != Some(*b) {
                        return false;
                    }
                }
                Value::String(s) => {
                    if left.as_string().as_deref() != Some(s) {
                        return false;
                    }
                }
                Value::Number(n) => {
                    if left
                        .as_number()
                        .is_none_or(|m| Decimal::parse(m.0) != Decimal::parse(&n.to_string()))
                    {
                        return false;
                    }
                }
                Value::Array(values) => {
                    let Some(items) = left.as_array() else {
                        return false;
                    };
                    if items.len() != values.len() {
                        return false;
                    }
                    // An element iterator may stop early when work runs out,
                    // so it promises no length; the pairs are counted here.
                    stack.reserve(items.len());
                    stack.extend(items.elements().zip(values));
                }
                Value::Object(members) => {
                    let Some(items) = left.as_object() else {
                        return false;
                    };
                    if items.len() != members.len() {
                        return false;
                    }
                    for (k, v) in members {
                        let Some(item) = items.get(k) else {
                            return false;
                        };
                        stack.push((item, v));
                    }
                }
            }
        }
        true
    }
    // Documented cold-path payload only. The SDK never exposes or formats it.
    // Equality, uniqueness, integer classification and numeric constraints use
    // the exact overrides; schema facts are recovered from original sources.
    // Redact the entire value rather than materializing unrepresentable numbers.
    fn to_value(&self) -> Cow<'a, Value> {
        Cow::Borrowed(&Value::Null)
    }
    #[inline]
    fn identity(&self) -> Option<NodeIdentity> {
        match self {
            Self::Stored(a, id) => Some(NodeIdentity::tagged(
                std::ptr::from_ref(*a) as usize,
                *id as u32,
            )),
            Self::Text(_) => None,
        }
    }
}
/// An object, its node and its member-name tokens.
pub struct ObjectView<'a>(&'a Arena, Id, &'a [u32]);
impl std::fmt::Debug for ObjectView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectView").finish_non_exhaustive()
    }
}

pub struct Members<'a>(&'a Arena, std::slice::Iter<'a, u32>);
impl std::fmt::Debug for Members<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Members").finish_non_exhaustive()
    }
}

// Iterating values need not interpret their member names. Mark unsupported
// access only when the engine requests the scalar name (matching or locating it).
pub struct MemberName<'a> {
    text: Cow<'a, str>,
    unpaired: bool,
}
impl std::fmt::Debug for MemberName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemberName").finish_non_exhaustive()
    }
}

impl AsRef<str> for MemberName<'_> {
    fn as_ref(&self) -> &str {
        if self.unpaired {
            interpreted_unpaired();
        }
        &self.text
    }
}
impl<'a> From<MemberName<'a>> for Cow<'a, str> {
    fn from(name: MemberName<'a>) -> Self {
        if name.unpaired {
            interpreted_unpaired();
        }
        name.text
    }
}
impl<'a> Iterator for Members<'a> {
    type Item = (MemberName<'a>, View<'a>);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if !charge(1) {
            return None;
        }
        self.1.next().map(|&name| {
            let name = name as usize;
            let scalar = self.0.string(name);
            (
                MemberName {
                    text: match scalar {
                        // Without an unpaired unit in the arena a name is
                        // presented as itself either way.
                        Some(text) if self.0.unpaired.is_empty() => Cow::Borrowed(text),
                        _ => presented_name(self.0, name, scalar),
                    },
                    unpaired: scalar.is_none(),
                },
                // A member's value is the node after its name.
                View::Stored(self.0, name + 1),
            )
        })
    }
}
// A member name of an arena holding an unpaired unit, as the pass presents
// it; kept out of line so that the iterator inlines.
#[cold]
#[inline(never)]
fn presented_name<'a>(arena: &'a Arena, name: Id, scalar: Option<&'a str>) -> Cow<'a, str> {
    if STRING_ACCESS.get().fixed {
        return fixed_presentation(arena, name).expect("member name is a string");
    }
    match scalar {
        Some(text) => Cow::Borrowed(text),
        None => present_string(arena, name).expect("member name is a string"),
    }
}
impl<'a> Object<'a, FlatJson> for ObjectView<'a> {
    type Node = View<'a>;
    type MemberName = MemberName<'a>;
    type MembersIter = Members<'a>;
    #[inline]
    fn len(&self) -> usize {
        self.2.len()
    }
    #[inline]
    fn get(&self, key: &String) -> Option<View<'a>> {
        // Share the arena index and charge every collision candidate. An
        // empty object answers without a charge.
        if self.2.is_empty() {
            return None;
        }
        let counting = counting();
        self.0
            .get_with_probe(self.1, key, || !counting || charge(1))
            .map(|id| View::Stored(self.0, id))
    }
    #[inline]
    fn members(&self) -> Members<'a> {
        Members(self.0, self.2.iter())
    }
}
pub struct ArrayView<'a>(&'a Arena, &'a [u32]);
impl std::fmt::Debug for ArrayView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArrayView").finish_non_exhaustive()
    }
}

pub struct Elements<'a>(&'a Arena, std::slice::Iter<'a, u32>);
impl std::fmt::Debug for Elements<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Elements").finish_non_exhaustive()
    }
}

impl<'a> Iterator for Elements<'a> {
    type Item = View<'a>;
    #[inline]
    fn next(&mut self) -> Option<View<'a>> {
        if !charge(1) {
            return None;
        }
        self.1.next().map(|&id| View::Stored(self.0, id as usize))
    }
}
impl<'a> Array<'a, FlatJson> for ArrayView<'a> {
    type Node = View<'a>;
    type ElementsIter = Elements<'a>;
    #[inline]
    fn len(&self) -> usize {
        self.1.len()
    }
    #[inline]
    fn elements(&self) -> Elements<'a> {
        Elements(self.0, self.1.iter())
    }
    fn is_unique(&self) -> bool {
        let Some(&first) = self.1.first() else {
            return true;
        };
        let first = first as usize;
        let end = subtree_end(self.0, *self.1.last().unwrap() as usize);
        let counting = counting();
        // One element is unique; its nodes are charged as hashing them would
        // charge each, which leaves the allowance as one charge of them all.
        if self.1.len() == 1 {
            return !counting || charge(end - first);
        }
        // Scalars only: no element is a container, empty ones included.
        if self.1.len() <= SMALL
            && end - first == self.1.len()
            && !self
                .1
                .iter()
                .any(|&id| self.0.nodes[id as usize].is_container())
        {
            return self.scalars_unique(counting);
        }
        self.hashed_unique(first, end, counting)
    }
}
/// Arrays of at most this many elements, or subtrees of this many nodes,
/// check uniqueness without allocating.
const SMALL: usize = 16;
// A scalar element as exact equality compares it: numbers by value, strings
// by their UTF-16 units, which for scalar text is the text and otherwise the
// string's own units.
#[derive(PartialEq)]
enum Scalar<'a> {
    Null,
    Bool(bool),
    Number(Decimal),
    Text(&'a str),
    // A string holding an unpaired unit, by its node; never equal to `Text`.
    Units(Id),
}
// The id after `id`'s subtree. A subtree's ids are contiguous in source order,
// so it ends after its last descendant, found down the chain of last items.
fn subtree_end(arena: &Arena, mut id: Id) -> Id {
    loop {
        if let Some(&item) = arena.elements(id).and_then(<[u32]>::last) {
            id = item as usize;
        } else if let Some(&name) = arena.member_names(id).and_then(<[u32]>::last) {
            // A member's value is the node after its name.
            id = name as usize + 1;
        } else {
            return id + 1;
        }
    }
}
impl ArrayView<'_> {
    // Hash every node of the elements, `first..end`, charging each, and
    // compare the elements whose hashes repeat. Each check is its own frame,
    // so a check holds only its own table on the stack.
    #[inline(never)]
    fn hashed_unique(&self, first: Id, end: Id, counting: bool) -> bool {
        use std::{collections::hash_map::RandomState, hash::BuildHasher};
        let hasher = RandomState::new();
        // A small array's hashes are kept on the stack.
        let (mut inline, mut held) = ([0u64; SMALL], Vec::new());
        let hashes: &mut [u64] = if end - first <= SMALL {
            &mut inline[..end - first]
        } else {
            held.resize(end - first, 0);
            &mut held
        };
        // One buffer each for every container's item hashes, reused: hashing
        // the slice hashes what hashing the collected vector did.
        let (mut items, mut pairs) = (Vec::new(), Vec::new());
        for id in (first..end).rev() {
            if counting && !charge(1) {
                return false;
            }
            let hash =
                match self.0.nodes[id].kind {
                    Kind::Null => hasher.hash_one((0u8, 0u8)),
                    Kind::Bool(b) => hasher.hash_one((1u8, b)),
                    Kind::Number => hasher.hash_one((2u8, Decimal::parse(self.0.raw(id)))),
                    Kind::String {
                        text: Text::UNPAIRED,
                        ..
                    } => hasher.hash_one((3u8, 1u8, self.0.units(id))),
                    Kind::String { .. } => hasher.hash_one((3u8, 0u8, self.0.string(id))),
                    Kind::Array { .. } => {
                        items.clear();
                        items.extend(
                            self.0
                                .elements(id)
                                .expect("an array")
                                .iter()
                                .map(|&i| hashes[i as usize - first]),
                        );
                        hasher.hash_one((4u8, items.as_slice()))
                    }
                    Kind::Object { .. } => {
                        pairs.clear();
                        pairs.extend(self.0.member_names(id).expect("an object").iter().map(
                            |&name| {
                                let name = name as usize;
                                (hashes[name - first], hashes[name + 1 - first])
                            },
                        ));
                        pairs.sort_unstable();
                        hasher.hash_one((5u8, pairs.as_slice()))
                    }
                };
            hashes[id - first] = hash;
        }
        let hashes = &*hashes;
        self.unique_by_hash(&|at| hashes[self.1[at] as usize - first], counting)
    }
    // A small array of scalars, compared directly. Scalars hash alike exactly
    // when they are equal, so the hash check charges each element, finds the
    // first element equal to an earlier one and compares it with its first
    // equal predecessor, its first candidate; these charges are the same.
    // (Keyed 64-bit hashes of two unequal scalars, which would add a
    // candidate, collide with probability 2^-64.)
    #[inline(never)]
    fn scalars_unique(&self, counting: bool) -> bool {
        if counting && !charge(self.1.len()) {
            return false;
        }
        // Each element read once, as exact equality reads it.
        let mut scalars: [Scalar<'_>; SMALL] = std::array::from_fn(|_| Scalar::Null);
        for (scalar, &id) in scalars.iter_mut().zip(self.1) {
            let id = id as usize;
            *scalar = match self.0.nodes[id].kind {
                Kind::Null => Scalar::Null,
                Kind::Bool(b) => Scalar::Bool(b),
                Kind::Number => Scalar::Number(Decimal::parse(self.0.raw(id))),
                Kind::String { .. } => match self.0.string(id) {
                    Some(text) => Scalar::Text(text),
                    None => Scalar::Units(id),
                },
                Kind::Array { .. } | Kind::Object { .. } => unreachable!("only scalars"),
            };
        }
        let same = |left: &Scalar<'_>, right: &Scalar<'_>| match (left, right) {
            (Scalar::Units(left), Scalar::Units(right)) => {
                let units = |id: &Id| self.0.unpaired_units(*id).expect("an unpaired string");
                units(left).eq(units(right))
            }
            _ => left == right,
        };
        for at in 1..self.1.len() {
            if let Some(prior) = (0..at).find(|&prior| same(&scalars[at], &scalars[prior])) {
                return self
                    .repeats(at, std::iter::once(prior), counting)
                    .unwrap_or(false);
            }
        }
        true
    }
    // Hashes only select candidates: an element is compared, in array order,
    // with every earlier element of its hash, each comparison charged and
    // verified exactly. Equal elements hash alike, but unequal ones may too:
    // objects holding one name twice, which equality reads by the name's first
    // occurrence while the hash counts every member, and collisions of keyed
    // 64-bit hashes. While no hash repeats, only the hashes seen are kept. The
    // first repeat finds its candidates by one scan; unless it ends the check,
    // the elements are then grouped by hash once, so that every later
    // element's candidates are listed directly, for work linear in the
    // comparisons charged. The table grows as usual while small, so an early
    // repeat costs little; past that it grows once, to hold every element
    // left, rather than keeping old and new tables while doubling.
    // `hash_of` reads the hash of the element at a position.
    fn unique_by_hash(&self, hash_of: &impl Fn(usize) -> u64, counting: bool) -> bool {
        const DOUBLING: usize = 4096;
        // A small array finds its first repeated hash by scanning the earlier
        // ones, as the table would find it.
        if self.1.len() <= SMALL {
            for at in 1..self.1.len() {
                let hash = hash_of(at);
                if (0..at).any(|prior| hash_of(prior) == hash) {
                    let earlier = (0..at).filter(|&prior| hash_of(prior) == hash);
                    return match self.repeats(at, earlier, counting) {
                        Some(unique) => unique,
                        None => self.unique_by_groups(at + 1, hash_of, counting),
                    };
                }
            }
            return true;
        }
        let mut seen = std::collections::HashSet::new();
        for at in 0..self.1.len() {
            let hash = hash_of(at);
            if seen.len() == seen.capacity() && seen.len() >= DOUBLING {
                seen.reserve(self.1.len() - at);
            }
            if seen.insert(hash) {
                continue;
            }
            drop(seen);
            let earlier = (0..at).filter(|&prior| hash_of(prior) == hash);
            return match self.repeats(at, earlier, counting) {
                Some(unique) => unique,
                None => self.unique_by_groups(at + 1, hash_of, counting),
            };
        }
        true
    }
    // Compare the element at `at` with each of `earlier`, charging each when
    // `counting`: an answer when an equal pair or exhausted work ends the check.
    fn repeats(
        &self,
        at: usize,
        earlier: impl Iterator<Item = usize>,
        counting: bool,
    ) -> Option<bool> {
        let element = View::Stored(self.0, self.1[at] as usize);
        for prior in earlier {
            if counting && !charge(1) {
                return Some(false);
            }
            if equals_charged(
                element,
                View::Stored(self.0, self.1[prior] as usize),
                counting,
            ) {
                return Some(false);
            }
        }
        None
    }
    // Every element from `from` on, compared with the earlier elements of its
    // hash, which positions sorted by hash list in array order.
    fn unique_by_groups(
        &self,
        from: usize,
        hash_of: &impl Fn(usize) -> u64,
        counting: bool,
    ) -> bool {
        let count = self.1.len();
        let mut order: Vec<u32> = (0..count as u32).collect();
        order.sort_unstable_by_key(|&at| (hash_of(at as usize), at));
        let mut rank = vec![0u32; count];
        for (index, &at) in order.iter().enumerate() {
            rank[at as usize] = index as u32;
        }
        for (at, &index) in rank.iter().enumerate().skip(from) {
            let index = index as usize;
            let hash = hash_of(at);
            // The group's start is found by stepping over the candidates.
            let start = order[..index]
                .iter()
                .rposition(|&prior| hash_of(prior as usize) != hash)
                .map_or(0, |before| before + 1);
            let earlier = order[start..index].iter().map(|&prior| prior as usize);
            if let Some(unique) = self.repeats(at, earlier, counting) {
                return unique;
            }
        }
        true
    }
}

/// Borrow an admitted exact value without parsing or allocating.
pub fn view(value: &crate::JsonValue) -> View<'_> {
    View::Stored(&value.owner, value.id)
}
/// Build only the JSON Pointer; source line/column scanning is unnecessary here.
pub fn pointer(value: &crate::JsonValue) -> Option<String> {
    value.owner.pointer(value.id)
}
/// Append the JSON Pointer [`pointer()`] builds to `out` without another
/// allocation; `false`, appending nothing, when it is unavailable.
pub fn pointer_into(value: &crate::JsonValue, out: &mut Vec<u8>) -> bool {
    value.owner.pointer_into(value.id, out)
}
/// Build only the exact JSON Pointer from the source's root, without line and
/// column, including names that hold unpaired units.
pub fn exact_pointer(value: crate::JsonRef<'_>) -> crate::Pointer {
    value.owner.exact_pointer(value.id)
}
/// Wrap escaped RFC 6901 text built from original sources, without checking it
/// again.
pub fn escaped_pointer(text: String) -> crate::Pointer {
    crate::Pointer::escaped(text)
}
/// Read a pointer from an exact JSON string, keeping unpaired units. `None` for
/// a non-string or text that is not an RFC 6901 pointer.
pub fn string_pointer(value: crate::JsonRef<'_>) -> Option<crate::Pointer> {
    crate::Pointer::parse_units(&value.utf16_units()?)
}
/// The exact pointer of `value` from its ancestor `root`, a value's own frame.
pub fn relative_pointer(value: crate::JsonRef<'_>, root: crate::JsonRef<'_>) -> crate::Pointer {
    debug_assert!(std::sync::Arc::ptr_eq(value.owner, root.owner));
    value.owner.exact_pointer_from(value.id, root.id)
}
/// Append escaped RFC 6901 text, such as a keyword's `/minimum`.
pub fn push_pointer(pointer: &mut crate::Pointer, escaped: &str) {
    pointer.push_escaped(escaped);
}
/// The bytes a pointer retains: its UTF-8 text, or two per UTF-16 unit.
pub fn pointer_bytes(pointer: &crate::Pointer) -> usize {
    pointer.size()
}
/// Whether two views read the same source.
pub fn same_source(value: crate::JsonRef<'_>, source: &crate::JsonValue) -> bool {
    std::sync::Arc::ptr_eq(value.owner, &source.owner)
}
/// Measure separately owned decoded UTF-8 strings in a borrowed exact subtree,
/// including member names. Raw source bytes are excluded. Unescaped strings borrow
/// their source; escaped scalar strings own their complete decoded value. This
/// visits the existing flat nodes without parsing or allocating. `None` means the
/// byte limit is exceeded or the subtree contains non-scalar UTF-16 strings.
pub fn decoded_string_size(value: crate::JsonRef<'_>, limit: usize) -> Option<usize> {
    let arena = value.owner.as_ref();
    let end = arena.nodes[value.id].span.end;
    let mut size = 0usize;
    for (offset, node) in arena.nodes[value.id..]
        .iter()
        .take_while(|node| node.span.start < end)
        .enumerate()
    {
        if let Kind::String { text, .. } = node.kind {
            if text == Text::UNPAIRED {
                return None;
            }
            if text.decoded().is_some() {
                size = size.checked_add(arena.decoded_len(value.id + offset))?;
                if size > limit {
                    return None;
                }
            }
        }
    }
    Some(size)
}
/// Measure an escaped source pointer before allocating it. `None` means that
/// the pointer is unavailable or would exceed the caller's remaining byte budget.
pub fn pointer_size(value: crate::JsonRef<'_>, limit: usize) -> Option<usize> {
    use crate::raw::Edge;
    let mut size = 0usize;
    let mut current = value.id;
    while let Some((parent, edge)) = value.owner.edge(current) {
        let remaining = limit.checked_sub(size)?.checked_sub(1)?;
        let segment = match edge {
            Edge::Index(index) => {
                if index == 0 {
                    1
                } else {
                    index.ilog10() as usize + 1
                }
            }
            Edge::Key(key) => {
                let name = value.owner.string(key)?;
                if name.len() > remaining {
                    return None;
                }
                name.len() + name.bytes().filter(|c| matches!(c, b'~' | b'/')).count()
            }
        };
        if segment > remaining {
            return None;
        }
        size += 1 + segment;
        current = parent;
    }
    Some(size)
}

/// Bound the bytes a retained location's exact pointer keeps before
/// constructing it: its UTF-8 bytes, or two bytes per UTF-16 unit when an
/// ancestor's name holds an unpaired unit. None means the budget is exceeded.
pub fn location_pointer_size(value: crate::JsonRef<'_>, limit: usize) -> Option<usize> {
    if let Some(size) = pointer_size(value, limit) {
        return Some(size);
    }
    // Distinguish a pointer through an unpaired name, held as units, from an
    // oversized one without allocating either. This second walk is needed only
    // after the ordinary bounded measure failed; normal locations take one walk.
    // It too stops once the units exceed the budget, so that a document whose
    // budget is spent pays nothing more for each further deep finding.
    let escaped = |unit: u16| unit == u16::from(b'~') || unit == u16::from(b'/');
    let budget = limit / 2;
    let (mut units, mut unpaired) = (0usize, false);
    let mut current = value.id;
    while let Some((parent, edge)) = value.owner.edge(current) {
        units += 1;
        if units > budget {
            return None;
        }
        let within = match edge {
            crate::raw::Edge::Index(index) => {
                units += index
                    .checked_ilog10()
                    .map_or(1, |digits| digits as usize + 1);
                units <= budget
            }
            crate::raw::Edge::Key(key) => {
                let mut count = |unit: u16| {
                    units += 1 + usize::from(escaped(unit));
                    units <= budget
                };
                match value.owner.unpaired_units(key) {
                    Some(mut name) => {
                        unpaired = true;
                        name.all(&mut count)
                    }
                    None => value.owner.string(key)?.encode_utf16().all(&mut count),
                }
            }
        };
        if !within {
            return None;
        }
        current = parent;
    }
    unpaired.then_some(units * 2)
}

/// Every node's [`location_pointer_size`], for a caller that measures many
/// locations of one source, measured in one pass over the source's nodes.
#[derive(Debug)]
pub struct LocationPointerSizes {
    /// Pointer bytes, saturating at the cap; `u32::MAX` through an unpaired name.
    scalar: Vec<u32>,
    /// Escaped UTF-16 units, saturating at the cap; only when the source holds
    /// an unpaired unit.
    units: Option<Vec<u32>>,
}
impl LocationPointerSizes {
    /// Measure every node of `value`'s source, up to `cap` bytes.
    pub fn new(value: crate::JsonRef<'_>, cap: usize) -> Self {
        let arena = value.owner.as_ref();
        let cap = u32::try_from(cap).unwrap_or(u32::MAX - 1).min(u32::MAX - 1);
        let add = |size: u32, segment: usize| {
            u32::try_from(segment).map_or(cap, |segment| size.saturating_add(segment).min(cap))
        };
        let escaped =
            |unit: u16| 1 + usize::from(unit == u16::from(b'~') || unit == u16::from(b'/'));
        let count = arena.nodes.len();
        let mut scalar = vec![0u32; count];
        let mut units = (!arena.unpaired.is_empty()).then(|| vec![0u32; count]);
        // Ids follow the source, so a container precedes its items.
        for id in 0..count {
            if let Some(elements) = arena.elements(id) {
                for (index, &element) in elements.iter().enumerate() {
                    let segment = 1 + index.checked_ilog10().map_or(1, |d| d as usize + 1);
                    let element = element as usize;
                    scalar[element] = match scalar[id] {
                        u32::MAX => u32::MAX,
                        size => add(size, segment),
                    };
                    if let Some(units) = &mut units {
                        units[element] = add(units[id], segment);
                    }
                }
            } else if let Some(names) = arena.member_names(id) {
                for &name in names {
                    let name = name as usize;
                    let text = arena.string(name);
                    let size = match (scalar[id], text) {
                        (u32::MAX, _) | (_, None) => u32::MAX,
                        (size, Some(text)) => add(size, 1 + crate::raw::escaped_len(text)),
                    };
                    // A member's value is the node after its name.
                    scalar[name] = size;
                    scalar[name + 1] = size;
                    if let Some(units) = &mut units {
                        let segment = 1 + match arena.unpaired_units(name) {
                            Some(name) => name.map(escaped).sum::<usize>(),
                            None => text
                                .into_iter()
                                .flat_map(str::encode_utf16)
                                .map(escaped)
                                .sum(),
                        };
                        let size = add(units[id], segment);
                        units[name] = size;
                        units[name + 1] = size;
                    }
                }
            }
        }
        Self { scalar, units }
    }
    /// [`location_pointer_size`] of `value`, a node of the measured source,
    /// for a `limit` below the cap.
    pub fn size(&self, value: crate::JsonRef<'_>, limit: usize) -> Option<usize> {
        match self.scalar[value.id] {
            u32::MAX => {
                let units = self.units.as_ref().expect("an unpaired name was measured")[value.id];
                let size = 2 * units as usize;
                (size <= limit).then_some(size)
            }
            size => (size as usize <= limit).then_some(size as usize),
        }
    }
}

/// The fixed phrase an [`crate::InputError`] code reports, which its `Display`
/// writes with the byte; `None` for a code the parser never emits.
pub fn input_error_description(code: &str) -> Option<&'static str> {
    crate::json::input_error_description(code)
}
/// The [`crate::JsonLimits`] field that raises a limit code's limit, in Rust and
/// as TypeScript's `limits` option names it; `None` for any other code.
pub fn limit_setting(code: &str) -> Option<(&'static str, &'static str)> {
    crate::json::limit_setting(code)
}
/// The location of an admission failure in the bytes it refused: no pointer,
/// since no value was parsed, at `byte_offset` clamped to the input, with the
/// line and UTF-8 byte column counted over the bytes before it. The bytes need
/// not be UTF-8.
pub fn input_location(input: &[u8], byte_offset: usize) -> crate::SourceLocation {
    let byte_offset = byte_offset.min(input.len());
    let prefix = &input[..byte_offset];
    crate::SourceLocation {
        pointer: None,
        byte_offset,
        line: 1 + prefix.iter().filter(|&&b| b == b'\n').count(),
        byte_column: 1 + prefix.iter().rev().take_while(|&&b| b != b'\n').count(),
    }
}

/// An admission refusal decided before any byte is parsed, such as an edit
/// batch that already exceeds a limit.
pub fn input_error(
    kind: crate::InputErrorKind,
    code: &'static str,
    byte_offset: usize,
) -> crate::InputError {
    crate::InputError {
        kind,
        byte_offset,
        code,
        route: crate::json::Route::Admission,
    }
}
/// The same refusal from a value check that admits under the default limits
/// only, so an exceeded limit names parsing with limits, then validating.
pub fn value_check_refusal(error: crate::InputError) -> crate::InputError {
    crate::InputError {
        route: crate::json::Route::Value,
        ..error
    }
}

/// Locate a bounded diagnostic batch while scanning each arena's source prefix
/// once. Sorting is validation-local; healthy documents build no line index.
/// Output order matches input order, including repeated nodes and mixed arenas.
pub fn locations(values: &[crate::JsonRef<'_>]) -> Vec<crate::SourceLocation> {
    locations_with_work(values).0
}
fn locations_with_work(values: &[crate::JsonRef<'_>]) -> (Vec<crate::SourceLocation>, usize) {
    let mut order: Vec<_> = (0..values.len()).collect();
    order.sort_unstable_by_key(|&index| {
        let value = values[index];
        (
            std::sync::Arc::as_ptr(value.owner),
            value.owner.nodes[value.id].span.start,
        )
    });
    let mut result = vec![None; values.len()];
    let mut previous: Option<&Arena> = None;
    let (mut cursor, mut line, mut line_start, mut scanned) = (0, 1, 0, 0);
    for index in order {
        let value = values[index];
        let arena = value.owner.as_ref();
        if !previous.is_some_and(|old| std::ptr::eq(old, arena)) {
            (cursor, line, line_start) = (0, 1, 0);
            previous = Some(arena);
        }
        let byte_offset = arena.nodes[value.id].span.start;
        for (relative, &byte) in arena.source().as_bytes()[cursor..byte_offset]
            .iter()
            .enumerate()
        {
            if byte == b'\n' {
                line += 1;
                line_start = cursor + relative + 1;
            }
        }
        scanned += byte_offset - cursor;
        cursor = byte_offset;
        result[index] = Some(crate::SourceLocation {
            pointer: Some(arena.exact_pointer(value.id)),
            byte_offset,
            line,
            byte_column: byte_offset - line_start + 1,
        });
    }
    (
        result
            .into_iter()
            .map(|at| at.expect("each input index is visited once"))
            .collect(),
        scanned,
    )
}
// Node ids are assigned in source order and a subtree's ids are contiguous,
// so the subtree's unpaired strings are one slice of the sorted id list,
// found in logarithmic time in the number of unpaired strings.
fn unpaired_within(value: &crate::JsonValue) -> &[u32] {
    let (arena, start) = (value.owner.as_ref(), value.id);
    let end = arena.nodes[start].span.end;
    let probe = |within: bool| {
        #[cfg(test)]
        arena
            .unpaired_query_probes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        within
    };
    let first = arena
        .unpaired
        .partition_point(|&id| probe((id as usize) < start));
    let last = first
        + arena.unpaired[first..]
            .partition_point(|&id| probe(arena.nodes[id as usize].span.start < end));
    &arena.unpaired[first..last]
}
pub fn has_unpaired(value: &crate::JsonValue) -> bool {
    !unpaired_within(value).is_empty()
}

/// Borrow string and member-name tokens containing unpaired UTF-16 units.
/// The exact parser retains these for values and draft inspection; document
/// conformance applies its own OBI-01 exclusion at the original token locations.
pub fn unpaired_strings(value: &crate::JsonValue) -> impl Iterator<Item = crate::JsonRef<'_>> {
    unpaired_within(value).iter().map(|&id| crate::JsonRef {
        owner: &value.owner,
        id: id as usize,
    })
}
/// Stack-safe exact comparison, charged to the enclosing evaluator work scope.
pub fn equals(left: View<'_>, right: View<'_>) -> bool {
    // Every step reaches the charge, which counts nothing outside a pass and
    // whose access to the thread's state the loop holds once: reading the
    // state up front as well would only add to each comparison.
    equals_charged(left, right, true)
}
// The comparison, charging each step when `counting`.
fn equals_charged(left: View<'_>, right: View<'_>, counting: bool) -> bool {
    // Pairs still to compare; comparing scalars allocates nothing.
    let mut stack = Vec::new();
    let mut pair = Some((left, right));
    while let Some((left, right)) = pair.take().or_else(|| stack.pop()) {
        if counting && !charge(1) {
            return false;
        }
        // One kind decides the comparison for both sides.
        let kind = left.json_type();
        if kind != right.json_type() {
            return false;
        }
        if kind == JsonType::Array {
            let (items, other) = (left.as_array().unwrap(), right.as_array().unwrap());
            if items.len() != other.len() {
                return false;
            }
            stack.reserve(items.len());
            stack.extend(items.elements().zip(other.elements()));
        } else if kind == JsonType::Object {
            let (items, other) = (left.as_object().unwrap(), right.as_object().unwrap());
            if items.len() != other.len() {
                return false;
            }
            // Exact object equality does not round-trip names through the scalar
            // lookup API. In particular, an encoded surrogate cannot alias a
            // genuine scalar member name containing the presentation marker.
            for &name in items.2 {
                let name = name as usize;
                if counting && !charge(1) {
                    return false;
                }
                // Names compare by their exact UTF-16 units (§5), so a name with
                // an unpaired unit decides equality without interpretation. A
                // scalar name is looked up as `Object::get` looks it up, each
                // probe charged, without copying it.
                let matched = if let Some(key) = items.0.string(name) {
                    other
                        .0
                        .get_with_probe(other.1, key, || !counting || charge(1))
                        .map(|id| View::Stored(other.0, id))
                } else {
                    let key = items
                        .0
                        .unpaired_units(name)
                        .expect("a name without scalar text");
                    let mut matched = None;
                    for &candidate in other.2 {
                        let candidate = candidate as usize;
                        if counting && !charge(1) {
                            return false;
                        }
                        // A scalar name's units are never those of one holding
                        // an unpaired unit.
                        if other
                            .0
                            .unpaired_units(candidate)
                            .is_some_and(|units| units.eq(key.clone()))
                        {
                            matched = Some(View::Stored(other.0, candidate + 1));
                            break;
                        }
                    }
                    matched
                };
                let Some(other) = matched else {
                    return false;
                };
                stack.push((View::Stored(items.0, name + 1), other));
            }
        } else if kind == JsonType::Number {
            let (n, other) = (left.as_number().unwrap(), right.as_number().unwrap().0);
            // Parsing both tokens as i64 proves exact representability, including
            // -0 == 0. Mixed, fractional, exponent and wider pairs use Decimal.
            let same = match (n.0.parse::<i64>(), other.parse::<i64>()) {
                (Ok(left), Ok(right)) => left == right,
                _ => Decimal::parse(n.0) == Decimal::parse(other),
            };
            if !same {
                return false;
            }
        } else if kind == JsonType::String {
            fn scalar(value: View<'_>) -> Option<&str> {
                match value {
                    View::Stored(arena, id) => arena.string(id),
                    View::Text(text) => Some(text),
                }
            }
            fn unpaired(value: View<'_>) -> Option<crate::raw::Utf16<'_>> {
                match value {
                    View::Stored(arena, id) => arena.unpaired_units(id),
                    View::Text(_) => None,
                }
            }
            // Strings are equal exactly when their UTF-16 units are (§5). A
            // string with an unpaired unit never equals a scalar one, and two
            // such strings compare their units, so neither needs interpretation.
            match (scalar(left), scalar(right)) {
                (Some(a), Some(b)) => {
                    if a != b {
                        return false;
                    }
                }
                _ => {
                    let same = match (unpaired(left), unpaired(right)) {
                        (Some(left), Some(right)) => left.eq(right),
                        (left, right) => left.is_none() && right.is_none(),
                    };
                    if !same {
                        return false;
                    }
                }
            }
        } else if left.as_boolean() != right.as_boolean() {
            return false;
        }
    }
    true
}

pub fn node_id(value: &crate::JsonValue) -> usize {
    value.id
}
/// The arena position of a borrowed value, for a later [`node`] lookup.
pub fn ref_id(value: crate::JsonRef<'_>) -> usize {
    value.id
}
/// Borrow the value at an arena position recorded by [`ref_id`] from any value
/// sharing that arena. `None` when the position does not exist.
pub fn node(value: &crate::JsonValue, id: usize) -> Option<crate::JsonRef<'_>> {
    (id < value.owner.nodes.len()).then_some(crate::JsonRef {
        owner: &value.owner,
        id,
    })
}
/// Rebase a retained child when it becomes an independent document resource.
pub fn standalone(value: crate::JsonValue) -> crate::JsonValue {
    if value.id == 0 {
        return value;
    }
    crate::JsonValue::parse_with_limits(
        value.bytes(),
        crate::JsonLimits {
            max_bytes: usize::MAX,
            max_depth: usize::MAX,
            max_nodes: u32::MAX as usize,
        },
    )
    .expect("an admitted value's source span is a complete JSON value")
}
/// Copy retained values into one new arena that holds exactly their texts, in
/// order, and return each as a view of it. Values kept together this way keep
/// nothing else of the arenas they came from, at one parse for all of them.
pub fn standalone_list(values: &[crate::JsonRef<'_>]) -> Vec<crate::JsonValue> {
    if values.is_empty() {
        return Vec::new();
    }
    let size = values
        .iter()
        .map(|value| value.text().len() + 1)
        .sum::<usize>()
        + 1;
    let mut text = String::with_capacity(size);
    for value in values {
        text.push(if text.is_empty() { '[' } else { ',' });
        text.push_str(value.text());
    }
    text.push(']');
    let list = crate::JsonValue::parse_with_limits(
        text,
        crate::JsonLimits {
            max_bytes: usize::MAX,
            max_depth: usize::MAX,
            max_nodes: u32::MAX as usize,
        },
    )
    .expect("admitted values' source spans are complete JSON values");
    let copies: Vec<_> = list
        .view()
        .elements()
        .expect("a list")
        .map(crate::JsonRef::to_owned)
        .collect();
    debug_assert_eq!(copies.len(), values.len());
    copies
}
/// Borrow duplicate-containing objects in parse order without materializing
/// pointers or coordinates. Callers cap this iterator before collecting nodes.
/// Document validation passes a standalone root, so this covers its whole arena.
pub fn duplicate_nodes(
    value: &crate::JsonValue,
) -> impl ExactSizeIterator<Item = crate::JsonRef<'_>> {
    value
        .owner
        .duplicates
        .iter()
        .map(|&(id, _)| crate::JsonRef {
            owner: &value.owner,
            id: id as usize,
        })
}

/// Borrow each offending repeated member-name token in parse order. Distinct
/// occurrences can share a JSON Pointer but retain distinct original byte spans.
/// This exposes the parser's existing key identities without rescanning objects.
pub fn duplicate_member_names(
    value: &crate::JsonValue,
) -> impl ExactSizeIterator<Item = crate::JsonRef<'_>> {
    value
        .owner
        .duplicates
        .iter()
        .map(|&(_, key)| crate::JsonRef {
            owner: &value.owner,
            id: key as usize,
        })
}

/// Count a value's nodes, including itself and its member-name tokens: the
/// measure [`crate::JsonLimits::max_nodes`] admits. Constant time for a parsed
/// root; a subtree's ids are contiguous in source order, so otherwise this is
/// one logarithmic search.
pub fn node_count(value: crate::JsonRef<'_>) -> usize {
    if value.id == 0 {
        return value.owner.nodes.len();
    }
    let nodes = &value.owner.nodes[value.id..];
    let end = nodes[0].span.end;
    nodes.partition_point(|node| node.span.start < end)
}

/// Whether [`depth`] is at most `limit`, without allocating for a value that
/// nests at most 32 levels, and without reading past the first container
/// deeper than `limit`.
pub fn depth_at_most(value: &crate::JsonValue, limit: usize) -> bool {
    const INLINE: usize = 32;
    let nodes = &value.owner.nodes[value.id..];
    let end = nodes[0].span.end;
    // The span ends of the containers enclosing the current node, outermost
    // first; the first `INLINE` live on the stack.
    let mut inline = [0usize; INLINE];
    let mut spilled = Vec::new();
    let mut open = 0usize;
    for node in nodes.iter().take_while(|node| node.span.start < end) {
        while open > 0 {
            let enclosing = if open <= INLINE {
                inline[open - 1]
            } else {
                spilled[open - 1 - INLINE]
            };
            if node.span.start < enclosing {
                break;
            }
            open -= 1;
        }
        if node.is_container() {
            if open == limit {
                return false;
            }
            if open < INLINE {
                inline[open] = node.span.end;
            } else if spilled.len() > open - INLINE {
                spilled[open - INLINE] = node.span.end;
            } else {
                spilled.push(node.span.end);
            }
            open += 1;
        }
    }
    true
}

pub fn depth(value: &crate::JsonValue) -> usize {
    let span = &value.owner.nodes[value.id].span;
    let end = value
        .owner
        .nodes
        .partition_point(|node| node.span.start < span.end);
    let mut depths = vec![0; end - value.id];
    let mut maximum = 0;
    for (offset, node) in value.owner.nodes[value.id..end].iter().enumerate() {
        let own = usize::from(node.is_container());
        let parent = if offset == 0 || node.parent == NO_PARENT {
            0
        } else {
            depths[node.parent as usize - value.id]
        };
        depths[offset] = parent + own;
        maximum = maximum.max(depths[offset]);
    }
    maximum
}

/// Logical live exact-storage owners, including process-lifetime fixed schemas.
pub fn live_arenas() -> usize {
    crate::raw::live_arenas()
}

/// Borrow a token's byte range without constructing its pointer or line coordinates.
pub fn byte_range(value: crate::JsonRef<'_>) -> std::ops::Range<usize> {
    value.owner.nodes[value.id].span.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_location_sizes_agree_with_each_walk() {
        // Escaped and multibyte names, multi-digit indices, nesting, and names
        // with unpaired units, alone and inside scalar and unpaired paths.
        let deep = format!("{}0{}", r#"{"~/":["#.repeat(40), "]}".repeat(40));
        let wide = format!("[{}]", vec!["{\"é\":true}"; 120].join(","));
        let sources = [
            r#"{"":{"a/b~c":[0,1,2,3,4,5,6,7,8,9,{"é":[[true]],"~~//":null}]},"x":{"\ud800":{"y":[1,{"\udc00~":2}]},"ok":"A"}}"#.to_owned(),
            r#"[[[[["leaf"]]]],{"é😀":{"k":[null]}}]"#.to_owned(),
            deep,
            wide,
            "7".to_owned(),
        ];
        for source in sources {
            let value = crate::JsonValue::parse(&source).unwrap();
            let sizes = LocationPointerSizes::new(value.view(), 4096);
            for id in 0..value.owner.nodes.len() {
                let at = crate::JsonRef {
                    owner: &value.owner,
                    id,
                };
                for limit in [0, 1, 2, 3, 5, 8, 13, 40, 120, 250, 1000, 4095] {
                    assert_eq!(
                        sizes.size(at, limit),
                        location_pointer_size(at, limit),
                        "{source} node {id} limit {limit}"
                    );
                }
            }
        }
    }

    #[test]
    fn decoded_string_size_counts_owned_subtree_strings_without_ancestors_or_siblings() {
        let value = crate::JsonValue::parse(
            r#"{"outside\n":"sibling\t","nested":{"key\"":"raw雪","list":["a\/b","\uD83D\uDE00",{"\u006b":"end\n"}]},"after":"other\r"}"#,
        ).unwrap();
        let nested = value.at("/nested").unwrap();
        // key\" (4), a\/b (3), emoji (4 UTF-8), \u006b (1), end\n (4).
        assert_eq!(decoded_string_size(nested, 16), Some(16));
        assert_eq!(decoded_string_size(nested, 15), None);
        assert_eq!(
            decoded_string_size(value.at("/nested/key\"").unwrap(), 0),
            Some(0)
        );
        assert_eq!(
            decoded_string_size(value.at("/nested/list/1").unwrap(), 4),
            Some(4)
        );
        assert_eq!(decoded_string_size(value.view(), 38), Some(38));
        let unsupported = crate::JsonValue::parse(r#"["\ud800"]"#).unwrap();
        assert_eq!(decoded_string_size(unsupported.view(), usize::MAX), None);
    }
    #[test]
    fn node_counts_cover_a_value_and_everything_below_it() {
        let source = r#"{"a":[1,{"b":null}],"c":"d"}"#;
        let value = crate::JsonValue::parse(source).unwrap();
        // The root, its two member names, the array and its two elements,
        // the inner name and null, and the string.
        assert_eq!(node_count(value.view()), 9);
        for (pointer, nodes) in [("/a", 5), ("/a/1", 3), ("/a/0", 1), ("/c", 1)] {
            assert_eq!(node_count(value.at(pointer).unwrap()), nodes, "{pointer}");
        }
        assert_eq!(node_count(crate::JsonValue::parse("7").unwrap().view()), 1);
    }
    #[test]
    fn bounded_depth_agrees_with_depth_for_values_and_subtrees() {
        let deep = format!("{}0{}", "[".repeat(40), "]".repeat(40));
        let mixed = format!(r#"{{"a":[1,{{"b":{deep}}}],"c":{{}},"d":[[[]]]}}"#);
        for text in [
            "0",
            "[]",
            "[0,[1],[[2]]]",
            r#"{"a":{"b":[]},"c":0}"#,
            &deep,
            &mixed,
        ] {
            let value = crate::JsonValue::parse(text).unwrap();
            let mut subtrees = vec![value.clone()];
            for pointer in ["/a", "/a/1", "/a/1/b", "/0", "/2"] {
                if let Some(subtree) = value.at(pointer) {
                    subtrees.push(subtree.to_owned());
                }
            }
            for subtree in subtrees {
                let depth = depth(&subtree);
                for limit in 0..depth + 3 {
                    assert_eq!(
                        depth_at_most(&subtree, limit),
                        depth <= limit,
                        "{text} {limit}"
                    );
                }
            }
        }
    }
    #[test]
    fn indexed_members_preserve_each_exact_occurrence() {
        let source = r#"{"z":9007199254740993,"\ud800":null,"z":0.29000000000000001}"#;
        let value = crate::JsonValue::parse(source).unwrap();
        for (index, (name, text)) in [
            (r#""z""#, "9007199254740993"),
            (r#""\ud800""#, "null"),
            (r#""z""#, "0.29000000000000001"),
        ]
        .into_iter()
        .enumerate()
        {
            let member = member_at(value.view(), index).unwrap();
            assert_eq!(member.name.text(), name);
            assert_eq!(member.value.text(), text);
        }
        let first = member_at(value.view(), 0).unwrap();
        let last = member_at(value.view(), 2).unwrap();
        assert_ne!(
            first.name.location().byte_offset,
            last.name.location().byte_offset
        );
        let retained = last.value.to_owned();
        assert!(member_at(value.view(), 3).is_none());
        assert!(member_at(value.view(), usize::MAX).is_none());
        drop(value);
        assert_eq!(retained.text(), "0.29000000000000001");
        for source in ["{}", "[]", "[1]", "null", "1", "true", r#""text""#] {
            let value = crate::JsonValue::parse(source).unwrap();
            assert!(member_at(value.view(), 0).is_none());
        }
    }

    #[test]
    fn unpaired_queries_match_subtree_membership_and_bound_search_work() {
        use std::sync::atomic::Ordering;
        let nested = r#"{"\ud800":{"a":"\udc00","b":["x","\ud800y"]},"c":"ok","d":[{"\udfff":0},[]],"e":"\ud83d\ude00","f":"\ude00\ud83d"}"#;
        // Many unpaired strings beside many clean subtrees: each query must
        // cost a search, not a scan of every unpaired string in the arena.
        let wide = format!(
            "[{},{}]",
            std::iter::repeat_n(r#"{"type":"string"}"#, 2048)
                .collect::<Vec<_>>()
                .join(","),
            std::iter::repeat_n(r#""\ud800""#, 4096)
                .collect::<Vec<_>>()
                .join(",")
        );
        for source in [nested, wide.as_str(), "[]", r#""\ud800""#, r#"{"a":1}"#] {
            let root = crate::JsonValue::parse(source).unwrap();
            let arena = &root.owner;
            assert!(arena.unpaired.windows(2).all(|pair| pair[0] < pair[1]));
            let bound = 2 * (arena.unpaired.len().max(1).ilog2() as usize + 2);
            for id in 0..arena.nodes.len() {
                let span = &arena.nodes[id].span;
                // Independent slow oracle: every unpaired token inside this value.
                let expected: Vec<_> = arena
                    .unpaired
                    .iter()
                    .map(|&inner| inner as usize)
                    .filter(|&inner| {
                        let inner = &arena.nodes[inner].span;
                        span.start <= inner.start && inner.end <= span.end
                    })
                    .collect();
                let value = crate::JsonValue {
                    owner: arena.clone(),
                    id,
                };
                arena.unpaired_query_probes.store(0, Ordering::Relaxed);
                assert_eq!(has_unpaired(&value), !expected.is_empty(), "node {id}");
                let probes = arena.unpaired_query_probes.load(Ordering::Relaxed);
                assert!(probes <= bound, "{probes} probes exceeds {bound}");
                let found: Vec<_> = unpaired_strings(&value).map(|at| at.id).collect();
                assert_eq!(found, expected, "node {id}");
            }
        }
    }

    #[test]
    fn duplicate_member_names_keep_each_original_token_occurrence() {
        let source = r#"{"a/~":{"k":0,"k":1,"k":2},"\ud800":{"j":0,"j":1}}"#;
        let value = crate::JsonValue::parse(source).unwrap();
        assert_eq!(duplicate_member_names(&value).len(), 3);
        let nodes: Vec<_> = duplicate_member_names(&value).collect();
        let at = locations(&nodes);
        assert_eq!(at[0].pointer.as_ref().unwrap(), "/a~1~0/k");
        assert_eq!(at[1].pointer, at[0].pointer);
        assert_eq!(at[0].byte_offset, source.find("\"k\":1").unwrap());
        assert_eq!(at[1].byte_offset, source.find("\"k\":2").unwrap());
        // A name with an unpaired unit is named exactly.
        let exact = at[2].pointer.as_ref().unwrap();
        assert_eq!(exact.as_str(), None);
        assert_eq!(exact.to_string(), "/\\ud800/j");
        assert_eq!(at[2].byte_offset, source.find("\"j\":1").unwrap());
        assert_eq!(duplicate_member_names(&value).take(1).count(), 1);
    }

    #[test]
    fn duplicate_locations_are_materialized_only_for_retained_nodes() {
        let source = format!(
            "{{\"pad\":\"{}\",\"nested\":{{\"k\":0{}}}}}",
            "x".repeat(128 * 1024),
            ",\"k\":0".repeat(5000)
        );
        let value = crate::JsonValue::parse(&source).unwrap();
        let mut visited = 0;
        let nodes: Vec<_> = duplicate_nodes(&value)
            .inspect(|_| visited += 1)
            .take(4096)
            .collect();
        assert_eq!(visited, 4096);
        assert_eq!(nodes.len(), 4096);
        let (locations, scanned) = locations_with_work(&nodes);
        let offset = source.find("{\"k\"").unwrap();
        assert_eq!(scanned, offset);
        assert!(
            locations
                .iter()
                .all(|at| at.byte_offset == offset && at.pointer.as_ref().unwrap() == "/nested")
        );
        assert_eq!(duplicate_nodes(&value).take(0).count(), 0);
    }

    #[test]
    fn pointers_through_unpaired_names_are_exact_and_charge_their_units() {
        let value = crate::JsonValue::parse(r#"{"\ud800/":{"k":0,"k":1}}"#).unwrap();
        let node = duplicate_nodes(&value).next().unwrap();
        // The scalar measure has no spelling for it; the exact one holds the
        // escaped units `/`, U+D800, `~`, `1`, two bytes each.
        assert_eq!(pointer_size(node, usize::MAX), None);
        assert_eq!(location_pointer_size(node, 7), None);
        assert_eq!(location_pointer_size(node, 8), Some(8));
        let at = locations(&[node]).remove(0);
        let pointer = at.pointer.as_ref().unwrap();
        assert_eq!(
            pointer.utf16_units().collect::<Vec<_>>(),
            [0x2f, 0xd800, 0x7e, 0x31]
        );
        assert_eq!(at.byte_offset, 11);
        assert_eq!(
            value.at_pointer(pointer).map(byte_range),
            Some(byte_range(node))
        );
        assert_eq!(value.locate(pointer).as_ref(), Some(&at));

        let scalar = crate::JsonValue::parse(r#"{"a/~":{"k":0,"k":1}}"#).unwrap();
        let node = duplicate_nodes(&scalar).next().unwrap();
        assert_eq!(location_pointer_size(node, 5), None);
        assert_eq!(location_pointer_size(node, 6), Some(6));
    }

    #[test]
    fn batched_locations_match_original_coordinates_with_linear_source_scans() {
        let first = crate::JsonValue::parse(" {\r\n\"é/~\\n\":[1,\n2],\"z\":false}").unwrap();
        let second = crate::JsonValue::parse(" \n[true, false]").unwrap();
        let members: Vec<_> = first.view().members().unwrap().collect();
        let values = [
            members[1].value,
            second.view(),
            members[0].name,
            first.at("/é~1~0\n/1").unwrap(),
            members[1].name,
            members[1].value,
            second.at("/1").unwrap(),
        ];
        let (locations, scanned) = locations_with_work(&values);
        assert_eq!(
            locations,
            values.iter().map(|v| v.location()).collect::<Vec<_>>()
        );
        let max_first = values
            .iter()
            .filter(|v| std::sync::Arc::ptr_eq(v.owner, &first.owner))
            .map(|v| v.owner.nodes[v.id].span.start)
            .max()
            .unwrap();
        let max_second = values
            .iter()
            .filter(|v| std::sync::Arc::ptr_eq(v.owner, &second.owner))
            .map(|v| v.owner.nodes[v.id].span.start)
            .max()
            .unwrap();
        assert_eq!(scanned, max_first + max_second);
        assert!(locations_with_work(&[]).0.is_empty());
        for value in values {
            let size = value.location().pointer.unwrap().as_str().unwrap().len();
            assert_eq!(pointer_size(value, size), Some(size));
            if size > 0 {
                assert_eq!(pointer_size(value, size - 1), None);
            }
        }
    }

    #[test]
    fn fixed_checks_read_strings_in_place_unless_unpaired_units_need_the_escape() {
        // Without an unpaired unit no string is encoded, so the fixed checks
        // need no escape for the marker and read every string in place; beside
        // one, the marker is doubled so that no string reads like an encoded
        // unit. The schema space's spelling always doubles it.
        let marker = '\u{e000}';
        fn read(value: &crate::JsonValue, at: usize) -> Cow<'_, str> {
            let element = value.view().element(at).unwrap();
            fixed_strings(|| read_string(element.owner, element.id)).unwrap()
        }
        let plain = crate::JsonValue::parse(format!(r#"["{marker}a{marker}","b"]"#)).unwrap();
        assert!(
            matches!(read(&plain, 0), Cow::Borrowed(text) if text == format!("{marker}a{marker}"))
        );
        let unpaired =
            crate::JsonValue::parse(format!(r#"["{marker}a{marker}","\ud800"]"#)).unwrap();
        let doubled = format!("{marker}{marker}a{marker}{marker}");
        assert!(matches!(read(&unpaired, 0), Cow::Owned(text) if text == doubled));
        assert_eq!(read(&unpaired, 1), format!("{marker}d800;"));
        assert_eq!(
            fixed_string(plain.view().element(0).unwrap()).unwrap(),
            doubled
        );
    }

    #[test]
    fn subtree_ends_are_found_down_the_last_items() {
        for text in [
            "7",
            "[]",
            "{}",
            r#"[1,[2,[3,{}]],{"a":[{"b":{}}],"c":[]}]"#,
            r#"{"a":1,"b":{"c":[1,2,{"d":null}],"e":{}},"f":"g"}"#,
            r#"[[[[["deep"]]]],{"\ud800":[true,{"x":[]}]}]"#,
        ] {
            let value = crate::JsonValue::parse(text).unwrap();
            let nodes = &value.owner.nodes;
            for id in 0..nodes.len() {
                let end = nodes.partition_point(|n| n.span.start < nodes[id].span.end);
                assert_eq!(subtree_end(&value.owner, id), end, "{text} {id}");
            }
        }
    }

    #[cfg(feature = "evaluator-work")]
    #[test]
    fn small_scalar_arrays_decide_and_charge_as_hashing_does() {
        // Equal scalars spelled differently, unpaired strings, and repeats at
        // every position, under every allowance from none to more than enough.
        let pool: Vec<&str> = concat!(
            r#"1 1.0 10e-1 2 -0 0 0e5 "a" "\u0061" "b" "\ud800" "\ud800" "\udc00" true false "#,
            r#"null 1e400 10e399 9007199254740993 9007199254740992 [] {} [1] {"a":1}"#
        )
        .split(' ')
        .collect();
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |bound: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % bound as u64) as usize
        };
        for _ in 0..400 {
            let length = 2 + next(SMALL - 1);
            let items: Vec<_> = (0..length).map(|_| pool[next(pool.len())]).collect();
            let value = crate::JsonValue::parse(format!("[{}]", items.join(","))).unwrap();
            // Containers, empty ones included, take the hash check.
            let decide = |chosen: bool, units: usize| {
                crate::work::scope(units, || {
                    let array = view(&value).as_array().unwrap();
                    let first = array.1[0] as usize;
                    let end = subtree_end(array.0, array.1[length - 1] as usize);
                    if chosen {
                        array.is_unique()
                    } else {
                        array.hashed_unique(first, end, counting())
                    }
                })
            };
            for units in 0..2 * length + 6 {
                assert_eq!(
                    decide(true, units),
                    decide(false, units),
                    "{items:?} {units}"
                );
            }
        }
    }

    #[test]
    fn uniqueness_verifies_every_forced_hash_collision() {
        for (text, expected) in [
            ("[]", true),
            ("[1,2,3,4]", true),
            ("[1,2,3,1.0]", false),
            ("[1,2,3,2e0]", false),
            ("[1,2,3,3]", false),
            (r#"[{"a":1},{"a":2},{"b":1}]"#, true),
            (r#"[{"a":1},{"a":2},{"a":2.0}]"#, false),
            ("[-0,1,2,0e999999]", false),
        ] {
            let value = crate::JsonValue::parse(text).unwrap();
            let array = view(&value).as_array().unwrap();
            // A constant hash makes every element a candidate for every later
            // one. Hash equality alone must never decide uniqueness.
            assert_eq!(array.unique_by_hash(&|_| 0, counting()), expected, "{text}");
        }
    }

    #[test]
    fn uniqueness_work_stays_linear_when_unequal_elements_share_hashes() {
        // Pairs of objects holding one name twice, in either order: the
        // hash counts every member, equality reads the first, so each pair
        // shares a hash without being equal. Every element is still compared
        // only with the earlier ones of its hash, found without rescanning
        // the array.
        let pairs = 20_000;
        let text = format!(
            "[{}]",
            (0..pairs)
                .map(|i| format!(
                    r#"{{"a":{0},"a":{1}}},{{"a":{1},"a":{0}}}"#,
                    2 * i,
                    2 * i + 1
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        let value = crate::JsonValue::parse(&text).unwrap();
        let array = view(&value).as_array().unwrap();
        let reads = std::cell::Cell::new(0usize);
        let hash_of = |at: usize| {
            reads.set(reads.get() + 1);
            (at / 2) as u64
        };
        assert!(array.unique_by_hash(&hash_of, counting()));
        assert!(
            reads.get() <= 64 * 2 * pairs,
            "{} hash reads for {} elements",
            reads.get(),
            2 * pairs
        );
        // An equal pair after many repeats still ends the check.
        let text = format!("{},{{\"a\":1}},{{\"a\":1}}]", &text[..text.len() - 1]);
        let value = crate::JsonValue::parse(&text).unwrap();
        let array = view(&value).as_array().unwrap();
        assert!(!array.unique_by_hash(&|at| (at / 2) as u64, counting()));
    }

    #[test]
    fn uniqueness_is_exact_and_stack_safe() {
        let validator = jsonschema::options_for::<FlatJson>()
            .with_draft(jsonschema::Draft::Draft202012)
            .offline()
            .build(&serde_json::json!({"uniqueItems":true}))
            .unwrap();
        for (text, want) in [
            ("[1,1.0]", false),
            (r#"[{"a":1,"b":2},{"b":2.0,"a":1e0}]"#, false),
            ("[9007199254740992,9007199254740993]", true),
        ] {
            let value = crate::JsonValue::parse(text).unwrap();
            assert_eq!(validator.is_valid(view(&value)), want);
        }
        let deep = format!("{}0{}", "[".repeat(9999), "]".repeat(9999));
        let value = crate::JsonValue::parse(format!("[{deep},{deep}]")).unwrap();
        assert!(!validator.is_valid(view(&value)));
    }
    #[test]
    fn deep_unevaluated_diagnostics_do_not_materialize_values() {
        let validator = jsonschema::options_for::<FlatJson>()
            .with_draft(jsonschema::Draft::Draft202012)
            .offline()
            .build(&serde_json::json!({"unevaluatedItems":false}))
            .unwrap();
        let value = crate::JsonValue::parse(format!("{}0{}", "[".repeat(10000), "]".repeat(10000)))
            .unwrap();
        let errors: Vec<_> = validator.iter_errors(view(&value)).collect();
        assert!(!errors.is_empty());
        assert_eq!(errors[0].instance_path().as_str(), "");
    }
}
