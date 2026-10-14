//! Reads whose field shapes are established by the retained document proof.
use super::*;

fn text<'a>(value: JsonRef<'a>, field: &str) -> Option<JsonStringRef<'a>> {
    value
        .get(field)
        .map(|v| v.as_string().expect("validated string"))
}
fn texts<'a>(
    value: JsonRef<'a>,
    field: &str,
) -> Option<impl ExactSizeIterator<Item = JsonStringRef<'a>>> {
    value.get(field).map(|v| {
        v.elements()
            .expect("validated array")
            .map(|v| v.as_string().expect("validated string"))
    })
}
macro_rules! validated_view {
    ($name:ident, $parsed:ident) => {
        #[doc = concat!("Retained ", stringify!($parsed), " with document conformance proof. Field reads are infallible; exact strings preserve unpaired UTF-16. Clones and views survive parent drop.")]
        #[derive(Clone, Debug)]
        pub struct $name {
            _document: ValidatedDocument,
            view: $parsed,
        }
        impl $name {
            /// Borrow the declaration's primary key.
            pub fn key(&self) -> &str { self.view.key() }
            /// Borrow the exact original object without evaluating its schemas or examples.
            pub fn value(&self) -> JsonRef<'_> { self.view.value() }
            /// Read optional exact descriptive text. None means absence.
            pub fn description(&self) -> Option<JsonStringRef<'_>> { text(self.value(), "description") }
        }
    };
}
validated_view!(ValidatedOperationView, OperationView);
validated_view!(ValidatedBindingView, BindingView);
validated_view!(ValidatedSourceView, SourceView);
validated_view!(ValidatedDependencyView, DependencyView);
validated_view!(ValidatedExampleView, ExampleView);

impl ValidatedDocument {
    /// Borrow the exact snapshot certified by this proof.
    pub fn value(&self) -> JsonRef<'_> {
        self.document.value().view()
    }
    /// Read the supported scalar SemVer declaration established by OBI-03.
    pub fn openbindings(&self) -> &str {
        self.value()
            .get("openbindings")
            .and_then(JsonRef::as_str)
            .expect("validated declaration")
    }
    /// Read the optional exact interface name.
    pub fn name(&self) -> Option<JsonStringRef<'_>> {
        text(self.value(), "name")
    }
    /// Read the optional exact interface version label, distinct from openbindings.
    pub fn version(&self) -> Option<JsonStringRef<'_>> {
        text(self.value(), "version")
    }
    /// Read optional exact descriptive text.
    pub fn description(&self) -> Option<JsonStringRef<'_>> {
        text(self.value(), "description")
    }
    /// Borrow optional schema declarations in lexical order; no evaluation occurs.
    /// None means absent; Some(empty) means a present empty namespace.
    pub fn schemas(&self) -> Option<Vec<(&str, JsonRef<'_>)>> {
        self.document.schemas().expect("validated schema namespace")
    }
    /// Borrow one exact object or boolean schema by primary key.
    pub fn schema(&self, name: &str) -> Option<JsonRef<'_>> {
        self.value().get("schemas").and_then(|v| v.get(name))
    }
    /// Retain all operations in lexical primary-key order, including an empty namespace.
    pub fn operations(&self) -> Vec<ValidatedOperationView> {
        self.document
            .operations()
            .expect("validated operations")
            .into_iter()
            .map(|view| ValidatedOperationView {
                _document: self.clone(),
                view,
            })
            .collect()
    }
    /// Select a primary key or alias. OBI-05 excludes ambiguity; None means missing.
    pub fn resolve_operation(&self, name: &str) -> Option<ValidatedOperationView> {
        match self
            .document
            .resolve_operation(name)
            .expect("validated operation namespace")
        {
            OperationSelection::Found(view) => Some(ValidatedOperationView {
                _document: self.clone(),
                view,
            }),
            OperationSelection::Missing => None,
            OperationSelection::Ambiguous { .. } => unreachable!("OBI-05 proves uniqueness"),
        }
    }
    /// Read binding keys in lexical presentation order, without ranking or selection.
    pub fn operation_bindings(&self, key: &str) -> Vec<String> {
        self.document
            .operation_bindings(key)
            .expect("validated bindings")
    }
    /// Return None for an absent dependency; otherwise compare its exact kind filter.
    pub fn dependency_accepts_kind(&self, key: &str, kind: &str) -> Option<bool> {
        self.dependency(key).map(|v| {
            v.kinds()
                .is_none_or(|mut kinds| kinds.any(|v| v.eq_str(kind)))
        })
    }
}
macro_rules! namespace {
    ($many:ident, $one:ident, $view:ident) => {
        impl ValidatedDocument {
            #[doc = concat!("Retain ", stringify!($many), " in lexical key order. None means absent; Some(empty) means present-empty.")]
            pub fn $many(&self) -> Option<Vec<$view>> {
                self.document.$many().expect("validated namespace").map(|rows| rows.into_iter().map(|view| $view { _document: self.clone(), view }).collect())
            }
            #[doc = concat!("Retain one ", stringify!($one), " by exact primary key; None means absent.")]
            pub fn $one(&self, key: &str) -> Option<$view> {
                self.document.$one(key).expect("validated namespace").map(|view| $view { _document: self.clone(), view })
            }
        }
    };
}
namespace!(bindings, binding, ValidatedBindingView);
namespace!(sources, source, ValidatedSourceView);
namespace!(dependencies, dependency, ValidatedDependencyView);

