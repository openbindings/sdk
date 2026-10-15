#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::json;
use std::{cell::Cell, sync::Arc};
use wasm_bindgen::prelude::*;

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
impl std::fmt::Debug for WasmJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmJson").finish_non_exhaustive()
    }
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
            handle: DocumentHandle::new(Document::from_json(self.value.clone())),
        }
    }
}
/// A retained source plus position, not a self-referential borrowed iterator.
#[wasm_bindgen]
pub struct WasmMembers {
    source: JsonValue,
    index: usize,
}
impl std::fmt::Debug for WasmMembers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmMembers").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmElements {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmElements").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmMember").finish_non_exhaustive()
    }
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
// Document-family wrappers deliberately share a representation. wasm-bindgen does
// not check method receivers, so their JavaScript class is never proof authority.
// Keep only the parsed snapshot here; every proof-producing/consuming entry derives
// a fresh core proof from that snapshot's cached validation at call time.
struct DocumentHandle {
    document: Document,
    taken: Cell<bool>,
}
impl DocumentHandle {
    fn new(document: Document) -> Self {
        Self {
            document,
            taken: Cell::new(false),
        }
    }
    fn proof(&self, code: &str) -> Result<Conformant<Document>, JsValue> {
        self.document
            .validate()
            .ok()
            .and_then(|v| v.conformant())
            .ok_or_else(|| error(code, "A conformant document is required."))
    }
}
#[wasm_bindgen]
#[repr(transparent)]
pub struct WasmDocument {
    handle: DocumentHandle,
}
impl std::fmt::Debug for WasmDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDocument").finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl WasmDocument {
    #[wasm_bindgen(js_name=parseBytes)]
    pub fn parse_bytes(bytes: &[u8]) -> Result<WasmDocument, JsValue> {
        Document::parse(bytes)
            .map(|document| Self {
                handle: DocumentHandle::new(document),
            })
            .map_err(input_error)
    }
    /// Apply an atomic array of {op, pointer, value?} edits. Values stay exact JSON.
    pub fn edit(&mut self, edits: &str) -> Result<(), JsValue> {
        let input = JsonValue::parse(edits).map_err(input_error)?;
        if input.has_duplicate_names() {
            return Err(edit_error(EditError::DuplicateMembers));
        }
        let rows = input
            .view()
            .elements()
            .ok_or_else(|| error("invalid-edits", "expected an edit array"))?;
        let mut values = Vec::new();
        let mut paths = Vec::new();
        for row in rows {
            let pointer = row
                .get("pointer")
                .and_then(|v| v.as_str())
                .ok_or_else(|| error("invalid-pointer", "expected a pointer string"))?;
            let value = match row.get("op").and_then(|v| v.as_str()) {
                Some("set") => Some(
                    row.get("value")
                        .ok_or_else(|| error("missing-value", "set requires a value"))?
                        .to_owned(),
                ),
                Some("remove") => None,
                _ => return Err(error("invalid-edits", "expected set or remove")),
            };
            paths.push(pointer);
            values.push(value);
        }
        let edits: Vec<_> = paths
            .iter()
            .zip(&values)
            .map(|(pointer, value)| match value {
                Some(value) => Edit::Set { pointer, value },
                None => Edit::Remove { pointer },
            })
            .collect();
        self.handle.document.edit(&edits).map_err(edit_error)
    }
    pub fn set(&mut self, pointer: &str, value: &WasmJson) -> Result<(), JsValue> {
        self.handle
            .document
            .set(pointer, &value.value)
            .map_err(edit_error)
    }
    pub fn remove(&mut self, pointer: &str) -> Result<(), JsValue> {
        self.handle.document.remove(pointer).map_err(edit_error)
    }
    #[wasm_bindgen(js_name=setDescription)]
    pub fn set_description(&mut self, value: &str) -> Result<(), JsValue> {
        self.handle
            .document
            .set_description(value)
            .map_err(edit_error)
    }
    #[wasm_bindgen(js_name=setName)]
    pub fn set_name(&mut self, value: &str) -> Result<(), JsValue> {
        self.handle.document.set_name(value).map_err(edit_error)
    }
    #[wasm_bindgen(js_name=setVersion)]
    pub fn set_version(&mut self, value: &str) -> Result<(), JsValue> {
        self.handle.document.set_version(value).map_err(edit_error)
    }
    #[wasm_bindgen(js_name=setOperationDescription)]
    pub fn set_operation_description(&mut self, key: &str, value: &str) -> Result<(), JsValue> {
        self.handle
            .document
            .set_operation_description(key, value)
            .map_err(edit_error)
    }
    #[wasm_bindgen(js_name=setOperationInput)]
    pub fn set_operation_input(&mut self, key: &str, value: &WasmJson) -> Result<(), JsValue> {
        self.handle
            .document
            .set_operation_input(key, &value.value)
            .map_err(edit_error)
    }
    #[wasm_bindgen(js_name=setOperationOutput)]
    pub fn set_operation_output(&mut self, key: &str, value: &WasmJson) -> Result<(), JsValue> {
        self.handle
            .document
            .set_operation_output(key, &value.value)
            .map_err(edit_error)
    }
    pub fn retain(&self) -> WasmDocument {
        Self {
            handle: DocumentHandle::new(self.handle.document.clone()),
        }
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.handle.document.value().clone(),
        }
    }
    #[wasm_bindgen(js_name=toBytes)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.handle.document.to_bytes().into()
    }
    pub fn validate(&self) -> WasmDocumentValidation {
        let _ = self.handle.document.validate();
        WasmDocumentValidation {
            handle: DocumentHandle::new(self.handle.document.clone()),
        }
    }
    pub fn openbindings(&self) -> Result<String, JsValue> {
        self.handle
            .document
            .openbindings()
            .map(|value| text_token(value).unwrap_or_else(|| "null".into()))
            .map_err(interpretation_error)
    }
    pub fn name(&self) -> Result<String, JsValue> {
        self.handle
            .document
            .name()
            .map(|value| text_token(value).unwrap_or_else(|| "null".into()))
            .map_err(interpretation_error)
    }
    pub fn version(&self) -> Result<String, JsValue> {
        self.handle
            .document
            .version()
            .map(|value| text_token(value).unwrap_or_else(|| "null".into()))
            .map_err(interpretation_error)
    }
    pub fn description(&self) -> Result<String, JsValue> {
        self.handle
            .document
            .description()
            .map(|value| text_token(value).unwrap_or_else(|| "null".into()))
            .map_err(interpretation_error)
    }
    #[wasm_bindgen(js_name=schemaNames)]
    pub fn schema_names(&self) -> Result<String, JsValue> {
        self.handle
            .document
            .schemas()
            .map(|v| {
                encoded(v.map(|rows| rows.into_iter().map(|(key, _)| key).collect::<Vec<_>>()))
            })
            .map_err(interpretation_error)
    }
    pub fn schema(&self, key: &str) -> Result<Option<WasmJson>, JsValue> {
        self.handle
            .document
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
        for operation in self
            .handle
            .document
            .operations()
            .map_err(interpretation_error)?
        {
            result.push(operation_metadata(&operation)?);
        }
        Ok(format!("[{}]", result.join(",")))
    }
    pub fn bindings(&self) -> Result<String, JsValue> {
        let rows = self
            .handle
            .document
            .bindings()
            .map_err(interpretation_error)?;
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
        self.handle
            .document
            .binding(key)
            .map(|v| v.map(|view| WasmBinding { view }))
            .map_err(interpretation_error)
    }
    pub fn sources(&self) -> Result<String, JsValue> {
        let rows = self
            .handle
            .document
            .sources()
            .map_err(interpretation_error)?;
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
        self.handle
            .document
            .source(key)
            .map(|v| v.map(|view| WasmSource { view }))
            .map_err(interpretation_error)
    }
    pub fn dependencies(&self) -> Result<String, JsValue> {
        let rows = self
            .handle
            .document
            .dependencies()
            .map_err(interpretation_error)?;
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
        self.handle
            .document
            .dependency(key)
            .map(|v| v.map(|view| WasmDependency { view }))
            .map_err(interpretation_error)
    }
    #[wasm_bindgen(js_name=resolveOperation)]
    pub fn resolve_operation(&self, name: &str) -> Result<WasmOperationSelection, JsValue> {
        self.handle
            .document
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
        self.handle
            .document
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
        self.handle
            .document
            .references_with_resources(resources.resources.clone(), &control(cancelled))
            .map(encoded)
            .map_err(interpretation_error)
    }
}
/// Evidence and single-use proof transfer tied to the same Rust snapshot.
#[wasm_bindgen]
#[repr(transparent)]
pub struct WasmDocumentValidation {
    handle: DocumentHandle,
}
impl std::fmt::Debug for WasmDocumentValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDocumentValidation")
            .finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl WasmDocumentValidation {
    pub fn report(&self) -> String {
        match self.handle.document.validate() {
            Ok(v) => encoded(json!({"status":"report", "report":v.report()})),
            Err(e) => encoded(json!({"status":"version-refused", "refusal":e})),
        }
    }
    #[wasm_bindgen(js_name=takeConformant)]
    pub fn take_conformant(&self) -> Option<WasmConformantDocument> {
        if self.handle.taken.replace(true) {
            return None;
        }
        self.handle
            .document
            .validate()
            .ok()?
            .conformant()
            .map(WasmConformantDocument::from_proof)
    }
}
/// Retained snapshot whose proof is re-derived at each proof-requiring entry.
#[wasm_bindgen]
#[repr(transparent)]
pub struct WasmConformantDocument {
    handle: DocumentHandle,
}
impl std::fmt::Debug for WasmConformantDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmConformantDocument")
            .finish_non_exhaustive()
    }
}

