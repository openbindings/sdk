use crate::{
    ComparisonFailure as Failure, ComparisonLimits, SchemaInput,
    budget::Attempt,
    normalize,
    reference::Resources,
    value::{self, Name},
};
use openbindings::{JsonRef, JsonValue};
use openbindings_internal_json::backend;
use openbindings_internal_schema::uri;
use std::collections::BTreeMap;

/// One original reference occurrence and its containing schema resource.
/// Accounting is caller policy; equal spellings in different resources may name
/// different targets. Formatting this context never prints caller values.
#[derive(Clone, Copy)]
pub struct ReferenceContext<'a> {
    pub(crate) spelling: &'a str,
    pub(crate) schema: JsonRef<'a>,
    pub(crate) resource: &'a JsonValue,
    pub(crate) base: Option<&'a str>,
}
impl std::fmt::Debug for ReferenceContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReferenceContext").finish_non_exhaustive()
    }
}
impl<'a> ReferenceContext<'a> {
    /// Decoded reference spelling, without URI normalization.
    pub fn spelling(&self) -> &'a str {
        self.spelling
    }
    /// Original schema containing the reference and its siblings.
    pub fn schema(&self) -> JsonRef<'a> {
        self.schema
    }
    /// Root of the containing schema resource, or the supplied document root.
    pub fn resource(&self) -> &'a JsonValue {
        self.resource
    }
    /// Absolute base of that resource, if the caller or a schema supplied one.
    pub fn base(&self) -> Option<&'a str> {
        self.base
    }
}

