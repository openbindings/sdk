//! Reads whose field shapes are established by the retained document proof.
use super::*;

fn text<'a>(value: JsonRef<'a>, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .map(|v| v.as_str().expect("conformant text"))
}
fn texts<'a>(value: JsonRef<'a>, field: &str) -> Option<impl ExactSizeIterator<Item = &'a str>> {
    value.get(field).map(|v| {
        v.elements()
            .expect("conformant array")
            .map(|v| v.as_str().expect("conformant text"))
    })
}
macro_rules! conformant_view {
    ($name:ident, $plain:ident) => {
        #[doc = concat!("Retained ", stringify!($plain), " with document conformance proof. Field reads are infallible; OBI-01 establishes scalar text. Original tokens and locations remain available through value(). Clones and views survive parent drop.")]
        #[derive(Clone, Debug)]
        pub struct $name {
            _document: Conformant<Document>,
            view: $plain,
        }
        impl $name {
            /// Borrow the declaration's primary key.
            pub fn key(&self) -> &str { self.view.key() }
            /// Borrow the exact original object without evaluating its schemas or examples.
            pub fn value(&self) -> JsonRef<'_> { self.view.value() }
            /// Borrow decoded descriptive text. None means absence.
            pub fn description(&self) -> Option<&str> { text(self.value(), "description") }
        }
    };
}
conformant_view!(ConformantOperationView, OperationView);
conformant_view!(ConformantBindingView, BindingView);
conformant_view!(ConformantSourceView, SourceView);
conformant_view!(ConformantDependencyView, DependencyView);
conformant_view!(ConformantExampleView, ExampleView);

