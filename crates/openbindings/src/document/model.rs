//! Canonical document concepts, owned by the immutable proof that establishes
//! their fields and relations. Raw JSON and authoring do not need parallel types.
use super::index::{Bindings, Namespace, Operations, namespace};
use super::*;
use crate::schema::Side;

#[derive(Clone)]
struct Entry {
    document: ConformantDocument,
    key: Arc<str>,
    value: JsonValue,
}
impl Entry {
    fn new(document: &ConformantDocument, key: &Arc<str>, value: &JsonValue) -> Self {
        Self {
            document: document.clone(),
            key: key.clone(),
            value: value.clone(),
        }
    }
}
fn text<'a>(value: JsonRef<'a>, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .map(|value| value.as_str().expect("conformant scalar text"))
}
fn texts<'a>(value: JsonRef<'a>, field: &str) -> Option<impl ExactSizeIterator<Item = &'a str>> {
    value.get(field).map(|value| {
        value
            .elements()
            .expect("conformant string array")
            .map(|value| value.as_str().expect("conformant scalar text"))
    })
}
fn flag(value: JsonRef<'_>, field: &str) -> Option<bool> {
    value
        .get(field)
        .map(|value| value.as_bool().expect("conformant boolean"))
}
macro_rules! entry {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone)]
        pub struct $name {
            entry: Entry,
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }
        impl $name {
            /// The exact decoded key in this entry's namespace.
            pub fn key(&self) -> &str {
                &self.entry.key
            }
            /// The immutable document proof retained by this entry.
            pub fn document(&self) -> &ConformantDocument {
                &self.entry.document
            }
            /// Borrow the original declaration without evaluating schemas or examples.
            pub fn value(&self) -> JsonRef<'_> {
                self.entry.value.view()
            }
            /// Optional human-facing description; absence is distinct from empty text.
            pub fn description(&self) -> Option<&str> {
                text(self.value(), "description")
            }
        }
    };
}
entry!(
    Operation,
    "An operation declared by a conformant document. Clones retain the original proof and entry after the caller drops its document handle."
);
entry!(
    Source,
    "A named source declared by a conformant document. Its kind defines the interpretation of its opaque data; this view makes no provider-support claim."
);
entry!(
    Binding,
    "A binding declared by a conformant document, relating one operation key to one source key in the same snapshot. Its realization claim is not a behavioral proof."
);
entry!(
    Dependency,
    "A dependency declared by a conformant document. Its optional kind filter compares exact names; it is not an unresolved binding."
);
entry!(
    Example,
    "A named example within one operation. Its exact original occurrence and document proof are retained; reading it does not validate the example's truth."
);

