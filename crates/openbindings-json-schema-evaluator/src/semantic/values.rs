//! Exact instance-value identities for semantic predicates. Evaluation-cycle
//! detection still uses occurrence identity: descending to an equal child is
//! consumption, but equal JSON values give the same predicate the same input.
use super::{Graph, NoVerdict};
use openbindings::{JsonKind, JsonRef};
use openbindings_internal_json::{backend, decimal::Decimal};
use std::collections::HashMap;

#[derive(PartialEq, Eq, Hash)]
enum Value {
    Null,
    Boolean(bool),
    Number(Decimal),
    String(Vec<u16>),
    Array(Vec<usize>),
    Object(Vec<(usize, usize)>),
}
#[derive(Default)]
pub(super) struct Values {
    occurrences: HashMap<usize, usize>,
    values: HashMap<Value, usize>,
}
impl Values {
    pub(super) fn intern(
        &mut self,
        value: JsonRef<'_>,
        graph: &mut Graph<'_>,
    ) -> Result<usize, NoVerdict> {
        let mut pending = vec![(value, false)];
        while let Some((current, expanded)) = pending.pop() {
            graph.tick()?;
            let occurrence = backend::ref_id(current);
            if self.occurrences.contains_key(&occurrence) {
                continue;
            }
            if !expanded && matches!(current.kind(), JsonKind::Array | JsonKind::Object) {
                graph.charge(64)?;
                pending.push((current, true));
                if let Some(items) = current.elements() {
                    for item in items {
                        graph.tick()?;
                        graph.charge(64)?;
                        pending.push((item, false));
                    }
                } else if let Some(members) = current.members() {
                    for member in members {
                        graph.tick()?;
                        graph.charge(128)?;
                        pending.push((member.name, false));
                        pending.push((member.value, false));
                    }
                }
                continue;
            }
            graph.charge(192)?;
            let key = match current.kind() {
                JsonKind::Null => Value::Null,
                JsonKind::Boolean => Value::Boolean(current.as_bool().expect("boolean")),
                JsonKind::Number => {
                    let text = current.number_text().expect("number");
                    graph.charge(text.len().saturating_mul(8))?;
                    Value::Number(Decimal::parse(text))
                }
                JsonKind::String => {
                    graph.charge(current.text().len().saturating_mul(2))?;
                    Value::String(current.utf16_units().expect("string"))
                }
                JsonKind::Array => {
                    let mut values = Vec::new();
                    for child in current.elements().expect("array") {
                        graph.tick()?;
                        graph.charge(16)?;
                        values.push(self.occurrences[&backend::ref_id(child)]);
                    }
                    Value::Array(values)
                }
                JsonKind::Object => {
                    let mut values = Vec::new();
                    for member in current.members().expect("object") {
                        graph.tick()?;
                        graph.charge(32)?;
                        values.push((
                            self.occurrences[&backend::ref_id(member.name)],
                            self.occurrences[&backend::ref_id(member.value)],
                        ));
                    }
                    // Names have exact interned identities, so sorting them is
                    // independent of source member order and spelling escapes.
                    values.sort_unstable();
                    Value::Object(values)
                }
            };
            let next = self.values.len();
            let identity = *self.values.entry(key).or_insert(next);
            self.occurrences.insert(occurrence, identity);
        }
        Ok(self.occurrences[&backend::ref_id(value)])
    }
}
