//! Schema regular expressions as OpenBindings reads them: ECMA-262 11th edition
//! patterns with Unicode semantics (the `u` flag) and no other flag, for
//! `pattern` values and `patternProperties` names alike.
//!
//! Every pattern is parsed here before the JSON Schema library sees it. A
//! pattern that is not valid with Unicode semantics is refused. A valid pattern
//! is matched by a linear-time engine (the `regex` crate) when it has a form in
//! that engine's syntax with the same meaning: [`Reading::Linear`] holds that
//! form, which spells every character set out as code points so the two
//! dialects read it alike. Three kinds of valid pattern are not matched: one
//! that only a backtracking engine can match (lookaround and backreferences),
//! one with a Unicode property escape, whose character tables the evaluator
//! does not qualify, and one with a non-word-boundary assertion `\B`. The word
//! boundary `\b` has a form: ECMA-262 reads it with `\w` as `[A-Za-z0-9_]`, as
//! the regex crate's ASCII word boundary does, and an ASCII word boundary never
//! falls inside a multi-byte character. Its complement `(?-u:\B)` does, and
//! the regex crate's `is_match` then answers no for a pattern that another
//! branch matches (`\x{E9}|(?-u:\B)` on "aéa"), so `\B` is not matched.
//!
//! The values the evaluator checks are strings of Unicode scalar values: a
//! string with an unpaired surrogate is refused before any pattern reads it. A
//! pattern can still name a lone surrogate with an escape; such an atom matches
//! nothing those strings hold, and a class gains nothing from it.
use std::fmt::Write;

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
/// Why a valid pattern is not matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// It needs a backtracking engine.
    Backtracking,
    /// It has a Unicode property escape.
    PropertyEscape,
    /// It has a non-word-boundary assertion, `\B`.
    NonWordBoundary,
    /// Its form is longer than the pattern byte limit, or exceeds the
    /// linear-time engine's compiled size limit, which is found when the
    /// pattern is first matched.
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

/// The compiled size the linear-time engine allows each pattern's form, in
/// bytes. Counted repetitions of large sets, such as `^.{1,10000}$`, need more
/// than the engine's default of 10 MiB; this bounds each pattern's automaton
/// instead.
pub(crate) const COMPILED_SIZE: usize = 32 * 1024 * 1024;

/// The pattern's form compiled for the linear-time engine, within
/// [`COMPILED_SIZE`]; `None` when it exceeds that size.
pub(crate) fn compile(form: &str) -> Option<regex::Regex> {
    regex::RegexBuilder::new(form)
        .size_limit(COMPILED_SIZE)
        .build()
        .ok()
}

