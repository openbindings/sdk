//! Schema regular expressions as OpenBindings reads them: ECMA-262 11th edition
//! patterns with Unicode semantics (the `u` flag) and no other flag, for
//! `pattern` values and `patternProperties` names alike.
//!
//! Every pattern is parsed here before its search program is prepared. A
//! pattern that is not valid with Unicode semantics has no reading, and the
//! evaluator does not match it: whether a string matches it is undefined. A
//! valid pattern is matched by a linear-time engine (the `regex` crate) when it
//! has a form in that engine's syntax with the same meaning:
//! [`Reading::Linear`] holds that form, which spells every character set out as
//! code points so the two dialects read it alike. Strict ECMA property names
//! select scalar ranges from the pinned engine's Unicode tables. A terminal
//! positive lookahead at the top level can consume its body because only match
//! existence is observed. Other lookaround, backreferences, properties absent
//! from those tables, and non-word-boundary assertions remain unsupported.
//! The word boundary `\b` has a form: ECMA-262 reads it with
//! `\w` as `[A-Za-z0-9_]`, as the regex crate's ASCII word boundary does, and
//! an ASCII word boundary never falls inside a multi-byte character. Its
//! complement `(?-u:\B)` does, and the regex crate's `is_match` then answers no
//! for a pattern that another branch matches (`\x{E9}|(?-u:\B)` on "aéa"), so
//! `\B` is not matched.
//!
//! The values the evaluator checks are strings of Unicode scalar values: a
//! string with an unpaired surrogate is refused before any pattern reads it. A
//! pattern can still name a lone surrogate with an escape; such an atom matches
//! nothing those strings hold, and a class gains nothing from it.
use std::{collections::BTreeSet, fmt::Write};

/// What the evaluator can do with a valid pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reading {
    /// The pattern in the `regex` crate's syntax, written to have the same
    /// matches.
    Linear(Linear),
    /// A valid pattern the evaluator does not match.
    Unsupported(Unsupported),
}
/// A pattern the linear-time engine matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Linear {
    /// The pattern in the `regex` crate's syntax.
    pub(crate) form: String,
    /// A matcher that needs no compiled automaton, for the patterns that are
    /// one anchored sequence of single-character runs.
    pub(crate) simple: Option<Simple>,
}
/// Why a pattern is not matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// It needs a backtracking engine.
    Backtracking,
    /// Its valid Unicode property is absent from the pinned engine tables.
    PropertyEscape,
    /// It has a non-word-boundary assertion, `\B`.
    NonWordBoundary,
    /// Its normalized form exceeds the admitted byte limit.
    Size,
}
/// Why a pattern has no reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// It is not a valid pattern with Unicode semantics.
    Invalid,
    /// Its groups nest more deeply than the admitted limit.
    Depth,
}

/// Supported authored-group recursion ceiling, independent of caller budgets.
pub(crate) const MAX_GROUP_DEPTH: usize = 32;

/// Read one pattern, refusing groups deeper than the lower of `max_depth` and
/// the supported authored-group ceiling. A
/// valid pattern whose form would be longer than `max_form` bytes is read as
/// [`Unsupported::Size`], and its form is not written out.
pub(crate) fn read(pattern: &str, max_depth: usize, max_form: usize) -> Result<Reading, Refusal> {
    if let Some((body, end)) = anchored_literal(pattern) {
        if pattern.len() > max_form {
            return Ok(Reading::Unsupported(Unsupported::Size));
        }
        return Ok(Reading::Linear(Linear {
            form: pattern.into(),
            simple: Some(Simple {
                end,
                table: Box::default(),
                steps: Box::new([Step {
                    ascii: [0; 2],
                    wide: TEXT,
                    count: body.len() as u32,
                }]),
                last: None,
                text: body.into(),
                ranges: Box::default(),
            }),
        }));
    }
    read_general(pattern, max_depth, max_form)
}

// This subset has no expansion, groups, character sets or per-character
// temporary ranges. Every byte already has the general Reader's spelling.
fn anchored_literal(source: &str) -> Option<(&str, bool)> {
    let body = source.strip_prefix('^')?;
    let end = body.ends_with('$');
    let body = if end { &body[..body.len() - 1] } else { body };
    if body.is_empty()
        || u32::try_from(body.len()).is_err()
        || !body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'/'))
    {
        return None;
    }
    Some((body, end))
}

fn read_general(pattern: &str, max_depth: usize, max_form: usize) -> Result<Reading, Refusal> {
    let max_depth = max_depth.min(MAX_GROUP_DEPTH);
    let chars: Vec<char> = pattern.chars().collect();
    let groups = Groups::scan(&chars)?;
    let mut parser = Parser {
        chars: &chars,
        at: 0,
        groups,
        max_depth,
        max_form,
        out: String::with_capacity(pattern.len() + 8),
        unsupported: None,
        shape: Some(Shape::default()),
    };
    parser.disjunction(0)?;
    if parser.at != chars.len() {
        // An unmatched `)`.
        return Err(Refusal::Invalid);
    }
    if parser.out.len() > max_form {
        parser.unsupported(Unsupported::Size);
    }
    Ok(match parser.unsupported {
        Some(unsupported) => Reading::Unsupported(unsupported),
        None => Reading::Linear(Linear {
            simple: parser.shape.and_then(Shape::finish),
            form: parser.out,
        }),
    })
}

/// Conservative reader work/allocation reservations, including expanded Unicode
/// properties, range-set unions, parser scratch, rendered forms and their retained
/// clone. A short property spelling is not a bound on its translated form.
pub(crate) fn preparation_cost(source: &str) -> Option<(usize, usize)> {
    if anchored_literal(source).is_some() {
        // The caller charges this eligibility scan. The direct reader scans
        // again, copies the form and body, and retains one fixed TEXT step;
        // there is no character inventory, group parser or range scratch.
        return Some((
            source.len().checked_mul(3)?.checked_add(512)?,
            source.len().checked_mul(2)?.checked_add(512)?,
        ));
    }
    // Only members of the same character class are repeatedly unioned. A
    // sequence of independent escapes creates independent sets, so charging
    // their Cartesian product would refuse otherwise modest linear patterns.
    let base = source.len().checked_mul(128)?.checked_add(512)?;
    let (mut work, mut allocation) = (base, base);
    let mut class = None;
    let (mut properties, mut shorthand) = (0usize, 0usize);
    let bytes = source.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' if at + 1 < bytes.len() => {
                let property = matches!(bytes[at + 1], b'p' | b'P');
                let short = matches!(bytes[at + 1], b'd' | b'D' | b'w' | b'W' | b's' | b'S');
                if class.is_some() {
                    properties += usize::from(property);
                    shorthand += usize::from(short);
                } else if property || short {
                    let ranges = if property {
                        crate::es2020_properties::MAX_ENGINE_PROPERTY_RANGES
                    } else {
                        16
                    };
                    work = work.checked_add(ranges.checked_mul(128)?)?;
                    allocation = allocation.checked_add(ranges.checked_mul(512)?)?;
                }
                at += 2;
                continue;
            }
            b'[' if class.is_none() => {
                class = Some(at);
                properties = 0;
                shorthand = 0;
            }
            b']' if class.is_some() => {
                let start = class.take().expect("open class");
                let (w, a) = class_cost(at - start + 1, properties, shorthand)?;
                work = work.checked_add(w)?;
                allocation = allocation.checked_add(a)?;
            }
            _ => {}
        }
        at += 1;
    }
    if let Some(start) = class {
        // An unterminated class still allocates its parsed prefix before the
        // syntax error is discovered.
        let (w, a) = class_cost(bytes.len() - start, properties, shorthand)?;
        work = work.checked_add(w)?;
        allocation = allocation.checked_add(a)?;
    }
    Some((work, allocation))
}
fn class_cost(bytes: usize, properties: usize, shorthand: usize) -> Option<(usize, usize)> {
    let property_ranges =
        properties.checked_mul(crate::es2020_properties::MAX_ENGINE_PROPERTY_RANGES)?;
    // Repeated shorthand sets are merged immediately. Their union has fewer
    // than 96 ranges; literal members can survive every subsequent merge.
    let ranges = bytes
        .checked_add(property_ranges)?
        .checked_add(if shorthand == 0 { 0 } else { 96 })?;
    let merges = properties.checked_add(shorthand)?.checked_add(2)?;
    let scratch = ranges.checked_mul(merges)?.checked_mul(64)?;
    let allocation = scratch
        .checked_add(ranges.checked_mul(256)?)?
        .checked_add(property_ranges.checked_mul(128)?)?;
    Some((scratch, allocation))
}

