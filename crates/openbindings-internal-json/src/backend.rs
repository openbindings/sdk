//! Shared internal adapter: the schema engine reads immutable flat JSON without re-parsing.
use crate::raw::{Arena, Id, Kind, Member};
use jsonschema::json::{Array, Json, JsonNumber, Node, NodeIdentity, Object};
use jsonschema_value::{LazyInstance, ob_decimal::Decimal, types::JsonType};
use serde_json::Value;
use std::{borrow::Cow, sync::OnceLock};
pub struct FlatJson;
#[derive(Clone, Copy)]
pub enum View<'a> {
    Stored(&'a Arena, Id),
    Text(&'a str),
}
impl Json for FlatJson {
    type Node<'a> = View<'a>;
    type PreparedKey = String;
    type StringBuffer = String;
    const KEYS_PER_LOOKUP: usize = 1;
    fn prepare_key(key: &str) -> String {
        key.to_owned()
    }
    fn with_string_node<T>(buffer: &mut String, text: &str, f: impl FnOnce(View<'_>) -> T) -> T {
        buffer.clear();
        buffer.push_str(text);
        f(View::Text(buffer))
    }
}
pub struct Number<'a>(&'a str);
impl JsonNumber for Number<'_> {
    fn as_u64(&self) -> Option<u64> {
        self.0.parse().ok()
    }
    fn as_i64(&self) -> Option<i64> {
        self.0.parse().ok()
    }
    fn as_f64(&self) -> Option<f64> {
        self.0.parse::<f64>().ok().filter(|v| v.is_finite())
    }
    fn as_str(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.0)
    }
    fn to_number(&self) -> Cow<'_, serde_json::Number> {
        Cow::Owned(self.0.parse().unwrap())
    }
    fn is_integer(&self) -> bool {
        Decimal::parse(self.0).is_integer()
    }
}
fn cold_value(bytes: &[u8], _: u32) -> Value {
    // Iterative materialization for the internal dependency interface. SDK diagnostics
    // expose paths and advisory text, so deep values are not materialized or dropped here.
    let arena = Arena::parse(
        bytes,
        crate::raw::Limits {
            max_depth: usize::MAX,
            max_nodes: usize::MAX,
        },
    )
    .expect("previously admitted JSON");
    let mut values: Vec<Option<Value>> = (0..arena.nodes.len()).map(|_| None).collect();
    for id in (0..arena.nodes.len()).rev() {
        let value = match &arena.nodes[id].kind {
            Kind::Null => Value::Null,
            Kind::Bool(b) => Value::Bool(*b),
            Kind::Number => Value::Number(arena.raw(id).parse().expect("admitted number")),
            Kind::String { .. } => Value::String(
                arena
                    .string(id)
                    .expect("unsupported strings are declined before evaluation")
                    .to_owned(),
            ),
            Kind::Array(items) => Value::Array(
                items
                    .iter()
                    .map(|&id| {
                        values[id]
                            .take()
                            .expect("child precedes parent consumption")
                    })
                    .collect(),
            ),
            Kind::Object(members) => Value::Object(
                members
                    .iter()
                    .map(|m| {
                        (
                            arena.string(m.key).expect("admitted name").to_owned(),
                            values[m.value]
                                .take()
                                .expect("child precedes parent consumption"),
                        )
                    })
                    .collect(),
            ),
        };
        values[id] = Some(value);
    }
    values[0].take().expect("nonempty JSON")
}
impl<'a> Node<'a, FlatJson> for View<'a> {
    type Object = ObjectView<'a>;
    type Array = ArrayView<'a>;
    type Number = Number<'a>;
    fn as_object(&self) -> Option<ObjectView<'a>> {
        match self {
            Self::Stored(a, id) => match &a.nodes[*id].kind {
                Kind::Object(m) => Some(ObjectView(a, m)),
                _ => None,
            },
            _ => None,
        }
    }
    fn as_array(&self) -> Option<ArrayView<'a>> {
        match self {
            Self::Stored(a, id) => match &a.nodes[*id].kind {
                Kind::Array(m) => Some(ArrayView(a, m)),
                _ => None,
            },
            _ => None,
        }
    }
    fn as_string(&self) -> Option<Cow<'a, str>> {
        match self {
            Self::Text(s) => Some(Cow::Borrowed(s)),
            Self::Stored(a, id) => a.string(*id).map(Cow::Borrowed),
        }
    }
    fn as_number(&self) -> Option<Number<'a>> {
        match self {
            Self::Stored(a, id) if matches!(a.nodes[*id].kind, Kind::Number) => {
                Some(Number(a.raw(*id)))
            }
            _ => None,
        }
    }
    fn as_boolean(&self) -> Option<bool> {
        match self {
            Self::Stored(a, id) => match a.nodes[*id].kind {
                Kind::Bool(b) => Some(b),
                _ => None,
            },
            _ => None,
        }
    }
    fn is_null(&self) -> bool {
        matches!(self,Self::Stored(a,id) if matches!(a.nodes[*id].kind,Kind::Null))
    }
    fn json_type(&self) -> JsonType {
        match self {
            Self::Text(_) => JsonType::String,
            Self::Stored(a, id) => match a.nodes[*id].kind {
                Kind::Null => JsonType::Null,
                Kind::Bool(_) => JsonType::Boolean,
                Kind::Number => JsonType::Number,
                Kind::String { .. } => JsonType::String,
                Kind::Array(_) => JsonType::Array,
                Kind::Object(_) => JsonType::Object,
            },
        }
    }
    fn equals_value(&self, expected: &Value) -> bool {
        let mut stack = vec![(*self, expected)];
        while let Some((left, right)) = stack.pop() {
            if !jsonschema::ob_work::charge(1) {
                return false;
            }
            match right {
                Value::Null => {
                    if !left.is_null() {
                        return false;
                    }
                }
                Value::Bool(b) => {
                    if left.as_boolean() != Some(*b) {
                        return false;
                    }
                }
                Value::String(s) => {
                    if left.as_string().as_deref() != Some(s) {
                        return false;
                    }
                }
                Value::Number(n) => {
                    if left
                        .as_number()
                        .is_none_or(|m| Decimal::parse(m.0) != Decimal::parse(n.as_str()))
                    {
                        return false;
                    }
                }
                Value::Array(values) => {
                    let Some(items) = left.as_array() else {
                        return false;
                    };
                    if items.len() != values.len() {
                        return false;
                    }
                    stack.extend(items.elements().zip(values));
                }
                Value::Object(members) => {
                    let Some(items) = left.as_object() else {
                        return false;
                    };
                    if items.len() != members.len() {
                        return false;
                    }
                    for (k, v) in members {
                        let Some(item) = items.get(k) else {
                            return false;
                        };
                        stack.push((item, v));
                    }
                }
            }
        }
        true
    }
    fn to_value(&self) -> Cow<'a, Value> {
        Cow::Owned(match self {
            Self::Text(s) => Value::String((*s).to_owned()),
            Self::Stored(a, id) => cold_value(a.raw(*id).as_bytes(), 0),
        })
    }
    fn diagnostic_text(&self) -> String {
        match self {
            Self::Stored(a, id) => {
                let raw = a.raw(*id);
                if raw.len() <= 4096 {
                    raw.into()
                } else {
                    format!("<JSON value: {} bytes>", raw.len())
                }
            }
            Self::Text(s) => serde_json::to_string(s).expect("Rust string"),
        }
    }
    fn lazy_value(&self) -> LazyInstance<'a> {
        match self {
            Self::Text(s) => LazyInstance::Ready(Cow::Owned(Value::String((*s).to_owned()))),
            Self::Stored(a, id) => LazyInstance::Deferred {
                bytes: a.raw(*id).as_bytes(),
                tag: 0,
                make: cold_value,
                cell: OnceLock::new(),
            },
        }
    }
    fn identity(&self) -> Option<NodeIdentity> {
        match self {
            Self::Stored(a, id) => Some(NodeIdentity::tagged(
                std::ptr::from_ref(*a) as usize,
                *id as u32,
            )),
            Self::Text(_) => None,
        }
    }
}
pub struct ObjectView<'a>(&'a Arena, &'a [Member]);
pub struct Members<'a>(&'a Arena, std::slice::Iter<'a, Member>);
impl<'a> Iterator for Members<'a> {
    type Item = (&'a str, View<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        if !jsonschema::ob_work::charge(1) {
            return None;
        }
        self.1.next().map(|m| {
            (
                self.0
                    .string(m.key)
                    .expect("unpaired member interpretation is declined by caller"),
                View::Stored(self.0, m.value),
            )
        })
    }
}
impl<'a> Object<'a, FlatJson> for ObjectView<'a> {
    type Node = View<'a>;
    type MemberName = &'a str;
    type MembersIter = Members<'a>;
    fn len(&self) -> usize {
        self.1.len()
    }
    fn get(&self, key: &String) -> Option<View<'a>> {
        for member in self.1 {
            if !jsonschema::ob_work::charge(1) {
                return None;
            }
            if self.0.string(member.key) == Some(key) {
                return Some(View::Stored(self.0, member.value));
            }
        }
        None
    }
    fn members(&self) -> Members<'a> {
        Members(self.0, self.1.iter())
    }
}
pub struct ArrayView<'a>(&'a Arena, &'a [Id]);
pub struct Elements<'a>(&'a Arena, std::slice::Iter<'a, Id>);
impl<'a> Iterator for Elements<'a> {
    type Item = View<'a>;
    fn next(&mut self) -> Option<View<'a>> {
        if !jsonschema::ob_work::charge(1) {
            return None;
        }
        self.1.next().map(|id| View::Stored(self.0, *id))
    }
}
impl<'a> Array<'a, FlatJson> for ArrayView<'a> {
    type Node = View<'a>;
    type ElementsIter = Elements<'a>;
    fn len(&self) -> usize {
        self.1.len()
    }
    fn elements(&self) -> Elements<'a> {
        Elements(self.0, self.1.iter())
    }
    fn is_unique(&self) -> bool {
        use std::{
            collections::{HashMap, hash_map::RandomState},
            hash::BuildHasher,
        };
        let Some(&first) = self.1.first() else {
            return true;
        };
        let last = *self.1.last().unwrap();
        let end = self
            .0
            .nodes
            .partition_point(|n| n.span.start < self.0.nodes[last].span.end);
        let hasher = RandomState::new();
        let mut hashes = vec![0u64; end - first];
        for id in (first..end).rev() {
            if !jsonschema::ob_work::charge(1) {
                return false;
            }
            let hash = match &self.0.nodes[id].kind {
                Kind::Null => hasher.hash_one((0u8, 0u8)),
                Kind::Bool(b) => hasher.hash_one((1u8, b)),
                Kind::Number => hasher.hash_one((2u8, Decimal::parse(self.0.raw(id)))),
                Kind::String { .. } => hasher.hash_one((3u8, self.0.string(id))),
                Kind::Array(items) => hasher.hash_one((
                    4u8,
                    items.iter().map(|&i| hashes[i - first]).collect::<Vec<_>>(),
                )),
                Kind::Object(members) => {
                    let mut pairs: Vec<_> = members
                        .iter()
                        .map(|m| (hashes[m.key - first], hashes[m.value - first]))
                        .collect();
                    pairs.sort_unstable();
                    hasher.hash_one((5u8, pairs))
                }
            };
            hashes[id - first] = hash;
        }
        let mut seen: HashMap<u64, Vec<Id>> = HashMap::new();
        for &id in self.1 {
            let bucket = seen.entry(hashes[id - first]).or_default();
            for &prior in bucket.iter() {
                if !jsonschema::ob_work::charge(1) {
                    return false;
                }
                if equals(View::Stored(self.0, id), View::Stored(self.0, prior)) {
                    return false;
                }
            }
            bucket.push(id);
        }
        true
    }
}