impl WasmConformantDocument {
    fn from_proof(document: Conformant<Document>) -> Self {
        Self {
            handle: DocumentHandle::new(document.as_document().clone()),
        }
    }
}
#[wasm_bindgen]
impl WasmConformantDocument {
    pub fn retain(&self) -> Result<WasmConformantDocument, JsValue> {
        self.handle
            .proof("document-conformance")
            .map(Self::from_proof)
    }
    #[wasm_bindgen(js_name=intoInner)]
    pub fn into_inner(self) -> WasmDocument {
        WasmDocument {
            handle: self.handle,
        }
    }
    #[wasm_bindgen(js_name=asDocument)]
    pub fn as_document(&self) -> WasmDocument {
        WasmDocument {
            handle: DocumentHandle::new(self.handle.document.clone()),
        }
    }
    #[wasm_bindgen(js_name=valueContracts)]
    pub fn value_contracts(
        &self,
        resources: &WasmResources,
        limits: &str,
        diagnostics: &str,
    ) -> Result<WasmValueContracts, JsValue> {
        let document = self.handle.proof("document-conformance")?;
        let limits: Limits =
            serde_json::from_str(limits).map_err(|e| error("invalid-evaluator-limits", e))?;
        let contracts = document.value_contracts(
            Arc::new(
                DefaultEvaluator::with_limits(limits).with_diagnostics(match diagnostics {
                    "coarse" => openbindings_json_schema_evaluator::Diagnostics::Coarse,
                    "upstream" => openbindings_json_schema_evaluator::Diagnostics::Upstream,
                    _ => {
                        return Err(error(
                            "invalid-diagnostics",
                            "diagnostics must be coarse or upstream",
                        ));
                    }
                }),
            ),
            resources.resources.clone(),
        );
        Ok(WasmValueContracts { contracts })
    }
    pub fn value(&self) -> WasmJson {
        self.as_document().value()
    }
    #[wasm_bindgen(js_name=toBytes)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.as_document().to_bytes()
    }
    pub fn validate(&self) -> WasmDocumentValidation {
        self.as_document().validate()
    }
    pub fn openbindings(&self) -> Result<String, JsValue> {
        self.as_document().openbindings()
    }
    pub fn name(&self) -> Result<String, JsValue> {
        self.as_document().name()
    }
    pub fn version(&self) -> Result<String, JsValue> {
        self.as_document().version()
    }
    pub fn description(&self) -> Result<String, JsValue> {
        self.as_document().description()
    }
    #[wasm_bindgen(js_name=schemaNames)]
    pub fn schema_names(&self) -> Result<String, JsValue> {
        self.as_document().schema_names()
    }
    pub fn schema(&self, key: &str) -> Result<Option<WasmJson>, JsValue> {
        self.as_document().schema(key)
    }
    pub fn operations(&self) -> Result<String, JsValue> {
        self.as_document().operations()
    }
    pub fn bindings(&self) -> Result<String, JsValue> {
        self.as_document().bindings()
    }
    pub fn binding(&self, key: &str) -> Result<Option<WasmBinding>, JsValue> {
        self.as_document().binding(key)
    }
    pub fn sources(&self) -> Result<String, JsValue> {
        self.as_document().sources()
    }
    pub fn source(&self, key: &str) -> Result<Option<WasmSource>, JsValue> {
        self.as_document().source(key)
    }
    pub fn dependencies(&self) -> Result<String, JsValue> {
        self.as_document().dependencies()
    }
    pub fn dependency(&self, key: &str) -> Result<Option<WasmDependency>, JsValue> {
        self.as_document().dependency(key)
    }
    #[wasm_bindgen(js_name=resolveOperation)]
    pub fn resolve_operation(&self, name: &str) -> Result<WasmOperationSelection, JsValue> {
        self.as_document().resolve_operation(name)
    }
    #[wasm_bindgen(js_name=dependencyAcceptsKind)]
    pub fn dependency_accepts_kind(
        &self,
        dependency: &str,
        kind: &str,
    ) -> Result<Option<bool>, JsValue> {
        self.as_document().dependency_accepts_kind(dependency, kind)
    }
    pub fn references(
        &self,
        resources: &WasmResources,
        cancelled: bool,
    ) -> Result<String, JsValue> {
        self.as_document().references(resources, cancelled)
    }
}
/// A private transfer wrapper. Dropping it drops an untaken operation owner.
#[wasm_bindgen]
pub struct WasmOperationSelection {
    selection: Option<OperationSelection>,
}
impl std::fmt::Debug for WasmOperationSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmOperationSelection")
            .finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmOperation").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmBinding").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmSource").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmDependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDependency").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmExample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmExample").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmResources").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmValueContracts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmValueContracts").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmPreparation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmPreparation").finish_non_exhaustive()
    }
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
impl std::fmt::Debug for WasmPrepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmPrepared").finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl WasmPrepared {
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
            encoded(json!({"status":"report","report":a.report(),"parsed":a.document().is_some()}))
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
impl std::fmt::Debug for WasmDiscovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDiscovery").finish_non_exhaustive()
    }
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
                json!({"status":"found","report":document.as_document().validate().unwrap().report()}),
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
            Found { document } => Some(document.as_document()),
            NonConformant { validation } | Undetermined { validation } => validation.document(),
            _ => None,
        };
        document.map(|d| WasmDocument {
            handle: DocumentHandle::new(d.clone()),
        })
    }
}
#[wasm_bindgen]
pub struct WasmPublication {
    publication: openbindings_http_discovery::Publication,
}
impl std::fmt::Debug for WasmPublication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmPublication").finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl WasmPublication {
    #[wasm_bindgen(constructor)]
    pub fn new(
        document: &WasmConformantDocument,
        allow_origin: &str,
    ) -> Result<WasmPublication, JsValue> {
        openbindings_http_discovery::Publication::new(
            &document.handle.proof("publication-conformance")?,
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

fn edit_error(error: EditError) -> JsValue {
    JsValue::from_str(&encoded(
        json!({"status":"edit-error", "code":error.code(), "message":error.to_string()}),
    ))
}
