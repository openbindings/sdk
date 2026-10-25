//! Schema regular expressions as OpenBindings reads them: ECMA-262 11th edition
//! patterns with Unicode semantics (the `u` flag) and no other flag, for
//! `pattern` values and `patternProperties` names alike.
//!
//! Every pattern is parsed here before the JSON Schema library sees it. A
//! pattern that is not valid with Unicode semantics has no reading, and the
//! evaluator does not match it: whether a string matches it is undefined. A
//! valid pattern is matched by a linear-time engine (the `regex` crate) when it
//! has a form in that engine's syntax with the same meaning:
//! [`Reading::Linear`] holds that form, which spells every character set out as
//! code points so the two dialects read it alike. Three kinds of valid pattern
//! are not matched: one that only a backtracking engine can match (lookaround
//! and backreferences), one with a Unicode property escape, whose character
//! tables the evaluator does not qualify, and one with a non-word-boundary
//! assertion `\B`. The word boundary `\b` has a form: ECMA-262 reads it with
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
/// Why a pattern is not matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// It is not a valid pattern with Unicode semantics, so whether a string
    /// matches it is undefined.
    Invalid,
    /// It needs a backtracking engine.
    Backtracking,
    /// It has a Unicode property escape.
    PropertyEscape,
    /// It has a non-word-boundary assertion, `\B`.
    NonWordBoundary,
    /// Its form is longer than the pattern byte limit, or does not compile
    /// within the most compiled size any one pattern is given
    /// ([`COMPILED_SIZE`]), which is found when the pattern is first compiled.
    Size,
    /// Its form does not compile within what the value contract's pattern memory
    /// gives it, found when the pattern is first compiled.
    Memory(Shortfall),
}
impl Unsupported {
    /// Whether the pattern is not matched for what it says rather than for a
    /// limit: its match is then a stand-in, which a value's verdict need not
    /// depend on. A pattern a limit leaves unmatched is no stand-in: as a
    /// `pattern`, it gives no verdict wherever a string reaches it, and as a
    /// `patternProperties` name, it refuses the value contract.
    pub(crate) fn stands_in(self) -> bool {
        matches!(
            self,
            Unsupported::Invalid
                | Unsupported::Backtracking
                | Unsupported::PropertyEscape
                | Unsupported::NonWordBoundary
        )
    }
}
/// How a value contract's pattern memory fell short, for the refusal to state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shortfall {
    /// The value contract's run matchers, holding `matched` bytes, and its
    /// `compiled` patterns' first allowances need more than its `total` bytes
    /// hold for them ([`Memory::most_patterns`]).
    Count {
        matched: usize,
        compiled: usize,
        total: usize,
    },
    /// One pattern needs a larger compiled size than it is given. The value
    /// contract's run matchers hold `matched` bytes, and each of its
    /// `patterns` compiled patterns holds [`FIRST`] for its form and `cache`
    /// for its search cache, which leaves `left` of the `total`; those that
    /// need more share it, and those of them that need less than an equal part
    /// keep only what they need, `kept` in all. The `sharing` patterns that
    /// need more than that share the rest equally, and this one does not
    /// compile within the `compiled` size that gives it.
    Pattern {
        matched: usize,
        patterns: usize,
        total: usize,
        cache: usize,
        left: usize,
        kept: usize,
        sharing: usize,
        compiled: usize,
    },
}
/// Why a pattern has no reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// It is not a valid pattern with Unicode semantics.
    Invalid,
    /// Its groups nest more deeply than the admitted limit.
    Depth,
}

/// The most compiled size the linear-time engine gives any one pattern's form,
/// in bytes. Counted repetitions of large sets, such as `^.{1,10000}$` (about
/// 13.2 MiB), need more than the engine's default of 10 MiB.
pub(crate) const COMPILED_SIZE: usize = 32 * 1024 * 1024;
/// The most a search through any one pattern keeps in its lazy DFA's cache, in
/// bytes.
pub(crate) const SEARCH_CACHE: usize = 2 * 1024 * 1024;
/// The compiled size every pattern's form first compiles within, in bytes.
/// Every compiled pattern also holds memory that neither of the engine's size
/// settings counts, so holding this much of the pattern memory bounds how many
/// patterns a value contract compiles as well as their size.
pub(crate) const FIRST: usize = 48 * 1024;