/// Borrow an admitted exact value without parsing or allocating.
pub fn view(value: &crate::JsonValue) -> View<'_> {
    View::Stored(&value.owner, value.id)
}
/// Build only the JSON Pointer; source line/column scanning is unnecessary here.
pub fn pointer(value: &crate::JsonValue) -> Option<String> {
    value.owner.pointer(value.id)
}
pub fn has_unpaired(value: &crate::JsonValue) -> bool {
    let span = &value.owner.nodes[value.id].span;
    value.owner.unpaired.iter().any(|id| {
        let inner = &value.owner.nodes[*id].span;
        span.start <= inner.start && inner.end <= span.end
    })
}
/// Stack-safe exact comparison, charged to the enclosing evaluator work scope.
pub fn equals(left: View<'_>, right: View<'_>) -> bool {
    let mut stack = vec![(left, right)];
    while let Some((left, right)) = stack.pop() {
        if !jsonschema::ob_work::charge(1) {
            return false;
        }
        if left.json_type() != right.json_type() {
            return false;
        }
        if let Some(items) = left.as_array() {
            let other = right.as_array().unwrap();
            if items.len() != other.len() {
                return false;
            }
            stack.extend(items.elements().zip(other.elements()));
        } else if let Some(items) = left.as_object() {
            let other = right.as_object().unwrap();
            if items.len() != other.len() {
                return false;
            }
            for (k, v) in items.members() {
                let Some(r) = other.get(&k.to_owned()) else {
                    return false;
                };
                stack.push((v, r));
            }
        } else if let Some(n) = left.as_number() {
            if Decimal::parse(n.0) != Decimal::parse(right.as_number().unwrap().0) {
                return false;
            }
        } else if left.as_string() != right.as_string() || left.as_boolean() != right.as_boolean() {
            return false;
        }
    }
    true
}