impl ConformantDocument {
    fn operations_index(&self) -> &Operations {
        self.document
            .inner
            .operations
            .get_or_init(|| Operations::new(self.value()))
    }
    fn bindings_index(&self) -> &Bindings {
        self.document
            .inner
            .bindings
            .get_or_init(|| Bindings::new(self.value()))
    }
    fn bindings_namespace(&self) -> Option<&Namespace> {
        self.bindings_index().entries.as_ref()
    }
    fn sources_index(&self) -> Option<&Namespace> {
        self.document
            .inner
            .sources
            .get_or_init(|| namespace(self.value(), "sources"))
            .as_ref()
    }
    fn dependencies_index(&self) -> Option<&Namespace> {
        self.document
            .inner
            .dependencies
            .get_or_init(|| namespace(self.value(), "dependencies"))
            .as_ref()
    }
    /// Borrow the exact original document value certified by this proof.
    pub fn value(&self) -> JsonRef<'_> {
        self.document.value().view()
    }
    /// The specification version declaration, distinct from the interface's version label.
    pub fn openbindings(&self) -> &str {
        text(self.value(), "openbindings").expect("conformant version")
    }
    /// Optional human-facing interface name; it is not a portable resource identity.
    pub fn name(&self) -> Option<&str> {
        text(self.value(), "name")
    }
    /// Optional interface version label, distinct from the specification version.
    pub fn version(&self) -> Option<&str> {
        text(self.value(), "version")
    }
    /// Optional human-facing interface description.
    pub fn description(&self) -> Option<&str> {
        text(self.value(), "description")
    }
    /// Retain all operations in lexical primary-key order.
    pub fn operations(&self) -> Vec<Operation> {
        self.operations_index()
            .entries
            .iter()
            .map(|(key, entry)| Operation {
                entry: Entry::new(self, key, &entry.value),
            })
            .collect()
    }
    /// Select by exact primary key. An alias is not a key for this lookup.
    pub fn operation(&self, key: &str) -> Option<Operation> {
        self.operations_index()
            .entries
            .get_key_value(key)
            .map(|(key, entry)| Operation {
                entry: Entry::new(self, key, &entry.value),
            })
    }
    /// Select by the shared operation-key/alias namespace established by conformance.
    pub fn resolve_operation(&self, name: &str) -> Option<Operation> {
        let key = self.operations_index().names.get(name)?;
        self.operation(key)
    }
    /// Retain named schemas in lexical key order. None means the namespace is absent.
    /// A present empty map is Some(empty); no schema evaluation occurs.
    pub fn schemas(&self) -> Option<Vec<(String, Schema)>> {
        namespace(self.value(), "schemas").map(|entries| {
            entries
                .into_iter()
                .map(|(key, value)| (key.to_string(), Schema::contained(self, value)))
                .collect()
        })
    }
    /// Select one named schema by its exact primary key, without evaluating it.
    pub fn schema(&self, key: &str) -> Option<Schema> {
        self.value()
            .get("schemas")?
            .get(key)
            .map(|value| Schema::contained(self, value.to_owned()))
    }
}
macro_rules! namespace_access {
    ($many:ident, $one:ident, $ty:ident, $index:expr) => {
        impl ConformantDocument {
            #[doc = concat!("Retain ", stringify!($many), " in lexical key order. None means absent; Some(empty) means present-empty.")]
            pub fn $many(&self) -> Option<Vec<$ty>> {
                $index(self).map(|entries| entries.iter().map(|(key, value)| $ty { entry: Entry::new(self, key, value) }).collect())
            }
            #[doc = concat!("Retain one ", stringify!($one), " by its exact primary key; None means absent.")]
            pub fn $one(&self, key: &str) -> Option<$ty> {
                $index(self)?.get_key_value(key).map(|(key, value)| $ty { entry: Entry::new(self, key, value) })
            }
        }
    };
}
namespace_access!(sources, source, Source, Self::sources_index);
namespace_access!(
    dependencies,
    dependency,
    Dependency,
    Self::dependencies_index
);
namespace_access!(bindings, binding, Binding, Self::bindings_namespace);