/// What the linear-time engine may build for one compiled pattern, through the
/// regex crate's two documented size settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Allowance {
    /// The compiled size of the pattern's form (`size_limit`).
    pub(crate) compiled: usize,
    /// The lazy DFA's cache for one search (`dfa_size_limit`).
    pub(crate) cache: usize,
}

/// The pattern's form compiled for the linear-time engine within `allowance`;
/// `None` when it does not fit.
pub(crate) fn compile(form: &str, allowance: Allowance) -> Option<regex::Regex> {
    regex::RegexBuilder::new(form)
        .size_limit(allowance.compiled)
        .dfa_size_limit(allowance.cache)
        .build()
        .ok()
}

/// A value contract's pattern memory (`Limits::total_pattern_memory`) and how it is
/// divided. A quarter of it, up to [`SEARCH_CACHE`] for each, is the compiled
/// patterns' search caches, in equal parts, which bound only how fast a search
/// runs. The run matchers hold the bytes they take, and each compiled
/// pattern's form first compiles within [`FIRST`]; the patterns whose forms do
/// not share what that leaves.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Memory {
    total: usize,
    patterns: usize,
    cache: usize,
    // The bytes the run matchers hold.
    matched: usize,
}
/// What a pattern that does not fit its first allowance is given.
#[derive(Debug)]
pub(crate) enum Given {
    /// Its form compiles within `compiled` bytes; `regex` is that compiled
    /// form when it was asked for.
    Compiled {
        regex: Option<regex::Regex>,
        compiled: usize,
    },
    /// Its form does not compile within what it can be given.
    Refused(Unsupported),
}
impl Memory {
    /// What `total` bytes hold for the run matchers and the first
    /// allowances: the search caches take at most a quarter.
    pub(crate) fn held(total: usize) -> usize {
        total - total / 4
    }
    /// The most patterns `total` bytes hold first allowances for, [`FIRST`]
    /// each, beside run matchers holding `matched` bytes; `None` when the run
    /// matchers alone need more than [`Self::held`].
    pub(crate) fn most_patterns(total: usize, matched: usize) -> Option<usize> {
        Self::held(total)
            .checked_sub(matched)
            .map(|rest| rest / FIRST)
    }
    /// The memory of a value contract compiling `patterns` patterns beside
    /// run matchers holding `matched` bytes, at most
    /// [`Self::most_patterns`].
    pub(crate) fn new(total: usize, patterns: usize, matched: usize) -> Self {
        Self {
            total,
            patterns,
            cache: (total / 4 / patterns.max(1)).min(SEARCH_CACHE),
            matched,
        }
    }
    /// The allowance every compiled pattern first compiles within.
    pub(crate) fn first(&self) -> Allowance {
        Allowance {
            compiled: FIRST,
            cache: self.cache,
        }
    }
    /// What the run matchers, the first allowances and the search caches
    /// leave for the forms that need more.
    fn left(&self) -> usize {
        self.total
            .saturating_sub(self.patterns.saturating_mul(FIRST + self.cache))
            .saturating_sub(self.matched)
    }
    /// Share what the first allowances leave among `forms`, none of which
    /// fits its first allowance. The first `names` are `patternProperties`
    /// names, which the library compiles: `together` compiles every name
    /// within one allowance and answers whether they all fit, so when they do
    /// in the first round the library's compilation is that round's. The
    /// others are the SDK's, and each keeps its compiled form.
    ///
    /// Each form is given an equal part; those that need less keep only what
    /// they need, to within a sixteenth, and the others share what that
    /// leaves again, until all of them fit their parts, none does, or the
    /// parts grow by no more than a sixteenth. Those that then do not fit are
    /// refused. Whether a form compiles depends only on the forms, not on
    /// their order.
    ///
    /// A round compiles each form within its part, which together come to
    /// what is left, and an attempt that does not fit stops building once it
    /// passes its size, so a round builds at most what is left, and each form
    /// keeps the least it needs once. The forms that fit a round's part needed
    /// more than the last round's, so they leave less than that growth to the
    /// others: for the parts to keep growing by more than a sixteenth, the
    /// forms still sharing must shrink geometrically, and the rounds grow only
    /// with the logarithm of the forms.
    pub(crate) fn share(
        &self,
        forms: &[&str],
        names: usize,
        together: &mut dyn FnMut(Allowance) -> bool,
    ) -> Vec<Given> {
        let keep = |at: usize| at >= names;
        let left = self.left();
        let mut given: Vec<Option<Given>> = forms.iter().map(|_| None).collect();
        let mut sharing: Vec<usize> = (0..forms.len()).collect();
        let mut rest = left;
        let mut previous: Option<usize> = None;
        while !sharing.is_empty() {
            let part = rest / sharing.len();
            let compiled = (FIRST + part).min(COMPILED_SIZE);
            let allowance = Allowance {
                compiled,
                cache: self.cache,
            };
            // In the first round every name is sharing, and the library
            // compiles them all at once.
            let named = previous.is_none() && names > 0 && together(allowance);
            let last =
                previous.is_some_and(|previous| part.saturating_sub(previous) <= previous / 16);
            let (mut fitted, mut short) = (Vec::new(), Vec::new());
            for &at in &sharing {
                if named && !keep(at) {
                    fitted.push((at, None));
                    continue;
                }
                match compile(forms[at], allowance) {
                    Some(regex) => fitted.push((at, keep(at).then_some(regex))),
                    None if compiled == COMPILED_SIZE => {
                        given[at] = Some(Given::Refused(Unsupported::Size));
                    }
                    None => short.push(at),
                }
            }
            if short.is_empty() || fitted.is_empty() || last {
                let shortfall = Shortfall::Pattern {
                    matched: self.matched,
                    patterns: self.patterns,
                    total: self.total,
                    cache: self.cache,
                    left,
                    kept: left - rest,
                    sharing: sharing.len(),
                    compiled,
                };
                for (at, regex) in fitted {
                    given[at] = Some(Given::Compiled { regex, compiled });
                }
                for at in short {
                    given[at] = Some(Given::Refused(Unsupported::Memory(shortfall)));
                }
                break;
            }
            // Those that fit keep only what they need, and the others share
            // the rest again.
            for (at, regex) in fitted {
                let (least, smaller) = least(forms[at], allowance, keep(at));
                rest -= least - FIRST;
                given[at] = Some(Given::Compiled {
                    regex: smaller.or(regex),
                    compiled: least,
                });
            }
            previous = Some(part);
            sharing = short;
        }
        given
            .into_iter()
            .map(|given| given.expect("every form is given"))
            .collect()
    }
}

