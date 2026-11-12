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

pub(crate) type ReferenceKey = ((usize, usize), bool, Name);

/// One original reference occurrence and its containing schema resource.
/// Accounting is caller policy; equal spellings in different resources may name
/// different targets. Formatting this context never prints caller values.
#[derive(Clone)]
pub struct ReferenceContext<'a> {
    pub(crate) spelling: &'a str,
    pub(crate) schema: JsonRef<'a>,
    pub(crate) resource: JsonValue,
    pub(crate) base: Option<String>,
    pub(crate) target: Result<JsonValue, Failure>,
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
    pub fn resource(&self) -> &JsonValue {
        &self.resource
    }
    /// Absolute base of that resource, if the caller or a schema supplied one.
    pub fn base(&self) -> Option<&str> {
        self.base.as_deref()
    }
    /// Initial resolved target, or its resolution failure. An accounted leaf
    /// may still be kept when its target is unavailable.
    pub fn target(&self) -> Result<&JsonValue, Failure> {
        self.target.as_ref().map_err(|e| *e)
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
/// Reference-rule identity with one shared work and memory policy and cooperative control.
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
            for (path, references) in &left.found {
                attempt.work(
                    path.len()
                        .saturating_add(1)
                        .saturating_mul(right.found.len().saturating_add(1).ilog2() as usize + 1),
                )?;
                if let Some(others) = right.found.get(path) {
                    for (name, schema, scope, dynamic) in references {
                        for (other, counterpart, other_scope, other_dynamic) in others {
                            attempt
                                .work(name.len().saturating_add(other.len()).saturating_add(1))?;
                            if name != other || dynamic != other_dynamic {
                                continue;
                            }
                            let keyword = if *dynamic { "$dynamicRef" } else { "$ref" };
                            let spelling = schema
                                .get(keyword)
                                .expect("reference")
                                .as_str()
                                .expect("validated reference");
                            let other_spelling = counterpart
                                .get(keyword)
                                .expect("reference")
                                .as_str()
                                .expect("validated reference");
                            if accounted_for(left.resources.context(
                                *scope,
                                schema.view(),
                                spelling,
                                &mut attempt,
                            )?) && accounted_for(right.resources.context(
                                *other_scope,
                                counterpart.view(),
                                other_spelling,
                                &mut attempt,
                            )?) {
                                for schema in [schema, counterpart] {
                                    attempt.allocate(
                                        path.len().saturating_mul(2).saturating_add(256),
                                    )?;
                                    common.insert(
                                        (
                                            backend::occurrence(schema.view()),
                                            *dynamic,
                                            path.clone(),
                                        ),
                                        (),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            if left.pending.is_empty() && right.pending.is_empty() {
                break;
            }
            left.expand(&common, &mut attempt)?;
            right.expand(&common, &mut attempt)?;
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
type Scoped = ((usize, usize), usize);
type RelativeReference = (Name, JsonValue, usize, bool);
type PendingReference = (JsonValue, usize, Name, bool, Vec<Scoped>);
struct Inventory {
    resources: Resources,
    seen: std::collections::BTreeSet<(Scoped, Name)>,
    templates: BTreeMap<Scoped, Vec<RelativeReference>>,
    found: BTreeMap<Name, Vec<RelativeReference>>,
    pending: Vec<PendingReference>,
}
impl Inventory {
    fn new(input: &SchemaInput, attempt: &mut Attempt<'_>) -> Result<Self, Failure> {
        let mut inventory = Self {
            resources: Resources::new(input, attempt)?,
            seen: Default::default(),
            templates: BTreeMap::new(),
            found: BTreeMap::new(),
            pending: Vec::new(),
        };
        inventory.scan(input.schema.view(), 0, Vec::new(), Vec::new(), attempt)?;
        Ok(inventory)
    }
    fn expand(
        &mut self,
        common: &BTreeMap<ReferenceKey, ()>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        for (schema, scope, path, dynamic, ancestors) in std::mem::take(&mut self.pending) {
            let reference = schema
                .get(if dynamic { "$dynamicRef" } else { "$ref" })
                .expect("pending reference");
            attempt.work(common.len().saturating_add(1).ilog2() as usize + 1)?;
            attempt.allocate(path.len().saturating_mul(2))?;
            if common.contains_key(&(backend::occurrence(schema.view()), dynamic, path.clone())) {
                continue;
            }
            let result = match self.resources.resolve(scope, reference, attempt) {
                Ok((target, target_scope)) => {
                    self.scan(target.view(), target_scope, path, ancestors, attempt)
                }
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
        path: Name,
        mut ancestors: Vec<Scoped>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        let scope = self.resources.enter(schema, scope, attempt)?;
        let identity = (backend::occurrence(schema), scope);
        attempt.work(
            ancestors
                .len()
                .saturating_add(self.seen.len())
                .saturating_add(1),
        )?;
        attempt.allocate(path.len().saturating_mul(2))?;
        if ancestors.contains(&identity) || self.seen.contains(&(identity, path.clone())) {
            return Ok(());
        }
        attempt.allocate(path.len().saturating_mul(2).saturating_add(256))?;
        self.seen.insert((identity, path.clone()));
        attempt.allocate(
            ancestors
                .len()
                .saturating_add(1)
                .saturating_mul(2 * std::mem::size_of_val(&identity)),
        )?;
        ancestors.push(identity);
        if !self.templates.contains_key(&identity) {
            let mut template = Vec::new();
            self.collect(schema, scope, Vec::new(), 0, &mut template, attempt)?;
            attempt.allocate(2048)?;
            self.templates.insert(identity, template);
        }
        for (relative, schema, scope, dynamic) in &self.templates[&identity] {
            let mut position = attempt.vector(path.len().saturating_add(relative.len()))?;
            attempt.work(path.len().saturating_add(relative.len()).saturating_add(1))?;
            position.extend_from_slice(&path);
            position.extend_from_slice(relative);
            let keyword = if *dynamic { "$dynamicRef" } else { "$ref" };
            let name = value::name(schema.get(keyword).expect("template reference"), attempt)?;
            attempt.allocate(
                position
                    .len()
                    .saturating_mul(4)
                    .saturating_add(1024)
                    .saturating_add(
                        ancestors
                            .len()
                            .saturating_mul(std::mem::size_of_val(&identity)),
                    ),
            )?;
            self.found.entry(position.clone()).or_default().push((
                name,
                schema.clone(),
                *scope,
                *dynamic,
            ));
            self.pending.push((
                schema.clone(),
                *scope,
                position,
                *dynamic,
                ancestors.clone(),
            ));
        }
        Ok(())
    }
    fn collect(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        path: Name,
        depth: usize,
        out: &mut Vec<RelativeReference>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        attempt.depth(depth)?;
        let scope = self.resources.enter(schema, scope, attempt)?;
        for (keyword, dynamic) in [("$ref", false), ("$dynamicRef", true)] {
            if let Some(reference) = schema.get(keyword) {
                let text = reference.as_str().ok_or(Failure::InvalidReference)?;
                attempt.work(text.len().saturating_mul(8).saturating_add(1))?;
                if !uri::valid(text) {
                    return Err(Failure::InvalidReference);
                }
                attempt.allocate(path.len().saturating_mul(2).saturating_add(256))?;
                out.push((path.clone(), schema.to_owned(), scope, dynamic));
            }
        }
        // Paths distinguish reference occurrences at different schema positions.
        // Following a reference is transparent at its current position.
        for keyword in ["properties", "dependentSchemas", "patternProperties"] {
            if let Some(properties) = schema.get(keyword).and_then(|v| v.members()) {
                for property in properties {
                    let mut child =
                        segment(&path, &keyword.encode_utf16().collect::<Vec<_>>(), attempt)?;
                    child = segment(&child, &value::name(property.name, attempt)?, attempt)?;
                    self.collect(property.value, scope, child, depth + 1, out, attempt)?;
                }
            }
        }
        for key in [
            "items",
            "additionalProperties",
            "not",
            "if",
            "then",
            "else",
            "contains",
            "propertyNames",
            "unevaluatedItems",
            "unevaluatedProperties",
        ] {
            if let Some(child) = schema.get(key) {
                let path = segment(&path, &key.encode_utf16().collect::<Vec<_>>(), attempt)?;
                self.collect(child, scope, path, depth + 1, out, attempt)?;
            }
        }
        for key in ["allOf", "oneOf", "anyOf", "prefixItems"] {
            if let Some(children) = schema.get(key).and_then(|v| v.elements()) {
                for child in children {
                    let path = segment(&path, &key.encode_utf16().collect::<Vec<_>>(), attempt)?;
                    self.collect(child, scope, path, depth + 1, out, attempt)?;
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn segment(
    path: &Name,
    name: &[u16],
    attempt: &mut Attempt<'_>,
) -> Result<Name, Failure> {
    attempt.work(path.len().saturating_add(name.len()).saturating_add(1))?;
    let mut out = attempt.vector(
        path.len()
            .saturating_add(name.len().saturating_mul(2))
            .saturating_add(2),
    )?;
    out.extend_from_slice(path);
    out.extend([0, 1]);
    for unit in name {
        out.push(*unit);
        if *unit == 0 {
            out.push(0);
        }
    }
    Ok(out)
}