/// Read one pattern, refusing groups nested more than `max_depth` deep. A
/// valid pattern whose form would be longer than `max_form` bytes is read as
/// [`Unsupported::Size`], and its form is not written out.
pub(crate) fn read(pattern: &str, max_depth: usize, max_form: usize) -> Result<Reading, Refusal> {
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
        shape: Some(Vec::new()),
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
            simple: parser.shape.and_then(Simple::new),
            form: parser.out,
        }),
    })
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
    names: Vec<String>,
}
impl Groups {
    fn scan(chars: &[char]) -> Result<Self, Refusal> {
        let mut groups = Groups {
            count: 0,
            names: Vec::new(),
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
                        if let Ok(name) = group_name(chars, &mut end) {
                            if groups.names.contains(&name) {
                                return Err(Refusal::Invalid);
                            }
                            groups.names.push(name);
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

mod identifier {
    //! `ID_Start` and `ID_Continue`, for group names, read from the `regex`
    //! crate's Unicode tables.
    use std::sync::OnceLock;
    fn class(cell: &'static OnceLock<regex::Regex>, set: &str, c: char) -> bool {
        let regex = cell.get_or_init(|| regex::Regex::new(set).expect("fixed class"));
        let mut buffer = [0; 4];
        regex.is_match(c.encode_utf8(&mut buffer))
    }
    pub(super) fn start(c: char) -> bool {
        static START: OnceLock<regex::Regex> = OnceLock::new();
        c.is_ascii_alphabetic() || (!c.is_ascii() && class(&START, r"\A\p{ID_Start}\z", c))
    }
    pub(super) fn part(c: char) -> bool {
        static PART: OnceLock<regex::Regex> = OnceLock::new();
        c.is_ascii_alphanumeric()
            || c == '_'
            || (!c.is_ascii() && class(&PART, r"\A\p{ID_Continue}\z", c))
    }
}

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
    // The top-level terms while the pattern is one sequence of anchors and
    // single-character atoms with simple quantifiers.
    shape: Option<Vec<Term>>,
}
// One top-level term of a simple pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Term {
    Start,
    End,
    Run(Run),
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
    // would stay unmatched even with property tables.
    fn unsupported(&mut self, kind: Unsupported) {
        if self.unsupported != Some(Unsupported::Backtracking) {
            self.unsupported = Some(kind);
        }
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
                    self.record(if c == '^' { Term::Start } else { Term::End });
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
                self.at += 3;
                return self.lookaround(depth);
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
                (Some(set), Some((min, max))) => self.record(Term::Run(Run { set, min, max })),
                _ => self.shape = None,
            }
        }
        Ok(())
    }
    fn record(&mut self, term: Term) {
        if let Some(shape) = &mut self.shape {
            shape.push(term);
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
            'p' | 'P' => {
                self.property()?;
                self.unsupported(Unsupported::PropertyEscape);
                Member::Set(Vec::new())
            }
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
    fn property(&mut self) -> Result<(), Refusal> {
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
        if crate::es2020_properties::valid(value, name) {
            Ok(())
        } else {
            Err(Refusal::Invalid)
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
                    Member::Set(set) => ranges.extend(set),
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

/// A run of characters from one set, between `min` and `max` of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Run {
    set: Vec<(u32, u32)>,
    min: u32,
    max: Option<u32>,
}
impl Run {
    fn contains(&self, c: char) -> bool {
        let c = c as u32;
        self.set
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
}
/// An anchored sequence of runs that matches without an automaton: every run
/// but the last has a fixed count, so each character's run is known from its
/// position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Simple {
    // Whether the match must reach the end of the string.
    end: bool,
    runs: Vec<Run>,
}
impl Simple {
    fn new(terms: Vec<Term>) -> Option<Self> {
        let mut terms = terms.into_iter().peekable();
        terms.next_if_eq(&Term::Start)?;
        let mut runs = Vec::new();
        let mut end = false;
        for term in terms {
            match term {
                Term::Run(run) if !end => runs.push(run),
                Term::End if !end => end = true,
                _ => return None,
            }
        }
        let (_, fixed) = runs.split_last()?;
        if fixed.iter().any(|run| run.max != Some(run.min)) {
            return None;
        }
        Some(Self { end, runs })
    }
    /// Whether `text` matches, as the pattern's form would.
    pub(crate) fn is_match(&self, text: &str) -> bool {
        let mut chars = text.chars();
        let (last, fixed) = self.runs.split_last().expect("a simple pattern has a run");
        for run in fixed {
            for _ in 0..run.min {
                match chars.next() {
                    Some(c) if run.contains(c) => {}
                    _ => return false,
                }
            }
        }
        let mut count: u64 = 0;
        for c in chars.by_ref() {
            if !self.end && count >= u64::from(last.min) {
                return true;
            }
            if !last.contains(c) {
                return !self.end && count >= u64::from(last.min);
            }
            count += 1;
            if last.max.is_some_and(|max| count > u64::from(max)) {
                return !self.end;
            }
        }
        count >= u64::from(last.min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn backtracking_and_property_patterns_are_read_but_not_matched() {
        for (pattern, kind) in [
            ("^(?=a)", Unsupported::Backtracking),
            ("(?<=a)b", Unsupported::Backtracking),
            (r"(a)\1", Unsupported::Backtracking),
            (r"\1(a)", Unsupported::Backtracking),
            (r"(?<x>a)\k<x>", Unsupported::Backtracking),
            (r"\p{Lu}", Unsupported::PropertyEscape),
            (r"[\P{Script=Greek}]", Unsupported::PropertyEscape),
            (r"\p{L}(?=a)", Unsupported::Backtracking),
            (r"\Bx", Unsupported::NonWordBoundary),
            (r"^.*$|\B", Unsupported::NonWordBoundary),
            (r"\B(?=a)", Unsupported::Backtracking),
        ] {
            assert_eq!(
                read(pattern, 32, 1 << 20),
                Ok(Reading::Unsupported(kind)),
                "{pattern}"
            );
        }
    }
    #[test]
    fn group_nesting_is_bounded() {
        let nested = |n: usize| format!("{}a{}", "(".repeat(n), ")".repeat(n));
        assert!(read(&nested(32), 32, 1 << 20).is_ok());
        assert_eq!(read(&nested(33), 32, 1 << 20), Err(Refusal::Depth));
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
