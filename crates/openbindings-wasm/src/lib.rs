//! Private Wasm ABI used by the supported TypeScript facade.
//! Semantic operations stay in core; no JavaScript callback enters a Send/Sync trait.
#![forbid(unsafe_code)]
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::json;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
mod draft;
use draft::WasmDraft;
/// Seed the engine's shared randomized tables while host entropy is available.
/// The supported facade verifies host capability before entering this function.
#[wasm_bindgen(js_name=initializeRuntime)]
pub fn initialize_runtime() {
    let _ = ahash::RandomState::new();
}
fn encoded(value: impl serde::Serialize) -> String {
    serde_json::to_string(&value).expect("SDK metadata is serializable")
}
fn error(code: &str, message: impl ToString) -> JsValue {
    JsValue::from_str(&encoded(json!({"code":code,"message":message.to_string()})))
}
fn input_error(e: InputError) -> JsValue {
    JsValue::from_str(&encoded(
        json!({"code":e.code,"kind":format!("{:?}",e.kind),"byte_offset":e.byte_offset,"message":e.to_string()}),
    ))
}
fn interpretation_error(e: InterpretationError) -> JsValue {
    JsValue::from_str(&encoded(
        json!({"code":"interpretation", "message":e.to_string(), "interpretation_code":e.code(), "location":e.source_location()}),
    ))
}
fn direction(direction: &str) -> Result<Direction, JsValue> {
    match direction {
        "input" => Ok(Direction::Input),
        "output" => Ok(Direction::Output),
        _ => Err(error(
            "invalid-direction",
            "direction must be input or output",
        )),
    }
}
fn control(cancelled: bool) -> std::sync::atomic::AtomicBool {
    std::sync::atomic::AtomicBool::new(cancelled)
}
#[wasm_bindgen]
pub struct WasmJson {
    value: JsonValue,
}
#[wasm_bindgen]
impl WasmJson {
    #[wasm_bindgen(js_name=parseBytes)]
    pub fn parse_bytes(bytes: &[u8]) -> Result<WasmJson, JsValue> {
        JsonValue::parse(bytes)
            .map(|value| Self { value })
            .map_err(input_error)
    }
    #[wasm_bindgen(js_name=parseText)]
    pub fn parse_text(text: &str) -> Result<WasmJson, JsValue> {
        Self::parse_bytes(text.as_bytes())
    }
    pub fn retain(&self) -> WasmJson {
        Self {
            value: self.value.clone(),
        }
    }
    pub fn text(&self) -> String {
        self.value.text().into()
    }
    pub fn bytes(&self) -> Vec<u8> {
        self.value.bytes().into()
    }
    pub fn at(&self, pointer: &str) -> Option<WasmJson> {
        self.value.at(pointer).map(|v| Self {
            value: v.to_owned(),
        })
    }
    pub fn get(&self, name: &str) -> Option<WasmJson> {
        self.value.get(name).map(|v| Self {
            value: v.to_owned(),
        })
    }
    pub fn metadata(&self) -> String {
        encoded(
            json!({"kind":format!("{:?}",self.value.kind()).to_lowercase(),"location":self.value.location(),"duplicate_names":self.value.has_duplicate_names()}),
        )
    }
    pub fn members(&self) -> Option<WasmMembers> {
        (self.value.kind() == JsonKind::Object).then(|| WasmMembers {
            source: self.value.clone(),
            index: 0,
        })
    }
    pub fn elements(&self) -> Option<WasmElements> {
        (self.value.kind() == JsonKind::Array).then(|| WasmElements {
            source: self.value.clone(),
            index: 0,
        })
    }
    pub fn equals(&self, other: &WasmJson) -> Option<bool> {
        self.value.semantic_eq(&other.value)
    }
    pub fn document(&self) -> WasmDocument {
        WasmDocument {
            document: ParsedDocument::from_json(self.value.clone()),
        }
    }
    /// Typed authoring checks known members while retaining exact/unknown fields.
    #[wasm_bindgen(js_name=authorDocument)]
    pub fn author_document(&self) -> Result<WasmDocument, JsValue> {
        Document::from_json(&self.value)
            .and_then(|b| b.build())
            .map(|document| WasmDocument { document })
            .map_err(|e| error("authoring", e))
    }
}
/// A retained source plus position, not a self-referential borrowed iterator.
#[wasm_bindgen]
pub struct WasmMembers {
    source: JsonValue,
    index: usize,
}
#[wasm_bindgen]
impl WasmMembers {
    #[wasm_bindgen(js_name = next)]
    pub fn advance(&mut self) -> Option<WasmMember> {
        let member =
            openbindings_internal_json::backend::member_at(self.source.view(), self.index)?;
        let result = WasmMember {
            index: self.index,
            name: member.name.to_owned(),
            value: member.value.to_owned(),
        };
        self.index += 1;
        Some(result)
    }
}
#[wasm_bindgen]
pub struct WasmElements {
    source: JsonValue,
    index: usize,
}
#[wasm_bindgen]
impl WasmElements {
    #[wasm_bindgen(js_name = next)]
    pub fn advance(&mut self) -> Option<WasmJson> {
        let value = self.source.view().element(self.index)?.to_owned();
        self.index += 1;
        Some(WasmJson { value })
    }
}
#[wasm_bindgen]
pub struct WasmMember {
    index: usize,
    name: JsonValue,
    value: JsonValue,
}
#[wasm_bindgen]
impl WasmMember {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn name(&self) -> WasmJson {
        WasmJson {
            value: self.name.clone(),
        }
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.value.clone(),
        }
    }
    pub fn retain(&self) -> WasmMember {
        Self {
            index: self.index,
            name: self.name.clone(),
            value: self.value.clone(),
        }
    }
}
#[wasm_bindgen]
pub struct WasmDocument {
    document: ParsedDocument,
}
#[wasm_bindgen]
impl WasmDocument {
    #[wasm_bindgen(js_name=parseBytes)]
    pub fn parse_bytes(bytes: &[u8]) -> Result<WasmDocument, JsValue> {
        ParsedDocument::parse(bytes)
            .map(|document| Self { document })
            .map_err(input_error)
    }
    pub fn retain(&self) -> WasmDocument {
        Self {
            document: self.document.clone(),
        }
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.document.value().clone(),
        }
    }
    #[wasm_bindgen(js_name=originalBytes)]
    pub fn original_bytes(&self) -> Vec<u8> {
        self.document.original_bytes().into()
    }
    pub fn validate(&self) -> WasmDocumentValidation {
        WasmDocumentValidation {
            validation: self.document.validate(),
            taken: false,
        }
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        Ok(exact_metadata(
            json!({}),
            &[
                (
                    "openbindings",
                    self.document
                        .openbindings()
                        .map_err(interpretation_error)?
                        .map(|v| v.json().text().to_owned()),
                ),
                (
                    "name",
                    self.document
                        .name()
                        .map_err(interpretation_error)?
                        .map(|v| v.json().text().to_owned()),
                ),
                (
                    "version",
                    self.document
                        .version()
                        .map_err(interpretation_error)?
                        .map(|v| v.json().text().to_owned()),
                ),
                (
                    "description",
                    self.document
                        .description()
                        .map_err(interpretation_error)?
                        .map(|v| v.json().text().to_owned()),
                ),
            ],
        ))
    }
    #[wasm_bindgen(js_name=schemaNames)]
    pub fn schema_names(&self) -> Result<String, JsValue> {
        self.document
            .schemas()
            .map(|v| {
                encoded(v.map(|rows| rows.into_iter().map(|(key, _)| key).collect::<Vec<_>>()))
            })
            .map_err(interpretation_error)
    }
    pub fn schema(&self, key: &str) -> Result<Option<WasmJson>, JsValue> {
        self.document
            .schema(key)
            .map(|v| {
                v.map(|v| WasmJson {
                    value: v.to_owned(),
                })
            })
            .map_err(interpretation_error)
    }
    /// Small metadata is materialized once by TypeScript for repeated UI reads.
    pub fn operations(&self) -> Result<String, JsValue> {
        let mut result = Vec::new();
        for operation in self.document.operations().map_err(interpretation_error)? {
            result.push(operation_metadata(&operation)?);
        }
        Ok(format!("[{}]", result.join(",")))
    }
    #[wasm_bindgen(js_name=toDraft)]
    pub fn to_draft(&self) -> Result<WasmDraft, JsValue> {
        draft::convert(self.document.value())
    }
    pub fn bindings(&self) -> Result<String, JsValue> {
        let rows = self.document.bindings().map_err(interpretation_error)?;
        let metadata = rows
            .map(|rows| {
                rows.iter()
                    .map(binding_metadata)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        Ok(metadata.map_or_else(|| "null".into(), |rows| format!("[{}]", rows.join(","))))
    }
    pub fn binding(&self, key: &str) -> Result<Option<WasmBinding>, JsValue> {
        self.document
            .binding(key)
            .map(|v| v.map(|view| WasmBinding { view }))
            .map_err(interpretation_error)
    }
    pub fn sources(&self) -> Result<String, JsValue> {
        let rows = self.document.sources().map_err(interpretation_error)?;
        let metadata = rows
            .map(|rows| {
                rows.iter()
                    .map(source_metadata)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        Ok(metadata.map_or_else(|| "null".into(), |rows| format!("[{}]", rows.join(","))))
    }
    pub fn source(&self, key: &str) -> Result<Option<WasmSource>, JsValue> {
        self.document
            .source(key)
            .map(|v| v.map(|view| WasmSource { view }))
            .map_err(interpretation_error)
    }
    pub fn dependencies(&self) -> Result<String, JsValue> {
        let rows = self.document.dependencies().map_err(interpretation_error)?;
        let metadata = rows
            .map(|rows| {
                rows.iter()
                    .map(dependency_metadata)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        Ok(metadata.map_or_else(|| "null".into(), |rows| format!("[{}]", rows.join(","))))
    }
    pub fn dependency(&self, key: &str) -> Result<Option<WasmDependency>, JsValue> {
        self.document
            .dependency(key)
            .map(|v| v.map(|view| WasmDependency { view }))
            .map_err(interpretation_error)
    }
    #[wasm_bindgen(js_name=resolveOperation)]
    pub fn resolve_operation(&self, name: &str) -> Result<WasmOperationSelection, JsValue> {
        self.document
            .resolve_operation(name)
            .map(|selection| WasmOperationSelection {
                selection: Some(selection),
            })
            .map_err(interpretation_error)
    }
    #[wasm_bindgen(js_name=dependencyAcceptsKind)]
    pub fn dependency_accepts_kind(
        &self,
        dependency: &str,
        kind: &str,
    ) -> Result<Option<bool>, JsValue> {
        let exact = JsonValue::parse(kind).map_err(input_error)?;
        let kind = exact
            .view()
            .as_string()
            .ok_or_else(|| error("invalid-kind", "kind must be an encoded JSON string"))?;
        self.document
            .dependency(dependency)
            .map_err(interpretation_error)?
            .map(|view| {
                view.kinds().map(|kinds| {
                    kinds.is_none_or(|mut kinds| kinds.any(|candidate| candidate == kind))
                })
            })
            .transpose()
            .map_err(interpretation_error)
    }
    pub fn references(
        &self,
        resources: &WasmResources,
        cancelled: bool,
    ) -> Result<String, JsValue> {
        self.document
            .references_with_resources(resources.resources.clone(), &control(cancelled))
            .map(encoded)
            .map_err(interpretation_error)
    }
}
/// Evidence and single-use proof transfer tied to the same Rust snapshot.
#[wasm_bindgen]
pub struct WasmDocumentValidation {
    validation: Result<DocumentValidation, VersionRefusal>,
    taken: bool,
}
#[wasm_bindgen]
impl WasmDocumentValidation {
    pub fn report(&self) -> String {
        match &self.validation {
            Ok(v) => encoded(json!({"status":"report", "report":v.report()})),
            Err(e) => encoded(json!({"status":"version-refused", "refusal":e})),
        }
    }
    #[wasm_bindgen(js_name=takeValidated)]
    pub fn take_validated(&mut self) -> Option<WasmValidatedDocument> {
        if self.taken {
            return None;
        }
        self.taken = true;
        self.validation
            .as_ref()
            .ok()?
            .validated()
            .map(|document| WasmValidatedDocument { document })
    }
}
/// A Rust-owned conformance proof; there is no public unchecked constructor.
#[wasm_bindgen]
pub struct WasmValidatedDocument {
    document: ValidatedDocument,
}
#[wasm_bindgen]
impl WasmValidatedDocument {
    pub fn retain(&self) -> WasmValidatedDocument {
        Self {
            document: self.document.clone(),
        }
    }
    pub fn parsed(&self) -> WasmDocument {
        WasmDocument {
            document: self.document.parsed().clone(),
        }
    }
    #[wasm_bindgen(js_name=valueContracts)]
    pub fn value_contracts(
        &self,
        resources: &WasmResources,
        limits: &str,
        include_schema_details: bool,
    ) -> Result<WasmValueContracts, JsValue> {
        let limits: Limits =
            serde_json::from_str(limits).map_err(|e| error("invalid-evaluator-limits", e))?;
        let contracts = self.document.value_contracts(
            Arc::new(
                DefaultEvaluator::with_limits(limits).with_schema_details(include_schema_details),
            ),
            resources.resources.clone(),
        );
        Ok(WasmValueContracts { contracts })
    }
    pub fn value(&self) -> WasmJson {
        self.parsed().value()
    }
    #[wasm_bindgen(js_name=originalBytes)]
    pub fn original_bytes(&self) -> Vec<u8> {
        self.parsed().original_bytes()
    }
    pub fn validate(&self) -> WasmDocumentValidation {
        self.parsed().validate()
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        self.parsed().metadata()
    }
    #[wasm_bindgen(js_name=schemaNames)]
    pub fn schema_names(&self) -> Result<String, JsValue> {
        self.parsed().schema_names()
    }
    pub fn schema(&self, key: &str) -> Result<Option<WasmJson>, JsValue> {
        self.parsed().schema(key)
    }
    pub fn operations(&self) -> Result<String, JsValue> {
        self.parsed().operations()
    }
    #[wasm_bindgen(js_name=toDraft)]
    pub fn to_draft(&self) -> Result<WasmDraft, JsValue> {
        self.parsed().to_draft()
    }
    pub fn bindings(&self) -> Result<String, JsValue> {
        self.parsed().bindings()
    }
    pub fn binding(&self, key: &str) -> Result<Option<WasmBinding>, JsValue> {
        self.parsed().binding(key)
    }
    pub fn sources(&self) -> Result<String, JsValue> {
        self.parsed().sources()
    }
    pub fn source(&self, key: &str) -> Result<Option<WasmSource>, JsValue> {
        self.parsed().source(key)
    }
    pub fn dependencies(&self) -> Result<String, JsValue> {
        self.parsed().dependencies()
    }
    pub fn dependency(&self, key: &str) -> Result<Option<WasmDependency>, JsValue> {
        self.parsed().dependency(key)
    }
    #[wasm_bindgen(js_name=resolveOperation)]
    pub fn resolve_operation(&self, name: &str) -> Result<WasmOperationSelection, JsValue> {
        self.parsed().resolve_operation(name)
    }
    #[wasm_bindgen(js_name=dependencyAcceptsKind)]
    pub fn dependency_accepts_kind(
        &self,
        dependency: &str,
        kind: &str,
    ) -> Result<Option<bool>, JsValue> {
        self.parsed().dependency_accepts_kind(dependency, kind)
    }
    pub fn references(
        &self,
        resources: &WasmResources,
        cancelled: bool,
    ) -> Result<String, JsValue> {
        self.parsed().references(resources, cancelled)
    }
}
/// A private transfer wrapper. Dropping it drops an untaken operation owner.
#[wasm_bindgen]
pub struct WasmOperationSelection {
    selection: Option<OperationSelection>,
}
#[wasm_bindgen]
impl WasmOperationSelection {
    pub fn result(&self) -> String {
        match &self.selection {
            Some(OperationSelection::Found(_)) => encoded(json!({"status":"found"})),
            Some(OperationSelection::Missing) => encoded(json!({"status":"missing"})),
            Some(OperationSelection::Ambiguous { candidates }) => {
                encoded(json!({"status":"ambiguous","candidates":candidates}))
            }
            None => encoded(json!({"status":"consumed"})),
        }
    }
    #[wasm_bindgen(js_name=takeOperation)]
    pub fn take_operation(&mut self) -> Option<WasmOperation> {
        match self.selection.take()? {
            OperationSelection::Found(op) => Some(WasmOperation { op }),
            _ => None,
        }
    }
}
#[wasm_bindgen]
pub struct WasmOperation {
    op: OperationView,
}
#[wasm_bindgen]
impl WasmOperation {
    pub fn key(&self) -> String {
        self.op.key().to_owned()
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.op.value().to_owned(),
        }
    }
    pub fn input(&self) -> Option<WasmJson> {
        self.op.input().map(|v| WasmJson {
            value: v.to_owned(),
        })
    }
    pub fn output(&self) -> Option<WasmJson> {
        self.op.output().map(|v| WasmJson {
            value: v.to_owned(),
        })
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        operation_metadata(&self.op)
    }
    pub fn examples(&self) -> Result<String, JsValue> {
        let rows = self.op.examples().map_err(interpretation_error)?;
        let metadata = rows
            .map(|rows| {
                rows.iter()
                    .map(example_metadata)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        Ok(metadata.map_or_else(|| "null".into(), |rows| format!("[{}]", rows.join(","))))
    }
    pub fn example(&self, key: &str) -> Result<Option<WasmExample>, JsValue> {
        self.op
            .example(key)
            .map(|v| v.map(|view| WasmExample { view }))
            .map_err(interpretation_error)
    }
    pub fn bindings(&self) -> Result<String, JsValue> {
        self.op
            .bindings()
            .map(encoded)
            .map_err(interpretation_error)
    }
}
// Exact text crosses the bridge as escaped JSON, never through Rust scalar strings.
fn exact_metadata(fields: serde_json::Value, exact: &[(&str, Option<String>)]) -> String {
    let mut result = encoded(fields);
    result.pop();
    for (key, value) in exact {
        if result.len() > 1 {
            result.push(',');
        }
        result.push_str(&encoded(key));
        result.push(':');
        result.push_str(value.as_deref().unwrap_or("null"));
    }
    result.push('}');
    result
}
fn text_token(value: Option<JsonStringRef<'_>>) -> Option<String> {
    value.map(|v| v.json().text().to_owned())
}
fn text_tokens<'a>(values: Option<impl Iterator<Item = JsonStringRef<'a>>>) -> Option<String> {
    values.map(|v| {
        format!(
            "[{}]",
            v.map(|v| v.json().text()).collect::<Vec<_>>().join(",")
        )
    })
}
fn operation_metadata(view: &OperationView) -> Result<String, JsValue> {
    Ok(exact_metadata(
        json!({"key":view.key(),
      "aliases":view.aliases().map_err(interpretation_error)?.map(|v|v.collect::<Vec<_>>()),
      "deprecated":view.deprecated().map_err(interpretation_error)?,
      "has_input":view.input().is_some(),"has_output":view.output().is_some()}),
        &[
            (
                "description",
                text_token(view.description().map_err(interpretation_error)?),
            ),
            (
                "tags",
                text_tokens(view.tags().map_err(interpretation_error)?),
            ),
        ],
    ))
}
fn binding_metadata(view: &BindingView) -> Result<String, JsValue> {
    Ok(exact_metadata(
        json!({"key":view.key(),"operation":view.operation().map_err(interpretation_error)?,
      "source":view.source().map_err(interpretation_error)?,
      "preference":view.preference().map_err(interpretation_error)?.map(|v|v.get()),
      "idempotent":view.idempotent().map_err(interpretation_error)?,"deprecated":view.deprecated().map_err(interpretation_error)?,
      "has_content":view.content().is_some()}),
        &[(
            "description",
            text_token(view.description().map_err(interpretation_error)?),
        )],
    ))
}
fn source_metadata(view: &SourceView) -> Result<String, JsValue> {
    Ok(exact_metadata(
        json!({"key":view.key(),"has_content":view.content().is_some()}),
        &[
            (
                "description",
                text_token(view.description().map_err(interpretation_error)?),
            ),
            (
                "kind",
                text_token(Some(view.kind().map_err(interpretation_error)?)),
            ),
        ],
    ))
}
fn dependency_metadata(view: &DependencyView) -> Result<String, JsValue> {
    Ok(exact_metadata(
        json!({"key":view.key(),"operation":view.operation().map_err(interpretation_error)?}),
        &[
            (
                "description",
                text_token(view.description().map_err(interpretation_error)?),
            ),
            (
                "kinds",
                text_tokens(view.kinds().map_err(interpretation_error)?),
            ),
        ],
    ))
}
fn example_metadata(view: &ExampleView) -> Result<String, JsValue> {
    Ok(exact_metadata(
        json!({"key":view.key(),"has_input":view.input().is_some(),"has_output":view.output().is_some()}),
        &[(
            "description",
            text_token(view.description().map_err(interpretation_error)?),
        )],
    ))
}
#[wasm_bindgen]
pub struct WasmBinding {
    view: BindingView,
}
#[wasm_bindgen]
impl WasmBinding {
    pub fn retain(&self) -> WasmBinding {
        Self {
            view: self.view.clone(),
        }
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        binding_metadata(&self.view)
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.view.value().to_owned(),
        }
    }
    pub fn content(&self) -> Option<WasmJson> {
        self.view.content().map(|value| WasmJson {
            value: value.to_owned(),
        })
    }
}
#[wasm_bindgen]
pub struct WasmSource {
    view: SourceView,
}
#[wasm_bindgen]
impl WasmSource {
    pub fn retain(&self) -> WasmSource {
        Self {
            view: self.view.clone(),
        }
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        source_metadata(&self.view)
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.view.value().to_owned(),
        }
    }
    pub fn content(&self) -> Option<WasmJson> {
        self.view.content().map(|value| WasmJson {
            value: value.to_owned(),
        })
    }
}
#[wasm_bindgen]
pub struct WasmDependency {
    view: DependencyView,
}
#[wasm_bindgen]
impl WasmDependency {
    pub fn retain(&self) -> WasmDependency {
        Self {
            view: self.view.clone(),
        }
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        dependency_metadata(&self.view)
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.view.value().to_owned(),
        }
    }
}
#[wasm_bindgen]
pub struct WasmExample {
    view: ExampleView,
}
#[wasm_bindgen]
impl WasmExample {
    pub fn retain(&self) -> WasmExample {
        Self {
            view: self.view.clone(),
        }
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        example_metadata(&self.view)
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.view.value().to_owned(),
        }
    }
    pub fn input(&self) -> Option<WasmJson> {
        self.view.input().map(|value| WasmJson {
            value: value.to_owned(),
        })
    }
    pub fn output(&self) -> Option<WasmJson> {
        self.view.output().map(|value| WasmJson {
            value: value.to_owned(),
        })
    }
}
#[wasm_bindgen]
pub struct WasmResources {
    resources: ResourceSet,
}
#[wasm_bindgen]
impl WasmResources {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmResources {
        Self {
            resources: ResourceSet::default(),
        }
    }
    pub fn add(&self, uri: &str, value: &WasmJson) -> Result<WasmResources, JsValue> {
        ResourceSet::new(self.resources.iter().cloned().chain([SchemaResource {
            uri: uri.into(),
            document: value.value.clone(),
        }]))
        .map(|resources| Self { resources })
        .map_err(|e| error("resource", e))
    }
    pub fn retain(&self) -> WasmResources {
        Self {
            resources: self.resources.clone(),
        }
    }
}
impl Default for WasmResources {
    fn default() -> Self {
        Self::new()
    }
}
#[wasm_bindgen]
pub struct WasmValueContracts {
    contracts: ValueContracts,
}
#[wasm_bindgen]
impl WasmValueContracts {
    pub fn prepare(
        &self,
        operation: &str,
        direction_name: &str,
        cancelled: bool,
    ) -> Result<WasmPreparation, JsValue> {
        Ok(WasmPreparation {
            preparation: Some(self.contracts.prepare_with_control(
                operation,
                direction(direction_name)?,
                &control(cancelled),
            )),
        })
    }
}
/// A private transfer wrapper. Dropping it drops any untaken compiled value contract.
#[wasm_bindgen]
pub struct WasmPreparation {
    preparation: Option<ValueContractPreparation>,
}
#[wasm_bindgen]
impl WasmPreparation {
    pub fn result(&self) -> String {
        match &self.preparation {
            Some(ValueContractPreparation::Ready(_)) => encoded(json!({"status":"ready"})),
            Some(ValueContractPreparation::NoValueContract) => {
                encoded(json!({"status":"no-value-contract"}))
            }
            Some(ValueContractPreparation::OperationMissing) => {
                encoded(json!({"status":"operation-missing"}))
            }
            Some(ValueContractPreparation::OperationAmbiguous { candidates }) => {
                encoded(json!({"status":"operation-ambiguous","candidates":candidates}))
            }
            Some(ValueContractPreparation::NoVerdict { detail }) => {
                encoded(json!({"status":"no-verdict","detail":detail}))
            }
            None => encoded(json!({"status":"consumed"})),
        }
    }
    #[wasm_bindgen(js_name=takeValueContract)]
    pub fn take_value_contract(&mut self) -> Option<WasmPrepared> {
        match self.preparation.take()? {
            ValueContractPreparation::Ready(prepared) => Some(WasmPrepared { prepared }),
            _ => None,
        }
    }
}
#[wasm_bindgen]
pub struct WasmPrepared {
    prepared: ValueContract,
}
#[wasm_bindgen]
impl WasmPrepared {
    pub fn resource_completeness(&self) -> String {
        encoded(self.prepared.resource_completeness())
    }
    pub fn validate(&self, value: &WasmJson, cancelled: bool) -> String {
        encoded(
            self.prepared
                .validate_with_control(&value.value, &control(cancelled)),
        )
    }
    pub fn retain(&self) -> WasmPrepared {
        Self {
            prepared: self.prepared.clone(),
        }
    }
}
#[wasm_bindgen(js_name=validateBytes)]
pub fn validate_bytes(bytes: &[u8]) -> String {
    match validate_document(bytes) {
        Ok(a) => {
            encoded(json!({"status":"report","report":a.report(),"parsed":a.parsed().is_some()}))
        }
        Err(e) => encoded(json!({"status":"version-refused","refusal":e})),
    }
}
#[wasm_bindgen(js_name=versionPolicy)]
pub fn version_policy() -> String {
    encoded(
        json!({"authoring_version":AUTHORING_VERSION,"supported_versions":SUPPORTED_VERSIONS,"applied_spec_revision":APPLIED_SPEC_REVISION}),
    )
}
#[wasm_bindgen(js_name=checkVersion)]
pub fn check_version_bridge(version: &str) -> String {
    format!("{:?}", check_version(version)).to_lowercase()
}
#[wasm_bindgen(js_name=liveStorageOwners)]
pub fn live_storage_owners() -> usize {
    openbindings_internal_json::backend::live_arenas()
}
#[wasm_bindgen(js_name=evaluatorLimits)]
pub fn evaluator_limits() -> String {
    encoded(Limits::default())
}

#[wasm_bindgen(js_name=discoveryEndpoint)]
pub fn discovery_endpoint(origin: &str) -> Result<String, JsValue> {
    openbindings_http_discovery::endpoint(origin).map_err(|e| error(e.code, e))
}
#[wasm_bindgen(js_name=discoveryPolicy)]
pub fn discovery_policy() -> String {
    use openbindings_http_discovery::*;
    encoded(
        json!({"version":COMPANION_VERSION,"revision":APPLIED_COMPANION_REVISION,"well_known_path":WELL_KNOWN_PATH,"media_type":MEDIA_TYPE,"accept":ACCEPT,"default_max_document_bytes":DEFAULT_MAX_DOCUMENT_BYTES}),
    )
}
#[wasm_bindgen]
pub struct WasmDiscovery {
    outcome: openbindings_http_discovery::DiscoveryOutcome,
}
#[wasm_bindgen]
impl WasmDiscovery {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> WasmDiscovery {
        Self {
            outcome: openbindings_http_discovery::validate_body(bytes),
        }
    }
    pub fn result(&self) -> String {
        use openbindings_http_discovery::DiscoveryOutcome::*;
        match &self.outcome {
            Found { document } => encoded(
                json!({"status":"found","report":document.parsed().validate().unwrap().report()}),
            ),
            NonConformant { validation } => {
                encoded(json!({"status":"non-conformant","report":validation.report()}))
            }
            Undetermined { validation } => {
                encoded(json!({"status":"undetermined","report":validation.report()}))
            }
            VersionRefused { refusal } => {
                encoded(json!({"status":"version-refused","refusal":refusal}))
            }
            _ => unreachable!("complete body validation only"),
        }
    }
    pub fn document(&self) -> Option<WasmDocument> {
        use openbindings_http_discovery::DiscoveryOutcome::*;
        let document = match &self.outcome {
            Found { document } => Some(document.parsed()),
            NonConformant { validation } | Undetermined { validation } => validation.parsed(),
            _ => None,
        };
        document.map(|d| WasmDocument {
            document: d.clone(),
        })
    }
}
#[wasm_bindgen]
pub struct WasmPublication {
    publication: openbindings_http_discovery::Publication,
}
#[wasm_bindgen]
impl WasmPublication {
    #[wasm_bindgen(constructor)]
    pub fn new(
        document: &WasmValidatedDocument,
        allow_origin: &str,
    ) -> Result<WasmPublication, JsValue> {
        openbindings_http_discovery::Publication::new(
            &document.document,
            openbindings_http_discovery::PublicationOptions {
                allow_origin: match allow_origin {
                    "*" => openbindings_http_discovery::AllowOrigin::Any,
                    "" => openbindings_http_discovery::AllowOrigin::Omit,
                    "null" => openbindings_http_discovery::AllowOrigin::Null,
                    origin => openbindings_http_discovery::AllowOrigin::Exact(origin.into()),
                },
            },
        )
        .map(|publication| Self { publication })
        .map_err(|e| error(e.code, e))
    }
    pub fn metadata(&self, method: &str, path: &str) -> String {
        let response = self.publication.respond(method, path);
        encoded(json!({"status":response.status,"headers":response.headers}))
    }
    pub fn body(&self, method: &str, path: &str) -> Vec<u8> {
        self.publication.respond(method, path).body.to_vec()
    }
}
