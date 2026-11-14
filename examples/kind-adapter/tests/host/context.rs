//! Context belongs to the exact source entry, not just its mutable document key.
use openbindings::{JsonValue, Source};
use openbindings_invocation::Context;
use std::{cmp::Ordering, collections::BTreeMap};

struct Entry {
    key: String,
    kind: String,
    data: Option<JsonValue>,
}
impl Entry {
    fn of(source: &Source) -> Self {
        Self {
            key: source.key().to_owned(),
            kind: source.kind().to_owned(),
            data: source.data(), // Retain a handle, without copying the source text.
        }
    }
    fn parts(&self) -> (&str, &str, Option<&str>) {
        (
            &self.key,
            &self.kind,
            self.data.as_ref().map(JsonValue::text),
        )
    }
}
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.parts() == other.parts()
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts().cmp(&other.parts())
    }
}

#[derive(Default)]
pub struct Contexts(BTreeMap<Entry, Context>);
impl Contexts {
    pub fn remember(&mut self, source: &Source, context: Context) {
        self.0.insert(Entry::of(source), context);
    }
    pub fn get(&self, source: &Source) -> Option<Context> {
        self.0.get(&Entry::of(source)).cloned()
    }
}