// A set nothing matches, and one that matches every scalar value. Every set
// is written as the positive class of the scalar values it matches, never as a
// negated class: the regex crate's negation of a class with ranges on both
// sides of the surrogate gap adds a range across the gap, so `[^...]` would
// match characters ECMA-262 excludes. The empty set is a character after the
// end of the string, since the library escapes class set operations in
// `patternProperties` names.
const NOTHING: &str = r"(?:$a)";
#[cfg(test)]
const ANYTHING: &str = r"[\x{0}-\x{D7FF}\x{E000}-\x{10FFFF}]";
const SURROGATES: (u32, u32) = (0xD800, 0xDFFF);
// `\s`: WhiteSpace and LineTerminator.
const SPACE: &[(u32, u32)] = &[
    (0x09, 0x0D),
    (0x20, 0x20),
    (0xA0, 0xA0),
    (0x1680, 0x1680),
    (0x2000, 0x200A),
    (0x2028, 0x2029),
    (0x202F, 0x202F),
    (0x205F, 0x205F),
    (0x3000, 0x3000),
    (0xFEFF, 0xFEFF),
];
const DIGIT: &[(u32, u32)] = &[(0x30, 0x39)];
const WORD: &[(u32, u32)] = &[(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)];
// The LineTerminators, which `.` excludes without the `s` flag.
const DOT: &[(u32, u32)] = &[(0x0A, 0x0A), (0x0D, 0x0D), (0x2028, 0x2029)];

// The capturing groups of the whole pattern, which a backreference may name
// before or after itself.
struct Groups {
    count: usize,
    names: BTreeSet<String>,
}
impl Groups {
    fn scan(chars: &[char]) -> Result<Self, Refusal> {
        let mut groups = Groups {
            count: 0,
            names: BTreeSet::new(),
        };
        let mut at = 0;
        let mut class = false;
        while at < chars.len() {
            match chars[at] {
                '\\' => at += 1,
                '[' => class = true,
                ']' => class = false,
                '(' if !class => {
                    if chars.get(at + 1) != Some(&'?') {
                        groups.count += 1;
                    } else if chars.get(at + 2) == Some(&'<')
                        && !matches!(chars.get(at + 3), Some('=' | '!'))
                    {
                        groups.count += 1;
                        let mut end = at + 3;
                        // Only a well-formed name counts; the parser refuses the rest.
                        if let Ok(name) = group_name(chars, &mut end)
                            && !groups.names.insert(name)
                        {
                            return Err(Refusal::Invalid);
                        }
                    }
                }
                _ => {}
            }
            at += 1;
        }
        Ok(groups)
    }
}

// A `RegExpIdentifierName` up to and including its closing `>`.
fn group_name(chars: &[char], at: &mut usize) -> Result<String, Refusal> {
    let mut name = String::new();
    loop {
        let c = *chars.get(*at).ok_or(Refusal::Invalid)?;
        *at += 1;
        let c = match c {
            '>' if !name.is_empty() => return Ok(name),
            '\\' => {
                if chars.get(*at) != Some(&'u') {
                    return Err(Refusal::Invalid);
                }
                *at += 1;
                let code = unicode_escape(chars, at)?;
                char::from_u32(code).ok_or(Refusal::Invalid)?
            }
            c => c,
        };
        let admitted = if name.is_empty() {
            c == '$' || c == '_' || identifier::start(c)
        } else {
            c == '$' || c == '\u{200C}' || c == '\u{200D}' || identifier::part(c)
        };
        if !admitted {
            return Err(Refusal::Invalid);
        }
        name.push(c);
    }
}

// The code point of a `RegExpUnicodeEscapeSequence` with Unicode semantics,
// after its `u`. An escaped surrogate pair is one code point; any other
// surrogate stays a lone surrogate code point.
fn unicode_escape(chars: &[char], at: &mut usize) -> Result<u32, Refusal> {
    if chars.get(*at) == Some(&'{') {
        *at += 1;
        let mut value: u32 = 0;
        let mut digits = 0;
        loop {
            let c = *chars.get(*at).ok_or(Refusal::Invalid)?;
            *at += 1;
            if c == '}' && digits > 0 {
                return Ok(value);
            }
            let digit = c.to_digit(16).ok_or(Refusal::Invalid)?;
            value = value
                .checked_mul(16)
                .and_then(|v| v.checked_add(digit))
                .filter(|v| *v <= 0x10FFFF)
                .ok_or(Refusal::Invalid)?;
            digits += 1;
        }
    }
    let lead = hex(chars, at, 4)?;
    if (0xD800..0xDC00).contains(&lead)
        && chars.get(*at) == Some(&'\\')
        && chars.get(*at + 1) == Some(&'u')
    {
        let mut next = *at + 2;
        if let Ok(trail) = hex(chars, &mut next, 4)
            && (0xDC00..0xE000).contains(&trail)
        {
            *at = next;
            return Ok(0x10000 + ((lead - 0xD800) << 10) + (trail - 0xDC00));
        }
    }
    Ok(lead)
}
fn hex(chars: &[char], at: &mut usize, digits: usize) -> Result<u32, Refusal> {
    let mut value = 0;
    for _ in 0..digits {
        let digit = chars
            .get(*at)
            .and_then(|c| c.to_digit(16))
            .ok_or(Refusal::Invalid)?;
        value = value * 16 + digit;
        *at += 1;
    }
    Ok(value)
}

pub(crate) mod identifier;

// One class member: a code point, or a set a class escape names.
enum Member {
    Char(u32),
    Set(Vec<(u32, u32)>),
}