impl Operation {
    /// Optional deprecation annotation; absence is distinct from false.
    pub fn deprecated(&self) -> Option<bool> {
        flag(self.value(), "deprecated")
    }
    /// Optional descriptive tags, retaining their authored order and presence.
    pub fn tags(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        texts(self.value(), "tags")
    }
    /// Optional alternate names in the document's shared operation namespace.
    pub fn aliases(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        texts(self.value(), "aliases")
    }
    /// The optional input schema in its retained original document context.
    pub fn input(&self) -> Option<Schema> {
        Schema::declared(self, Side::Input)
    }
    /// The optional output schema in its retained original document context.
    pub fn output(&self) -> Option<Schema> {
        Schema::declared(self, Side::Output)
    }
    pub(crate) fn field(&self, side: Side) -> Option<JsonRef<'_>> {
        self.value().get(match side {
            Side::Input => "input",
            Side::Output => "output",
        })
    }
    /// Retain the bindings for this exact operation key, without ranking or selection.
    pub fn bindings(&self) -> Vec<Binding> {
        let index = self.document().bindings_index();
        index
            .operations
            .get(self.key())
            .into_iter()
            .flatten()
            .map(|key| {
                self.document()
                    .binding(key)
                    .expect("conformant binding index")
            })
            .collect()
    }
    fn examples_index(&self) -> Option<&Namespace> {
        self.document()
            .operations_index()
            .entries
            .get(self.key())
            .expect("retained operation")
            .examples
            .get_or_init(|| namespace(self.value(), "examples"))
            .as_ref()
    }
    /// Retain named examples in lexical key order. Absence is distinct from a present empty map.
    pub fn examples(&self) -> Option<Vec<Example>> {
        self.examples_index().map(|entries| {
            entries
                .iter()
                .map(|(key, value)| Example {
                    entry: Entry::new(self.document(), key, value),
                })
                .collect()
        })
    }
    /// Select an example in this operation's local namespace.
    pub fn example(&self, key: &str) -> Option<Example> {
        self.examples_index()?
            .get_key_value(key)
            .map(|(key, value)| Example {
                entry: Entry::new(self.document(), key, value),
            })
    }
}
impl Binding {
    /// The declared primary operation key, never an alias.
    pub fn operation_key(&self) -> &str {
        text(self.value(), "operation").expect("conformant operation key")
    }
    /// Retain the referenced operation in the same document snapshot.
    pub fn operation(&self) -> Operation {
        self.document()
            .operation(self.operation_key())
            .expect("conformant operation relation")
    }
    /// The declared primary source key.
    pub fn source_key(&self) -> &str {
        text(self.value(), "source").expect("conformant source key")
    }
    /// Retain the referenced source in the same document snapshot.
    pub fn source(&self) -> Source {
        self.document()
            .source(self.source_key())
            .expect("conformant source relation")
    }
    /// Optional exact interoperable integer. Absence does not assign a default rank.
    pub fn preference(&self) -> Option<i64> {
        // Conformance proves an integer in ±(2^53-1), so binary64 conversion is
        // exact even when the original token uses a decimal point or exponent.
        self.value().get("preference").map(|value| {
            value
                .number_text()
                .expect("conformant number")
                .parse::<f64>()
                .expect("conformant number token") as i64
        })
    }
    /// Optional idempotence claim; absence is distinct from false.
    pub fn idempotent(&self) -> Option<bool> {
        flag(self.value(), "idempotent")
    }
    /// Optional deprecation annotation; absence is distinct from false.
    pub fn deprecated(&self) -> Option<bool> {
        flag(self.value(), "deprecated")
    }
    /// Retain opaque binding data, preserving explicit null separately from absence.
    pub fn data(&self) -> Option<JsonValue> {
        self.value().get("data").map(JsonRef::to_owned)
    }
}
impl Source {
    /// The exact decoded kind name, without asserting compatibility or provider support.
    pub fn kind(&self) -> &str {
        text(self.value(), "kind").expect("conformant kind")
    }
    /// Retain opaque source data, preserving explicit null separately from absence.
    pub fn data(&self) -> Option<JsonValue> {
        self.value().get("data").map(JsonRef::to_owned)
    }
}
impl Dependency {
    /// The declared primary operation key, never an alias.
    pub fn operation_key(&self) -> &str {
        text(self.value(), "operation").expect("conformant operation key")
    }
    /// Retain the referenced operation in the same document snapshot.
    pub fn operation(&self) -> Operation {
        self.document()
            .operation(self.operation_key())
            .expect("conformant operation relation")
    }
    /// Optional exact kind names. Absence accepts every kind; a present list is nonempty.
    pub fn kinds(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        texts(self.value(), "kinds")
    }
    /// Compare exact kind names without assuming compatibility or retrieval.
    pub fn accepts_kind(&self, kind: &str) -> bool {
        self.kinds()
            .is_none_or(|mut names| names.any(|name| name == kind))
    }
}
impl Example {
    /// Retain the example's input value; absence differs from explicit null.
    pub fn input(&self) -> Option<JsonValue> {
        self.value().get("input").map(JsonRef::to_owned)
    }
    /// Retain the example's output value; absence differs from explicit null.
    pub fn output(&self) -> Option<JsonValue> {
        self.value().get("output").map(JsonRef::to_owned)
    }
}
