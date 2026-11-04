//! Borrowed exact equality and uniqueness. These predicates need no persistent
//! semantic identities; effect correlation continues to use `values::Values`.
use super::{Graph, NoVerdict, decision, storage};
use openbindings::{JsonKind, JsonRef};
use openbindings_internal_json::{backend, decimal::Decimal};
use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, DefaultHasher, Hash, Hasher},
};

pub(super) fn number(value: JsonRef<'_>, graph: &mut Graph<'_>) -> Result<Decimal, NoVerdict> {
    let text = value.number_text().expect("number");
    graph.work(text.len().saturating_mul(8).saturating_add(1))?;
    // Validated i64 tokens keep their exact Decimal representation inline.
    if text.parse::<i64>().is_err() {
        graph.charge(text.len().saturating_mul(16).saturating_add(256))?;
    }
    Ok(Decimal::parse(text))
}

pub(super) fn integer(value: JsonRef<'_>, graph: &mut Graph<'_>) -> Result<bool, NoVerdict> {
    let text = value.number_text().expect("number");
    // Keep number()'s ordered admission policy. A successful i64 parse proves
    // integrality without constructing a Decimal; wider tokens retain it.
    graph.work(text.len().saturating_mul(8).saturating_add(1))?;
    if text.parse::<i64>().is_ok() {
        return Ok(true);
    }
    graph.charge(text.len().saturating_mul(16).saturating_add(256))?;
    Ok(Decimal::parse(text).is_integer())
}

fn strings_equal(
    left: JsonRef<'_>,
    right: JsonRef<'_>,
    graph: &mut Graph<'_>,
) -> Result<bool, NoVerdict> {
    graph.work(
        left.text()
            .len()
            .saturating_add(right.text().len())
            .saturating_mul(16)
            .saturating_add(1),
    )?;
    let mut left = left.as_string().expect("string").utf16_units();
    let mut right = right.as_string().expect("string").utf16_units();
    loop {
        graph.tick()?;
        for _ in 0..1024 {
            match (left.next(), right.next()) {
                (None, None) => return Ok(true),
                (Some(a), Some(b)) if a == b => {}
                _ => return Ok(false),
            }
        }
    }
}

struct EqualityFrame<'a, 'b> {
    left: JsonRef<'a>,
    right: JsonRef<'b>,
    next: usize,
}

/// Both inputs have recursively unique names. The public evaluation boundary
/// admits the instance; literal admission is checked before calling this helper.
fn equal(left: JsonRef<'_>, right: JsonRef<'_>, graph: &mut Graph<'_>) -> Result<bool, NoVerdict> {
    let mut current = Some((left, right));
    let mut pending: Vec<EqualityFrame<'_, '_>> = Vec::new();
    loop {
        graph.tick()?;
        if let Some((left, right)) = current.take() {
            if left.kind() != right.kind() {
                return Ok(false);
            }
            let same = match left.kind() {
                JsonKind::Null => true,
                JsonKind::Boolean => left.as_bool() == right.as_bool(),
                JsonKind::Number => number(left, graph)? == number(right, graph)?,
                JsonKind::String => strings_equal(left, right, graph)?,
                JsonKind::Array | JsonKind::Object => {
                    if left.len() != right.len() {
                        return Ok(false);
                    }
                    if left.len() != Some(0) {
                        let needed = pending.len().saturating_add(1);
                        storage::reserve(&mut pending, needed, graph)?;
                        pending.push(EqualityFrame {
                            left,
                            right,
                            next: 0,
                        });
                    }
                    true
                }
            };
            if !same {
                return Ok(false);
            }
        }
        let Some(frame) = pending.last_mut() else {
            return Ok(true);
        };
        if frame.next == frame.left.len().expect("container") {
            pending.pop();
            continue;
        }
        let at = frame.next;
        frame.next += 1;
        if frame.left.kind() == JsonKind::Array {
            current = Some((
                frame.left.element(at).expect("element"),
                frame.right.element(at).expect("element"),
            ));
        } else {
            let member = backend::member_at(frame.left, at).expect("member");
            let candidates =
                backend::member_candidates(frame.right, member.name, |units| graph.work(units))?;
            let mut found = None;
            for candidate in candidates {
                graph.tick()?;
                if strings_equal(member.name, candidate.name, graph)? {
                    found = Some(candidate.value);
                    break;
                }
            }
            let Some(other) = found else { return Ok(false) };
            current = Some((member.value, other));
        }
    }
}

