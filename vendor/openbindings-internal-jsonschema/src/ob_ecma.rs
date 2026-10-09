//! Internal ECMA-262 11th-edition and bounded-work adapter.
use std::cell::Cell;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exhausted {
    Work,
    UnsupportedProperty,
}
#[derive(Clone, Copy)]
struct State {
    active: bool,
    unsupported: bool,
}
thread_local! {static STATE:Cell<State>=const{Cell::new(State{active:false,unsupported:false})};}
struct Scope(State);
impl Drop for Scope {
    fn drop(&mut self) {
        let after = STATE.get();
        let mut before = self.0;
        if before.active {
            before.unsupported |= after.unsupported;
        }
        STATE.set(before);
    }
}
pub fn top_level<T>(limit: usize, f: impl FnOnce() -> T) -> Result<T, Exhausted> {
    let previous = STATE.replace(State {
        active: true,
        unsupported: false,
    });
    let _scope = Scope(previous);
    let result = regress::ob_budget::top_level(limit, f);
    if STATE.get().unsupported {
        Err(Exhausted::UnsupportedProperty)
    } else {
        result.map_err(|_| Exhausted::Work)
    }
}

#[derive(Debug, Clone)]
pub struct Regex {
    compiled: regress::Regex,
    source: String,
    limit: usize,
    unsupported_property: bool,
}
impl Regex {
    pub fn new(pattern: &str) -> Result<Self, ()> {
        RegexBuilder::new(pattern).build()
    }
    pub fn as_str(&self) -> &str {
        &self.source
    }
    pub fn is_match(&self, value: &str) -> Result<bool, fancy_regex::Error> {
        if self.unsupported_property {
            let mut state = STATE.get();
            state.unsupported = true;
            STATE.set(state);
            return Err(fancy_regex::Error::RuntimeError(
                fancy_regex::RuntimeError::BacktrackLimitExceeded,
            ));
        }
        regress::ob_budget::run(self.limit, || self.compiled.find(value).is_some()).map_err(|_| {
            fancy_regex::Error::RuntimeError(fancy_regex::RuntimeError::BacktrackLimitExceeded)
        })
    }
}
pub struct RegexBuilder<'a> {
    source: &'a str,
    limit: usize,
}
impl<'a> RegexBuilder<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            limit: 1_000_000,
        }
    }
    pub fn backtrack_limit(&mut self, limit: usize) -> &mut Self {
        self.limit = limit;
        self
    }
    pub fn delegate_size_limit(&mut self, _limit: usize) -> &mut Self {
        self
    }
    pub fn delegate_dfa_size_limit(&mut self, _limit: usize) -> &mut Self {
        self
    }
    pub fn build(&self) -> Result<Regex, ()> {
        regress::Regex::with_flags(
            self.source,
            regress::Flags {
                unicode: true,
                es2020: true,
                ..Default::default()
            },
        )
        .map(|compiled| Regex {
            compiled,
            source: self.source.to_owned(),
            limit: self.limit,
            unsupported_property: has_property_escape(self.source),
        })
        .map_err(|_| ())
    }
}

fn has_property_escape(pattern: &str) -> bool {
    let mut bytes = pattern.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'\\' {
            if matches!(bytes.next(), Some(b'p' | b'P')) {
                return true;
            }
        }
    }
    false
}