impl Conformant<Document> {
    /// Borrow the exact snapshot certified by this proof.
    pub fn value(&self) -> JsonRef<'_> {
        self.document.value().view()
    }
    /// Read the supported scalar SemVer declaration established by OBI-03.
    pub fn openbindings(&self) -> &str {
        self.value()
            .get("openbindings")
            .and_then(JsonRef::as_str)
            .expect("conformant declaration")
    }
    /// Borrow the optional decoded interface name.
    pub fn name(&self) -> Option<&str> {
        text(self.value(), "name")
    }
    /// Borrow the optional decoded interface version label, distinct from openbindings.
    pub fn version(&self) -> Option<&str> {
        text(self.value(), "version")
    }
    /// Borrow optional decoded descriptive text.
    pub fn description(&self) -> Option<&str> {
        text(self.value(), "description")
    }
    /// Borrow optional schema declarations in lexical order; no evaluation occurs.
    /// None means absent; Some(empty) means a present empty namespace.
    pub fn schemas(&self) -> Option<Vec<(&str, JsonRef<'_>)>> {
        self.document
            .schemas()
            .expect("conformant schema namespace")
    }
    /// Borrow one exact object or boolean schema by primary key.
    pub fn schema(&self, name: &str) -> Option<JsonRef<'_>> {
        self.value().get("schemas").and_then(|v| v.get(name))
    }
    /// Retain all operations in lexical primary-key order, including an empty namespace.
    pub fn operations(&self) -> Vec<ConformantOperationView> {
        self.document
            .operations()
            .expect("conformant operations")
            .into_iter()
            .map(|view| ConformantOperationView {
                _document: self.clone(),
                view,
            })
            .collect()
    }
    /// Select a primary key or alias. OBI-05 excludes ambiguity; None means missing.
    pub fn resolve_operation(&self, name: &str) -> Option<ConformantOperationView> {
        match self
            .document
            .resolve_operation(name)
            .expect("conformant operation namespace")
        {
            OperationSelection::Found(view) => Some(ConformantOperationView {
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
            .expect("conformant bindings")
    }
    /// Return None for an absent dependency; otherwise compare its decoded kind filter.
    pub fn dependency_accepts_kind(&self, key: &str, kind: &str) -> Option<bool> {
        self.dependency(key).map(|v| v.accepts_kind(kind))
    }
}
macro_rules! namespace {
    ($many:ident, $one:ident, $view:ident) => {
        impl Conformant<Document> {
            #[doc = concat!("Retain ", stringify!($many), " in lexical key order. None means absent; Some(empty) means present-empty.")]
            pub fn $many(&self) -> Option<Vec<$view>> {
                self.document.$many().expect("conformant namespace").map(|rows| rows.into_iter().map(|view| $view { _document: self.clone(), view }).collect())
            }
            #[doc = concat!("Retain one ", stringify!($one), " by exact primary key; None means absent.")]
            pub fn $one(&self, key: &str) -> Option<$view> {
                self.document.$one(key).expect("conformant namespace").map(|view| $view { _document: self.clone(), view })
            }
        }
    };
}
namespace!(bindings, binding, ConformantBindingView);
namespace!(sources, source, ConformantSourceView);
namespace!(dependencies, dependency, ConformantDependencyView);

impl ConformantOperationView {
    /// Borrow decoded tags, preserving absent versus present-empty.
    pub fn tags(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        texts(self.value(), "tags")
    }
    /// Read optional scalar aliases, whose identifier spelling is proved by OBI-04.
    pub fn aliases(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        texts(self.value(), "aliases")
    }
    /// Read optional deprecation; absence is distinct from false.
    pub fn deprecated(&self) -> Option<bool> {
        self.value()
            .get("deprecated")
            .map(|v| v.as_bool().expect("conformant boolean"))
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
    pub fn examples(&self) -> Option<Vec<ConformantExampleView>> {
        self.view
            .examples()
            .expect("conformant examples")
            .map(|rows| {
                rows.into_iter()
                    .map(|view| ConformantExampleView {
                        _document: self._document.clone(),
                        view,
                    })
                    .collect()
            })
    }
    /// Retain one named example; None means absent.
    pub fn example(&self, key: &str) -> Option<ConformantExampleView> {
        self.view
            .example(key)
            .expect("conformant example")
            .map(|view| ConformantExampleView {
                _document: self._document.clone(),
                view,
            })
    }
}
impl ConformantBindingView {
    /// Read the primary operation key proved by OBI-06.
    pub fn operation(&self) -> &str {
        self.value()
            .get("operation")
            .and_then(JsonRef::as_str)
            .expect("conformant operation key")
    }
    /// Read the source key proved by OBI-07.
    pub fn source(&self) -> &str {
        self.value()
            .get("source")
            .and_then(JsonRef::as_str)
            .expect("conformant source key")
    }
    /// Read the optional interoperable integer without assigning a default rank.
    pub fn preference(&self) -> Option<Preference> {
        self.view.preference().expect("conformant preference")
    }
    /// Read optional idempotence; absence is distinct from false.
    pub fn idempotent(&self) -> Option<bool> {
        self.value()
            .get("idempotent")
            .map(|v| v.as_bool().expect("conformant boolean"))
    }
    /// Read optional deprecation; absence is distinct from false.
    pub fn deprecated(&self) -> Option<bool> {
        self.value()
            .get("deprecated")
            .map(|v| v.as_bool().expect("conformant boolean"))
    }
    /// Borrow opaque content, preserving present null separately from absence.
    pub fn content(&self) -> Option<JsonRef<'_>> {
        self.view.content()
    }
}
impl ConformantSourceView {
    /// Borrow the decoded kind string without asserting provider support.
    pub fn kind(&self) -> &str {
        text(self.value(), "kind").expect("conformant kind")
    }
    /// Borrow opaque content, preserving present null separately from absence.
    pub fn content(&self) -> Option<JsonRef<'_>> {
        self.view.content()
    }
}
impl ConformantDependencyView {
    /// Read the primary operation key proved by OBI-08.
    pub fn operation(&self) -> &str {
        self.value()
            .get("operation")
            .and_then(JsonRef::as_str)
            .expect("conformant operation key")
    }
    /// Borrow decoded kind filters; absence accepts every kind.
    pub fn kinds(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        texts(self.value(), "kinds")
    }
    /// Compare kind text independent of escape spelling; absence accepts all.
    pub fn accepts_kind(&self, kind: &str) -> bool {
        self.kinds()
            .is_none_or(|mut kinds| kinds.any(|v| v == kind))
    }
}
impl ConformantExampleView {
    /// Borrow an example input, preserving present null separately from absence.
    pub fn input(&self) -> Option<JsonRef<'_>> {
        self.view.input()
    }
    /// Borrow an example output, preserving present null separately from absence.
    pub fn output(&self) -> Option<JsonRef<'_>> {
        self.view.output()
    }
}
