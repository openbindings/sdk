//! Ordered enum membership over borrowed exact JSON, without fingerprints.
use super::{Graph, NoVerdict, storage, value_predicates};
use openbindings::{JsonKind, JsonRef};
use openbindings_internal_json::{backend, decimal::Decimal};
use std::cmp::Ordering;
use storage::Ledger;

pub(super) const LINEAR: usize = 8;

fn rank(value: JsonRef<'_>) -> u8 {
    match value.kind() {
        JsonKind::Null => 0,
        JsonKind::Boolean => 1,
        JsonKind::Number => 2,
        JsonKind::String => 3,
        JsonKind::Array => 4,
        JsonKind::Object => 5,
    }
}
fn strings<L: Ledger>(
    a: JsonRef<'_>,
    b: JsonRef<'_>,
    ledger: &mut L,
) -> Result<Ordering, L::Error> {
    ledger.work(
        a.text()
            .len()
            .saturating_add(b.text().len())
            .saturating_mul(16)
            .saturating_add(1),
    )?;
    let mut a = a.as_string().expect("string").utf16_units();
    let mut b = b.as_string().expect("string").utf16_units();
    loop {
        ledger.work(1)?;
        for _ in 0..1024 {
            match (a.next(), b.next()) {
                (None, None) => return Ok(Ordering::Equal),
                (a, b) if a != b => return Ok(a.cmp(&b)),
                _ => {}
            }
        }
    }
}
fn number<L: Ledger>(value: JsonRef<'_>, ledger: &mut L) -> Result<Decimal, L::Error> {
    let text = value.number_text().expect("number");
    ledger.work(text.len().saturating_mul(8).saturating_add(1))?;
    if text.parse::<i64>().is_err() {
        // Covers parsing and the compact adjusted-exponent comparison scratch.
        ledger.allocation(text.len().saturating_mul(32).saturating_add(512))?;
    }
    Ok(Decimal::parse(text))
}
fn named<'a, L: Ledger>(
    object: JsonRef<'a>,
    name: JsonRef<'_>,
    ledger: &mut L,
) -> Result<Option<JsonRef<'a>>, L::Error> {
    if name.as_str().is_none() {
        // The arena's candidate lookup decodes an unpaired name into scratch.
        ledger.allocation(name.text().len().saturating_mul(2).saturating_add(64))?;
    }
    for candidate in backend::member_candidates(object, name, |units| ledger.work(units))? {
        if strings(name, candidate.name, ledger)? == Ordering::Equal {
            return Ok(Some(candidate.value));
        }
    }
    Ok(None)
}

struct Frame<'a> {
    left: JsonRef<'a>,
    right: JsonRef<'a>,
    next: usize,
    name: Option<JsonRef<'a>>,
    difference: Option<(JsonRef<'a>, Ordering)>,
}
impl<'a> Frame<'a> {
    fn record<L: Ledger>(
        &mut self,
        name: JsonRef<'a>,
        result: Ordering,
        ledger: &mut L,
    ) -> Result<(), L::Error> {
        if result != Ordering::Equal
            && match self.difference {
                None => true,
                Some((prior, _)) => strings(name, prior, ledger)? == Ordering::Less,
            }
        {
            self.difference = Some((name, result));
        }
        Ok(())
    }
    fn next<L: Ledger>(
        &mut self,
        ledger: &mut L,
    ) -> Result<Option<(JsonRef<'a>, JsonRef<'a>)>, L::Error> {
        let len = self.left.len().expect("container");
        while self.next < len {
            ledger.work(1)?;
            let at = self.next;
            self.next += 1;
            if self.left.kind() == JsonKind::Array {
                return Ok(Some((
                    self.left.element(at).expect("element"),
                    self.right.element(at).expect("element"),
                )));
            }
            let member = backend::member_at(self.left, at).expect("member");
            if let Some(other) = named(self.right, member.name, ledger)? {
                self.name = Some(member.name);
                return Ok(Some((member.value, other)));
            }
            self.record(member.name, Ordering::Greater, ledger)?;
        }
        // Object order uses the least differing decoded name in the union of
        // the two maps, with absent < present. No source member order survives.
        while self.left.kind() == JsonKind::Object && self.next - len < len {
            ledger.work(1)?;
            let member = backend::member_at(self.right, self.next - len).expect("member");
            self.next += 1;
            if named(self.left, member.name, ledger)?.is_none() {
                self.record(member.name, Ordering::Less, ledger)?;
            }
        }
        Ok(None)
    }
}