pub(super) fn literal_equal(
    value: JsonRef<'_>,
    literal: JsonRef<'_>,
    graph: &mut Graph<'_>,
) -> Result<bool, NoVerdict> {
    graph.tick()?;
    if matches!(literal.kind(), JsonKind::Array | JsonKind::Object) {
        // Duplicate lookup is a binary search over u32-bounded node positions.
        // A repeated-name literal cannot equal the admitted unique-name value.
        graph.work(64)?;
        if literal.to_owned().has_duplicate_names() {
            return Ok(false);
        }
    }
    equal(value, literal, graph)
}

fn string_hash(
    value: JsonRef<'_>,
    state: &mut DefaultHasher,
    graph: &mut Graph<'_>,
) -> Result<(), NoVerdict> {
    graph.work(value.text().len().saturating_mul(16).saturating_add(1))?;
    let mut units = value.as_string().expect("string").utf16_units();
    loop {
        graph.tick()?;
        for _ in 0..1024 {
            let Some(unit) = units.next() else {
                return Ok(());
            };
            state.write_u16(unit);
        }
    }
}

struct HashFrame<'a> {
    value: JsonRef<'a>,
    next: usize,
    state: DefaultHasher,
    sum: u64,
    name: u64,
}

fn fingerprint<'a>(
    value: JsonRef<'a>,
    seed: &RandomState,
    pending: &mut Vec<HashFrame<'a>>,
    graph: &mut Graph<'_>,
) -> Result<u64, NoVerdict> {
    debug_assert!(pending.is_empty());
    let mut current = Some(value);
    let mut completed = None;
    loop {
        graph.tick()?;
        if let Some(value) = current.take() {
            let mut state = seed.build_hasher();
            let container = match value.kind() {
                JsonKind::Null => {
                    state.write_u8(0);
                    false
                }
                JsonKind::Boolean => {
                    state.write_u8(1);
                    value.as_bool().expect("boolean").hash(&mut state);
                    false
                }
                JsonKind::Number => {
                    state.write_u8(2);
                    number(value, graph)?.hash(&mut state);
                    false
                }
                JsonKind::String => {
                    state.write_u8(3);
                    string_hash(value, &mut state, graph)?;
                    false
                }
                JsonKind::Array => {
                    state.write_u8(4);
                    true
                }
                JsonKind::Object => {
                    state.write_u8(5);
                    true
                }
            };
            if container {
                state.write_usize(value.len().expect("container"));
                let needed = pending.len().saturating_add(1);
                storage::reserve(pending, needed, graph)?;
                pending.push(HashFrame {
                    value,
                    next: 0,
                    state,
                    sum: 0,
                    name: 0,
                });
            } else {
                completed = Some(state.finish());
            }
        }
        if let Some(hash) = completed.take() {
            let Some(parent) = pending.last_mut() else {
                return Ok(hash);
            };
            graph.work(4)?;
            if parent.value.kind() == JsonKind::Array {
                parent.state.write_u64(hash);
            } else {
                // Member order cannot affect an object's candidate hash. Exact
                // equality, never this accumulator, decides a repeated value.
                parent.sum = parent.sum.wrapping_add(seed.hash_one((parent.name, hash)));
            }
        }
        let frame = pending.last_mut().expect("pending container");
        if frame.next == frame.value.len().expect("container") {
            let mut frame = pending.pop().expect("container");
            if frame.value.kind() == JsonKind::Object {
                frame.state.write_u64(frame.sum);
            }
            completed = Some(frame.state.finish());
            continue;
        }
        let at = frame.next;
        frame.next += 1;
        if frame.value.kind() == JsonKind::Array {
            current = frame.value.element(at);
        } else {
            let member = backend::member_at(frame.value, at).expect("member");
            let mut state = seed.build_hasher();
            state.write_u8(3);
            string_hash(member.name, &mut state, graph)?;
            frame.name = state.finish();
            current = Some(member.value);
        }
    }
}

