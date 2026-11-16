use crate::ComparisonFailureKind as FailureKind;
use crate::{
    ComparisonFailure as Failure, ComparisonLimits, SchemaInput,
    budget::Attempt,
    normalize::{get_value as get, is, outside},
    value::{
        self, Object, Value,
        membership::{Membership, contains_sorted},
    },
};
use std::{cmp::Ordering, fmt};

macro_rules! report {
    ($attempt:expr, $($arg:tt)*) => { report_text(format_args!($($arg)*), $attempt)? };
}
// Reserve each formatting fragment before String grows. This also bounds
// diagnostic generation for enormous exact numbers, enums and property names.
fn report_text(args: fmt::Arguments<'_>, attempt: &mut Attempt<'_>) -> Result<String, Failure> {
    struct Writer<'a, 'b> {
        text: String,
        attempt: &'a mut Attempt<'b>,
        failure: Option<Failure>,
    }
    impl fmt::Write for Writer<'_, '_> {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            if let Err(failure) = self
                .attempt
                .work(text.len().saturating_add(1))
                .and_then(|()| self.attempt.allocate(text.len().saturating_mul(2)))
            {
                self.failure = Some(failure);
                return Err(fmt::Error);
            }
            self.text.push_str(text);
            Ok(())
        }
    }
    attempt.allocate(16)?;
    let mut writer = Writer {
        text: String::new(),
        attempt,
        failure: None,
    };
    if fmt::write(&mut writer, args).is_err() {
        return Err(writer.failure.expect("budgeted formatting"));
    }
    Ok(writer.text)
}

