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
fn side(side: &str) -> Result<Side, JsValue> {
    match side {
        "input" => Ok(Side::Input),
        "output" => Ok(Side::Output),
        _ => Err(error("invalid-side", "side must be input or output")),
    }
}
fn control(cancelled: bool) -> WorkControl {
    let control = WorkControl::new();
    if cancelled {
        control.cancel();
    }
    control
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
        DocumentBuilder::from_json(&self.value)
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
        // JsonRef::members maps a slice iterator; nth skips in constant time.
        // It does not walk or clone the preceding members or their contents.
        let member = self.source.view().members()?.nth(self.index)?;
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
    pub fn assess(&self) -> String {
        match self.document.assess() {
            Ok(a) => encoded(json!({"status":"assessed","report":a.report()})),
            Err(e) => encoded(json!({"status":"version-refused","refusal":e})),
        }
    }
    /// Small metadata is materialized once by TypeScript for repeated UI reads.
    pub fn operations(&self) -> Result<String, JsValue> {
        let mut result = Vec::new();
        for operation in self.document.operations().map_err(interpretation_error)? {
            result.push(operation_metadata(&operation)?);
        }
        Ok(encoded(result))
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
        Ok(encoded(metadata))
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
        Ok(encoded(metadata))
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
        Ok(encoded(metadata))
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
        self.document
            .dependency_accepts_kind(dependency, kind)
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
    pub fn contracts(
        &self,
        resources: &WasmResources,
        limits: &str,
        cache_capacity: Option<usize>,
        include_schema_details: bool,
    ) -> Result<WasmContracts, JsValue> {
        let limits: Limits =
            serde_json::from_str(limits).map_err(|e| error("invalid-evaluator-limits", e))?;
        self.document
            .value_contracts_with_options(
                Arc::new(
                    DefaultEvaluator::with_limits(limits)
                        .with_schema_details(include_schema_details),
                ),
                resources.resources.clone(),
                ValueContractOptions {
                    cache_capacity: cache_capacity
                        .unwrap_or_else(|| ValueContractOptions::default().cache_capacity),
                },
            )
            .map(|contracts| WasmContracts { contracts })
            .map_err(interpretation_error)
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
    pub fn metadata(&self) -> Result<String, JsValue> {
        operation_metadata(&self.op).map(encoded)
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
        Ok(encoded(metadata))
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
fn operation_metadata(view: &OperationView) -> Result<serde_json::Value, JsValue> {
    Ok(
        json!({"key":view.key(),"description":view.description().map_err(interpretation_error)?,
      "aliases":view.aliases().map_err(interpretation_error)?.map(|v|v.collect::<Vec<_>>()),
      "tags":view.tags().map_err(interpretation_error)?.map(|v|v.collect::<Vec<_>>()),
      "deprecated":view.deprecated().map_err(interpretation_error)?,
      "has_input":view.input().is_some(),"has_output":view.output().is_some()}),
    )
}
fn binding_metadata(view: &BindingView) -> Result<serde_json::Value, JsValue> {
    Ok(
        json!({"key":view.key(),"operation":view.operation().map_err(interpretation_error)?,
      "source":view.source().map_err(interpretation_error)?,"description":view.description().map_err(interpretation_error)?,
      "preference":view.preference().map_err(interpretation_error)?.map(|v|v.get()),
      "idempotent":view.idempotent().map_err(interpretation_error)?,"deprecated":view.deprecated().map_err(interpretation_error)?,
      "has_content":view.content().is_some()}),
    )
}
fn source_metadata(view: &SourceView) -> Result<serde_json::Value, JsValue> {
    Ok(
        json!({"key":view.key(),"kind":view.kind().map_err(interpretation_error)?,
      "description":view.description().map_err(interpretation_error)?,"has_content":view.content().is_some()}),
    )
}
fn dependency_metadata(view: &DependencyView) -> Result<serde_json::Value, JsValue> {
    Ok(
        json!({"key":view.key(),"operation":view.operation().map_err(interpretation_error)?,
      "description":view.description().map_err(interpretation_error)?,
      "kinds":view.kinds().map_err(interpretation_error)?.map(|v|v.collect::<Vec<_>>())}),
    )
}
fn example_metadata(view: &ExampleView) -> Result<serde_json::Value, JsValue> {
    Ok(
        json!({"key":view.key(),"description":view.description().map_err(interpretation_error)?,
      "has_input":view.input().is_some(),"has_output":view.output().is_some()}),
    )
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
        binding_metadata(&self.view).map(encoded)
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
        source_metadata(&self.view).map(encoded)
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
        dependency_metadata(&self.view).map(encoded)
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
        example_metadata(&self.view).map(encoded)
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
pub struct WasmContracts {
    contracts: ValueContracts,
}
#[wasm_bindgen]
impl WasmContracts {
    pub fn prepare(
        &self,
        operation: &str,
        side_name: &str,
        cancelled: bool,
    ) -> Result<WasmPreparation, JsValue> {
        Ok(WasmPreparation {
            preparation: Some(self.contracts.prepare_with_control(
                operation,
                side(side_name)?,
                &control(cancelled),
            )),
        })
    }
}
/// A private transfer wrapper. Dropping it drops any untaken compiled contract.
#[wasm_bindgen]
pub struct WasmPreparation {
    preparation: Option<ContractPreparation>,
}
#[wasm_bindgen]
impl WasmPreparation {
    pub fn result(&self) -> String {
        match &self.preparation {
            Some(ContractPreparation::Ready(_)) => encoded(json!({"status":"ready"})),
            Some(ContractPreparation::NoContract) => encoded(json!({"status":"no-contract"})),
            Some(ContractPreparation::OperationMissing) => {
                encoded(json!({"status":"operation-missing"}))
            }
            Some(ContractPreparation::OperationAmbiguous { candidates }) => {
                encoded(json!({"status":"operation-ambiguous","candidates":candidates}))
            }
            Some(ContractPreparation::NoVerdict { detail }) => {
                encoded(json!({"status":"no-verdict","detail":detail}))
            }
            None => encoded(json!({"status":"consumed"})),
        }
    }
    #[wasm_bindgen(js_name=takeContract)]
    pub fn take_contract(&mut self) -> Option<WasmPrepared> {
        match self.preparation.take()? {
            ContractPreparation::Ready(prepared) => Some(WasmPrepared { prepared }),
            _ => None,
        }
    }
}
#[wasm_bindgen]
pub struct WasmPrepared {
    prepared: PreparedContract,
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
#[wasm_bindgen(js_name=assessBytes)]
pub fn assess_bytes(bytes: &[u8]) -> String {
    match assess_document(bytes) {
        Ok(a) => {
            encoded(json!({"status":"assessed","report":a.report(),"parsed":a.parsed().is_some()}))
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
            outcome: openbindings_http_discovery::assess_body(bytes),
        }
    }
    pub fn result(&self) -> String {
        use openbindings_http_discovery::DiscoveryOutcome::*;
        match &self.outcome {
            Found { document } => encoded(
                json!({"status":"found","report":document.parsed().assess().unwrap().report()}),
            ),
            NonConformant { assessment } => {
                encoded(json!({"status":"non-conformant","report":assessment.report()}))
            }
            Undetermined { assessment } => {
                encoded(json!({"status":"undetermined","report":assessment.report()}))
            }
            VersionRefused { refusal } => {
                encoded(json!({"status":"version-refused","refusal":refusal}))
            }
            _ => unreachable!("complete body assessment only"),
        }
    }
    pub fn document(&self) -> Option<WasmDocument> {
        use openbindings_http_discovery::DiscoveryOutcome::*;
        let document = match &self.outcome {
            Found { document } => Some(document.parsed()),
            NonConformant { assessment } | Undetermined { assessment } => assessment.parsed(),
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
    pub fn new(document: &WasmDocument, allow_origin: &str) -> Result<WasmPublication, JsValue> {
        let assessed = document
            .document
            .assess()
            .map_err(|e| error("version-refused", e))?;
        let validated = assessed.validated().ok_or_else(|| {
            error(
                "publication-conformance",
                "publication requires established conformance",
            )
        })?;
        openbindings_http_discovery::Publication::new(
            &validated,
            openbindings_http_discovery::PublicationOptions {
                allow_origin: allow_origin.into(),
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