pub fn node_id(value: &crate::JsonValue) -> usize {
    value.id
}
/// Rebase a retained child when it becomes an independent document resource.
pub fn standalone(value: crate::JsonValue) -> crate::JsonValue {
    if value.id == 0 {
        return value;
    }
    crate::JsonValue::parse_with_limits(
        value.bytes(),
        crate::JsonLimits {
            max_bytes: usize::MAX,
            max_depth: usize::MAX,
            max_nodes: u32::MAX as usize,
        },
    )
    .expect("an admitted value's source span is a complete JSON value")
}
pub fn duplicate_locations(value: &crate::JsonValue) -> Vec<crate::SourceLocation> {
    value
        .owner
        .duplicates
        .iter()
        .map(|(id, _)| {
            crate::JsonRef {
                owner: &value.owner,
                id: *id,
            }
            .location()
        })
        .collect()
}

pub fn depth(value: &crate::JsonValue) -> usize {
    let span = &value.owner.nodes[value.id].span;
    let end = value
        .owner
        .nodes
        .partition_point(|node| node.span.start < span.end);
    let mut depths = vec![0; end - value.id];
    let mut maximum = 0;
    for (offset, node) in value.owner.nodes[value.id..end].iter().enumerate() {
        let own = usize::from(matches!(node.kind, Kind::Array(_) | Kind::Object(_)));
        let parent = if offset == 0 {
            0
        } else {
            node.parent
                .map_or(0, |(parent, _)| depths[parent - value.id])
        };
        depths[offset] = parent + own;
        maximum = maximum.max(depths[offset]);
    }
    maximum
}