/// The operation slot whose substitution is being checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// The candidate must accept everything the target accepts.
    Input,
    /// The candidate must produce only values the target admits.
    Output,
}
/// A profile verdict, ordered by the profile's collapse dominance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// The candidate can stand in for the target in this direction.
    Compatible,
    /// Inclusion is unresolved, including boundaries and ineligible failures.
    Indeterminate,
    /// A profile rule rejects substitution, subject to the profile vacuity exception.
    Incompatible,
}
impl Verdict {
    /// Collapse outcomes using incompatible > indeterminate > compatible.
    /// An empty collection is compatible. Normalization failures are not verdicts.
    pub fn collapse(verdicts: impl IntoIterator<Item = Self>) -> Self {
        verdicts.into_iter().max().unwrap_or(Self::Compatible)
    }
}
/// A directional answer. Reasons are report data, never diagnostic formatting.
#[derive(Clone, PartialEq, Eq)]
pub struct Comparison {
    verdict: Verdict,
    reason: Option<String>,
    boundary: bool,
}
impl fmt::Debug for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Comparison")
            .field("verdict", &self.verdict)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.verdict {
            Verdict::Compatible => "compatible",
            Verdict::Incompatible => "incompatible",
            Verdict::Indeterminate => "indeterminate",
        })
    }
}
impl Comparison {
    /// The directional verdict, separate from normalization failure.
    pub fn verdict(&self) -> Verdict {
        self.verdict
    }
    /// The first deciding rule's reason, including nested paths and exact values.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    /// The profile finding for an indeterminate answer.
    pub fn finding(&self) -> Option<&'static str> {
        (self.verdict == Verdict::Indeterminate).then_some(if self.boundary {
            "outside-profile"
        } else {
            "unresolved"
        })
    }
    fn compatible() -> Self {
        Self {
            verdict: Verdict::Compatible,
            reason: None,
            boundary: false,
        }
    }
}
/// Compare two present schemas under the profile's directional rules.
/// Absent slots belong to [`crate::check_interface`], which skips them.
pub fn compare(
    direction: Direction,
    target: &SchemaInput,
    candidate: &SchemaInput,
    limits: &ComparisonLimits,
) -> Result<Comparison, Failure> {
    compare_with_control(
        direction,
        target,
        candidate,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
/// Compare with one shared work and memory policy and cooperative cancellation.
pub fn compare_with_control(
    direction: Direction,
    target: &SchemaInput,
    candidate: &SchemaInput,
    limits: &ComparisonLimits,
    control: &dyn openbindings::WorkControl,
) -> Result<Comparison, Failure> {
    let mut attempt = Attempt::new(*limits, control);
    let result = inputs(
        direction,
        target,
        candidate,
        (
            crate::ComparisonSide::Target,
            crate::ComparisonSide::Candidate,
        ),
        &mut attempt,
    )?;
    attempt.check()?;
    Ok(result)
}
pub(crate) fn inputs(
    direction: Direction,
    target: &SchemaInput,
    candidate: &SchemaInput,
    sides: (crate::ComparisonSide, crate::ComparisonSide),
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    let target = crate::normalize::normalize(target, sides.0, attempt)?;
    let candidate = crate::normalize::normalize(candidate, sides.1, attempt)?;
    walk(direction, &target, &candidate, 0, attempt)
}
fn answer(
    verdict: Verdict,
    reason: String,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    // Reason builders reserve their components before allocation. Charge retained
    // report storage too, including any prefix copied while unwinding recursion.
    attempt.work(reason.len().saturating_add(1))?;
    attempt.allocate(reason.len())?;
    Ok(Comparison {
        verdict,
        reason: Some(reason),
        boundary: false,
    })
}
fn reject(reason: &str, attempt: &mut Attempt<'_>) -> Result<Comparison, Failure> {
    attempt.allocate(reason.len())?;
    answer(Verdict::Incompatible, reason.to_owned(), attempt)
}
fn nested(
    prefix: &str,
    mut result: Comparison,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    if let Some(reason) = result.reason.take() {
        attempt.allocate(prefix.len().saturating_add(reason.len()).saturating_add(2))?;
        result.reason = Some(report!(attempt, "{prefix}: {reason}"));
    }
    Ok(result)
}
fn object(value: &Value) -> Result<&Object, Failure> {
    if let Value::Object(object) = value.view() {
        Ok(object)
    } else {
        Err(Failure::from(FailureKind::InvalidSchema))
    }
}
fn array<'a>(map: &'a Object, key: &str) -> &'a [Value] {
    match get(map, key) {
        Some(Value::Array(values)) => values,
        _ => &[],
    }
}
fn properties(map: &Object) -> Option<&Object> {
    match get(map, "properties") {
        Some(Value::Object(map)) => Some(map),
        _ => None,
    }
}
fn includes(map: &Object, name: &str) -> bool {
    get(map, "type").is_none()
        || array(map, "type")
            .iter()
            .any(|v| matches!(v, Value::String(s) if is(s,name)))
}
fn name_eq(a: &[u16], b: &[u16], attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    attempt.work(a.len().min(b.len()).saturating_add(1))?;
    Ok(a == b)
}
fn covers(map: &Object, name: &[u16], attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    if get(map, "type").is_none() {
        return Ok(true);
    }
    for value in array(map, "type") {
        if let Value::String(s) = value
            && (name_eq(s, name, attempt)? || (is(name, "integer") && is(s, "number")))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn boundary(attempt: &mut Attempt<'_>) -> Result<Comparison, Failure> {
    let mut result = answer(
        Verdict::Indeterminate,
        "outside-profile: differing position has no profile decision rule".into(),
        attempt,
    )?;
    result.boundary = true;
    Ok(result)
}
fn unresolved(
    mut result: Comparison,
    why: &str,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    result.verdict = Verdict::Indeterminate;
    result.boundary = false;
    if let Some(reason) = result.reason.take() {
        result.reason = Some(report!(attempt, "{reason}; unresolved: {why}"));
    }
    Ok(result)
}
fn fold(current: &mut Comparison, result: Comparison) {
    if result.verdict > current.verdict {
        *current = result;
    }
}
fn finite(map: &Object) -> bool {
    get(map, "const").is_some() || get(map, "enum").is_some()
}
fn guard(
    result: Comparison,
    flow: &Object,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    if result.verdict != Verdict::Incompatible {
        return Ok(result);
    }
    if finite(flow) {
        return unresolved(
            result,
            "flowing const or enum may exclude the failure",
            attempt,
        );
    }
    if get(flow, "anyOf").is_some() {
        return unresolved(result, "flowing anyOf requires union evidence", attempt);
    }
    Ok(result)
}
fn recurse(
    direction: Direction,
    flow: &Value,
    accepts: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    if direction == Direction::Input {
        walk(direction, flow, accepts, depth, attempt)
    } else {
        walk(direction, accepts, flow, depth, attempt)
    }
}
fn walk(
    direction: Direction,
    target: &Value,
    candidate: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    walk_position(direction, target, candidate, depth, true, attempt)
}
fn walk_position(
    direction: Direction,
    target: &Value,
    candidate: &Value,
    depth: usize,
    coverage: bool,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    attempt.depth(depth)?;
    if value::equal(target, candidate, depth, attempt)? {
        return Ok(Comparison::compatible());
    }
    if matches!(target.view(), Value::Foreign(_)) || matches!(candidate.view(), Value::Foreign(_)) {
        return boundary(attempt);
    }
    let (t, c) = (object(target)?, object(candidate)?);
    let input = direction == Direction::Input;
    let (flow, accepts, receiver) = if input {
        (t, c, candidate)
    } else {
        (c, t, target)
    };
    if outside(flow, depth, attempt)?
        || [t, c]
            .iter()
            .any(|m| get(m, "allOf").is_some() || get(m, "oneOf").is_some())
    {
        return boundary(attempt);
    }
    let receiver_outside = outside(accepts, depth, attempt)?;
    if !receiver_outside && crate::normalize::top(receiver, depth, attempt)? {
        return Ok(Comparison::compatible());
    }
    // Emptiness is local to this position and precedes all failure sources.
    if all_lengths_empty(flow, attempt)? {
        return if receiver_outside {
            boundary(attempt)
        } else {
            Ok(Comparison::compatible())
        };
    }
    // A union covers whole counterparts. In particular a nominally untyped
    // flowing union must not fail the plain receiver's type after coverage.
    if coverage && (union(flow).is_some() || union(accepts).is_some()) {
        return unions(direction, t, c, depth, attempt);
    }
    let flowing_value = if input { target } else { candidate };
    let mut result = Comparison::compatible();
    if crate::normalize::top(flowing_value, depth, attempt)?
        && !length_only_top(accepts, depth, attempt)?
    {
        fold(
            &mut result,
            answer(
                Verdict::Indeterminate,
                "top: inclusion in constrained receiver is unresolved".into(),
                attempt,
            )?,
        );
    }
    fold(&mut result, types(direction, t, c, attempt)?);
    if let Some(values) = values(direction, t, c, depth, attempt)? {
        fold(&mut result, values);
    }
    if includes(flow, "object") {
        let objects = objects(direction, flow, accepts, depth, attempt)?;
        fold(&mut result, guard(objects, flow, attempt)?);
    }
    if includes(flow, "array") && !length_empty(flow, "minItems", "maxItems", attempt)? {
        let items = if get(accepts, "prefixItems").is_some() {
            boundary(attempt)?
        } else if zero_items(flow, attempt)? {
            Comparison::compatible()
        } else {
            let top = Value::Object(Object::new());
            nested(
                "items",
                recurse(
                    direction,
                    get(flow, "items").unwrap_or(&top),
                    get(accepts, "items").unwrap_or(&top),
                    depth + 1,
                    attempt,
                )?,
                attempt,
            )?
        };
        fold(&mut result, guard(items, flow, attempt)?);
    }
    if includes(flow, "number") || includes(flow, "integer") {
        for (key, exclusive, lower) in [
            ("minimum", "exclusiveMinimum", true),
            ("maximum", "exclusiveMaximum", false),
        ] {
            let tb = bound(t, key, Some(exclusive), lower, attempt)?;
            let cb = bound(c, key, Some(exclusive), lower, attempt)?;
            if let Some(mut failure) = bounds(direction, key, tb, cb, lower, attempt)? {
                let (fb, rb) = if input { (tb, cb) } else { (cb, tb) };
                if !includes(flow, "number") && !integer_endpoint(fb, rb, attempt)? {
                    failure = unresolved(
                        failure,
                        "integer endpoint does not establish a failure",
                        attempt,
                    )?;
                }
                fold(&mut result, guard(failure, flow, attempt)?);
            }
        }
    }
    for (ty, min, max) in [
        ("string", "minLength", "maxLength"),
        ("array", "minItems", "maxItems"),
    ] {
        if includes(flow, ty) && !length_empty(flow, min, max, attempt)? {
            for (key, lower) in [(min, true), (max, false)] {
                let tb = length_bound(t, key, lower, attempt)?;
                let cb = length_bound(c, key, lower, attempt)?;
                if let Some(failure) = bounds(direction, key, tb, cb, lower, attempt)? {
                    fold(&mut result, guard(failure, flow, attempt)?);
                }
            }
        }
    }
    if receiver_outside {
        fold(&mut result, boundary(attempt)?);
    }
    Ok(result)
}
const TYPES: [&str; 7] = [
    "array", "boolean", "integer", "null", "number", "object", "string",
];
fn types(
    direction: Direction,
    t: &Object,
    c: &Object,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    let input = direction == Direction::Input;
    let (flow, accepts) = if input { (t, c) } else { (c, t) };
    let mut result = Comparison::compatible();
    for name in TYPES {
        attempt.work(1)?;
        if !includes(flow, name) || excluded_length(flow, name, attempt)? {
            continue;
        }
        let encoded = value::literal(name, attempt)?;
        if !covers(accepts, &encoded, attempt)? {
            let ty = quote(&encoded, attempt)?;
            let mut failure = answer(
                Verdict::Incompatible,
                if input {
                    report!(attempt, "type: candidate does not allow {ty}")
                } else {
                    report!(
                        attempt,
                        "type: candidate may produce {ty} which target does not allow"
                    )
                },
                attempt,
            )?;
            if name == "number"
                && includes(accepts, "integer")
                && singleton_interval(flow, attempt)?
            {
                failure = unresolved(
                    failure,
                    "flowing interval admits at most one number",
                    attempt,
                )?;
            }
            fold(&mut result, guard(failure, flow, attempt)?);
        }
    }
    Ok(result)
}
fn member(
    direction: Direction,
    flow: &Value,
    accepts: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    if crate::normalize::empty_member(flow) || crate::normalize::top(accepts, depth, attempt)? {
        return Ok(Comparison::compatible());
    }
    if crate::normalize::top(flow, depth, attempt)? && crate::normalize::empty_member(accepts) {
        return reject(
            if direction == Direction::Input {
                "additionalProperties: candidate forbids but target allows"
            } else {
                "additionalProperties: target forbids but candidate allows"
            },
            attempt,
        );
    }
    recurse(direction, flow, accepts, depth, attempt)
}
fn objects(
    direction: Direction,
    flow: &Object,
    accepts: &Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    let mut result = Comparison::compatible();
    for name in array(accepts, "required") {
        if !contains_sorted(array(flow, "required"), name, depth + 1, attempt)? {
            let name = render(name, depth, attempt)?;
            fold(
                &mut result,
                answer(
                    Verdict::Incompatible,
                    if direction == Direction::Input {
                        report!(
                            attempt,
                            "required: candidate requires {name} but target does not"
                        )
                    } else {
                        report!(
                            attempt,
                            "required: target requires {name} but candidate does not"
                        )
                    },
                    attempt,
                )?,
            );
        }
    }
    let top = Value::Object(Object::new());
    let (fp, rp) = (properties(flow), properties(accepts));
    let fa = get(flow, "additionalProperties").unwrap_or(&top);
    let ra = get(accepts, "additionalProperties").unwrap_or(&top);
    let skipped = get(accepts, "patternProperties").is_some();
    let mut fi = fp.into_iter().flat_map(|m| m.iter()).peekable();
    let mut ri = rp.into_iter().flat_map(|m| m.iter()).peekable();
    while fi.peek().is_some() || ri.peek().is_some() {
        attempt.work(1)?;
        let order = match (fi.peek(), ri.peek()) {
            (Some((a, _)), Some((b, _))) => {
                attempt.work(a.len().min(b.len()).saturating_add(1))?;
                a.cmp(b)
            }
            (Some(_), None) => Ordering::Less,
            _ => Ordering::Greater,
        };
        let (name, f, r) = match order {
            Ordering::Less => {
                let (name, f) = fi.next().expect("flowing property");
                if skipped {
                    continue;
                }
                (name, f, ra)
            }
            Ordering::Greater => {
                let (name, r) = ri.next().expect("receiving property");
                (name, fa, r)
            }
            Ordering::Equal => {
                let (name, f) = fi.next().expect("flowing property");
                let (_, r) = ri.next().expect("receiving property");
                (name, f, r)
            }
        };
        let comparison = if order == Ordering::Less
            && crate::normalize::empty_member(r)
            && crate::normalize::top(f, depth + 1, attempt)?
        {
            reject(
                if direction == Direction::Output {
                    "candidate property not allowed by target"
                } else {
                    "target property not allowed by candidate"
                },
                attempt,
            )?
        } else {
            member(direction, f, r, depth + 1, attempt)?
        };
        if comparison.verdict != Verdict::Compatible {
            let name = quote(name, attempt)?;
            fold(
                &mut result,
                nested(&report!(attempt, "properties[{name}]"), comparison, attempt)?,
            );
        }
    }
    if skipped {
        fold(&mut result, boundary(attempt)?);
    } else {
        fold(
            &mut result,
            nested(
                "additionalProperties",
                member(direction, fa, ra, depth + 1, attempt)?,
                attempt,
            )?,
        );
    }
    Ok(result)
}
fn zero() -> &'static openbindings::JsonValue {
    static ZERO: std::sync::OnceLock<openbindings::JsonValue> = std::sync::OnceLock::new();
    ZERO.get_or_init(|| openbindings::JsonValue::parse("0").expect("zero"))
}
fn length_bound<'a>(
    map: &'a Object,
    key: &str,
    lower: bool,
    attempt: &mut Attempt<'_>,
) -> Result<Option<Bound<'a>>, Failure> {
    let bound = bound(map, key, None, lower, attempt)?;
    let below_zero = if let Some(b) = bound {
        value::numbers(b.value, zero(), attempt)? == Ordering::Less
    } else {
        true
    };
    if lower && below_zero {
        // The default lower endpoint is zero, not negative infinity.
        return Ok(Some(Bound {
            value: zero(),
            exclusive: false,
        }));
    }
    Ok(bound)
}
fn length_empty(
    map: &Object,
    min: &str,
    max: &str,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    let lower = length_bound(map, min, true, attempt)?.expect("length minimum");
    let upper = length_bound(map, max, false, attempt)?;
    Ok(if let Some(upper) = upper {
        value::numbers(lower.value, upper.value, attempt)? == Ordering::Greater
    } else {
        false
    })
}
fn excluded_length(map: &Object, name: &str, attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    match name {
        "array" => length_empty(map, "minItems", "maxItems", attempt),
        "string" => length_empty(map, "minLength", "maxLength", attempt),
        _ => Ok(false),
    }
}
fn all_lengths_empty(map: &Object, attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    for name in TYPES {
        if includes(map, name) && !excluded_length(map, name, attempt)? {
            return Ok(false);
        }
    }
    Ok(true)
}
// Kept definitions are identity context, never directional constraints.
fn constraints(map: &Object) -> impl Iterator<Item = (&value::Name, &Value)> {
    map.iter().filter(|(key, _)| !is(key, "$defs"))
}
fn length_only_top(map: &Object, depth: usize, attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    for (key, value) in constraints(map) {
        attempt.work(512)?;
        if is(key, "minLength") || is(key, "minItems") {
            let Value::Number(n) = value else {
                return Ok(false);
            };
            if value::numbers(n, zero(), attempt)? == Ordering::Greater {
                return Ok(false);
            }
        } else if !crate::normalize::top_keyword(key, value, depth, attempt)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn zero_items(map: &Object, attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    Ok(if let Some(Value::Number(max)) = get(map, "maxItems") {
        value::numbers(max, zero(), attempt)? == Ordering::Equal
    } else {
        false
    })
}
fn singleton_interval(map: &Object, attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    let lo = bound(map, "minimum", Some("exclusiveMinimum"), true, attempt)?;
    let hi = bound(map, "maximum", Some("exclusiveMaximum"), false, attempt)?;
    Ok(if let (Some(lo), Some(hi)) = (lo, hi) {
        value::numbers(lo.value, hi.value, attempt)? != Ordering::Less
    } else {
        false
    })
}
fn integer_endpoint(
    flow: Option<Bound<'_>>,
    receives: Option<Bound<'_>>,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    let Some(f) = flow else { return Ok(true) };
    let Some(r) = receives else { return Ok(true) };
    if !value::integer(f.value, attempt)? || !value::integer(r.value, attempt)? {
        return Ok(false);
    }
    Ok((!f.exclusive && !r.exclusive)
        || (!f.exclusive
            && r.exclusive
            && value::numbers(f.value, r.value, attempt)? == Ordering::Equal))
}
fn only(map: &Object, keys: &[&str], attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    for (key, _) in constraints(map) {
        attempt.work(512)?;
        if !keys.iter().any(|k| is(key, k)) && applicable(map, key) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn applicable(map: &Object, key: &[u16]) -> bool {
    let types: &[&str] = if ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"]
        .iter()
        .any(|k| is(key, k))
    {
        &["number", "integer"]
    } else if ["minLength", "maxLength"].iter().any(|k| is(key, k)) {
        &["string"]
    } else if ["items", "minItems", "maxItems"].iter().any(|k| is(key, k)) {
        &["array"]
    } else if ["properties", "required", "additionalProperties"]
        .iter()
        .any(|k| is(key, k))
    {
        &["object"]
    } else {
        return true;
    };
    types.iter().any(|ty| includes(map, ty))
}
fn typed_value(map: &Object, v: &Value, attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    let ty = match v.view() {
        Value::Null => "null",
        Value::Boolean(_) => "boolean",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Number(n) => {
            return Ok(includes(map, "number")
                || (includes(map, "integer") && value::integer(n, attempt)?));
        }
        _ => return Err(FailureKind::InvalidSchema.into()),
    };
    Ok(includes(map, ty))
}
fn values(
    direction: Direction,
    t: &Object,
    c: &Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Option<Comparison>, Failure> {
    let input = direction == Direction::Input;
    let (flow, accepts, side, other) = if input {
        (t, c, "target", "candidate")
    } else {
        (c, t, "candidate", "target")
    };
    if !finite(accepts) {
        return Ok(None);
    }
    let source = if let Some(v) = get(flow, "const") {
        std::slice::from_ref(v)
    } else {
        array(flow, "enum")
    };
    let mut result = Comparison::compatible();
    if !finite(flow) {
        let key = if get(accepts, "const").is_some() {
            "const"
        } else {
            "enum"
        };
        let failure = reject(
            &report!(
                attempt,
                "{key}: {side} is unconstrained but {other} has {key}"
            ),
            attempt,
        )?;
        let eligible = only(flow, &["type"], attempt)?
            && ["string", "number", "integer", "array", "object"]
                .iter()
                .any(|ty| includes(flow, ty));
        return Ok(Some(if eligible {
            failure
        } else {
            unresolved(
                failure,
                "finite restriction has no eligible flowing value",
                attempt,
            )?
        }));
    }
    let clean = only(flow, &["type", "const", "enum"], attempt)?;
    let source_enum = if get(flow, "const").is_some() && get(flow, "enum").is_some() {
        Some(Membership::new(
            array(flow, "enum"),
            source.len(),
            depth + 1,
            attempt,
        )?)
    } else {
        None
    };
    for key in ["const", "enum"] {
        let Some(constraint) = get(accepts, key) else {
            continue;
        };
        let membership = if key == "enum" {
            Some(Membership::new(
                array(accepts, "enum"),
                source.len(),
                depth + 1,
                attempt,
            )?)
        } else {
            None
        };
        for v in source {
            let admitted = if key == "const" {
                value::equal(v, constraint, depth + 1, attempt)?
            } else {
                membership
                    .as_ref()
                    .expect("enum membership")
                    .contains(v, depth + 1, attempt)?
            };
            if admitted {
                continue;
            }
            let eligible = clean
                && typed_value(flow, v, attempt)?
                && if let Some(index) = &source_enum {
                    index.contains(v, depth + 1, attempt)?
                } else {
                    true
                };
            let rendered = render(v, depth + 1, attempt)?;
            let reason = if key == "const" && get(flow, "const").is_some() {
                let cv = render(get(c, "const").expect("const"), depth + 1, attempt)?;
                let tv = render(get(t, "const").expect("const"), depth + 1, attempt)?;
                report!(
                    attempt,
                    "const: candidate const {cv} does not match target const {tv}"
                )
            } else if key == "const" {
                let constraint = render(constraint, depth + 1, attempt)?;
                report!(
                    attempt,
                    "const: {side} value {rendered} does not match {other} const {constraint}"
                )
            } else {
                report!(attempt, "enum: {side} value {rendered} not in {other} enum")
            };
            let failure = answer(Verdict::Incompatible, reason, attempt)?;
            fold(
                &mut result,
                if eligible {
                    failure
                } else {
                    unresolved(
                        failure,
                        "authored value does not establish a flowing failure",
                        attempt,
                    )?
                },
            );
            if result.verdict == Verdict::Incompatible {
                return Ok(Some(result));
            }
        }
    }
    Ok(Some(result))
}
#[derive(Clone, Copy)]
struct Bound<'a> {
    value: &'a openbindings::JsonValue,
    exclusive: bool,
}
fn bound<'a>(
    map: &'a Object,
    key: &str,
    exclusive: Option<&str>,
    lower: bool,
    attempt: &mut Attempt<'_>,
) -> Result<Option<Bound<'a>>, Failure> {
    let plain = match get(map, key) {
        Some(Value::Number(v)) => Some(Bound {
            value: v,
            exclusive: false,
        }),
        _ => None,
    };
    let strict = exclusive.and_then(|key| match get(map, key) {
        Some(Value::Number(v)) => Some(Bound {
            value: v,
            exclusive: true,
        }),
        _ => None,
    });
    Ok(match (plain, strict) {
        (Some(a), Some(b)) => {
            let cmp = value::numbers(a.value, b.value, attempt)?;
            Some(
                if cmp == Ordering::Equal
                    || (lower && cmp == Ordering::Less)
                    || (!lower && cmp == Ordering::Greater)
                {
                    b
                } else {
                    a
                },
            )
        }
        (a, b) => a.or(b),
    })
}
fn bounds(
    direction: Direction,
    key: &str,
    t: Option<Bound<'_>>,
    c: Option<Bound<'_>>,
    lower: bool,
    attempt: &mut Attempt<'_>,
) -> Result<Option<Comparison>, Failure> {
    let input = direction == Direction::Input;
    let bad = match (t, c) {
        (None, None) => false,
        (_, None) if input => false,
        (None, _) if !input => false,
        (None, Some(_)) | (Some(_), None) => true,
        (Some(t), Some(c)) => {
            let mut order = value::numbers(c.value, t.value, attempt)?;
            if order == Ordering::Equal {
                order = if lower {
                    c.exclusive.cmp(&t.exclusive)
                } else {
                    t.exclusive.cmp(&c.exclusive)
                };
            }
            if input == lower {
                order == Ordering::Greater
            } else {
                order == Ordering::Less
            }
        }
    };
    if !bad {
        return Ok(None);
    }
    let reason = match (t, c) {
        (Some(t), Some(c)) => {
            let tv = t.value.text();
            let cv = c.value.text();
            attempt.allocate(tv.len().saturating_add(cv.len()).saturating_add(256))?;
            let te = if t.exclusive { "exclusive " } else { "" };
            let ce = if c.exclusive { "exclusive " } else { "" };
            let order = if input == lower { "greater" } else { "less" };
            report!(
                attempt,
                "{key}: candidate {key} {ce}{cv} is {order} than target {key} {te}{tv}"
            )
        }
        (None, Some(c)) => {
            attempt.allocate(c.value.text().len().saturating_add(128))?;
            report!(
                attempt,
                "{key}: candidate has {key} {}{} but target is unconstrained",
                if c.exclusive { "exclusive " } else { "" },
                c.value.text()
            )
        }
        (Some(t), None) => {
            attempt.allocate(t.value.text().len().saturating_add(128))?;
            report!(
                attempt,
                "{key}: candidate missing {key} required by target {}{}",
                if t.exclusive { "exclusive " } else { "" },
                t.value.text()
            )
        }
        _ => unreachable!(),
    };
    answer(Verdict::Incompatible, reason, attempt).map(Some)
}
fn union(map: &Object) -> Option<&[Value]> {
    match get(map, "anyOf") {
        Some(Value::Union(v)) => Some(v),
        _ => None,
    }
}
fn siblings(map: &Object, depth: usize, attempt: &mut Attempt<'_>) -> Result<Value, Failure> {
    let mut result = Object::new();
    for (key, v) in constraints(map) {
        attempt.work(1)?;
        if is(key, "anyOf") {
            continue;
        }
        attempt.allocate(key.len().saturating_mul(2))?;
        value::insert(
            &mut result,
            key.clone(),
            value::copy(v, depth + 1, attempt)?,
            attempt,
        )?;
    }
    Ok(Value::Object(result))
}
fn conjoined(
    variant: &Value,
    siblings: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Option<Value>, Failure> {
    if matches!(siblings,Value::Object(m) if m.is_empty()) {
        return Ok(Some(value::copy(variant, depth, attempt)?));
    }
    let mut branches = attempt.vector(2)?;
    branches.push(value::copy(siblings, depth + 1, attempt)?);
    branches.push(value::copy(variant, depth + 1, attempt)?);
    match crate::merge::all_of(Object::new(), branches, depth + 1, attempt) {
        Ok(value) => Ok(Some(value)),
        // This conjunction is an evidence probe, not an authored allOf. A local
        // empty merge supplies no counterexample and is not a schema error.
        Err(e)
            if matches!(
                e.kind(),
                FailureKind::UnsatisfiableMerge | FailureKind::UnsupportedNormalization
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
fn common_type_failure(
    flow: &Value,
    choices: &[Value],
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    let Value::Object(flow) = flow.view() else {
        return Ok(false);
    };
    if finite(flow)
        || get(flow, "anyOf").is_some()
        || get(flow, "oneOf").is_some()
        || get(flow, "allOf").is_some()
        || outside(flow, depth, attempt)?
    {
        return Ok(false);
    }
    for ty in TYPES {
        if !includes(flow, ty) || excluded_length(flow, ty, attempt)? {
            continue;
        }
        let name = value::literal(ty, attempt)?;
        let mut excluded = true;
        for choice in choices {
            let Value::Object(receiver) = choice.view() else {
                excluded = false;
                break;
            };
            if get(receiver, "oneOf").is_some()
                || get(receiver, "allOf").is_some()
                || covers(receiver, &name, attempt)?
                || (ty == "number"
                    && includes(receiver, "integer")
                    && singleton_interval(flow, attempt)?)
            {
                excluded = false;
                break;
            }
        }
        if excluded {
            return Ok(true);
        }
    }
    Ok(false)
}
fn unions(
    direction: Direction,
    t: &Object,
    c: &Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    let input = direction == Direction::Input;
    let (flow, accepts, side, other) = if input {
        (t, c, "target", "candidate")
    } else {
        (c, t, "candidate", "target")
    };
    let flow_whole = Value::Object(copy_map(flow, depth, attempt)?);
    let receiver_whole = Value::Object(copy_map(accepts, depth, attempt)?);
    let variants = union(flow).unwrap_or_else(|| std::slice::from_ref(&flow_whole));
    let choices = union(accepts).unwrap_or_else(|| std::slice::from_ref(&receiver_whole));
    let flow_siblings = if union(flow).is_some() {
        siblings(flow, depth, attempt)?
    } else {
        Value::Object(Object::new())
    };
    let mut result = Comparison::compatible();
    if union(accepts).is_some() {
        let receiving_siblings = siblings(accepts, depth, attempt)?;
        if !matches!(&receiving_siblings,Value::Object(m) if m.is_empty()) {
            let mut sibling_result = if input {
                walk_position(
                    direction,
                    &flow_whole,
                    &receiving_siblings,
                    depth + 1,
                    false,
                    attempt,
                )?
            } else {
                walk_position(
                    direction,
                    &receiving_siblings,
                    &flow_whole,
                    depth + 1,
                    false,
                    attempt,
                )?
            };
            if sibling_result.verdict != Verdict::Compatible {
                let evidence = recurse(
                    direction,
                    &flow_whole,
                    &receiving_siblings,
                    depth + 1,
                    attempt,
                )?;
                if evidence.verdict == Verdict::Incompatible {
                    fold(&mut sibling_result, evidence);
                }
            }
            fold(&mut result, sibling_result);
        }
    }
    for (index, variant) in variants.iter().enumerate() {
        let mut covered = false;
        for choice in choices {
            attempt.work(1)?;
            if recurse(direction, variant, choice, depth + 1, attempt)?.verdict
                == Verdict::Compatible
            {
                covered = true;
                break;
            }
        }
        if covered {
            continue;
        }
        let evidence = conjoined(variant, &flow_siblings, depth + 1, attempt)?;
        let mut failed = false;
        if let Some(evidence) = &evidence {
            if choices.len() == 1 {
                let comparison = recurse(direction, evidence, &choices[0], depth + 1, attempt)?;
                if comparison.verdict == Verdict::Incompatible {
                    failed = true;
                }
            } else if common_type_failure(evidence, choices, depth + 1, attempt)? {
                failed = true;
            }
        }
        let reason = report!(
            attempt,
            "anyOf: {side} variant {index} has no compatible {other} variant"
        );
        let comparison = if failed {
            // Keep the profile's union prefix and real authored index.
            answer(Verdict::Incompatible, reason, attempt)?
        } else {
            let comparison = answer(Verdict::Indeterminate, reason, attempt)?;
            unresolved(
                comparison,
                "pairwise coverage does not establish inclusion or a common failure",
                attempt,
            )?
        };
        fold(&mut result, comparison);
    }
    Ok(result)
}
fn copy_map(map: &Object, depth: usize, attempt: &mut Attempt<'_>) -> Result<Object, Failure> {
    let mut result = Object::new();
    for (key, v) in constraints(map) {
        attempt.work(key.len().saturating_add(1))?;
        attempt.allocate(key.len().saturating_mul(2))?;
        value::insert(
            &mut result,
            key.clone(),
            value::copy(v, depth + 1, attempt)?,
            attempt,
        )?;
    }
    Ok(result)
}
// Exact JSON diagnostics. Non-scalar UTF-16 units are escaped, never replaced.
fn quote(name: &[u16], attempt: &mut Attempt<'_>) -> Result<String, Failure> {
    let size = name.len().saturating_mul(6).saturating_add(2);
    attempt.work(size)?;
    attempt.allocate(size)?;
    let mut s = String::with_capacity(size);
    s.push('"');
    use std::fmt::Write;
    for ch in char::decode_utf16(name.iter().copied()) {
        match ch {
            Ok('"') => s.push_str("\\\""),
            Ok('\\') => s.push_str("\\\\"),
            Ok(c) if c < ' ' => {
                write!(s, "\\u{:04x}", c as u32).expect("string");
            }
            Ok(c) => s.push(c),
            Err(e) => {
                write!(s, "\\u{:04x}", e.unpaired_surrogate()).expect("string");
            }
        }
    }
    s.push('"');
    Ok(s)
}
fn render(v: &Value, depth: usize, attempt: &mut Attempt<'_>) -> Result<String, Failure> {
    attempt.depth(depth)?;
    attempt.allocate(16)?;
    let mut out = String::new();
    match v.view() {
        Value::Shared(_) => unreachable!("shared values contain an owned tree"),
        Value::Reference(authored, _) => return render(authored, depth + 1, attempt),
        Value::Foreign(value) => return render(value, depth + 1, attempt),
        Value::Null => out.push_str("null"),
        Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            attempt.allocate(n.text().len())?;
            attempt.work(n.text().len())?;
            out.push_str(n.text());
        }
        Value::String(s) => return quote(s, attempt),
        Value::Array(a) | Value::Union(a) => {
            out.push('[');
            for (i, v) in a.iter().enumerate() {
                let v = render(v, depth + 1, attempt)?;
                attempt.allocate(v.len().saturating_add(1))?;
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&v);
            }
            out.push(']');
        }
        Value::Object(m) => {
            out.push('{');
            for (i, (k, v)) in m.iter().enumerate() {
                let k = quote(k, attempt)?;
                let v = render(v, depth + 1, attempt)?;
                attempt.allocate(k.len().saturating_add(v.len()).saturating_add(2))?;
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&k);
                out.push(':');
                out.push_str(&v);
            }
            out.push('}');
        }
    }
    Ok(out)
}