struct Parser<'a> {
    chars: &'a [char],
    at: usize,
    groups: Groups,
    max_depth: usize,
    // The longest form written; a set or literal past it is not written.
    max_form: usize,
    out: String,
    unsupported: Option<Unsupported>,
    // The run matcher while the top-level terms are one sequence of anchors
    // and single-character atoms with simple quantifiers, and the pattern is
    // one the evaluator matches.
    shape: Option<Shape>,
}
impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }
    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }
    fn next(&mut self) -> Result<char, Refusal> {
        let c = self.peek().ok_or(Refusal::Invalid)?;
        self.at += 1;
        Ok(c)
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    // A backtracking construct takes precedence over a property escape: it
    // would stay unmatched even with property tables. A pattern the evaluator
    // does not match has no run matcher.
    fn unsupported(&mut self, kind: Unsupported) {
        if self.unsupported != Some(Unsupported::Backtracking) {
            self.unsupported = Some(kind);
        }
        self.shape = None;
    }
    fn disjunction(&mut self, depth: usize) -> Result<(), Refusal> {
        self.alternative(depth)?;
        while self.eat('|') {
            if depth == 0 {
                self.shape = None;
            }
            self.out.push('|');
            self.alternative(depth)?;
        }
        Ok(())
    }
    fn alternative(&mut self, depth: usize) -> Result<(), Refusal> {
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            self.term(depth)?;
        }
        Ok(())
    }
    fn term(&mut self, depth: usize) -> Result<(), Refusal> {
        // Assertions take no quantifier with Unicode semantics.
        match (self.peek(), self.peek_at(1), self.peek_at(2)) {
            (Some(c @ ('^' | '$')), ..) => {
                self.at += 1;
                self.out.push(c);
                if depth == 0 {
                    self.record(|shape| if c == '^' { shape.start() } else { shape.end() });
                }
                return Ok(());
            }
            // An ECMA-262 word boundary reads `\w` as `[A-Za-z0-9_]`, as the
            // regex crate's ASCII word boundary does. A non-word boundary is
            // read but not matched.
            (Some('\\'), Some(c @ ('b' | 'B')), _) => {
                self.at += 2;
                if c == 'b' {
                    self.out.push_str("(?-u:\\b)");
                } else {
                    self.unsupported(Unsupported::NonWordBoundary);
                }
                if depth == 0 {
                    self.shape = None;
                }
                return Ok(());
            }
            (Some('('), Some('?'), Some('=' | '!')) => {
                let positive = self.peek_at(2) == Some('=');
                self.at += 3;
                self.out.push_str("(?:");
                self.group_body(depth)?;
                self.out.push(')');
                // Only existence of a match is observable. A positive lookahead
                // ending a top-level alternative is equivalent to consuming its
                // body there. Nested/negative/nonterminal assertions retain the
                // explicit unsupported result; they cannot use this equivalence.
                if !(positive && depth == 0 && matches!(self.peek(), None | Some('|'))) {
                    self.unsupported(Unsupported::Backtracking);
                }
                self.shape = None;
                return Ok(());
            }
            (Some('('), Some('?'), Some('<')) if matches!(self.peek_at(3), Some('=' | '!')) => {
                self.at += 4;
                return self.lookaround(depth);
            }
            _ => {}
        }
        let set = self.atom(depth)?;
        let count = self.quantifier()?;
        if depth == 0 {
            match (set, count) {
                (Some(set), Some((min, max))) => self.record(|shape| shape.run(set, min, max)),
                _ => self.shape = None,
            }
        }
        Ok(())
    }
    fn record(&mut self, term: impl FnOnce(&mut Shape) -> Option<()>) {
        if let Some(shape) = &mut self.shape
            && term(shape).is_none()
        {
            self.shape = None;
        }
    }
    fn lookaround(&mut self, depth: usize) -> Result<(), Refusal> {
        self.unsupported(Unsupported::Backtracking);
        self.group_body(depth)
    }
    fn group_body(&mut self, depth: usize) -> Result<(), Refusal> {
        if depth >= self.max_depth {
            return Err(Refusal::Depth);
        }
        self.disjunction(depth + 1)?;
        if !self.eat(')') {
            return Err(Refusal::Invalid);
        }
        Ok(())
    }
    // An atom; for one that matches a single character, the scalar values it
    // matches.
    fn atom(&mut self, depth: usize) -> Result<Option<Vec<(u32, u32)>>, Refusal> {
        Ok(match self.next()? {
            '.' => Some(self.set(DOT, true)),
            '(' => {
                if self.eat('?') {
                    match self.next()? {
                        ':' => self.out.push_str("(?:"),
                        '<' => {
                            group_name(self.chars, &mut self.at)?;
                            self.out.push('(');
                        }
                        _ => return Err(Refusal::Invalid),
                    }
                } else {
                    self.out.push('(');
                }
                self.group_body(depth)?;
                self.out.push(')');
                None
            }
            '[' => Some(self.class()?),
            '\\' => self.atom_escape()?,
            '*' | '+' | '?' | '{' | '}' | ']' | ')' | '|' => return Err(Refusal::Invalid),
            c => Some(self.literal(c as u32)),
        })
    }
    fn atom_escape(&mut self) -> Result<Option<Vec<(u32, u32)>>, Refusal> {
        match self.peek().ok_or(Refusal::Invalid)? {
            '1'..='9' => {
                let mut value: usize = 0;
                while let Some(digit) = self.peek().and_then(|c| c.to_digit(10)) {
                    value = value.saturating_mul(10).saturating_add(digit as usize);
                    self.at += 1;
                }
                if value > self.groups.count {
                    return Err(Refusal::Invalid);
                }
                self.unsupported(Unsupported::Backtracking);
                Ok(None)
            }
            'k' => {
                self.at += 1;
                if !self.eat('<') {
                    return Err(Refusal::Invalid);
                }
                let name = group_name(self.chars, &mut self.at)?;
                if !self.groups.names.contains(&name) {
                    return Err(Refusal::Invalid);
                }
                self.unsupported(Unsupported::Backtracking);
                Ok(None)
            }
            _ => Ok(Some(match self.class_escape(false)? {
                Member::Char(c) => self.literal(c),
                Member::Set(set) => self.set(&set, false),
            })),
        }
    }
    // A `CharacterClassEscape` or `CharacterEscape`, after its `\`; in a class,
    // also `\b` (backspace) and `\-`.
    fn class_escape(&mut self, in_class: bool) -> Result<Member, Refusal> {
        let c = self.next()?;
        let set = |ranges: &[(u32, u32)], negated: bool| {
            Member::Set(if negated {
                complement(ranges)
            } else {
                ranges.to_vec()
            })
        };
        Ok(match c {
            'd' | 'D' => set(DIGIT, c == 'D'),
            's' | 'S' => set(SPACE, c == 'S'),
            'w' | 'W' => set(WORD, c == 'W'),
            'p' | 'P' => match self.property()? {
                Some(ranges) => set(&ranges, c == 'P'),
                None => {
                    self.unsupported(Unsupported::PropertyEscape);
                    Member::Set(Vec::new())
                }
            },
            'f' => Member::Char(0x0C),
            'n' => Member::Char(0x0A),
            'r' => Member::Char(0x0D),
            't' => Member::Char(0x09),
            'v' => Member::Char(0x0B),
            'c' => {
                let letter = self.next()?;
                if !letter.is_ascii_alphabetic() {
                    return Err(Refusal::Invalid);
                }
                Member::Char(letter as u32 % 32)
            }
            '0' => {
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    return Err(Refusal::Invalid);
                }
                Member::Char(0)
            }
            'x' => Member::Char(hex(self.chars, &mut self.at, 2)?),
            'u' => Member::Char(unicode_escape(self.chars, &mut self.at)?),
            '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
            | '/' => Member::Char(c as u32),
            'b' if in_class => Member::Char(0x08),
            '-' if in_class => Member::Char('-' as u32),
            _ => return Err(Refusal::Invalid),
        })
    }
    // `{Name=Value}` or `{NameOrValue}` after `\p` or `\P`.
    fn property(&mut self) -> Result<Option<Vec<(u32, u32)>>, Refusal> {
        if !self.eat('{') {
            return Err(Refusal::Invalid);
        }
        let start = self.at;
        while self.peek().is_some_and(|c| c != '}') {
            self.at += 1;
        }
        let body: String = self.chars[start..self.at].iter().collect();
        if !self.eat('}') {
            return Err(Refusal::Invalid);
        }
        let (name, value) = match body.split_once('=') {
            Some((name, value)) => (Some(name), value),
            None => (None, body.as_str()),
        };
        if !crate::es2020_properties::valid(value, name) {
            return Err(Refusal::Invalid);
        }
        // ECMA names are case-sensitive and admit no loose matching. Validate
        // them above before asking the engine for its Unicode scalar ranges.
        // Expanding a single property cannot introduce engine-only syntax into
        // the caller's pattern. The ordinary form/automaton memory limits apply.
        let expression = format!("\\p{{{body}}}");
        let Ok(hir) = regex_syntax::Parser::new().parse(&expression) else {
            return Ok(None); // Valid ECMA property absent from this table set.
        };
        match hir.kind() {
            regex_syntax::hir::HirKind::Class(regex_syntax::hir::Class::Unicode(class)) => {
                Ok(Some(
                    class
                        .iter()
                        .map(|range| (range.start() as u32, range.end() as u32))
                        .collect(),
                ))
            }
            regex_syntax::hir::HirKind::Literal(literal) => {
                let text = std::str::from_utf8(&literal.0).expect("Unicode property literal");
                let mut chars = text.chars();
                let only = chars.next().expect("nonempty property literal");
                assert!(chars.next().is_none());
                Ok(Some(vec![(only as u32, only as u32)]))
            }
            _ => Ok(None),
        }
    }
    fn class(&mut self) -> Result<Vec<(u32, u32)>, Refusal> {
        let negated = self.eat('^');
        let mut ranges = Vec::new();
        loop {
            if self.eat(']') {
                break;
            }
            let first = self.class_atom()?;
            if self.peek() == Some('-') && self.peek_at(1).is_some_and(|c| c != ']') {
                self.at += 1;
                let last = self.class_atom()?;
                let (Member::Char(first), Member::Char(last)) = (first, last) else {
                    // A class escape cannot bound a range.
                    return Err(Refusal::Invalid);
                };
                if first > last {
                    return Err(Refusal::Invalid);
                }
                ranges.push((first, last));
            } else {
                match first {
                    Member::Char(c) => ranges.push((c, c)),
                    Member::Set(set) => {
                        ranges.extend(set);
                        // A short property escape can expand to hundreds of
                        // ranges. Merge each expansion immediately so repeated
                        // properties hold a union, never a copy per occurrence.
                        // Literal members between expansions remain bounded by
                        // the source length, as before property matching.
                        ranges = scalar(&ranges);
                    }
                }
            }
        }
        Ok(self.set(&ranges, negated))
    }
    fn class_atom(&mut self) -> Result<Member, Refusal> {
        match self.next()? {
            '\\' => self.class_escape(true),
            c => Ok(Member::Char(c as u32)),
        }
    }
    // A quantifier after an atom, and the counts it allows (one occurrence
    // when there is none); `None` when a count exceeds `u32`.
    fn quantifier(&mut self) -> Result<Option<(u32, Option<u32>)>, Refusal> {
        let count = match self.peek() {
            Some(c @ ('*' | '+' | '?')) => {
                self.at += 1;
                self.out.push(c);
                Some(match c {
                    '*' => (0, None),
                    '+' => (1, None),
                    _ => (0, Some(1)),
                })
            }
            Some('{') => {
                self.at += 1;
                let low = self.decimal()?;
                let high = if self.eat(',') {
                    if self.peek() == Some('}') {
                        None
                    } else {
                        Some(self.decimal()?)
                    }
                } else {
                    Some(low.clone())
                };
                if !self.eat('}') {
                    return Err(Refusal::Invalid);
                }
                match &high {
                    Some(high) if (low.len(), low.as_str()) > (high.len(), high.as_str()) => {
                        return Err(Refusal::Invalid);
                    }
                    Some(high) if *high == low => {
                        let _ = write!(self.out, "{{{low}}}");
                    }
                    Some(high) => {
                        let _ = write!(self.out, "{{{low},{high}}}");
                    }
                    None => {
                        let _ = write!(self.out, "{{{low},}}");
                    }
                }
                let parse = |digits: &str| digits.parse::<u32>().ok();
                match (parse(&low), high.as_deref().map(parse)) {
                    (Some(low), Some(Some(high))) => Some((low, Some(high))),
                    (Some(low), None) => Some((low, None)),
                    _ => None,
                }
            }
            _ => return Ok(Some((1, Some(1)))),
        };
        if self.eat('?') {
            self.out.push('?');
        }
        Ok(count)
    }
    // `DecimalDigits` without leading zeros, so two values compare by length
    // and then by text.
    fn decimal(&mut self) -> Result<String, Refusal> {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        if self.at == start {
            return Err(Refusal::Invalid);
        }
        let digits: String = self.chars[start..self.at].iter().collect();
        let trimmed = digits.trim_start_matches('0');
        Ok(if trimmed.is_empty() { "0" } else { trimmed }.to_owned())
    }
    // Whether the form has outgrown its limit; a set or literal is then read
    // but not written.
    fn oversized(&mut self) -> bool {
        let oversized = self.out.len() > self.max_form;
        if oversized {
            self.unsupported(Unsupported::Size);
        }
        oversized
    }
    fn literal(&mut self, c: u32) -> Vec<(u32, u32)> {
        if self.oversized() {
            return match char::from_u32(c) {
                Some(_) => vec![(c, c)],
                None => Vec::new(),
            };
        }
        match char::from_u32(c) {
            Some(c) if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/') => {
                self.out.push(c)
            }
            Some(_) => {
                let _ = write!(self.out, r"\x{{{c:X}}}");
            }
            // A lone surrogate: nothing a scalar string holds.
            None => {
                self.out.push_str(NOTHING);
                return Vec::new();
            }
        }
        vec![(c, c)]
    }
    // A class of `ranges`, or of their complement, written as the positive
    // class of the scalar values it matches, which it returns.
    fn set(&mut self, ranges: &[(u32, u32)], negated: bool) -> Vec<(u32, u32)> {
        let ranges = if negated {
            complement(ranges)
        } else {
            scalar(ranges)
        };
        if self.oversized() {
            return ranges;
        }
        if ranges.is_empty() {
            self.out.push_str(NOTHING);
        } else {
            self.out.push('[');
            for &(first, last) in &ranges {
                push_class_char(&mut self.out, first);
                if last > first {
                    self.out.push('-');
                    push_class_char(&mut self.out, last);
                }
            }
            self.out.push(']');
        }
        ranges
    }
}
fn push_class_char(out: &mut String, c: u32) {
    match char::from_u32(c) {
        Some(c) if c.is_ascii_alphanumeric() => out.push(c),
        _ => {
            let _ = write!(out, r"\x{{{c:X}}}");
        }
    }
}
// Sorted, merged ranges without surrogate code points.
fn scalar(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(sorted.len());
    for (first, last) in sorted {
        let mut pieces = [(first, last), (1, 0)];
        if first <= SURROGATES.1 && last >= SURROGATES.0 {
            pieces = [(first, SURROGATES.0 - 1), (SURROGATES.1 + 1, last)];
        }
        for (first, last) in pieces {
            if first > last {
                continue;
            }
            match merged.last_mut() {
                Some(previous) if first <= previous.1.saturating_add(1) => {
                    previous.1 = previous.1.max(last);
                }
                _ => merged.push((first, last)),
            }
        }
    }
    merged
}
// Every scalar value not in `ranges`.
fn complement(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut next = 0;
    for &(first, last) in &scalar(ranges) {
        if first > next {
            out.push((next, first - 1));
        }
        next = last + 1;
    }
    if next <= 0x10FFFF {
        out.push((next, 0x10FFFF));
    }
    scalar(&out)
}

