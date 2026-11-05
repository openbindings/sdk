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
pub(crate) use crate::pattern::{Simple, exact};

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
    checked_compile(form, allowance, None)
}
pub(crate) fn checked_compile(
    form: &str,
    allowance: Allowance,
    control: Option<&dyn openbindings::WorkControl>,
) -> Option<regex::Regex> {
    if control.is_some_and(|c| c.check().is_err()) {
        return None;
    }
    #[cfg(test)]
    crate::compiled::entries::record(1);
    let result = regex::RegexBuilder::new(form)
        .size_limit(allowance.compiled)
        .dfa_size_limit(allowance.cache)
        .build()
        .ok();
    if control.is_some_and(|c| c.check().is_err()) {
        return None;
    }
    result
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
    pub(crate) fn left(&self) -> usize {
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
        control: Option<&dyn openbindings::WorkControl>,
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
                match checked_compile(forms[at], allowance, control) {
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
                let (least, smaller) = least(forms[at], allowance, keep(at), control);
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
fn least(
    form: &str,
    fits: Allowance,
    keep: bool,
    control: Option<&dyn openbindings::WorkControl>,
) -> (usize, Option<regex::Regex>) {
    let attempt = |compiled| {
        checked_compile(
            form,
            Allowance {
                compiled,
                cache: fits.cache,
            },
            control,
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
// The baseline backend now consumes the current admitted Reader, so Rust
// identifier tables, normalization and authored group policy have one owner.
pub(crate) fn read(source: &str, depth: usize, bytes: usize) -> Result<Reading, Refusal> {
    crate::pattern::read(source, depth, bytes)
        .map(|reading| match reading {
            crate::pattern::Reading::Linear(linear) => Reading::Linear(Linear {
                form: linear.form,
                simple: linear.simple,
            }),
            crate::pattern::Reading::Unsupported(reason) => Reading::Unsupported(match reason {
                crate::pattern::Unsupported::Backtracking => Unsupported::Backtracking,
                crate::pattern::Unsupported::PropertyEscape => Unsupported::PropertyEscape,
                crate::pattern::Unsupported::NonWordBoundary => Unsupported::NonWordBoundary,
                crate::pattern::Unsupported::Size => Unsupported::Size,
            }),
        })
        .map_err(|reason| match reason {
            crate::pattern::Refusal::Invalid => Refusal::Invalid,
            crate::pattern::Refusal::Depth => Refusal::Depth,
        })
}
