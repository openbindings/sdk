//! Retained normative reading. Identity indexes never interpret unrelated rows.
use super::*;

pub(super) type NamespaceCache = OnceLock<Result<Option<NamespaceIndex>, InterpretationError>>;

#[derive(Debug)]
pub(super) struct NamespaceIndex {
    entries: BTreeMap<Arc<str>, JsonValue>,
}
impl NamespaceIndex {
    fn build(
        parent: JsonRef<'_>,
        field: &str,
        code: &'static str,
    ) -> Result<Option<Self>, InterpretationError> {
        let Some(value) = parent.get(field) else {
            return Ok(None);
        };
        let members = value
            .members()
            .ok_or_else(|| InterpretationError::invalid(code, value))?;
        let mut entries = BTreeMap::new();
        for member in members {
            let key = member
                .name
                .as_str()
                .ok_or(InterpretationError::UnpairedString)?;
            if entries
                .insert(Arc::from(key), member.value.to_owned())
                .is_some()
            {
                return Err(InterpretationError::DuplicateMembers);
            }
        }
        Ok(Some(Self { entries }))
    }
}

fn namespace<'a>(
    cache: &'a NamespaceCache,
    parent: JsonRef<'_>,
    field: &str,
    code: &'static str,
) -> Result<Option<&'a NamespaceIndex>, InterpretationError> {
    cache
        .get_or_init(|| NamespaceIndex::build(parent, field, code))
        .as_ref()
        .map(Option::as_ref)
        .map_err(Clone::clone)
}

fn object(value: &JsonValue, code: &'static str) -> Result<(), InterpretationError> {
    if value.kind() != JsonKind::Object {
        return Err(InterpretationError::invalid(code, value.view()));
    }
    Ok(())
}
fn string<'a>(
    value: &'a JsonValue,
    field: &str,
    code: &'static str,
) -> Result<Option<&'a str>, InterpretationError> {
    value
        .get(field)
        .map(|v| {
            v.as_str()
                .ok_or_else(|| InterpretationError::invalid(code, v))
        })
        .transpose()
}
fn required_string<'a>(
    value: &'a JsonValue,
    field: &str,
    missing: &'static str,
    invalid: &'static str,
) -> Result<&'a str, InterpretationError> {
    string(value, field, invalid)?
        .ok_or_else(|| InterpretationError::invalid(missing, value.view()))
}
fn boolean(
    value: &JsonValue,
    field: &str,
    code: &'static str,
) -> Result<Option<bool>, InterpretationError> {
    value
        .get(field)
        .map(|v| {
            v.as_bool()
                .ok_or_else(|| InterpretationError::invalid(code, v))
        })
        .transpose()
}
fn strings<'a>(
    value: &'a JsonValue,
    field: &str,
    array_code: &'static str,
    item_code: &'static str,
) -> Result<Option<impl ExactSizeIterator<Item = &'a str>>, InterpretationError> {
    value
        .get(field)
        .map(|value| {
            let items = value
                .elements()
                .ok_or_else(|| InterpretationError::invalid(array_code, value))?;
            for item in value.elements().expect("checked array") {
                if item.as_str().is_none() {
                    return Err(InterpretationError::invalid(item_code, item));
                }
            }
            Ok(items.map(|item| item.as_str().expect("checked string list")))
        })
        .transpose()
}