/// An anchored sequence of single-character runs that matches without an
/// automaton: every run but the last has a fixed count, so each character's
/// run is known from its position. The fixed runs are steps: a character
/// matched once is literal text, adjacent literal characters share one step
/// and are compared as bytes, and adjacent runs of one set share one step.
/// The last run is a step too when its count is fixed. A run carries its
/// set's ASCII members as a bitmap, so an ASCII character is tested by its
/// byte alone, and its members past ASCII, if any, are its share of
/// `ranges`, which a match walks in order. A short fixed part of ASCII is a
/// table of the members each of its characters may be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Simple {
    // Whether the match must reach the end of the string.
    end: bool,
    // The ASCII members each character of the fixed part may be, when every
    // step is ASCII text or a run of a set without members past ASCII, they
    // cover at most [`TABLE`] characters and two at least are runs;
    // otherwise empty, and the steps hold the fixed part.
    table: Box<[[u64; 2]]>,
    steps: Box<[Step]>,
    // The last run when its count is not fixed.
    last: Option<Last>,
    // The literal steps' text, one after another.
    text: Box<str>,
    // The members past ASCII of each step's set, one step after another,
    // then the last run's.
    ranges: Box<[(u32, u32)]>,
}
/// One fixed step of a run matcher: `count` characters of a set, whose ASCII
/// members are `ascii`, one bit each in two words so that a test shifts one
/// machine word, and whose members past ASCII are its next `wide` ranges; or,
/// when `wide` is [`TEXT`], the next `count` bytes of the matcher's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Step {
    ascii: [u64; 2],
    wide: u32,
    count: u32,
}
/// The last run of a run matcher, between `min` and `max` characters of the
/// set whose ASCII members are `ascii` and whose wider ones are the ranges
/// the steps leave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Last {
    ascii: [u64; 2],
    min: u32,
    max: Option<u32>,
}
// A step's `wide` for literal text.
const TEXT: u32 = u32::MAX;
// The most characters a run matcher's fixed part is tabled for.
const TABLE: usize = 64;
impl Simple {
    /// The heap bytes the matcher holds, which a schema's pattern
    /// memory counts: 16 for each character of its table, 24 for each step,
    /// 8 for each range of members past ASCII and a byte for each byte of
    /// text, on every host.
    pub(crate) fn heap_bytes(&self) -> usize {
        size_of_val(&*self.table)
            + size_of_val(&*self.steps)
            + self.text.len()
            + size_of_val(&*self.ranges)
    }
    /// Whether `text` matches, as the pattern's form would. A matcher that
    /// is its last run alone and one with a fixed part each match through a
    /// body of their own, which compiles each one's loops apart.
    #[inline]
    pub(crate) fn is_match(&self, text: &str) -> bool {
        if self.table.is_empty() && self.steps.is_empty() {
            self.last_alone(text)
        } else {
            self.after_fixed(text)
        }
    }
    #[inline(never)]
    fn last_alone(&self, text: &str) -> bool {
        self.last_run(text, &self.ranges)
    }
    #[inline(never)]
    fn after_fixed(&self, text: &str) -> bool {
        let mut rest = text;
        if !self.table.is_empty() {
            let Some(head) = rest.as_bytes().get(..self.table.len()) else {
                return false;
            };
            for (&byte, ascii) in head.iter().zip(&self.table) {
                if byte >= 0x80 || !holds(ascii, byte) {
                    return false;
                }
            }
            // The table's characters are ASCII, so the rest starts one.
            rest = &rest[head.len()..];
        }
        // Steps without text match through a loop that never tests for it.
        if self.text.is_empty() {
            self.steps::<false>(rest)
        } else {
            self.steps::<true>(rest)
        }
    }
    // Whether `rest` matches the steps and then the last run, the steps
    // holding text only when `TEXT_STEPS` holds.
    #[inline(always)]
    fn steps<const TEXT_STEPS: bool>(&self, rest: &str) -> bool {
        let (mut literal, mut ranges) = (0, &*self.ranges);
        let mut chars = rest.chars();
        for step in &self.steps {
            if TEXT_STEPS && step.wide == TEXT {
                let length = step.count as usize;
                let expected = &self.text.as_bytes()[literal..literal + length];
                literal += length;
                let rest = chars.as_str();
                match rest.as_bytes().get(..length) {
                    // The text is whole characters, so the rest starts one.
                    Some(found) if same(found, expected) => chars = rest[length..].chars(),
                    _ => return false,
                }
                continue;
            }
            let (wide, after) = ranges.split_at(step.wide as usize);
            ranges = after;
            for _ in 0..step.count {
                match chars.next() {
                    Some(c) if member(&step.ascii, wide, c) => {}
                    _ => return false,
                }
            }
        }
        let rest = chars.as_str();
        self.last_run(rest, ranges)
    }
    // Whether `rest`, after the fixed part, matches the last run, whose
    // members past ASCII are `ranges`.
    #[inline(always)]
    fn last_run(&self, rest: &str, ranges: &[(u32, u32)]) -> bool {
        let Some(last) = &self.last else {
            return !self.end || rest.is_empty();
        };
        // Held as an `Option`, for which the loop past ASCII below compiles
        // to faster code than for the slice itself.
        let wide = (!ranges.is_empty()).then_some(ranges);
        if !self.end {
            // The match need not reach the end, so the run's least count
            // decides it.
            let mut chars = rest.chars();
            return (0..last.min)
                .all(|_| chars.next().is_some_and(|c| member(&last.ascii, ranges, c)));
        }
        let max = last.max.map_or(u64::MAX, u64::from);
        let (bytes, mut at, mut count) = (rest.as_bytes(), 0, 0u64);
        // ASCII characters are tested by their bytes; from the first
        // character past ASCII, the rest are read as characters.
        while let Some(&byte) = bytes.get(at) {
            if byte >= 0x80 {
                break;
            }
            at += 1;
            count += 1;
            if !holds(&last.ascii, byte) || count > max {
                return false;
            }
        }
        for c in rest[at..].chars() {
            count += 1;
            let c = c as u32;
            let member = if c < 0x80 {
                holds(&last.ascii, c as u8)
            } else {
                wide.is_some_and(|wide| among(wide, c))
            };
            if !member || count > max {
                return false;
            }
        }
        count >= u64::from(last.min)
    }
}
// Whether the ASCII character `byte` is among the members `ascii`.
#[inline(always)]
fn holds(ascii: &[u64; 2], byte: u8) -> bool {
    ascii[usize::from(byte >> 6)] >> (byte & 63) & 1 == 1
}
// Whether `c` is in the set whose ASCII members are `ascii` and whose wider
// ones are `wide`.
#[inline(always)]
fn member(ascii: &[u64; 2], wide: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    if c < 0x80 {
        holds(ascii, c as u8)
    } else {
        among(wide, c)
    }
}
// Whether the scalar value `c` is among the sorted, disjoint `ranges`.
#[inline(always)]
fn among(ranges: &[(u32, u32)], c: u32) -> bool {
    ranges
        .binary_search_by(|&(first, last)| {
            if last < c {
                std::cmp::Ordering::Less
            } else if first > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}
// Whether two equally long byte strings are equal. Short ones, as literal
// steps between runs mostly are, compare in a loop rather than a call.
fn same(found: &[u8], expected: &[u8]) -> bool {
    if expected.len() <= 16 {
        found.iter().zip(expected).all(|(a, b)| a == b)
    } else {
        found == expected
    }
}
/// `items` in an allocation of exactly their size. A vector shrunk in place
/// can keep its larger block, as macOS's allocator does, so one that has
/// spare capacity is moved into a new one.
pub(crate) fn exact<T>(items: Vec<T>) -> Box<[T]> {
    if items.len() == items.capacity() {
        return items.into_boxed_slice();
    }
    let mut moved = Vec::with_capacity(items.len());
    moved.extend(items);
    moved.into_boxed_slice()
}
/// A run matcher being read, while the pattern is one anchored sequence of
/// single-character runs.
#[derive(Default)]
struct Shape {
    // Whether the start anchor has been read, and the end anchor.
    start: bool,
    end: bool,
    steps: Vec<Step>,
    text: String,
    ranges: Vec<(u32, u32)>,
    // The run read last, which is the last run unless another follows.
    pending: Option<Pending>,
}
// A run as the parser reads it: its set's members and its counts.
struct Pending {
    members: Vec<(u32, u32)>,
    min: u32,
    max: Option<u32>,
}
impl Shape {
    // Each method answers `None` when the pattern is not such a sequence: it
    // starts with the start anchor, then holds runs, at least one, and then
    // at most the end anchor, and every run but the last has a fixed count.
    fn start(&mut self) -> Option<()> {
        (!self.start).then(|| self.start = true)
    }
    fn end(&mut self) -> Option<()> {
        (self.start && !self.end).then(|| self.end = true)
    }
    fn run(&mut self, set: Vec<(u32, u32)>, min: u32, max: Option<u32>) -> Option<()> {
        if !self.start || self.end {
            return None;
        }
        if let Some(run) = self.pending.take() {
            if run.max != Some(run.min) {
                return None;
            }
            self.fixed(&run.members, run.min)?;
        }
        self.pending = Some(Pending {
            members: set,
            min,
            max,
        });
        Some(())
    }
    fn fixed(&mut self, members: &[(u32, u32)], count: u32) -> Option<()> {
        if count == 0 {
            return Some(());
        }
        if let (1, &[(first, last)]) = (count, members)
            && first == last
            && let Some(c) = char::from_u32(first)
        {
            let length = c.len_utf8() as u32;
            match self.steps.last_mut() {
                Some(step) if step.wide == TEXT && step.count <= u32::MAX - length => {
                    step.count += length;
                }
                _ => self.steps.push(Step {
                    ascii: [0; 2],
                    wide: TEXT,
                    count: length,
                }),
            }
            self.text.push(c);
            return Some(());
        }
        let start = self.ranges.len();
        let ascii = self.set(members);
        let wide = u32::try_from(self.ranges.len() - start)
            .ok()
            .filter(|&wide| wide < TEXT)?;
        // A run of the set the step before is a run of, whose ranges are
        // the ones just before these, joins it.
        if let Some(step) = self.steps.last_mut()
            && step.wide == wide
            && step.ascii == ascii
            && step.count <= u32::MAX - count
            && self.ranges[..start].ends_with(&self.ranges[start..])
        {
            step.count += count;
            self.ranges.truncate(start);
            return Some(());
        }
        self.steps.push(Step { ascii, wide, count });
        Some(())
    }
    // The ASCII members of the set of `members`, which are sorted and
    // disjoint, adding its members past ASCII to the ranges.
    fn set(&mut self, members: &[(u32, u32)]) -> [u64; 2] {
        let mut ascii = [0u64; 2];
        for &(first, last) in members {
            for c in first..=last.min(127) {
                ascii[c as usize >> 6] |= 1 << (c & 63);
            }
        }
        self.ranges.extend(
            members
                .iter()
                .filter(|&&(_, last)| last >= 128)
                .map(|&(first, last)| (first.max(128), last)),
        );
        ascii
    }
    // The fixed part as one ASCII set for each character, when every step
    // is ASCII text or a run of a set without members past ASCII, they cover
    // at most [`TABLE`] characters and two at least are runs; otherwise
    // empty. One run, and text alone, match as fast as steps.
    fn table(&self) -> Box<[[u64; 2]]> {
        let (mut characters, mut runs) = (0, 0);
        for step in &self.steps {
            characters += step.count as usize;
            runs += usize::from(step.wide == 0);
            if characters > TABLE || !matches!(step.wide, TEXT | 0) {
                return Box::default();
            }
        }
        if runs < 2 || !self.text.is_ascii() {
            return Box::default();
        }
        let mut table = Vec::with_capacity(characters);
        let mut text = self.text.bytes();
        for step in &self.steps {
            for _ in 0..step.count {
                let ascii = if step.wide == TEXT {
                    let byte = text.next().expect("a text step's bytes are the text's");
                    let mut ascii = [0; 2];
                    ascii[usize::from(byte >> 6)] = 1 << (byte & 63);
                    ascii
                } else {
                    step.ascii
                };
                table.push(ascii);
            }
        }
        table.into_boxed_slice()
    }
    fn finish(mut self) -> Option<Simple> {
        let run = self.pending.take()?;
        let last = if run.max == Some(run.min) {
            self.fixed(&run.members, run.min)?;
            None
        } else {
            Some(Last {
                ascii: self.set(&run.members),
                min: run.min,
                max: run.max,
            })
        };
        let table = self.table();
        if !table.is_empty() {
            self.steps.clear();
            self.text.clear();
        }
        let mut text = String::with_capacity(self.text.len());
        text.push_str(&self.text);
        Some(Simple {
            end: self.end,
            table,
            steps: exact(self.steps),
            last,
            text: text.into_boxed_str(),
            ranges: exact(self.ranges),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_anchored_literals_equal_the_general_unicode_reader() {
        for body in ["a".into(), "AZaz09_/-".into(), "a".repeat(10000)] {
            for end in ["", "$"] {
                let source = format!("^{body}{end}");
                for maximum in [0, source.len() - 1, source.len(), source.len() + 1] {
                    assert_eq!(read(&source, 0, maximum), read_general(&source, 0, maximum));
                }
                let Reading::Linear(linear) = read(&source, 0, source.len()).unwrap() else {
                    panic!()
                };
                let matcher = linear.simple.unwrap();
                for text in [
                    body.clone(),
                    format!("{body}x"),
                    format!("x{body}"),
                    format!("{body}\n"),
                    String::new(),
                ] {
                    assert_eq!(
                        matcher.is_match(&text),
                        regex::Regex::new(&linear.form).unwrap().is_match(&text),
                        "{source}/{text}"
                    );
                }
            }
        }
        for source in [
            "^",
            "^$",
            "a",
            "a$",
            "^a$$",
            "^^a",
            "^(a)$",
            "^a+$",
            "^[a]$",
            r"^\x61$",
            r"^\u0061$",
            "^é$",
            "^a.b$",
            "^a|b$",
            "^a b$",
            "^a\n$",
        ] {
            assert!(anchored_literal(source).is_none(), "{source}");
            assert_eq!(read(source, 32, 1 << 20), read_general(source, 32, 1 << 20));
        }
    }

    #[test]
    fn a_run_holds_exactly_its_set_inside_and_beyond_ascii() {
        for set in [
            vec![],
            vec![(0x30, 0x39)],
            vec![(0x30, 0x39), (0x5f, 0x5f), (0x61, 0x7a)],
            vec![(0, 0x7f)],
            vec![(0x7f, 0x80)],
            vec![(0x64, 0xc8), (0x1f600, 0x1f64f)],
            vec![(0, 0x10ffff)],
        ] {
            let mut shape = Shape::default();
            shape.start().unwrap();
            shape.run(set.clone(), 1, None).unwrap();
            let simple = shape.finish().unwrap();
            let last = simple.last.unwrap();
            let holds = |c: char| member(&last.ascii, &simple.ranges, c);
            for c in (0..0x300).chain([0x1f5ff, 0x1f600, 0x1f64f, 0x1f650, 0x10ffff]) {
                let Some(c) = char::from_u32(c) else { continue };
                let member = set
                    .iter()
                    .any(|&(first, last)| (first..=last).contains(&(c as u32)));
                assert_eq!(holds(c), member, "{set:?} {c:?}");
            }
        }
    }
    fn simple(pattern: &str) -> Option<Simple> {
        match read(pattern, 32, 1 << 20) {
            Ok(Reading::Linear(linear)) => linear.simple,
            other => panic!("{pattern}: {other:?}"),
        }
    }
    // Literal characters share one step of text, adjacent runs of one set
    // share one step, a fixed last run is a step, each run's members past
    // ASCII follow the step before's, and a short fixed part of ASCII sets
    // and text is a table of a set for each character.
    #[test]
    fn a_run_matcher_holds_a_step_for_each_change_of_set() {
        let shape = |matcher: &Simple| -> Vec<(u32, u32)> {
            matcher
                .steps
                .iter()
                .map(|step| (step.wide, step.count))
                .collect()
        };
        let literal = format!("^t0x{}$", "a".repeat(9_997));
        let matcher = simple(&literal).unwrap();
        assert_eq!(shape(&matcher), [(TEXT, 10_000)]);
        assert!(matcher.table.is_empty() && matcher.ranges.is_empty());
        assert_eq!(matcher.last, None);
        assert_eq!(
            (
                size_of::<Step>(),
                size_of::<[u64; 2]>(),
                size_of::<(u32, u32)>()
            ),
            (24, 16, 8)
        );
        assert_eq!(matcher.heap_bytes(), 24 + 10_000);
        assert!(matcher.is_match(&literal[1..literal.len() - 1]));
        assert!(!matcher.is_match(&literal[1..literal.len() - 2]));
        // Three runs and text of ten ASCII characters are a table.
        let date = simple(r"^\d\d\d\d-\d{2}-[0-9][0-9]$").unwrap();
        assert!(date.steps.is_empty() && date.text.is_empty());
        assert_eq!((date.table.len(), date.heap_bytes()), (10, 160));
        assert!(date.is_match("2026-10-25") && !date.is_match("2026-10-2x"));
        assert!(!date.is_match("2026-10-2") && !date.is_match("2026-10-2é"));
        // Past the table's characters, and with one run, they are steps.
        let long = simple(r"^\d{40}-\d{40}$").unwrap();
        assert_eq!(shape(&long), [(0, 40), (TEXT, 1), (0, 40)]);
        let one = simple(r"^\d\d\d\d$").unwrap();
        assert_eq!(shape(&one), [(0, 4)]);
        let open = simple(r"^é\d{3}é[a-zé]{2,}").unwrap();
        assert_eq!(shape(&open), [(TEXT, 2), (0, 3), (TEXT, 2)]);
        assert_eq!(open.last.map(|last| (last.min, last.max)), Some((2, None)));
        assert_eq!(open.ranges.len(), 1);
        assert!(open.is_match("é123éabé!") && !open.is_match("é123éa!"));
        // `.` holds three ranges past ASCII; adjacent runs of it share them.
        let dots = simple("^..a.$").unwrap();
        assert_eq!(shape(&dots), [(3, 2), (TEXT, 1), (3, 1)]);
        assert_eq!(dots.ranges.len(), 6);
        assert!(!dots.is_match("é😀a\u{2028}") && dots.is_match("é😀a中"));
        // A pattern whose runs vary before its last, or that is not one
        // anchored sequence, has no run matcher.
        for pattern in [
            "^a*b", "a", "^", "^$", "^a|b", "^(a)", "^a$b", "^^a", r"^a\b",
        ] {
            assert!(simple(pattern).is_none(), "{pattern}");
        }
    }
    // A small deterministic generator, so the differential needs no crate.
    struct Generator(u64);
    impl Generator {
        fn below(&mut self, bound: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % bound as u64) as usize
        }
        fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
            items[self.below(items.len())]
        }
    }
    // Run matchers decide as their forms do in the regex crate, for generated
    // anchored sequences of literals and sets with every kind of count, on
    // strings built from members of the patterns' sets, some changed by one
    // character, and on strings of characters drawn at random.
    #[test]
    fn run_matchers_match_as_their_forms_do() {
        // Each atom with characters it matches, if any.
        const ATOMS: &[(&str, &[&str])] = &[
            ("a", &["a"]),
            ("b", &["b"]),
            ("é", &["é"]),
            ("😀", &["😀"]),
            ("-", &["-"]),
            (r"\.", &["."]),
            (r"\d", &["1", "9"]),
            (r"\w", &["a", "_", "1"]),
            (r"\s", &[" ", "\n", "\u{3000}"]),
            (r"\S", &["a", "é"]),
            (".", &["a", "é", "😀"]),
            ("[ab]", &["a", "b"]),
            ("[^a]", &["b", "é", "\n"]),
            ("[a-zé]", &["c", "é"]),
            ("[😀-😂]", &["😁"]),
            (r"[à-ÿ]", &["é", "è"]),
            (r"\ud800", &[]),
            ("[]", &[]),
        ];
        const COUNTS: &[&str] = &["", "", "", "{2}", "{0}", "{1,3}", "{2,}", "*", "+", "?"];
        const FIXED: &[&str] = &["", "", "{2}", "{0}", "{1}", "{3}"];
        const OTHERS: &[&str] = &[
            "a", "c", "é", "è", "😀", "😁", "-", ".", "1", " ", "\n", "_",
        ];
        let mut generator = Generator(0x9E37_79B9_7F4A_7C15);
        let (mut matched, mut unmatched) = (0, 0);
        for _ in 0..4_000 {
            let mut pattern = String::from("^");
            let mut runs = Vec::new();
            let count = 1 + generator.below(6);
            for at in 0..count {
                let (atom, members) = ATOMS[generator.below(ATOMS.len())];
                let quantifier = generator.pick(if at + 1 == count { COUNTS } else { FIXED });
                pattern.push_str(atom);
                pattern.push_str(quantifier);
                runs.push((members, quantifier));
            }
            if generator.below(2) == 0 {
                pattern.push('$');
            }
            let Ok(Reading::Linear(linear)) = read(&pattern, 32, 1 << 20) else {
                panic!("{pattern}")
            };
            let matcher = linear.simple.expect(&pattern);
            let regex = regex::Regex::new(&linear.form).expect(&pattern);
            for attempt in 0..40 {
                let mut text = String::new();
                if attempt % 2 == 0 {
                    for &(members, quantifier) in &runs {
                        let times = match quantifier {
                            "" | "{1}" => 1,
                            "{0}" => 0,
                            "{2}" => 2,
                            "{3}" => 3,
                            _ => generator.below(5),
                        };
                        for _ in 0..times {
                            text.push_str(match members {
                                [] => generator.pick(OTHERS),
                                members => generator.pick(members),
                            });
                        }
                    }
                    if attempt % 4 == 0 {
                        text.push_str(generator.pick(OTHERS));
                    }
                    if attempt % 8 == 2 && !text.is_empty() {
                        let mut chars: Vec<char> = text.chars().collect();
                        let at = generator.below(chars.len());
                        chars[at] = generator.pick(OTHERS).chars().next().unwrap();
                        text = chars.into_iter().collect();
                    }
                } else {
                    for _ in 0..generator.below(9) {
                        text.push_str(generator.pick(OTHERS));
                    }
                }
                let expected = regex.is_match(&text);
                assert_eq!(matcher.is_match(&text), expected, "{pattern} on {text:?}");
                if expected {
                    matched += 1;
                } else {
                    unmatched += 1;
                }
            }
        }
        // 4,000 patterns with 40 strings each.
        assert_eq!((matched, unmatched), (38_142, 121_858));
    }
    fn linear(pattern: &str) -> String {
        match read(pattern, 32, 1 << 20) {
            Ok(Reading::Linear(linear)) => linear.form,
            other => panic!("{pattern}: {other:?}"),
        }
    }
    #[test]
    fn linear_forms_spell_sets_out() {
        for (pattern, form) in [
            ("", ""),
            ("^x-", "^x-"),
            ("^[a-z0-9-]{8,36}$", r"^[\x{2D}0-9a-z]{8,36}$"),
            (r"^\d+$", "^[0-9]+$"),
            (
                ".",
                r"[\x{0}-\x{9}\x{B}-\x{C}\x{E}-\x{2027}\x{202A}-\x{D7FF}\x{E000}-\x{10FFFF}]",
            ),
            (r"[^\D]", "[0-9]"),
            (r"[^\u0000-\uffff]", r"[\x{10000}-\x{10FFFF}]"),
            (
                r"[^\S]",
                r"[\x{9}-\x{D}\x{20}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}-\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]",
            ),
            ("(a|b)", "(a|b)"),
            ("(?<n>a)(?:b)", "(a)(?:b)"),
            ("a{002,010}?", "a{2,10}?"),
            ("a{3}", "a{3}"),
            ("a{3,}", "a{3,}"),
            ("\u{1F600}", r"\x{1F600}"),
            (r"\ud83d\ude00", r"\x{1F600}"),
            (r"\ud83d", NOTHING),
            (r"[\ud800-\udfff]", NOTHING),
            (r"[^\ud800]", ANYTHING),
            ("[]", NOTHING),
            ("[^]", ANYTHING),
            (r"[\b]", r"[\x{8}]"),
            (r"^a\b", r"^a(?-u:\b)"),
        ] {
            assert_eq!(linear(pattern), form, "{pattern}");
        }
    }
    #[test]
    fn no_form_negates_a_class() {
        for pattern in [
            r"[^\S]",
            r"[^\D]",
            r"[^\W]",
            r"[^\s]",
            r"[^a]",
            r"[^\u0000-\uffff]",
            r"[^\ud7ff\ue000]",
            r"\S",
            r"\D",
            r"\W",
            ".",
            "[^]",
            "[]",
            r"\ud800",
        ] {
            assert!(!linear(pattern).contains("[^"), "{pattern}");
        }
    }
    #[test]
    fn invalid_patterns_are_refused() {
        for pattern in [
            "(",
            ")",
            "[",
            "a{",
            "a{1",
            "a{2,1}",
            "*",
            "a**",
            "^*",
            r"\b+",
            "(?=a)*",
            "]",
            "}",
            r"\-",
            r"\a",
            r"\1",
            r"\k<x>",
            r"\01",
            r"\c1",
            r"\x1",
            r"\u12",
            r"\u{110000}",
            r"\p{Nope}",
            r"\p{Script=Kawi}",
            "[b-a]",
            r"[\d-z]",
            r"[a-\d]",
            r"[\B]",
            r"[\1]",
            "(?<a>x)(?<a>y)",
            r"(?<\u0061>x)(?<a>y)",
            r"\B(?<a>x)(?<\u0061>y)",
            r"(?<a>x)\k<missing>",
            "(?i:a)",
            "(?<1a>x)",
            r"\p",
            "(?",
        ] {
            assert_eq!(
                read(pattern, 32, 1 << 20),
                Err(Refusal::Invalid),
                "{pattern}"
            );
        }
    }
    #[test]
    fn unsupported_assertions_and_backreferences_are_read_but_not_matched() {
        for (pattern, kind) in [
            ("^(?=a)b", Unsupported::Backtracking),
            ("(?<=a)b", Unsupported::Backtracking),
            (r"(a)\1", Unsupported::Backtracking),
            (r"\1(a)", Unsupported::Backtracking),
            (r"(?<x>a)\k<x>", Unsupported::Backtracking),
            (r"\k<x>(?<x>a)", Unsupported::Backtracking),
            (r"(?<z>x)(?<a>y)\2", Unsupported::Backtracking),
            ("(?<é>x)\\k<é>", Unsupported::Backtracking),
            (r"\p{L}(?=a)b", Unsupported::Backtracking),
            (r"\Bx", Unsupported::NonWordBoundary),
            (r"^.*$|\B", Unsupported::NonWordBoundary),
            (r"\B(?=a)b", Unsupported::Backtracking),
        ] {
            assert_eq!(
                read(pattern, 32, 1 << 20),
                Ok(Reading::Unsupported(kind)),
                "{pattern}"
            );
        }
    }
    #[test]
    fn many_names_preserve_source_order_and_reference_identity() {
        let prefix = "q".repeat(128);
        let mut source = String::new();
        for index in (0..1024).rev() {
            write!(&mut source, "(?<{prefix}{index:04x}>a)").unwrap();
        }
        assert_eq!(linear(&source), "(a)".repeat(1024));
        write!(&mut source, r"\k<{prefix}0000>").unwrap();
        assert_eq!(
            read(&source, 32, 1 << 20),
            Ok(Reading::Unsupported(Unsupported::Backtracking))
        );
        write!(&mut source, "(?<{prefix}03ff>b)").unwrap();
        assert_eq!(read(&source, 32, 1 << 20), Err(Refusal::Invalid));
    }
    #[test]
    fn group_nesting_is_bounded() {
        let nested = |n: usize| format!("{}a{}", "(".repeat(n), ")".repeat(n));
        assert!(read(&nested(32), 32, 1 << 20).is_ok());
        assert_eq!(read(&nested(33), 32, 1 << 20), Err(Refusal::Depth));
        assert_eq!(read(&nested(32), 31, 1 << 20), Err(Refusal::Depth));
        assert!(read(&nested(31), 31, 1 << 20).is_ok());
        assert!(read(&nested(32), usize::MAX, 1 << 20).is_ok());
        assert_eq!(read(&nested(33), usize::MAX, 1 << 20), Err(Refusal::Depth));
        assert_eq!(
            read(&nested(10_000), usize::MAX, 1 << 20),
            Err(Refusal::Depth)
        );
        assert!(read(r"\b", 0, 1 << 20).is_ok());
        assert_eq!(read("(a)", 0, 1 << 20), Err(Refusal::Depth));
    }
    #[test]
    fn a_form_past_its_limit_is_not_written() {
        // Each `\S` is written as a class of about 200 bytes.
        let pattern = r"\S".repeat(50_000);
        assert_eq!(
            read(&pattern, 32, 100_000),
            Ok(Reading::Unsupported(Unsupported::Size))
        );
        assert!(matches!(
            read(&pattern, 32, 1 << 24),
            Ok(Reading::Linear(_))
        ));
        // An invalid pattern is still refused after the form stops.
        assert_eq!(
            read(&format!("{pattern}("), 32, 100_000),
            Err(Refusal::Invalid)
        );
    }
}