/// The least compiled size, to within a sixteenth, between [`FIRST`], which
/// `form` does not fit, and the size `fits` gives, which it does; with the
/// form compiled within it when `keep` holds and that size is smaller. It
/// doubles from [`FIRST`] to the first size that fits and then halves the gap
/// below it. An attempt that does not fit stops building once it passes its
/// size, so the attempts together build a few times the form's compiled size.
fn least(form: &str, fits: Allowance, keep: bool) -> (usize, Option<regex::Regex>) {
    let attempt = |compiled| {
        compile(
            form,
            Allowance {
                compiled,
                cache: fits.cache,
            },
        )
    };
    let (mut below, mut size, mut regex) = (FIRST, fits.compiled, None);
    let mut next = FIRST * 2;
    while next < size {
        match attempt(next) {
            Some(fitted) => {
                (size, regex) = (next, keep.then_some(fitted));
                break;
            }
            None => (below, next) = (next, next * 2),
        }
    }
    while size - below > size / 16 {
        let middle = below + (size - below) / 2;
        match attempt(middle) {
            Some(fitted) => (size, regex) = (middle, keep.then_some(fitted)),
            None => below = middle,
        }
    }
    (size, regex)
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
    /// The heap bytes the matcher holds, which a value contract's pattern
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
    fn the_pattern_memory_is_divided_between_caches_and_forms() {
        const KIB: usize = 1024;
        const MIB: usize = 1024 * KIB;
        assert_eq!(Memory::most_patterns(64 * MIB, 0), Some(1024));
        assert_eq!(Memory::most_patterns(0, 0), Some(0));
        // Run matchers hold their bytes of the three quarters the search
        // caches leave, so any bytes they hold take a first allowance's
        // place at the default, and more than the three quarters admit none.
        assert_eq!(Memory::most_patterns(64 * MIB, 1), Some(1023));
        assert_eq!(Memory::most_patterns(64 * MIB, 48 * KIB), Some(1023));
        assert_eq!(Memory::most_patterns(64 * MIB, 48 * KIB + 1), Some(1022));
        assert_eq!(Memory::most_patterns(64 * MIB, 48 * MIB), Some(0));
        assert_eq!(Memory::most_patterns(64 * MIB, 48 * MIB + 1), None);
        assert_eq!(Memory::most_patterns(0, 1), None);
        let first = |total, patterns| Memory::new(total, patterns, 0).first();
        let allowance = |compiled, cache| Allowance { compiled, cache };
        // A quarter of the memory for the search caches, up to 2 MiB each.
        assert_eq!(first(64 * MIB, 1), allowance(48 * KIB, 2 * MIB));
        assert_eq!(first(64 * MIB, 8), allowance(48 * KIB, 2 * MIB));
        assert_eq!(first(64 * MIB, 64), allowance(48 * KIB, 256 * KIB));
        assert_eq!(first(64 * MIB, 1024), allowance(48 * KIB, 16 * KIB));
        // The run matchers take nothing from the search caches.
        assert_eq!(Memory::new(64 * MIB, 64, MIB).first(), first(64 * MIB, 64));
        assert_eq!(Memory::new(64 * MIB, 1024, 0).left(), 0);
        assert_eq!(Memory::new(64 * MIB, 4, 0).left(), 56 * MIB - 4 * 48 * KIB);
        assert_eq!(
            Memory::new(64 * MIB, 4, MIB).left(),
            55 * MIB - 4 * 48 * KIB
        );
        for matched in [0, 1, 100 * KIB, 47 * MIB] {
            let most = Memory::most_patterns(64 * MIB, matched).unwrap();
            for patterns in 1..=most {
                let memory = Memory::new(64 * MIB, patterns, matched);
                assert!(
                    matched + patterns * (FIRST + memory.cache) + memory.left() <= 64 * MIB,
                    "{matched} {patterns}"
                );
            }
        }
    }
    // A form that does not fit its first allowance is given the least size
    // that fits, to within a sixteenth.
    #[test]
    fn a_form_keeps_the_least_size_it_compiles_within() {
        // About 1 MiB compiled.
        let form = "^.{1,1000}x$";
        let fits = Allowance {
            compiled: COMPILED_SIZE,
            cache: SEARCH_CACHE,
        };
        let (size, regex) = least(form, fits, true);
        assert!(regex.is_some_and(|regex| regex.is_match("ax")));
        let within = |compiled| compile(form, Allowance { compiled, cache: 0 }).is_some();
        assert!(within(size));
        assert!(!within(size - size / 16));
        assert_eq!(least(form, fits, false).0, size);
        assert!(least(form, fits, false).1.is_none());
    }
    // Forms that do not fit their first allowances share what is left: equal
    // parts, of which those that need less keep only what they need.
    #[test]
    fn forms_share_what_the_first_allowances_leave() {
        const MIB: usize = 1024 * 1024;
        let compiled = |given: &Given| match given {
            Given::Compiled { compiled, .. } => Some(*compiled),
            Given::Refused(_) => None,
        };
        // About 1 and 10 MiB compiled; one part of 6 MiB is too small for the
        // larger, which takes what the smaller leaves of its part.
        let small = "^.{1,1000}x$";
        let large = "^.{1,10000}x$";
        let memory = Memory::new(16 * MIB, 2, 0);
        let given = memory.share(&[large, small], 2, &mut |_| false);
        let large_size = compiled(&given[0]).expect("the larger fits what the smaller leaves");
        let small_size = compiled(&given[1]).expect("the smaller fits its part");
        assert!(small_size < 2 * MIB && large_size > 8 * MIB);
        assert!(large_size + small_size <= memory.left() + 2 * FIRST);
        // The order of the forms does not change what each is given.
        let reversed = memory.share(&[small, large], 2, &mut |_| false);
        assert_eq!(compiled(&reversed[0]), Some(small_size));
        assert_eq!(compiled(&reversed[1]), Some(large_size));
        // Two larger forms need more together than is left, and neither
        // fits its part.
        let given = memory.share(&[large, large], 0, &mut |_| unreachable!());
        for given in &given {
            assert!(matches!(
                given,
                Given::Refused(Unsupported::Memory(Shortfall::Pattern { sharing: 2, .. }))
            ));
        }
        // A form needing more than any one pattern is given is refused for
        // its size.
        let given =
            Memory::new(256 * MIB, 1, 0).share(&["^.{1,40000}x$"], 0, &mut |_| unreachable!());
        assert!(matches!(given[0], Given::Refused(Unsupported::Size)));
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