macro_rules! view {
    ($name:ident, $description:literal, $object_code:literal, $description_code:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug)]
        pub struct $name {
            key: Arc<str>,
            value: JsonValue,
        }
        impl $name {
            fn new(key: &Arc<str>, value: &JsonValue) -> Result<Self, InterpretationError> {
                object(value, $object_code)?;
                Ok(Self {
                    key: key.clone(),
                    value: value.clone(),
                })
            }
            /// Borrow this declaration's primary map key.
            pub fn key(&self) -> &str {
                &self.key
            }
            /// Borrow the original exact object. This does not establish conformance.
            pub fn value(&self) -> JsonRef<'_> {
                self.value.view()
            }
            /// Interpret only this optional description, retaining original error coordinates.
            pub fn description(&self) -> Result<Option<&str>, InterpretationError> {
                string(&self.value, "description", $description_code)
            }
        }
    };
}
view!(
    BindingView,
    "Immutable retained binding declaration. Clones share source storage; metadata access does not select or invoke a binding.",
    "invalid-binding-object",
    "invalid-binding-description"
);
view!(
    SourceView,
    "Immutable retained source declaration. Metadata does not establish availability or kind support. The view survives document drop.",
    "invalid-source-object",
    "invalid-source-description"
);
view!(
    DependencyView,
    "Immutable retained dependency declaration. Inspection preserves the declared filter without selecting a provider.",
    "invalid-dependency-object",
    "invalid-dependency-description"
);
view!(
    ExampleView,
    "Immutable retained named example. Exact values borrow this owner; use to_owned to retain one beyond it. No example truth is implied.",
    "invalid-example-object",
    "invalid-example-description"
);

macro_rules! document_namespace {
    ($many:ident, $one:ident, $view:ident, $field:literal, $code:literal) => {
        impl ParsedDocument {
            #[doc = concat!("Allocate retained ", $field, " views in lexical key order. None means an absent namespace; an empty vector means a present empty object. Checks identity/object shapes, not metadata fields; each accessor reports its own field errors. Global interpretation prerequisites still apply.")]
            pub fn $many(&self) -> Result<Option<Vec<$view>>, InterpretationError> {
                self.interpretable()?;
                namespace(&self.inner.$many, self.value().view(), $field, $code)?
                    .map(|index| index.entries.iter().map(|(key, value)| $view::new(key, value)).collect())
                    .transpose()
            }
            #[doc = concat!("Retain one ", $field, " declaration by exact key. Missing namespace/key returns None. Checks the selected object's shape without interpreting unrelated entries. Indexed identities are shared across document clones; acquisition is not conformance proof.")]
            pub fn $one(&self, key: &str) -> Result<Option<$view>, InterpretationError> {
                self.interpretable()?;
                let index = namespace(&self.inner.$many, self.value().view(), $field, $code)?;
                index.and_then(|index| index.entries.get_key_value(key))
                    .map(|(key, value)| $view::new(key, value)).transpose()
            }
        }
    };
}
document_namespace!(
    bindings,
    binding,
    BindingView,
    "bindings",
    "invalid-bindings-object"
);
document_namespace!(
    sources,
    source,
    SourceView,
    "sources",
    "invalid-sources-object"
);
document_namespace!(
    dependencies,
    dependency,
    DependencyView,
    "dependencies",
    "invalid-dependencies-object"
);