// On recursively unique-name values, Equal is exactly literal_equal: the same
// kind, mathematical Decimal value, decoded UTF-16 strings, positional arrays,
// and name/value maps independent of source order. Length-first containers and
// absent < present at the least differing name only order unequal values.
// Repeated-name literals cannot equal an admitted instance and are excluded
// from the index before comparison. Heap frames preserve deep-value support.
pub(super) fn order<L: Ledger>(
    left: JsonRef<'_>,
    right: JsonRef<'_>,
    ledger: &mut L,
) -> Result<Ordering, L::Error> {
    let mut pending: Vec<Frame<'_>> = Vec::new();
    let mut current = Some((left, right));
    let mut completed = None;
    loop {
        ledger.work(1)?;
        if let Some((a, b)) = current.take() {
            let kinds = rank(a).cmp(&rank(b));
            let result = if kinds != Ordering::Equal {
                kinds
            } else {
                match a.kind() {
                    JsonKind::Null => Ordering::Equal,
                    JsonKind::Boolean => a.as_bool().cmp(&b.as_bool()),
                    JsonKind::Number => number(a, ledger)?.compare(&number(b, ledger)?),
                    JsonKind::String => strings(a, b, ledger)?,
                    JsonKind::Array | JsonKind::Object => {
                        let lengths = a.len().cmp(&b.len());
                        if lengths == Ordering::Equal && a.len() != Some(0) {
                            let needed = pending.len() + 1;
                            storage::reserve(&mut pending, needed, ledger)?;
                            pending.push(Frame {
                                left: a,
                                right: b,
                                next: 0,
                                name: None,
                                difference: None,
                            });
                            continue;
                        }
                        lengths
                    }
                }
            };
            completed = Some(result);
        }
        if let Some(result) = completed.take() {
            let Some(frame) = pending.last_mut() else {
                return Ok(result);
            };
            if frame.left.kind() == JsonKind::Array {
                if result != Ordering::Equal {
                    pending.pop();
                    completed = Some(result);
                    continue;
                }
            } else {
                frame.record(frame.name.expect("compared member"), result, ledger)?;
            }
        }
        let frame = pending.last_mut().expect("container");
        current = frame.next(ledger)?;
        if current.is_none() {
            completed = Some(
                frame
                    .difference
                    .map_or(Ordering::Equal, |(_, result)| result),
            );
            pending.pop();
        }
    }
}

pub(super) fn prepare<L: Ledger>(
    choices: JsonRef<'_>,
    ledger: &mut L,
) -> Result<Vec<u32>, L::Error> {
    let count = choices.len().expect("enum array");
    let mut sorted = Vec::new();
    storage::reserve(&mut sorted, count, ledger)?;
    for (at, choice) in choices.elements().expect("enum array").enumerate() {
        ledger.work(1)?;
        if matches!(choice.kind(), JsonKind::Array | JsonKind::Object) {
            ledger.work(64)?;
            if choice.to_owned().has_duplicate_names() {
                continue;
            }
        }
        // Arena positions and hence array lengths are u32-bounded on all hosts.
        sorted.push(u32::try_from(at).expect("arena array position"));
    }
    // Fallible in-place heapsort keeps worst-case n log n construction and
    // cooperative work/refusal checks outside any infallible Ord callback.
    for root in (0..sorted.len() / 2).rev() {
        sift(&mut sorted, choices, root, ledger)?;
    }
    for end in (1..sorted.len()).rev() {
        sorted.swap(0, end);
        sift(&mut sorted[..end], choices, 0, ledger)?;
    }
    Ok(sorted)
}
fn sift<L: Ledger>(
    values: &mut [u32],
    choices: JsonRef<'_>,
    mut root: usize,
    ledger: &mut L,
) -> Result<(), L::Error> {
    while root < values.len() / 2 {
        let mut child = 2 * root + 1;
        let choice = |at: usize| choices.element(values[at] as usize).expect("choice");
        if child + 1 < values.len()
            && order(choice(child), choice(child + 1), ledger)? == Ordering::Less
        {
            child += 1;
        }
        if order(choice(root), choice(child), ledger)? != Ordering::Less {
            break;
        }
        values.swap(root, child);
        root = child;
    }
    Ok(())
}
#[inline(always)]
pub(super) fn contains(
    choices: JsonRef<'_>,
    sorted: Option<&[u32]>,
    value: JsonRef<'_>,
    graph: &mut Graph<'_>,
) -> Result<bool, NoVerdict> {
    if let Some(sorted) = sorted {
        let (mut lo, mut hi) = (0, sorted.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let choice = choices.element(sorted[mid] as usize).expect("choice");
            match order(value, choice, graph)? {
                Ordering::Less => hi = mid,
                Ordering::Greater => lo = mid + 1,
                Ordering::Equal => return Ok(true),
            }
        }
        return Ok(false);
    }
    for choice in choices.elements().ok_or_else(super::primitive::shape)? {
        graph.tick()?;
        if value_predicates::literal_equal(value, choice, graph)? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Default)]
