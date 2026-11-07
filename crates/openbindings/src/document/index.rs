//! Lookup storage derived from a conformant immutable document. The indexes own
//! exact entry values, never another independently mutable document model.
use super::*;

pub(super) type Namespace = BTreeMap<Arc<str>, JsonValue>;
pub(super) type NamespaceCache = OnceLock<Option<Namespace>>;

pub(super) fn namespace(parent: JsonRef<'_>, field: &str) -> Option<Namespace> {
    parent.get(field).map(|value| {
        value
            .members()
            .expect("conformant namespace")
            .map(|member| {
                (
                    Arc::from(member.name.as_str().expect("conformant scalar key")),
                    member.value.to_owned(),
                )
            })
            .collect()
    })
}

pub(super) struct OperationEntry {
    pub value: JsonValue,
    pub examples: NamespaceCache,
}
pub(super) struct Operations {
    pub entries: BTreeMap<Arc<str>, OperationEntry>,
    pub names: BTreeMap<Arc<str>, Arc<str>>,
}
impl Operations {
    pub fn new(value: JsonRef<'_>) -> Self {
        let mut names = BTreeMap::new();
        let entries = namespace(value, "operations")
            .expect("conformant operations")
            .into_iter()
            .map(|(key, value)| {
                let previous = names.insert(key.clone(), key.clone());
                debug_assert!(
                    previous.is_none(),
                    "conformance establishes one name namespace"
                );
                if let Some(aliases) = value.get("aliases") {
                    for alias in aliases.elements().expect("conformant aliases") {
                        let previous = names.insert(
                            Arc::from(alias.as_str().expect("conformant alias")),
                            key.clone(),
                        );
                        debug_assert!(previous.is_none(), "conformance establishes unique aliases");
                    }
                }
                (
                    key,
                    OperationEntry {
                        value,
                        examples: OnceLock::new(),
                    },
                )
            })
            .collect();
        Self { entries, names }
    }
}

pub(super) struct Bindings {
    pub entries: Option<Namespace>,
    pub operations: BTreeMap<Arc<str>, Vec<Arc<str>>>,
}
impl Bindings {
    pub fn new(value: JsonRef<'_>) -> Self {
        let entries = namespace(value, "bindings");
        let mut operations: BTreeMap<Arc<str>, Vec<Arc<str>>> = BTreeMap::new();
        if let Some(entries) = &entries {
            for (key, value) in entries {
                let operation = value
                    .get("operation")
                    .and_then(JsonRef::as_str)
                    .expect("conformant operation key");
                if let Some(keys) = operations.get_mut(operation) {
                    keys.push(key.clone());
                } else {
                    operations.insert(Arc::from(operation), vec![key.clone()]);
                }
            }
        }
        Self {
            entries,
            operations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_share_one_primary_allocation() {
        let primary = "p".repeat(4096);
        for count in [0, 1, 64, 256] {
            let aliases: Vec<_> = (0..count).map(|i| format!("a{i}")).collect();
            let document = Document::parse(serde_json::json!({"openbindings":"0.2.0","operations":{&primary:{"aliases":aliases}}}).to_string()).unwrap();
            let proof = document.validate().unwrap().conformant().unwrap();
            let selected = aliases.last().map(String::as_str).unwrap_or(&primary);
            assert_eq!(proof.resolve_operation(selected).unwrap().key(), primary);
            let index = document.inner.operations.get().unwrap();
            let key = index.entries.keys().next().unwrap();
            assert_eq!(index.entries.len(), 1);
            assert_eq!(index.names.len(), count + 1);
            assert!(index.names.values().all(|value| Arc::ptr_eq(key, value)));
            assert!(Arc::ptr_eq(
                key,
                index.names.get_key_value(primary.as_str()).unwrap().0
            ));
        }
    }
    #[test]
    fn cached_namespaces_and_retained_entries_have_no_owner_cycle() {
        let doc = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"examples":{"e":{"input":null}}}},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"op","source":"s"}},"dependencies":{"d":{"operation":"op"}}}"#).unwrap();
        let weak = Arc::downgrade(&doc.inner);
        let proof = doc.validate().unwrap().conformant().unwrap();
        let op = proof.operation("op").unwrap();
        let example = op.example("e").unwrap();
        let binding = op.bindings().remove(0);
        let source = binding.source();
        let dependency = proof.dependency("d").unwrap();
        drop((doc, proof, op, example, binding, source));
        assert!(weak.upgrade().is_some());
        assert_eq!(dependency.operation().key(), "op");
        drop(dependency);
        assert!(weak.upgrade().is_none());
    }
}
