use crate::{PullRequest, PullValidationError as Failure};
use openbindings::WorkControl;

/// Finite local handoff and relationship-checking policy.
/// Provider work and already admitted JSON are outside these allowances. Core
/// prospective editing/validation use the original document's JSON limits and
/// core finding limits, not the relationship work/allocation ledger. Cancellation
/// is checked before and after those synchronous core calls, not within them.
#[derive(Clone, Copy, Debug)]
pub struct PullLimits {
    /// Maximum inline request bytes, including separately supplied context; default 32 MiB.
    pub max_request_bytes: usize,
    /// Maximum original response or challenge JSON bytes; default 32 MiB.
    pub max_response_bytes: usize,
    /// Cumulative indexed entries and declared uses labels; default 100,000.
    pub max_entries: usize,
    /// Monotone relationship scanning/search work; default 20 million.
    pub max_relationship_work: usize,
    /// Cumulative reservations for relationship indexes and edit metadata; default 64 MiB.
    pub max_relationship_allocation_bytes: usize,
}

impl Default for PullLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: 32 * 1024 * 1024,
            max_response_bytes: 32 * 1024 * 1024,
            max_entries: 100_000,
            max_relationship_work: 20_000_000,
            max_relationship_allocation_bytes: 64 * 1024 * 1024,
        }
    }
}
pub(crate) struct Attempt<'a> {
    limits: PullLimits,
    control: &'a dyn WorkControl,
    work: usize,
    allocation: usize,
    entries: usize,
    failure: Option<&'static str>,
}
impl<'a> Attempt<'a> {
    pub(crate) fn new(limits: PullLimits, control: &'a dyn WorkControl) -> Self {
        Self {
            limits,
            control,
            work: 0,
            allocation: 0,
            entries: 0,
            failure: None,
        }
    }
    pub(crate) fn check(&self) -> Result<(), Failure> {
        if self.control.is_cancelled() {
            Err(Failure::Cancelled)
        } else if let Some(resource) = self.failure {
            Err(Failure::LimitExceeded(resource))
        } else {
            Ok(())
        }
    }
    fn refuse(&mut self, resource: &'static str) -> Failure {
        self.failure = Some(resource);
        Failure::LimitExceeded(resource)
    }
    pub(crate) fn work(&mut self, amount: usize) -> Result<(), Failure> {
        self.check()?;
        self.work = self
            .work
            .checked_add(amount)
            .ok_or_else(|| self.refuse("relationship-work"))?;
        if self.work > self.limits.max_relationship_work {
            return Err(self.refuse("relationship-work"));
        }
        Ok(())
    }
    pub(crate) fn allocate(&mut self, amount: usize) -> Result<(), Failure> {
        self.check()?;
        self.allocation = self
            .allocation
            .checked_add(amount)
            .ok_or_else(|| self.refuse("relationship-allocation-bytes"))?;
        if self.allocation > self.limits.max_relationship_allocation_bytes {
            return Err(self.refuse("relationship-allocation-bytes"));
        }
        Ok(())
    }
    pub(crate) fn entry(&mut self) -> Result<(), Failure> {
        self.work(1)?;
        self.entries = self
            .entries
            .checked_add(1)
            .ok_or_else(|| self.refuse("entries"))?;
        if self.entries > self.limits.max_entries {
            return Err(self.refuse("entries"));
        }
        Ok(())
    }
    pub(crate) fn response(&mut self, bytes: usize) -> Result<(), Failure> {
        self.check()?;
        if bytes > self.limits.max_response_bytes {
            return Err(self.refuse("response-bytes"));
        }
        self.work(bytes)
    }
    pub(crate) fn request(&mut self, request: &PullRequest) -> Result<(), Failure> {
        self.check()?;
        // Exact compact envelope size, retaining original interface/context text.
        // The raw source length bounds the scan before counting JSON escapes.
        let source = request.source();
        if source.len() > self.limits.max_request_bytes {
            return Err(self.refuse("request-bytes"));
        }
        self.work(source.len())?;
        let mut quoted = 2usize;
        for byte in source.bytes() {
            quoted = quoted
                .checked_add(match byte {
                    b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
                    0..=31 => 6,
                    _ => 1,
                })
                .ok_or_else(|| self.refuse("request-bytes"))?;
        }
        let bytes = 24usize
            .checked_add(request.interface().as_bytes().len())
            .and_then(|n| n.checked_add(quoted))
            .and_then(|n| match request.context() {
                Some(context) => n
                    .checked_add(11)?
                    .checked_add(context.value().value().text().len()),
                None => Some(n),
            })
            .ok_or_else(|| self.refuse("request-bytes"))?;
        if bytes > self.limits.max_request_bytes {
            return Err(self.refuse("request-bytes"));
        }
        self.work(bytes)?;
        let root = request.interface().value();
        // No lazy core namespace indexes are populated by this boundary.
        let found = root.get("sources").and_then(|s| s.get(source));
        if found.is_none() {
            return Err(Failure::MissingSource);
        }
        Ok(())
    }
    pub(crate) fn search(&mut self, entries: usize, key_bytes: usize) -> Result<(), Failure> {
        let height = entries.saturating_add(1).ilog2() as usize + 1;
        self.work(
            12usize
                .saturating_mul(height)
                .saturating_mul(key_bytes.saturating_add(1)),
        )
    }
    pub(crate) fn map_insert<K, V>(
        &mut self,
        entries: usize,
        key_bytes: usize,
    ) -> Result<(), Failure> {
        self.entry()?;
        self.search(entries, key_bytes)?;
        // Insertion-only std BTreeMap: two maximal nodes per insertion cover
        // cumulative splits/new roots; borrowed keys and values allocate nothing.
        self.allocate(2 * (12 * std::mem::size_of::<(K, V)>() + 256))
    }
    pub(crate) fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, Failure> {
        self.check()?;
        let bytes = count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| self.refuse("relationship-allocation-bytes"))?;
        self.allocate(bytes)?;
        Ok(Vec::with_capacity(count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings::{Document, JsonValue};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn compact_request_size_counts_context_and_all_source_escape_forms() {
        let document = Document::parse(
            r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example@1"}}}"#,
        )
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
        let context = crate::Context::try_from(
            JsonValue::parse(r#"{ "opaque":"\ud800", "large":1e9999 }"#).unwrap(),
        )
        .unwrap();
        for source in ["s", "\"", "\\", "\0\n\t\r\u{8}\u{c}", "π", "🪴"] {
            for supplied_context in [None, Some(context.clone())] {
                let mut request = PullRequest::new(document.clone(), source);
                if let Some(context) = supplied_context {
                    request = request.with_context(context);
                }
                let context_field = request
                    .context()
                    .map(|c| format!(",\"context\":{}", c.value().value().text()))
                    .unwrap_or_default();
                let encoded = format!(
                    "{{\"interface\":{},\"source\":{}{context_field}}}",
                    document.document().value().text(),
                    serde_json::to_string(source).unwrap()
                );
                let control = AtomicBool::new(false);
                let limits = PullLimits {
                    max_request_bytes: encoded.len(),
                    ..PullLimits::default()
                };
                let accepted = Attempt::new(limits, &control).request(&request);
                if source == "s" {
                    assert!(accepted.is_ok());
                } else {
                    assert!(matches!(accepted, Err(Failure::MissingSource)));
                }
                let too_small = PullLimits {
                    max_request_bytes: encoded.len() - 1,
                    ..limits
                };
                assert!(matches!(
                    Attempt::new(too_small, &control).request(&request),
                    Err(Failure::LimitExceeded("request-bytes"))
                ));
            }
        }
    }

    #[test]
    fn ledger_refusals_and_overflows_do_not_reset_on_later_operations() {
        let control = AtomicBool::new(false);
        for route in 0..5 {
            let mut attempt = Attempt::new(
                PullLimits {
                    max_relationship_work: usize::MAX,
                    max_relationship_allocation_bytes: usize::MAX,
                    ..PullLimits::default()
                },
                &control,
            );
            let error = match route {
                0 => {
                    attempt.work(usize::MAX).unwrap();
                    attempt.work(1)
                }
                1 => {
                    attempt.allocate(usize::MAX).unwrap();
                    attempt.allocate(1)
                }
                2 => attempt.vector::<u64>(usize::MAX).map(|_| ()),
                3 => {
                    attempt.limits.max_entries = 0;
                    attempt.entry()
                }
                _ => {
                    attempt.limits.max_response_bytes = 0;
                    attempt.response(1)
                }
            }
            .unwrap_err();
            let Failure::LimitExceeded(expected) = error else {
                panic!("limit")
            };
            for later in [
                attempt.check(),
                attempt.work(0),
                attempt.allocate(0),
                attempt.entry(),
                attempt.vector::<u64>(usize::MAX).map(|_| ()),
            ] {
                assert!(matches!(later, Err(Failure::LimitExceeded(actual)) if actual == expected));
            }
        }
    }
}
