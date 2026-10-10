//! Private flat exact-JSON storage; iterative parsing and destruction.
use std::{
    collections::{HashSet, hash_map::RandomState},
    hash::{BuildHasher, Hasher},
    ops::Range,
    sync::Arc,
};

pub type Id = usize;
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
#[derive(Debug)]
pub struct Arena {
    _lifetime: Lifetime,
    source: Arc<str>,
    pub nodes: Vec<Node>,
    pub duplicates: Vec<(Id, Id)>,
    pub unpaired: Vec<Id>,
}
impl Arena {
    pub fn parse(bytes: &[u8], limits: Limits) -> Result<Self, Problem> {
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
                nodes: Vec::new(),
                duplicates: Vec::new(),
                unpaired: Vec::new(),
            },
            pos: 0,
            stack: Vec::new(),
            limits,
            key_hasher: RandomState::new(),
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
                parser.stack.pop();
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
                let hash = parser.hash_key(key);
                let seen_hash = !parser.stack.last_mut().unwrap().keys.insert(hash);
                if seen_hash {
                    let Kind::Object(members) = &parser.out.nodes[id].kind else {
                        unreachable!()
                    };
                    if members.iter().any(|m| parser.out.equal_keys(m.key, key)) {
                        parser.out.duplicates.push((id, key));
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
        match (self.string(left), self.string(right)) {
            (Some(a), Some(b)) => a == b,
            _ => self.units(left) == self.units(right),
        }
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn raw(&self, id: Id) -> &str {
        &self.source[self.nodes[id].span.clone()]
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
        let Kind::Object(members) = &self.nodes[id].kind else {
            return None;
        };
        members
            .iter()
            .find(|m| self.string(m.key) == Some(key))
            .map(|m| m.value)
    }
    pub fn at(&self, pointer: &str) -> Option<Id> {
        if pointer.is_empty() {
            return Some(0);
        }
        let rest = pointer.strip_prefix('/')?;
        let mut current = 0;
        for segment in rest.split('/') {
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
            current = match &self.nodes[current].kind {
                Kind::Object(_) => self.get(current, &name)?,
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
        // An unrepresentable ancestor makes the whole pointer unavailable. Check
        // before copying any other (possibly large) segments. Scalar-only arenas
        // keep the ordinary single walk.
        if !self.unpaired.is_empty() {
            let mut current = id;
            while let Some((parent, edge)) = self.nodes[current].parent {
                if let Edge::Key(key) = edge {
                    self.string(key)?;
                }
                current = parent;
            }
        }
        let mut segments = Vec::new();
        let mut current = id;
        while let Some((parent, edge)) = self.nodes[current].parent {
            segments.push(match edge {
                Edge::Index(i) => i.to_string(),
                Edge::Key(k) => self.string(k)?.replace('~', "~0").replace('/', "~1"),
            });
            current = parent;
        }
        segments.reverse();
        Some(if segments.is_empty() {
            String::new()
        } else {
            format!("/{}", segments.join("/"))
        })
    }
    pub fn position(&self, offset: usize) -> Option<(usize, usize)> {
        if offset > self.source.len() || !self.source.is_char_boundary(offset) {
            return None;
        }
        let prefix = &self.source[..offset];
        Some((
            prefix.bytes().filter(|&c| c == b'\n').count() + 1,
            prefix.rfind('\n').map_or(offset + 1, |n| offset - n),
        ))
    }
}
struct Frame {
    id: Id,
    state: u8,
    keys: HashSet<u64>,
}
struct Parser {
    out: Arena,
    pos: usize,
    stack: Vec<Frame>,
    limits: Limits,
    key_hasher: RandomState,
}
impl Parser {
    fn hash_key(&self, id: Id) -> u64 {
        let mut hasher = self.key_hasher.build_hasher();
        if let Some(s) = self.out.string(id) {
            for unit in s.encode_utf16() {
                hasher.write_u16(unit);
            }
        } else if let Kind::String {
            unpaired: Some(units),
            ..
        } = &self.out.nodes[id].kind
        {
            for &unit in units.iter() {
                hasher.write_u16(unit);
            }
        }
        hasher.finish()
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
                    keys: HashSet::new(),
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