struct State {
    queries: usize,
    sorted: Option<Vec<u32>>,
}
impl State {
    fn contains(
        &mut self,
        choices: JsonRef<'_>,
        value: JsonRef<'_>,
        graph: &mut Graph<'_>,
    ) -> Result<bool, NoVerdict> {
        if self.sorted.is_none() {
            self.queries += 1;
            if self.queries > choices.len().expect("enum array").ilog2() as usize {
                self.sorted = Some(prepare(choices, graph)?);
            }
        }
        contains(choices, self.sorted.as_deref(), value, graph)
    }
}

// Nothing is cached in Plan or across evaluations: cold and warm calls against
// the same prepared schema must spend the same work and reach the same outcome.
#[derive(Default)]
pub(super) struct Queries {
    // The common single-query evaluation keeps its counter inline, without a
    // heap allocation or additional work charge. Other nodes use the same
    // accounted schema-position entries as the evaluation's pattern searches.
    first: Option<(usize, State)>,
    others: storage::Entries<usize, State>,
}
impl Queries {
    // Keep counter/index storage code out of the bounded primitive walk.
    #[inline(never)]
    pub(super) fn contains(
        &mut self,
        node: usize,
        choices: JsonRef<'_>,
        value: JsonRef<'_>,
        graph: &mut Graph<'_>,
    ) -> Result<bool, NoVerdict> {
        if choices.kind() != JsonKind::Array || choices.len().expect("enum array") <= LINEAR {
            return contains(choices, None, value, graph);
        }
        self.state(node, graph)?.contains(choices, value, graph)
    }
    fn state(&mut self, node: usize, graph: &mut Graph<'_>) -> Result<&mut State, NoVerdict> {
        let (first_node, first) = self.first.get_or_insert_with(|| (node, State::default()));
        if *first_node == node {
            return Ok(first);
        }
        let index = if let Some(index) = self.others.find(&node, 1, graph)? {
            index
        } else {
            let index = self.others.len();
            self.others.push(node, State::default(), 1, graph)?;
            index
        };
        Ok(self.others.value_mut(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings::JsonValue;
    use std::sync::atomic::AtomicBool;

    const MIXED: &[&str] = &[
        "null",
        "false",
        "true",
        "0",
        "-0.0e300",
        "1",
        "1.0",
        "1e0",
        "-3.5",
        "9007199254740992",
        "9007199254740993",
        "1.0000000000000000001",
        "1e999999999999999999999",
        "10e999999999999999999998",
        "-1e-999999999999999999999",
        "-10e-1000000000000000000000",
        r#""a""#,
        r#""\u0061""#,
        r#""\ud800""#,
        r#""\uD800""#,
        r#""\ud801""#,
        r#""é""#,
        r#""e\u0301""#,
        r#""😀""#,
        r#""\ud83d\ude00""#,
        "[]",
        "{}",
        "[0,1]",
        "[1,0]",
        "[0.0,1e0]",
        r#"{"z":[1.0,"\ud800"],"a":null}"#,
        r#"{"a":null,"z":[1e0,"\uD800"]}"#,
        r#"{"a":1,"b":0}"#,
        r#"{"a":0,"b":1}"#,
        r#"{"b":0,"c":1}"#,
        r#"{"\ud800":{"😀":null}}"#,
        r#"{"\uD800":{"\ud83d\ude00":null}}"#,
        r#"{"a":[{"b":false},[]]}"#,
        r#"{"a":0,"b":1,"c":2,"d":3,"e":4,"f":5,"g":6,"h":7,"\ud800":[0]}"#,
        r#"{"\uD800":[-0.0],"h":7.0,"g":6.0,"f":5.0,"e":4.0,"d":3.0,"c":2.0,"b":1.0,"a":0.0}"#,
        r#"{"a":0,"b":1,"c":2,"d":3,"e":4,"f":5,"g":6,"h":7,"😀":[0]}"#,
    ];

    #[test]
    fn total_order_equal_class_is_literal_equality() {
        let values: Vec<_> = MIXED
            .iter()
            .map(|text| JsonValue::parse(text).unwrap())
            .collect();
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(usize::MAX, None, 0, &control);
        for a in &values {
            for b in &values {
                let ab = order(a.view(), b.view(), &mut graph).unwrap();
                assert_eq!(
                    ab == Ordering::Equal,
                    value_predicates::literal_equal(a.view(), b.view(), &mut graph).unwrap()
                );
                assert_eq!(ab, order(b.view(), a.view(), &mut graph).unwrap().reverse());
                for c in &values {
                    if ab != Ordering::Greater
                        && order(b.view(), c.view(), &mut graph).unwrap() != Ordering::Greater
                    {
                        assert_ne!(
                            order(a.view(), c.view(), &mut graph).unwrap(),
                            Ordering::Greater
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn indexed_and_small_membership_agree_with_plain_literal_walk() {
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(usize::MAX, None, 0, &control);
        let probes: Vec<_> = MIXED
            .iter()
            .chain(["99", r#""missing""#, "[null]", r#"{"missing":null}"#].iter())
            .map(|text| JsonValue::parse(text).unwrap())
            .collect();
        // Each small set has every JSON kind and repeated numeric choices.
        let small = [
            "null",
            "false",
            "1.0",
            r#""\u0061""#,
            "[0,1]",
            r#"{"a":1,"b":0}"#,
            "1",
            "1e0",
        ];
        for texts in [&small[..3], &small[..], &MIXED[..9], MIXED] {
            let choices = JsonValue::parse(format!("[{}]", texts.join(","))).unwrap();
            let sorted = if texts.len() > LINEAR {
                Some(prepare(choices.view(), &mut graph).unwrap())
            } else {
                None
            };
            for value in &probes {
                let expected = choices.view().elements().unwrap().any(|choice| {
                    value_predicates::literal_equal(value.view(), choice, &mut graph).unwrap()
                });
                assert_eq!(
                    contains(choices.view(), sorted.as_deref(), value.view(), &mut graph).unwrap(),
                    expected
                );
            }
        }
        // Duplicate member names, including nested ones, never enter the order.
        let choices =
            JsonValue::parse(r#"[{"a":1,"a":2},null,true,false,0,1,2,3,4,{"x":{"a":1,"a":2}}]"#)
                .unwrap();
        let sorted = prepare(choices.view(), &mut graph).unwrap();
        assert_eq!(sorted.len(), 8);
        for text in [r#"{"a":1}"#, r#"{"x":{"a":1}}"#] {
            assert!(
                !contains(
                    choices.view(),
                    Some(&sorted),
                    JsonValue::parse(text).unwrap().view(),
                    &mut graph
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn small_enum_work_never_exceeds_a_complete_walk() {
        let control = AtomicBool::new(false);
        for n in 1..=LINEAR {
            let choices = JsonValue::parse(format!("[{}]", MIXED[..n].join(","))).unwrap();
            for text in MIXED {
                let value = JsonValue::parse(text).unwrap();
                let mut old = Graph::new(usize::MAX, Some(usize::MAX), 0, &control);
                let mut expected = false;
                for choice in choices.view().elements().unwrap() {
                    old.tick().unwrap();
                    expected |=
                        value_predicates::literal_equal(value.view(), choice, &mut old).unwrap();
                }
                let mut new = Graph::new(usize::MAX, Some(usize::MAX), 0, &control);
                assert_eq!(
                    contains(choices.view(), None, value.view(), &mut new).unwrap(),
                    expected
                );
                assert!(new.remaining().work.unwrap() >= old.remaining().work.unwrap());
            }
        }
    }

    #[test]
    fn query_trigger_and_schema_positions_are_local_to_each_run() {
        let control = AtomicBool::new(false);
        let choices = JsonValue::parse("[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]").unwrap();
        for token in ["15", "16"] {
            let value = JsonValue::parse(token).unwrap();
            let mut graph = Graph::new(1 << 20, None, 0, &control);
            let mut queries = Queries::default();
            for q in 1..=5 {
                for node in [10, 20] {
                    assert_eq!(
                        queries
                            .contains(node, choices.view(), value.view(), &mut graph)
                            .unwrap(),
                        token == "15"
                    );
                    let state = queries.state(node, &mut graph).unwrap();
                    assert_eq!(state.sorted.is_some(), q > 4);
                }
            }
            let mut fresh = Queries::default();
            fresh
                .contains(10, choices.view(), value.view(), &mut graph)
                .unwrap();
            assert!(fresh.first.unwrap().1.sorted.is_none());
        }
    }

    #[test]
    fn malformed_enum_never_enters_the_query_index() {
        let control = AtomicBool::new(false);
        let choices =
            JsonValue::parse(r#"{"a":0,"b":1,"c":2,"d":3,"e":4,"f":5,"g":6,"h":7,"i":8}"#).unwrap();
        let value = JsonValue::parse("0").unwrap();
        let mut graph = Graph::new(0, None, 0, &control);
        let mut queries = Queries::default();
        for _ in 0..10 {
            assert_eq!(
                queries
                    .contains(99, choices.view(), value.view(), &mut graph)
                    .unwrap_err()
                    .code,
                "semantic-predicate-schema"
            );
        }
        assert!(queries.first.is_none());
    }

    #[test]
    fn single_large_query_and_all_small_queries_add_no_allocation_or_work() {
        let control = AtomicBool::new(false);
        for m in [8, 9, 1_000] {
            let choices = JsonValue::parse(format!(
                "[{}]",
                (0..m).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
            ))
            .unwrap();
            for token in ["0", &(m - 1).to_string(), &m.to_string()] {
                let value = JsonValue::parse(token).unwrap();
                let mut old = Graph::new(0, Some(usize::MAX), 0, &control);
                let mut expected = false;
                for choice in choices.view().elements().unwrap() {
                    old.tick().unwrap();
                    expected |=
                        value_predicates::literal_equal(value.view(), choice, &mut old).unwrap();
                }
                let mut new = Graph::new(0, Some(usize::MAX), 0, &control);
                let mut queries = Queries::default();
                for _ in 0..if m <= LINEAR { 100 } else { 1 } {
                    let before = new.remaining().work.unwrap();
                    assert_eq!(
                        queries
                            .contains(99, choices.view(), value.view(), &mut new)
                            .unwrap(),
                        expected
                    );
                    assert!(
                        before - new.remaining().work.unwrap()
                            <= usize::MAX - old.remaining().work.unwrap()
                    );
                    assert_eq!(new.remaining().bytes, 0);
                }
                if m <= LINEAR {
                    assert!(queries.first.is_none());
                }
            }
        }
    }

    #[test]
    fn deep_order_uses_accounted_heap_frames_and_refusals() {
        let control = AtomicBool::new(false);
        let a = JsonValue::parse(format!("{}0{}", "[".repeat(2000), "]".repeat(2000))).unwrap();
        let b = JsonValue::parse(format!("{}0.0{}", "[".repeat(2000), "]".repeat(2000))).unwrap();
        let mut graph = Graph::new(usize::MAX, None, 0, &control);
        assert_eq!(
            order(a.view(), b.view(), &mut graph).unwrap(),
            Ordering::Equal
        );
        assert_eq!(
            order(a.view(), b.view(), &mut Graph::new(0, None, 0, &control))
                .unwrap_err()
                .code,
            "semantic-analysis-memory-limit"
        );
        assert_eq!(
            order(
                a.view(),
                b.view(),
                &mut Graph::new(usize::MAX, Some(0), 0, &control)
            )
            .unwrap_err()
            .code,
            "evaluation-work-limit"
        );
        let stopped = AtomicBool::new(true);
        assert_eq!(
            order(
                a.view(),
                b.view(),
                &mut Graph::new(usize::MAX, None, 0, &stopped)
            )
            .unwrap_err()
            .code,
            "caller-cancelled"
        );
    }
}