/// Compare identity under the source puller's reference rule. The caller decides
/// which reference targets its report accounts for. Same-spelled references on
/// both sides satisfying `accounted_for` remain leaves; siblings and containing
/// allOf stay unmerged. Every other reference is followed, with no I/O.
///
/// To ask whether a difference lies only behind unaccounted targets, compare
/// again with `&|_| true`. If that comparison is identical but this one differs,
/// the entry is unassessed. The companion never reads or constructs a pull report.
pub fn identical_with_references(
    left: Option<&SchemaInput>,
    right: Option<&SchemaInput>,
    accounted_for: &dyn Fn(ReferenceContext<'_>) -> bool,
    limits: &ComparisonLimits,
) -> Result<bool, Failure> {
    identical_with_references_and_control(
        left,
        right,
        accounted_for,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
/// Reference-rule identity with one monotone allowance and cooperative control.
/// The callback receives each original reference and its resource context.
pub fn identical_with_references_and_control(
    left: Option<&SchemaInput>,
    right: Option<&SchemaInput>,
    accounted_for: &dyn Fn(ReferenceContext<'_>) -> bool,
    limits: &ComparisonLimits,
    control: &dyn openbindings::WorkControl,
) -> Result<bool, Failure> {
    let mut attempt = Attempt::new(*limits, control);
    attempt.check()?;
    let mut common = BTreeMap::new();
    if let (Some(left), Some(right)) = (left, right) {
        let mut left = Inventory::new(left, &mut attempt)?;
        let mut right = Inventory::new(right, &mut attempt)?;
        loop {
            for spelling in left.found.keys() {
                let cost = spelling
                    .len()
                    .saturating_add(1)
                    .saturating_mul(
                        right
                            .found
                            .len()
                            .saturating_add(common.len())
                            .saturating_add(1)
                            .ilog2() as usize
                            + 1,
                    )
                    .saturating_mul(24);
                attempt.work(cost)?;
                if right.found.contains_key(spelling) && !common.contains_key(spelling) {
                    let mut key = attempt.vector(spelling.len())?;
                    key.extend_from_slice(spelling);
                    value::insert(&mut common, key, (), &mut attempt)?;
                }
            }
            if left.pending.is_empty() && right.pending.is_empty() {
                break;
            }
            left.expand(&common, accounted_for, &mut attempt)?;
            right.expand(&common, accounted_for, &mut attempt)?;
        }
    }
    let left = left
        .map(|i| normalize::normalize_with_references(i, &common, accounted_for, &mut attempt))
        .transpose()?;
    let right = right
        .map(|i| normalize::normalize_with_references(i, &common, accounted_for, &mut attempt))
        .transpose()?;
    let equal = match (&left, &right) {
        (None, None) => true,
        (Some(a), Some(b)) => value::equal(a, b, 0, &mut attempt)?,
        _ => false,
    };
    attempt.check()?;
    Ok(equal)
}
// Discover references a schema walk can reach in rounds. A common accounted
// leaf is never followed merely to inventory its target. Rounds also discover
// common leaves reached through differently spelled outer references.
struct Inventory {
    resources: Resources,
    seen: Vec<((usize, usize), usize)>,
    found: BTreeMap<Name, ()>,
    pending: Vec<(JsonValue, usize)>,
}
impl Inventory {
    fn new(input: &SchemaInput, attempt: &mut Attempt<'_>) -> Result<Self, Failure> {
        let mut inventory = Self {
            resources: Resources::new(input, attempt)?,
            seen: Vec::new(),
            found: BTreeMap::new(),
            pending: Vec::new(),
        };
        inventory.scan(input.schema.view(), 0, 0, attempt)?;
        Ok(inventory)
    }
    fn expand(
        &mut self,
        common: &BTreeMap<Name, ()>,
        accounted_for: &dyn Fn(ReferenceContext<'_>) -> bool,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        for (schema, scope) in std::mem::take(&mut self.pending) {
            let reference = schema.get("$ref").expect("pending reference");
            let name = value::name(reference, attempt)?;
            attempt.work(
                name.len()
                    .saturating_add(1)
                    .saturating_mul(common.len().saturating_add(1).ilog2() as usize + 1)
                    .saturating_mul(12),
            )?;
            if common.contains_key(&name)
                && accounted_for(self.resources.context(
                    scope,
                    schema.view(),
                    reference.as_str().expect("validated reference"),
                ))
            {
                continue;
            }
            let result = match self.resources.resolve(scope, reference, attempt) {
                Ok((target, target_scope)) => self.scan(target.view(), target_scope, 0, attempt),
                Err(failure) => Err(failure),
            };
            // A later round may discover a counterpart and keep this reference.
            // Normalization enforces every non-leaf failure after inventory.
            if matches!(result, Err(Failure::LimitExceeded(_) | Failure::Cancelled)) {
                return result;
            }
        }
        Ok(())
    }
    fn scan(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        attempt.depth(depth)?;
        let scope = self.resources.enter(schema, scope, attempt)?;
        let identity = (backend::occurrence(schema), scope);
        attempt.work(self.seen.len().saturating_add(1))?;
        if self.seen.contains(&identity) {
            return Ok(());
        }
        attempt.allocate(2 * std::mem::size_of_val(&identity))?;
        self.seen.push(identity);
        if let Some(reference) = schema.get("$ref") {
            let text = reference.as_str().ok_or(Failure::InvalidReference)?;
            attempt.work(text.len().saturating_mul(8).saturating_add(1))?;
            if !uri::valid(text) {
                return Err(Failure::InvalidReference);
            }
            let name = value::name(reference, attempt)?;
            attempt.work(
                name.len()
                    .saturating_add(1)
                    .saturating_mul(self.found.len().saturating_add(1).ilog2() as usize + 1)
                    .saturating_mul(12),
            )?;
            if !self.found.contains_key(&name) {
                value::insert(&mut self.found, name, (), attempt)?;
            }
            attempt.allocate(2 * std::mem::size_of::<(JsonValue, usize)>())?;
            self.pending.push((schema.to_owned(), scope));
        }
        attempt.work(schema.text().len().saturating_add(1))?;
        // Match normalization's schema positions. Outside-keyword contents and
        // unused $defs are opaque; their references acquire no new semantics.
        if let Some(properties) = schema.get("properties").and_then(|v| v.members()) {
            for property in properties {
                self.scan(property.value, scope, depth + 1, attempt)?;
            }
        }
        for key in ["items", "additionalProperties"] {
            if let Some(child) = schema.get(key) {
                self.scan(child, scope, depth + 1, attempt)?;
            }
        }
        for key in ["allOf", "oneOf", "anyOf"] {
            if let Some(children) = schema.get(key).and_then(|v| v.elements()) {
                for child in children {
                    self.scan(child, scope, depth + 1, attempt)?;
                }
            }
        }
        Ok(())
    }
}