/// Logical live exact-storage owners, including process-lifetime fixed schemas.
pub fn live_arenas() -> usize {
    crate::raw::live_arenas()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uniqueness_is_exact_and_stack_safe() {
        let validator = jsonschema::options_for::<FlatJson>()
            .with_draft(jsonschema::Draft::Draft202012)
            .offline()
            .build(&serde_json::json!({"uniqueItems":true}))
            .unwrap();
        for (text, want) in [
            ("[1,1.0]", false),
            (r#"[{"a":1,"b":2},{"b":2.0,"a":1e0}]"#, false),
            ("[9007199254740992,9007199254740993]", true),
        ] {
            let value = crate::JsonValue::parse(text).unwrap();
            assert_eq!(validator.is_valid(view(&value)), want);
        }
        let deep = format!("{}0{}", "[".repeat(9999), "]".repeat(9999));
        let value = crate::JsonValue::parse(format!("[{deep},{deep}]")).unwrap();
        assert!(!validator.is_valid(view(&value)));
    }
    #[test]
    fn deep_unevaluated_diagnostics_do_not_materialize_values() {
        let validator = jsonschema::options_for::<FlatJson>()
            .with_draft(jsonschema::Draft::Draft202012)
            .offline()
            .build(&serde_json::json!({"unevaluatedItems":false}))
            .unwrap();
        let value = crate::JsonValue::parse(format!("{}0{}", "[".repeat(10000), "]".repeat(10000)))
            .unwrap();
        let errors: Vec<_> = validator.iter_errors(view(&value)).collect();
        assert!(!errors.is_empty());
        assert_eq!(errors[0].instance_path().as_str(), "");
    }
}