impl BindingView {
    /// Read the required operation name; its target need not exist for inspection.
    pub fn operation(&self) -> Result<&str, InterpretationError> {
        required_string(
            &self.value,
            "operation",
            "missing-binding-operation",
            "invalid-binding-operation",
        )
    }
    /// Read the required source key without resolving or acquiring it.
    pub fn source(&self) -> Result<&str, InterpretationError> {
        required_string(
            &self.value,
            "source",
            "missing-binding-source",
            "invalid-binding-source",
        )
    }
    /// Read the optional exact interoperable integer; absence is not a default preference.
    pub fn preference(&self) -> Result<Option<Preference>, InterpretationError> {
        self.value
            .get("preference")
            .map(|value| {
                Preference::from_json_value(value)
                    .map_err(|_| InterpretationError::invalid("invalid-binding-preference", value))
            })
            .transpose()
    }
    /// Read the optional idempotence annotation, preserving absence versus false.
    pub fn idempotent(&self) -> Result<Option<bool>, InterpretationError> {
        boolean(&self.value, "idempotent", "invalid-binding-idempotent")
    }
    /// Read the optional deprecation annotation, preserving absence versus false.
    pub fn deprecated(&self) -> Result<Option<bool>, InterpretationError> {
        boolean(&self.value, "deprecated", "invalid-binding-deprecated")
    }
    /// Borrow opaque binding content; absence differs from present JSON null.
    pub fn content(&self) -> Option<JsonRef<'_>> {
        self.value.get("content")
    }
}
impl SourceView {
    /// Read the required declared kind without interpreting or executing it.
    pub fn kind(&self) -> Result<&str, InterpretationError> {
        required_string(
            &self.value,
            "kind",
            "missing-source-kind",
            "invalid-source-kind",
        )
    }
    /// Borrow opaque source content; absence differs from present JSON null.
    pub fn content(&self) -> Option<JsonRef<'_>> {
        self.value.get("content")
    }
}
impl DependencyView {
    /// Read the required operation name without resolving its target.
    pub fn operation(&self) -> Result<&str, InterpretationError> {
        required_string(
            &self.value,
            "operation",
            "missing-dependency-operation",
            "invalid-dependency-operation",
        )
    }
    /// Read all declared kinds after checking every item. None accepts every kind;
    /// an empty iterator matches none but is nonconformant: a present list must be
    /// nonempty. This does not establish conformance or select a provider.
    pub fn kinds(
        &self,
    ) -> Result<Option<impl ExactSizeIterator<Item = &str>>, InterpretationError> {
        strings(
            &self.value,
            "kinds",
            "invalid-dependency-kinds",
            "invalid-dependency-kind",
        )
    }
}
impl ExampleView {
    /// Borrow the supplied input instance, preserving absence versus JSON null.
    pub fn input(&self) -> Option<JsonRef<'_>> {
        self.value.get("input")
    }
    /// Borrow the supplied output instance, preserving absence versus JSON null.
    pub fn output(&self) -> Option<JsonRef<'_>> {
        self.value.get("output")
    }
}
impl OperationView {
    /// Read optional tags after checking every item. Absence and an empty array differ.
    pub fn tags(&self) -> Result<Option<impl ExactSizeIterator<Item = &str>>, InterpretationError> {
        strings(
            &self.value,
            "tags",
            "invalid-operation-tags",
            "invalid-operation-tag",
        )
    }
    /// Read the optional operation deprecation annotation, preserving absence versus false.
    pub fn deprecated(&self) -> Result<Option<bool>, InterpretationError> {
        boolean(&self.value, "deprecated", "invalid-operation-deprecated")
    }
    /// Allocate retained examples in lexical key order. None means absence, while
    /// an empty vector means a present empty object. Checks example object shapes;
    /// description interpretation remains local to its accessor.
    pub fn examples(&self) -> Result<Option<Vec<ExampleView>>, InterpretationError> {
        namespace(
            &self.examples,
            self.value(),
            "examples",
            "invalid-examples-object",
        )?
        .map(|index| {
            index
                .entries
                .iter()
                .map(|(key, value)| ExampleView::new(key, value))
                .collect()
        })
        .transpose()
    }
    /// Retain an example by exact key without interpreting unrelated entries.
    /// Its identity index is shared across operation views and their clones.
    pub fn example(&self, key: &str) -> Result<Option<ExampleView>, InterpretationError> {
        namespace(
            &self.examples,
            self.value(),
            "examples",
            "invalid-examples-object",
        )?
        .and_then(|index| index.entries.get_key_value(key))
        .map(|(key, value)| ExampleView::new(key, value))
        .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexes_and_keys_are_reused_across_retained_views() {
        let doc = ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{"run":{"examples":{"e":{}}}},"sources":{"s":{"kind":"k"}}}"#).unwrap();
        assert!(doc.inner.sources.get().is_none());
        let first = doc.source("s").unwrap().unwrap();
        let clone = doc.clone();
        let second = clone.source("s").unwrap().unwrap();
        assert!(Arc::ptr_eq(&first.key, &second.key));
        assert!(std::ptr::eq(
            doc.inner.sources.get().unwrap(),
            clone.inner.sources.get().unwrap()
        ));
        let first_op = doc.operations().unwrap().remove(0);
        let second_op = doc.operations().unwrap().remove(0);
        assert!(Arc::ptr_eq(&first_op.examples, &second_op.examples));
        assert!(first_op.examples.get().is_none());
        let first_example = first_op.example("e").unwrap().unwrap();
        let second_example = second_op.example("e").unwrap().unwrap();
        assert!(Arc::ptr_eq(&first_example.key, &second_example.key));
    }
}