impl ValidatedOperationView {
    /// Read optional tags exactly, preserving absent versus present-empty.
    pub fn tags(&self) -> Option<impl ExactSizeIterator<Item = JsonStringRef<'_>>> {
        texts(self.value(), "tags")
    }
    /// Read optional scalar aliases, whose identifier spelling is proved by OBI-04.
    pub fn aliases(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        texts(self.value(), "aliases").map(|v| v.map(|s| s.as_str().expect("validated alias")))
    }
    /// Read optional deprecation; absence is distinct from false.
    pub fn deprecated(&self) -> Option<bool> {
        self.value()
            .get("deprecated")
            .map(|v| v.as_bool().expect("validated boolean"))
    }
    /// Borrow the exact input schema. Absence, true, false and object remain distinct.
    pub fn input(&self) -> Option<JsonRef<'_>> {
        self.view.input()
    }
    /// Borrow the exact output schema without preparing it.
    pub fn output(&self) -> Option<JsonRef<'_>> {
        self.view.output()
    }
    /// Read binding keys without selecting or ranking them.
    pub fn bindings(&self) -> Vec<String> {
        self._document.operation_bindings(self.key())
    }
    /// Retain named examples in lexical order; no example truth is implied.
    pub fn examples(&self) -> Option<Vec<ValidatedExampleView>> {
        self.view
            .examples()
            .expect("validated examples")
            .map(|rows| {
                rows.into_iter()
                    .map(|view| ValidatedExampleView {
                        _document: self._document.clone(),
                        view,
                    })
                    .collect()
            })
    }
    /// Retain one named example; None means absent.
    pub fn example(&self, key: &str) -> Option<ValidatedExampleView> {
        self.view
            .example(key)
            .expect("validated example")
            .map(|view| ValidatedExampleView {
                _document: self._document.clone(),
                view,
            })
    }
}
impl ValidatedBindingView {
    /// Read the primary operation key proved by OBI-06.
    pub fn operation(&self) -> &str {
        self.value()
            .get("operation")
            .and_then(JsonRef::as_str)
            .expect("validated operation key")
    }
    /// Read the source key proved by OBI-07.
    pub fn source(&self) -> &str {
        self.value()
            .get("source")
            .and_then(JsonRef::as_str)
            .expect("validated source key")
    }
    /// Read the optional interoperable integer without assigning a default rank.
    pub fn preference(&self) -> Option<Preference> {
        self.view.preference().expect("validated preference")
    }
    /// Read optional idempotence; absence is distinct from false.
    pub fn idempotent(&self) -> Option<bool> {
        self.value()
            .get("idempotent")
            .map(|v| v.as_bool().expect("validated boolean"))
    }
    /// Read optional deprecation; absence is distinct from false.
    pub fn deprecated(&self) -> Option<bool> {
        self.value()
            .get("deprecated")
            .map(|v| v.as_bool().expect("validated boolean"))
    }
    /// Borrow opaque content, preserving present null separately from absence.
    pub fn content(&self) -> Option<JsonRef<'_>> {
        self.view.content()
    }
}
impl ValidatedSourceView {
    /// Read the exact kind string without asserting provider support.
    pub fn kind(&self) -> JsonStringRef<'_> {
        text(self.value(), "kind").expect("validated kind")
    }
    /// Borrow opaque content, preserving present null separately from absence.
    pub fn content(&self) -> Option<JsonRef<'_>> {
        self.view.content()
    }
}
impl ValidatedDependencyView {
    /// Read the primary operation key proved by OBI-08.
    pub fn operation(&self) -> &str {
        self.value()
            .get("operation")
            .and_then(JsonRef::as_str)
            .expect("validated operation key")
    }
    /// Read optional exact kind filters; absence accepts every kind.
    pub fn kinds(&self) -> Option<impl ExactSizeIterator<Item = JsonStringRef<'_>>> {
        texts(self.value(), "kinds")
    }
    /// Compare a kind by decoded UTF-16 equality; absence of a filter accepts all.
    pub fn accepts_kind(&self, kind: JsonStringRef<'_>) -> bool {
        self.kinds()
            .is_none_or(|mut kinds| kinds.any(|v| v == kind))
    }
}
impl ValidatedExampleView {
    /// Borrow an example input, preserving present null separately from absence.
    pub fn input(&self) -> Option<JsonRef<'_>> {
        self.view.input()
    }
    /// Borrow an example output, preserving present null separately from absence.
    pub fn output(&self) -> Option<JsonRef<'_>> {
        self.view.output()
    }
}
