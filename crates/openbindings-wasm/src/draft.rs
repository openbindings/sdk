//! Private projection of the native typed authoring model. No opaque JSON is
//! converted into an ordinary JS number or recursively copied into metadata.
use super::{WasmJson, encoded};
use openbindings::*;
use serde::Serialize;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
enum Node {
    Object(Vec<(String, Node)>),
    Plain(serde_json::Value),
    Exact(usize),
}
#[derive(Default)]
struct Projection {
    exact: Vec<Option<JsonValue>>,
}
trait Project {
    fn project(self, p: &mut Projection) -> Node;
}
impl Project for JsonValue {
    fn project(self, p: &mut Projection) -> Node {
        let index = p.exact.len();
        p.exact.push(Some(self));
        Node::Exact(index)
    }
}
impl<T: Project> Project for BTreeMap<String, T> {
    fn project(self, p: &mut Projection) -> Node {
        Node::Object(
            self.into_iter()
                .map(|(key, value)| (key, value.project(p)))
                .collect(),
        )
    }
}
fn plain(value: impl Serialize) -> Node {
    Node::Plain(serde_json::to_value(value).expect("typed metadata is serializable"))
}
fn optional<T>(
    fields: &mut Vec<(String, Node)>,
    key: &str,
    value: Option<T>,
    map: impl FnOnce(T) -> Node,
) {
    if let Some(value) = value {
        fields.push((key.into(), map(value)));
    }
}
impl Project for DocumentBuilder {
    fn project(self, p: &mut Projection) -> Node {
        let mut fields = vec![
            ("openbindings".into(), plain(self.openbindings)),
            ("operations".into(), self.operations.project(p)),
        ];
        optional(&mut fields, "name", self.name, plain);
        optional(&mut fields, "version", self.version, plain);
        optional(&mut fields, "description", self.description, plain);
        optional(&mut fields, "schemas", self.schemas, |v| v.project(p));
        optional(&mut fields, "sources", self.sources, |v| v.project(p));
        optional(&mut fields, "bindings", self.bindings, |v| v.project(p));
        optional(&mut fields, "dependencies", self.dependencies, |v| {
            v.project(p)
        });
        fields.push(("additionalFields".into(), self.additional_fields.project(p)));
        Node::Object(fields)
    }
}
impl Project for Operation {
    fn project(self, p: &mut Projection) -> Node {
        let mut fields = vec![];
        optional(&mut fields, "description", self.description, plain);
        optional(&mut fields, "deprecated", self.deprecated, plain);
        optional(&mut fields, "tags", self.tags, plain);
        optional(&mut fields, "aliases", self.aliases, plain);
        optional(&mut fields, "input", self.input, |v| v.project(p));
        optional(&mut fields, "output", self.output, |v| v.project(p));
        optional(&mut fields, "examples", self.examples, |v| v.project(p));
        fields.push(("additionalFields".into(), self.additional_fields.project(p)));
        Node::Object(fields)
    }
}
impl Project for OperationExample {
    fn project(self, p: &mut Projection) -> Node {
        let mut fields = vec![];
        optional(&mut fields, "description", self.description, plain);
        optional(&mut fields, "input", self.input, |v| v.project(p));
        optional(&mut fields, "output", self.output, |v| v.project(p));
        fields.push(("additionalFields".into(), self.additional_fields.project(p)));
        Node::Object(fields)
    }
}
impl Project for Source {
    fn project(self, p: &mut Projection) -> Node {
        let mut fields = vec![("kind".into(), plain(self.kind))];
        optional(&mut fields, "description", self.description, plain);
        optional(&mut fields, "content", self.content, |v| v.project(p));
        fields.push(("additionalFields".into(), self.additional_fields.project(p)));
        Node::Object(fields)
    }
}
impl Project for Binding {
    fn project(self, p: &mut Projection) -> Node {
        let mut fields = vec![
            ("operation".into(), plain(self.operation)),
            ("source".into(), plain(self.source)),
        ];
        optional(&mut fields, "description", self.description, plain);
        optional(&mut fields, "content", self.content, |v| v.project(p));
        optional(&mut fields, "preference", self.preference, |v| {
            plain(v.get())
        });
        optional(&mut fields, "idempotent", self.idempotent, plain);
        optional(&mut fields, "deprecated", self.deprecated, plain);
        fields.push(("additionalFields".into(), self.additional_fields.project(p)));
        Node::Object(fields)
    }
}
impl Project for Dependency {
    fn project(self, p: &mut Projection) -> Node {
        let mut fields = vec![("operation".into(), plain(self.operation))];
        optional(&mut fields, "description", self.description, plain);
        optional(&mut fields, "kinds", self.kinds, plain);
        fields.push(("additionalFields".into(), self.additional_fields.project(p)));
        Node::Object(fields)
    }
}

/// Transfer wrapper owns every exact root until the facade takes it. Dropping
/// this wrapper releases all untaken values, including failed partial transfer.
#[wasm_bindgen]
pub struct WasmDraft {
    shape: String,
    exact: Vec<Option<JsonValue>>,
}
#[wasm_bindgen]
impl WasmDraft {
    pub fn shape(&self) -> String {
        self.shape.clone()
    }
    #[wasm_bindgen(js_name = takeExact)]
    pub fn take_exact(&mut self, index: usize) -> Option<WasmJson> {
        self.exact
            .get_mut(index)?
            .take()
            .map(|value| WasmJson { value })
    }
}
pub(crate) fn convert(value: &JsonValue) -> Result<WasmDraft, JsValue> {
    let builder = DocumentBuilder::from_json(value).map_err(|error| {
        let code = match error.kind() {
            AuthoringErrorKind::InvalidField => "invalid-field",
            AuthoringErrorKind::DuplicateMembers => "duplicate-members",
            AuthoringErrorKind::Limit => "authoring-limit",
            AuthoringErrorKind::FieldCollision => "field-collision",
            _ => "invalid-draft",
        };
        JsValue::from_str(&encoded(serde_json::json!({"status":"authoring-error", "error":{
            "code":code,"draft_pointer":error.draft_pointer(),"source_location":error.source_location(),"message":error.message()
        }})))
    })?;
    let mut projection = Projection::default();
    let shape = encoded(builder.project(&mut projection));
    Ok(WasmDraft {
        shape,
        exact: projection.exact,
    })
}
