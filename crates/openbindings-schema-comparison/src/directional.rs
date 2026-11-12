use crate::{
    ComparisonFailure as Failure, ComparisonLimits, SchemaInput,
    budget::Attempt,
    normalize::{get, is, outside},
    value::{self, Object, Value},
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
    /// A profile rule rejects the substitution.
    Incompatible,
    /// A differing position is outside the profile's decision procedure.
    Indeterminate,
}
impl Verdict {
    /// Collapse outcomes using indeterminate > incompatible > compatible.
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
        (self.verdict == Verdict::Indeterminate).then_some("outside-profile")
    }
    fn compatible() -> Self {
        Self {
            verdict: Verdict::Compatible,
            reason: None,
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
/// Compare with one monotone budget and cooperative cancellation.
pub fn compare_with_control(
    direction: Direction,
    target: &SchemaInput,
    candidate: &SchemaInput,
    limits: &ComparisonLimits,
    control: &dyn openbindings::WorkControl,
) -> Result<Comparison, Failure> {
    let mut attempt = Attempt::new(*limits, control);
    let result = inputs(direction, target, candidate, &mut attempt)?;
    attempt.check()?;
    Ok(result)
}
pub(crate) fn inputs(
    direction: Direction,
    target: &SchemaInput,
    candidate: &SchemaInput,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    let target = crate::normalize::normalize(target, attempt)?;
    let candidate = crate::normalize::normalize(candidate, attempt)?;
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
    if let Value::Object(object) = value {
        Ok(object)
    } else {
        Err(Failure::InvalidSchema)
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
fn walk(
    direction: Direction,
    target: &Value,
    candidate: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    attempt.depth(depth)?;
    if value::equal(target, candidate, depth, attempt)? {
        return Ok(Comparison::compatible());
    }
    let (t, c) = (object(target)?, object(candidate)?);
    if outside(t, depth, attempt)?
        || outside(c, depth, attempt)?
        || get(t, "allOf").is_some()
        || get(c, "allOf").is_some()
        || (get(t, "oneOf").is_some() && get(t, "anyOf").is_some())
        || (get(c, "oneOf").is_some() && get(c, "anyOf").is_some())
    {
        return answer(
            Verdict::Indeterminate,
            "outside-profile: differing position has no profile decision rule".into(),
            attempt,
        );
    }
    let input = direction == Direction::Input;
    if (input && c.is_empty()) || (!input && t.is_empty()) {
        return Ok(Comparison::compatible());
    }
    if input && t.is_empty() {
        return reject(
            "top: target accepts anything but candidate is constrained",
            attempt,
        );
    }
    if !input && c.is_empty() {
        return reject(
            "top: candidate may produce anything but target is constrained",
            attempt,
        );
    }
    let (flow, accepts) = if input { (t, c) } else { (c, t) };
    let all_types;
    let types = if get(flow, "type").is_some() {
        array(flow, "type")
    } else {
        all_types = {
            let mut types = attempt.vector(7)?;
            for name in [
                "array", "boolean", "integer", "null", "number", "object", "string",
            ] {
                types.push(Value::String(value::literal(name, attempt)?));
            }
            types
        };
        &all_types
    };
    for ty in types {
        let Value::String(name) = ty else {
            return Err(Failure::InvalidSchema);
        };
        if !covers(accepts, name, attempt)? {
            let ty = render(ty, depth, attempt)?;
            return answer(
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
            );
        }
    }
    if let Some(result) = values(direction, t, c, depth, attempt)? {
        return Ok(result);
    }
    if includes(t, "object") || includes(c, "object") {
        let (needed, supplied) = if input { (c, t) } else { (t, c) };
        for name in array(needed, "required") {
            let mut found = false;
            for other in array(supplied, "required") {
                if value::equal(name, other, depth + 1, attempt)? {
                    found = true;
                    break;
                }
            }
            if !found {
                let name = render(name, depth, attempt)?;
                return answer(
                    Verdict::Incompatible,
                    if input {
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
                );
            }
        }
        let (tp, cp) = (properties(t), properties(c));
        // Merge traversal charges every property and name comparison; no map probe
        // or hostile common-prefix comparison runs outside the allowance.
        let mut ti = tp.into_iter().flat_map(|m| m.iter()).peekable();
        let mut ci = cp.into_iter().flat_map(|m| m.iter()).peekable();
        while ti.peek().is_some() || ci.peek().is_some() {
            attempt.work(1)?;
            let order = match (ti.peek(), ci.peek()) {
                (Some((a, _)), Some((b, _))) => {
                    attempt.work(a.len().min(b.len()).saturating_add(1))?;
                    a.cmp(b)
                }
                (Some(_), None) => Ordering::Less,
                _ => Ordering::Greater,
            };
            match order {
                Ordering::Less => {
                    ti.next();
                }
                Ordering::Greater => {
                    let (name, _) = ci.next().expect("candidate property");
                    if !input
                        && matches!(get(t, "additionalProperties"), Some(Value::Boolean(false)))
                    {
                        let name = quote(name, attempt)?;
                        return answer(
                            Verdict::Incompatible,
                            report!(
                                attempt,
                                "properties[{name}]: candidate property not allowed by target"
                            ),
                            attempt,
                        );
                    }
                }
                Ordering::Equal => {
                    let (name, tv) = ti.next().expect("target property");
                    let (_, cv) = ci.next().expect("candidate property");
                    let result = walk(direction, tv, cv, depth + 1, attempt)?;
                    if result.verdict != Verdict::Compatible {
                        let name = quote(name, attempt)?;
                        return nested(&report!(attempt, "properties[{name}]"), result, attempt);
                    }
                }
            }
        }
        if !input {
            match (
                get(t, "additionalProperties"),
                get(c, "additionalProperties"),
            ) {
                (Some(Value::Boolean(false)), Some(Value::Boolean(false))) => {}
                (Some(Value::Boolean(false)), _) => {
                    return reject(
                        "additionalProperties: target forbids but candidate allows",
                        attempt,
                    );
                }
                (Some(Value::Object(_)), Some(Value::Boolean(false))) => {}
                (Some(tv @ Value::Object(_)), Some(cv @ Value::Object(_))) => {
                    let result = walk(direction, tv, cv, depth + 1, attempt)?;
                    if result.verdict != Verdict::Compatible {
                        return nested("additionalProperties", result, attempt);
                    }
                }
                (Some(Value::Object(_)), _) => {
                    return reject(
                        "additionalProperties: target constrains but candidate allows anything",
                        attempt,
                    );
                }
                _ => {}
            }
        }
    }
    if includes(t, "array") || includes(c, "array") {
        let top = Value::Object(Object::new());
        let result = walk(
            direction,
            get(t, "items").unwrap_or(&top),
            get(c, "items").unwrap_or(&top),
            depth + 1,
            attempt,
        )?;
        if result.verdict != Verdict::Compatible {
            return nested("items", result, attempt);
        }
    }
    if includes(t, "number")
        || includes(t, "integer")
        || includes(c, "number")
        || includes(c, "integer")
    {
        for (key, exclusive, lower) in [
            ("minimum", "exclusiveMinimum", true),
            ("maximum", "exclusiveMaximum", false),
        ] {
            let tb = bound(t, key, Some(exclusive), lower, attempt)?;
            let cb = bound(c, key, Some(exclusive), lower, attempt)?;
            if let Some(result) = bounds(direction, key, tb, cb, lower, attempt)? {
                return Ok(result);
            }
        }
    }
    for (ty, keys) in [
        ("string", [("minLength", true), ("maxLength", false)]),
        ("array", [("minItems", true), ("maxItems", false)]),
    ] {
        if includes(t, ty) || includes(c, ty) {
            for (key, lower) in keys {
                let tb = bound(t, key, None, lower, attempt)?;
                let cb = bound(c, key, None, lower, attempt)?;
                if let Some(result) = bounds(direction, key, tb, cb, lower, attempt)? {
                    return Ok(result);
                }
            }
        }
    }
    unions(direction, t, c, depth, attempt)
}
fn values(
    direction: Direction,
    t: &Object,
    c: &Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Option<Comparison>, Failure> {
    let input = direction == Direction::Input;
    let (flow, accepts) = if input { (t, c) } else { (c, t) };
    let (side, other) = if input {
        ("target", "candidate")
    } else {
        ("candidate", "target")
    };
    let (key, constraint) = if let Some(v) = get(accepts, "const") {
        ("const", v)
    } else if let Some(v) = get(accepts, "enum") {
        ("enum", v)
    } else {
        return Ok(None);
    };
    let source = if let Some(v) = get(flow, "const") {
        std::slice::from_ref(v)
    } else if get(flow, "enum").is_some() {
        array(flow, "enum")
    } else {
        return reject(
            &report!(
                attempt,
                "{key}: {side} is unconstrained but {other} has {key}"
            ),
            attempt,
        )
        .map(Some);
    };
    for v in source {
        let admitted = if key == "const" {
            value::equal(v, constraint, depth + 1, attempt)?
        } else {
            let Value::Array(choices) = constraint else {
                return Err(Failure::InvalidSchema);
            };
            let mut found = false;
            for choice in choices {
                if value::equal(v, choice, depth + 1, attempt)? {
                    found = true;
                    break;
                }
            }
            found
        };
        if !admitted {
            let v = render(v, depth + 1, attempt)?;
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
                    "const: {side} value {v} does not match {other} const {constraint}"
                )
            } else {
                report!(attempt, "enum: {side} value {v} not in {other} enum")
            };
            return answer(Verdict::Incompatible, reason, attempt).map(Some);
        }
    }
    // A const and enum together are separate constraints. Check both rather than
    // allowing one to erase the other when a caller supplies both keywords.
    if key == "const" && get(accepts, "enum").is_some() {
        for v in source {
            let mut found = false;
            for choice in array(accepts, "enum") {
                if value::equal(v, choice, depth + 1, attempt)? {
                    found = true;
                    break;
                }
            }
            if !found {
                let v = render(v, depth + 1, attempt)?;
                return answer(
                    Verdict::Incompatible,
                    report!(attempt, "enum: {side} value {v} not in {other} enum"),
                    attempt,
                )
                .map(Some);
            }
        }
    }
    Ok(None)
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
fn union(map: &Object) -> Option<(&'static str, &[Value])> {
    if get(map, "oneOf").is_some() && get(map, "anyOf").is_some() {
        return None;
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Union(v)) = get(map, key) {
            return Some((key, v));
        }
    }
    None
}
fn unions(
    direction: Direction,
    t: &Object,
    c: &Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Comparison, Failure> {
    let (tu, cu) = (union(t), union(c));
    let (Some((tk, tv)), Some((ck, cv))) = (tu, cu) else {
        if let Some((key, _)) = tu.or(cu) {
            return reject(
                &report!(
                    attempt,
                    "{key}: only one side is a union; no cross-form rule"
                ),
                attempt,
            );
        }
        return Ok(Comparison::compatible());
    };
    let input = direction == Direction::Input;
    let (flow, choices, key, side, other) = if input {
        (tv, cv, tk, "target", "candidate")
    } else {
        (cv, tv, ck, "candidate", "target")
    };
    for (index, variant) in flow.iter().enumerate() {
        attempt.work(1)?;
        let mut uncertain = None;
        let mut found = false;
        for choice in choices {
            let result = if input {
                walk(direction, variant, choice, depth + 1, attempt)?
            } else {
                walk(direction, choice, variant, depth + 1, attempt)?
            };
            match result.verdict {
                Verdict::Compatible => {
                    found = true;
                    break;
                }
                Verdict::Indeterminate => {
                    if uncertain.is_none() {
                        uncertain = Some(result)
                    }
                }
                Verdict::Incompatible => {}
            }
        }
        if !found {
            if let Some(result) = uncertain {
                return nested(
                    &report!(attempt, "{key}: {side} variant {index}"),
                    result,
                    attempt,
                );
            }
            return answer(
                Verdict::Incompatible,
                report!(
                    attempt,
                    "{key}: {side} variant {index} has no compatible {other} variant"
                ),
                attempt,
            );
        }
    }
    Ok(Comparison::compatible())
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
    match v {
        Value::Reference(authored, _) => return render(authored, depth + 1, attempt),
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
