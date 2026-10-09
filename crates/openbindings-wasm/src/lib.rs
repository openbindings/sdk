//! Private Wasm ABI used by the supported TypeScript facade.
//! Semantic operations stay in core; no JavaScript callback enters a Send/Sync trait.
#![forbid(unsafe_code)]
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::json;
use std::sync::Arc;
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
        for operation in self
            .document
            .operations()
            .map_err(|e| error("interpretation", e))?
        {
            result.push(json!({"key":operation.key,"description":operation.value.get("description").and_then(|v|v.as_str()),"aliases":operation.value.get("aliases").and_then(|v|v.elements()).map(|v|v.filter_map(|x|x.as_str()).collect::<Vec<_>>()),"has_input":operation.value.get("input").is_some(),"has_output":operation.value.get("output").is_some()}));
        }
        Ok(encoded(result))
    }
    #[wasm_bindgen(js_name=resolveOperation)]
    pub fn resolve_operation(&self, name: &str) -> Result<Option<WasmOperation>, JsValue> {
        self.document
            .resolve_operation(name)
            .map(|op| {
                op.map(|op| WasmOperation {
                    document: self.document.clone(),
                    op,
                })
            })
            .map_err(|e| error("interpretation", e))
    }
    #[wasm_bindgen(js_name=dependencyAcceptsKind)]
    pub fn dependency_accepts_kind(
        &self,
        dependency: &str,
        kind: &str,
    ) -> Result<Option<bool>, JsValue> {
        self.document
            .dependency_accepts_kind(dependency, kind)
            .map_err(|e| error("interpretation", e))
    }
    pub fn references(
        &self,
        resources: &WasmResources,
        cancelled: bool,
    ) -> Result<String, JsValue> {
        self.document
            .references_with_resources(resources.resources.clone(), &control(cancelled))
            .map(encoded)
            .map_err(|e| error("interpretation", e))
    }
    pub fn contracts(
        &self,
        resources: &WasmResources,
        limits: &str,
        cache_capacity: Option<usize>,
    ) -> Result<WasmContracts, JsValue> {
        let limits: Limits =
            serde_json::from_str(limits).map_err(|e| error("invalid-evaluator-limits", e))?;
        self.document
            .value_contracts_with_options(
                Arc::new(DefaultEvaluator::with_limits(limits)),
                resources.resources.clone(),
                ValueContractOptions {
                    cache_capacity: cache_capacity
                        .unwrap_or_else(|| ValueContractOptions::default().cache_capacity),
                },
            )
            .map(|contracts| WasmContracts { contracts })
            .map_err(|e| error("interpretation", e))
    }
}
#[wasm_bindgen]
pub struct WasmOperation {
    document: ParsedDocument,
    op: OperationView,
}
#[wasm_bindgen]
impl WasmOperation {
    pub fn key(&self) -> String {
        self.op.key.clone()
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.op.value.clone(),
        }
    }
    pub fn bindings(&self) -> Result<String, JsValue> {
        self.document
            .operation_bindings(&self.op.key)
            .map(encoded)
            .map_err(|e| error("interpretation", e))
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
    ) -> Result<WasmPrepared, JsValue> {
        Ok(WasmPrepared {
            prepared: self.contracts.prepare_with_control(
                operation,
                side(side_name)?,
                &control(cancelled),
            ),
        })
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