fn verify_candidates(
    value: JsonRef<'_>,
    candidates: &[(u64, usize)],
    graph: &mut Graph<'_>,
) -> Result<bool, NoVerdict> {
    let mut start = 0;
    for at in 0..candidates.len() {
        graph.tick()?;
        if candidates[at].0 != candidates[start].0 {
            start = at;
        }
        for prior in start..at {
            graph.tick()?;
            if equal(
                value.element(candidates[at].1).expect("element"),
                value.element(candidates[prior].1).expect("element"),
                graph,
            )? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(super) fn unique(
    value: JsonRef<'_>,
    seed: &RandomState,
    graph: &mut Graph<'_>,
) -> Result<bool, NoVerdict> {
    graph.tick()?;
    let count = value.len().expect("array");
    if count < 2 {
        return Ok(true);
    }
    let mut pending = Vec::new();
    let mut candidates = Vec::new();
    storage::reserve(&mut candidates, count, graph)?;
    for (index, item) in value.elements().expect("array").enumerate() {
        graph.tick()?;
        candidates.push((fingerprint(item, seed, &mut pending, graph)?, index));
    }
    // The comparator is infallible and fixed-width. Prepay the SDK's n log n
    // admission charge, then observe control after sorting. This policy is not
    // an exact operation-count certificate for the standard library's sort.
    let work = count
        .checked_mul(count.ilog2() as usize + 1)
        .and_then(|n| n.checked_mul(64))
        .ok_or_else(|| decision::limit("evaluation-work-limit"))?;
    graph.work(work)?;
    candidates.sort_unstable();
    graph.tick()?;
    verify_candidates(value, &candidates, graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings::JsonValue;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn integer_predicate_preserves_numeric_admission_and_exact_answers() {
        let mut cases: Vec<_> = [
            ("0", true),
            ("-0", true),
            ("-9223372036854775808", true),
            ("9223372036854775807", true),
            ("-9223372036854775809", true),
            ("9223372036854775808", true),
            ("1.0", true),
            ("100e-2", true),
            ("100e-3", false),
            ("1e99999", true),
            ("1e-99999", false),
            ("-0.000e-99999", true),
            ("9223372036854775807.1", false),
        ]
        .into_iter()
        .map(|(text, expected)| (text.to_owned(), expected))
        .collect();
        cases.push(("9".repeat(10_000), true));
        const BYTES: usize = 1 << 20;
        const WORK: usize = 1 << 20;
        for (text, expected) in cases {
            let value = JsonValue::parse(&text).unwrap();
            let run = |specialized, bytes, steps, cancelled| {
                let control = AtomicBool::new(cancelled);
                let mut graph = Graph::new(bytes, steps, 0, &control);
                let result = if specialized {
                    integer(value.view(), &mut graph)
                } else {
                    number(value.view(), &mut graph).map(|number| number.is_integer())
                }
                .map_err(|detail| (detail.reason, detail.code));
                (result, graph.remaining().bytes, graph.remaining().work)
            };
            let original = run(false, BYTES, Some(WORK), false);
            assert_eq!(original.0, Ok(expected), "{text}");
            let bytes = BYTES - original.1;
            let work = WORK - original.2.unwrap();
            for (bytes, steps) in [
                (BYTES, Some(WORK)),
                (0, None),
                (BYTES, Some(0)),
                (bytes, Some(work)),
                (bytes.saturating_sub(1), Some(work)),
                (bytes, Some(work.saturating_sub(1))),
            ] {
                for cancelled in [false, true] {
                    assert_eq!(
                        run(true, bytes, steps, cancelled),
                        run(false, bytes, steps, cancelled),
                        "{text}: bytes={bytes}, steps={steps:?}, cancelled={cancelled}"
                    );
                }
            }
        }
    }

    #[test]
    fn exact_values_keep_number_string_order_and_kind_meanings() {
        let cases = [
            ("null", "null", true),
            ("true", "false", false),
            ("0", "-0.0e300", true),
            ("9007199254740993", "9007199254740992", false),
            ("9223372036854775808", "92233720368547758080e-1", true),
            ("1e400", "10e399", true),
            (r#""a""#, r#""\u0061""#, true),
            (r#""\uD800""#, r#""\ud800""#, true),
            (r#""\uD800""#, r#""\uFFFD""#, false),
            (r#""😀""#, r#""\ud83d\ude00""#, true),
            ("[]", "{}", false),
            ("[0,1]", "[1,0]", false),
            (
                r#"{"a":1,"b":[2,3]}"#,
                r#"{"b":[2.0,30e-1],"\u0061":1.0}"#,
                true,
            ),
            (
                r#"{"\ud800":{"😀":null}}"#,
                r#"{"\uD800":{"\ud83d\ude00":null}}"#,
                true,
            ),
            (r#"{"a":1}"#, r#"{"b":1}"#, false),
        ];
        let control = AtomicBool::new(false);
        let seed = RandomState::new();
        for (a, b, expected) in cases {
            let a = JsonValue::parse(a).unwrap();
            let b = JsonValue::parse(b).unwrap();
            let mut graph = Graph::new(1 << 20, None, 0, &control);
            assert_eq!(equal(a.view(), b.view(), &mut graph).unwrap(), expected);
            assert_eq!(
                literal_equal(a.view(), b.view(), &mut graph).unwrap(),
                expected
            );
            if expected {
                let mut scratch = Vec::new();
                let ah = fingerprint(a.view(), &seed, &mut scratch, &mut graph).unwrap();
                let bh = fingerprint(b.view(), &seed, &mut scratch, &mut graph).unwrap();
                assert_eq!(ah, bh, "equal values must share a candidate hash");
                assert!(scratch.is_empty());
            }
        }
    }

    #[test]
    fn forced_collisions_verify_nonadjacent_candidates_exactly() {
        let control = AtomicBool::new(false);
        for (text, expected) in [
            (r#"[{"a":1},{"b":2},{"a":1.0}]"#, false),
            (r#"[{"a":1},{"b":2},{"a":3}]"#, true),
            (r#"[[0,1],{},[1,0],null,true,"0",0]"#, true),
            (r#"[0.1,false,0.10]"#, false),
            (r#"[{"a":1,"b":2},{},{"b":2,"a":1}]"#, false),
        ] {
            let value = JsonValue::parse(text).unwrap();
            let candidates: Vec<_> = (0..value.view().len().unwrap()).map(|at| (0, at)).collect();
            let mut graph = Graph::new(1 << 20, None, 0, &control);
            assert_eq!(
                verify_candidates(value.view(), &candidates, &mut graph).unwrap(),
                expected
            );
            assert_eq!(
                unique(value.view(), &RandomState::new(), &mut graph).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn duplicate_literal_check_is_subtree_local_and_does_not_select_first_member() {
        let value = JsonValue::parse(r#"{"a":1,"b":2}"#).unwrap();
        let literal =
            JsonValue::parse(r#"{"other":{"x":1,"x":2},"good":{"b":2,"a":1},"bad":{"a":1,"a":2}}"#)
                .unwrap();
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(1 << 20, None, 0, &control);
        assert!(literal_equal(value.view(), literal.get("good").unwrap(), &mut graph).unwrap());
        assert!(!literal_equal(value.view(), literal.get("bad").unwrap(), &mut graph).unwrap());
    }

    #[test]
    fn refusal_cannot_become_a_false_predicate_and_fresh_retry_succeeds() {
        let value = JsonValue::parse(r#"[{"a":[1,2]},{"a":[1,3]}]"#).unwrap();
        let cancelled = AtomicBool::new(true);
        let available = AtomicBool::new(false);
        assert_eq!(
            unique(
                value.view(),
                &RandomState::new(),
                &mut Graph::new(1 << 20, None, 0, &cancelled)
            )
            .unwrap_err()
            .code,
            "caller-cancelled"
        );
        assert_eq!(
            unique(
                value.view(),
                &RandomState::new(),
                &mut Graph::new(0, None, 0, &available)
            )
            .unwrap_err()
            .code,
            "semantic-analysis-memory-limit"
        );
        assert_eq!(
            unique(
                value.view(),
                &RandomState::new(),
                &mut Graph::new(1 << 20, Some(0), 0, &available)
            )
            .unwrap_err()
            .code,
            "evaluation-work-limit"
        );
        assert!(
            unique(
                value.view(),
                &RandomState::new(),
                &mut Graph::new(1 << 20, None, 0, &available)
            )
            .unwrap()
        );
    }
}
