use crate::*;
use std::{collections::BTreeMap, fmt, sync::Arc};
/// A closed JSON Schema projection with an original-source map. Names are private
/// to this program; consumers must not rely on their spelling or numbering.
#[derive(Clone)]
pub struct Program {
    /// Private absolute URI selecting the projected entry; never expose it as an original diagnostic location.
    pub entry_uri: String,
    /// Owned projected schema resources sufficient for this program; an evaluator may release them after compilation retains its required state.
    pub resources: Vec<ProjectedResource>,
    pub locations: Arc<BTreeMap<String, ProgramOrigin>>,
}
// Private provenance boundaries. An Exact ancestor is terminal: descendants
// cannot inherit an invented original suffix. Synthetic nodes stop lookup too.
#[derive(Clone, Debug)]
pub enum ProgramOrigin {
    Prefix(Location),
    Exact(Location),
    Synthetic,
}
/// Original schema location or URI-decoding scratch exceeds the caller's UTF-8
/// byte allowance. No partial location is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocationBudgetExceeded;
impl fmt::Display for LocationBudgetExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("original schema location exceeds the diagnostic byte allowance")
    }
}
impl std::error::Error for LocationBudgetExceeded {}
impl Program {
    /// Map a generated absolute keyword URI to original source, including a proved
    /// upper oneOf summary. Exact summary boundaries do not map their descendants.
    /// Return `None` for synthetic/unmapped origins. This establishes no verdict:
    /// only diagnose completed upper failure, preserving outer error boundaries.
    pub fn original_location(&self, generated_uri: &str) -> Option<Location> {
        self.original_location_bounded(generated_uri, usize::MAX)
            .ok()
            .flatten()
    }
    /// Map an original location when its resource URI plus complete pointer fit
    /// `max_bytes` UTF-8 bytes. Admission precedes copying original strings.
    /// URI fragment percent-decoding uses one normalized-string buffer with a
    /// separate logical UTF-8 allowance of `max_bytes`. Its encoded input length
    /// is a conservative preallocation bound; allocator capacity/reallocation is
    /// not measured. Percent escapes in the URI base are preserved verbatim.
    /// `Ok(None)` means a synthetic/unmapped origin, a descendant blocked by an
    /// exact origin, or an invalid percent-encoded fragment;
    /// [`LocationBudgetExceeded`] means either allowance was insufficient.
    /// This helper establishes neither a verdict nor a total heap bound.
    ///
    pub fn original_location_bounded(
        &self,
        generated_uri: &str,
        max_bytes: usize,
    ) -> Result<Option<Location>, LocationBudgetExceeded> {
        let normalized = match generated_uri.split_once('#') {
            Some((base, fragment)) if fragment.contains('%') => {
                if generated_uri.len() > max_bytes {
                    return Err(LocationBudgetExceeded);
                }
                let Some(mut decoded) = crate::uri::decode_fragment(fragment) else {
                    return Ok(None);
                };
                // Grow this admitted buffer instead of retaining both decoded
                // and formatted URI strings. Logical length never exceeds the
                // encoded URI length admitted above.
                decoded.insert(0, '#');
                decoded.insert_str(0, base);
                std::borrow::Cow::Owned(decoded)
            }
            _ => std::borrow::Cow::Borrowed(generated_uri),
        };
        let generated_uri = normalized.as_ref();
        let mut end = generated_uri.len();
        loop {
            if let Some(origin) = self.locations.get(&generated_uri[..end]) {
                let location = match origin {
                    ProgramOrigin::Prefix(location) => location,
                    ProgramOrigin::Exact(location) if end == generated_uri.len() => location,
                    ProgramOrigin::Exact(_) | ProgramOrigin::Synthetic => return Ok(None),
                };
                let bytes = location
                    .resource
                    .as_ref()
                    .map_or(0, String::len)
                    .saturating_add(location.pointer.len())
                    .saturating_add(generated_uri.len() - end);
                if bytes > max_bytes {
                    return Err(LocationBudgetExceeded);
                }
                let mut pointer =
                    String::with_capacity(location.pointer.len() + generated_uri.len() - end);
                pointer.push_str(&location.pointer);
                pointer.push_str(&generated_uri[end..]);
                return Ok(Some(Location {
                    resource: location.resource.clone(),
                    pointer,
                }));
            }
            let Some(previous) = generated_uri[..end].rfind('/') else {
                return Ok(None);
            };
            end = previous;
        }
    }
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Program").finish_non_exhaustive()
    }
}
