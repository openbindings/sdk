#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator, Limits};
use serde_json::json;
use std::cell::Cell;
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
// JSON text for named fields. Exact pointer fields use the private adapter
// explicitly; ordinary SDK serialization always produces scalar strings.
macro_rules! object {
    ($($key:literal: $value:expr),* $(,)?) => {{
        let mut text = String::from("{");
        $(
            if text.len() > 1 {
                text.push(',');
            }
            text.push_str(&encoded($key));
            text.push(':');
            text.push_str(&encoded(&$value));
        )*
        text.push('}');
        text
    }};
}
mod exact;

// A schema location as exact JSON text, so its pointer keeps unpaired units.
fn schema_location(text: &str) -> Result<SchemaOrigin, JsValue> {
    let invalid = || {
        error(
            "invalid-schema-location",
            "expected a resource URI or null and an RFC 6901 pointer",
        )
    };
    let value = JsonValue::parse(text).map_err(|_| invalid())?;
    let resource = match value.get("source_uri") {
        Some(resource) if resource.kind() == JsonKind::Null => None,
        Some(resource) => Some(resource.as_str().ok_or_else(invalid)?.to_owned()),
        None => return Err(invalid()),
    };
    let pointer = value
        .get("pointer")
        .and_then(openbindings_internal_json::backend::string_pointer)
        .ok_or_else(invalid)?;
    Ok(SchemaOrigin::new(resource, pointer))
}
fn error(code: &str, message: impl ToString) -> JsValue {
    JsValue::from_str(&encoded(json!({"code":code,"message":message.to_string()})))
}
fn input_error(e: InputError) -> JsValue {
    JsValue::from_str(&encoded(
        json!({"code":e.code(),"kind":format!("{:?}",e.kind()),"byte_offset":e.byte_offset(),"message":e.to_string()}),
    ))
}
// Admission limits as the facade passes them: bytes, nodes and nesting, each a
// checked 32-bit integer; the facade fills omitted entries from `jsonLimits`.
fn admission(limits: &[u32]) -> Result<JsonLimits, JsValue> {
    let invalid = || error("invalid-json-limits", "expected bytes, nodes and depth");
    let &[max_bytes, max_nodes, max_depth] = limits else {
        return Err(invalid());
    };
    let size = |value: u32| usize::try_from(value).map_err(|_| invalid());
    Ok(JsonLimits {
        max_bytes: size(max_bytes)?,
        max_nodes: size(max_nodes)?,
        max_depth: size(max_depth)?,
    })
}
fn admission_or_default(limits: Option<Vec<u32>>) -> Result<JsonLimits, JsValue> {
    limits.map_or(Ok(JsonLimits::default()), |limits| admission(&limits))
}
// A document's admission limits for the facade, which bounds ordinary encoding
// for its edits by them.
fn limit_values(limits: JsonLimits) -> Vec<u32> {
    [limits.max_bytes, limits.max_nodes, limits.max_depth]
        .map(|value| u32::try_from(value).unwrap_or(u32::MAX))
        .to_vec()
}
// Absent limits mean the defaults, so ordinary preparation parses nothing.
fn evaluator(limits: Option<String>) -> Result<DefaultEvaluator, JsValue> {
    let limits: Limits = match limits {
        Some(text) => {
            serde_json::from_str(&text).map_err(|e| error("invalid-evaluator-limits", e))?
        }
        None => Limits::default(),
    };
    Ok(DefaultEvaluator::with_limits(limits))
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
    /// The bindings copy the bytes into module memory once; the value then
    /// keeps that copy as its source.
    #[wasm_bindgen(js_name=parseBytes)]
    pub fn parse_bytes(bytes: Vec<u8>, limits: Option<Vec<u32>>) -> Result<WasmJson, JsValue> {
        JsonValue::parse_owned_with_limits(bytes, admission_or_default(limits)?)
            .map(|value| Self { value })
            .map_err(input_error)
    }
    /// As [`Self::parse_bytes`], for text: the bindings encode it into module
    /// memory directly, so the facade never holds a copy of its UTF-8.
    #[wasm_bindgen(js_name=parseText)]
    pub fn parse_text(text: String, limits: Option<Vec<u32>>) -> Result<WasmJson, JsValue> {
        Self::parse_bytes(text.into_bytes(), limits)
    }
    #[must_use]
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
    /// Resolve a pointer, given as an exact JSON string, as `locate` reads it;
    /// a malformed pointer selects nothing, as it does for `at`.
    #[wasm_bindgen(js_name=atPointer)]
    pub fn at_pointer(&self, pointer: &str) -> Option<WasmJson> {
        let pointer = JsonValue::parse(pointer)
            .ok()
            .and_then(|text| openbindings_internal_json::backend::string_pointer(text.view()))?;
        self.value.at_pointer(&pointer).map(|v| Self {
            value: v.to_owned(),
        })
    }
    /// Locate a pointer, given as an exact JSON string, relative to this value.
    pub fn locate(&self, pointer: &str) -> Result<String, JsValue> {
        let pointer = JsonValue::parse(pointer)
            .ok()
            .and_then(|text| openbindings_internal_json::backend::string_pointer(text.view()))
            .ok_or_else(|| error("invalid-pointer", "expected an RFC 6901 pointer"))?;
        Ok(encoded(exact::json(&self.value.locate(&pointer))))
    }
    pub fn get(&self, name: &str) -> Option<WasmJson> {
        self.value.get(name).map(|v| Self {
            value: v.to_owned(),
        })
    }
    pub fn metadata(&self) -> String {
        object!(
            "duplicate_names": self.value.has_duplicate_names(),
            "kind": format!("{:?}", self.value.kind()).to_lowercase(),
            "location": exact::json(&self.value.location()),
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
    #[must_use]
    pub fn retain(&self) -> WasmMember {
        Self {
            index: self.index,
            name: self.name.clone(),
            value: self.value.clone(),
        }
    }
}
// wasm-bindgen permits receiver substitution between these transparent wrappers.
// Keep their layout identical, but make authority and mutability Rust enum states.
// A proven revision contains no editable Document, even under receiver substitution.
enum DocumentRevision {
    Editable(Document),
    ConformantDocument(ConformantDocument),
}
struct DocumentHandle {
    revision: DocumentRevision,
    taken: Cell<bool>,
}
impl DocumentHandle {
    fn new(document: Document) -> Self {
        Self {
            revision: DocumentRevision::Editable(document),
            taken: Cell::new(false),
        }
    }
    fn document(&self) -> &Document {
        match &self.revision {
            DocumentRevision::Editable(document) => document,
            DocumentRevision::ConformantDocument(proof) => proof.document(),
        }
    }
    fn editable_document(&mut self) -> Option<&mut Document> {
        match &mut self.revision {
            DocumentRevision::Editable(document) => Some(document),
            DocumentRevision::ConformantDocument(_) => None,
        }
    }
    fn editable(&mut self) -> Result<&mut Document, JsValue> {
        self.editable_document().ok_or_else(|| {
            error(
                "immutable-document",
                "A conformant revision cannot be edited.",
            )
        })
    }
    // The facade edits a newly retained handle. Replace its immutable snapshot
    // only after the complete batch succeeds; proof handles cannot be edited.
    fn edit(
        &mut self,
        apply: impl FnOnce(&Document) -> Result<Document, EditError>,
    ) -> Result<(), JsValue> {
        let next = apply(self.editable()?).map_err(edit_error)?;
        self.revision = DocumentRevision::Editable(next);
        Ok(())
    }
    fn into_document(self) -> Document {
        match self.revision {
            DocumentRevision::Editable(document) => document,
            DocumentRevision::ConformantDocument(proof) => proof.into_inner(),
        }
    }
    fn proof(&self, code: &str) -> Result<ConformantDocument, JsValue> {
        match &self.revision {
            DocumentRevision::ConformantDocument(proof) => Ok(proof.clone()),
            DocumentRevision::Editable(document) => document
                .validate()
                .ok()
                .and_then(|v| v.conformant())
                .ok_or_else(|| error(code, "A conformant document is required.")),
        }
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
    /// The bindings copy the bytes into module memory once; the document then
    /// keeps that copy as its text.
    #[wasm_bindgen(js_name=parseBytes)]
    pub fn parse_bytes(bytes: Vec<u8>, limits: Option<Vec<u32>>) -> Result<WasmDocument, JsValue> {
        Document::parse_owned_with_limits(bytes, admission_or_default(limits)?)
            .map(|document| Self {
                handle: DocumentHandle::new(document),
            })
            .map_err(input_error)
    }
    /// As [`Self::parse_bytes`], for text encoded into module memory directly.
    #[wasm_bindgen(js_name=parseText)]
    pub fn parse_text(text: String, limits: Option<Vec<u32>>) -> Result<WasmDocument, JsValue> {
        Self::parse_bytes(text.into_bytes(), limits)
    }
    /// The admission limits this revision keeps: bytes, nodes and nesting.
    pub fn limits(&self) -> Vec<u32> {
        limit_values(self.handle.document().limits())
    }
    /// Create an editable empty OBI.
    pub fn create() -> WasmDocument {
        Self {
            handle: DocumentHandle::new(Document::new()),
        }
    }
    /// One bridge call for ordinary fragments interleaved with exact values.
    /// The facade temporarily transfers each distinct exact owner once. Every
    /// outcome returns those owners, including refusals, before exposing errors.
    /// A batch comes with `envelope`, the UTF-8 bytes and nodes of its own
    /// structure (its array, rows, operations and pointers) beside the values,
    /// so the document's limits apply to what the edit writes, as in Rust.
    #[wasm_bindgen(js_name=editEncoded)]
    pub fn edit_encoded(
        &mut self,
        chunks: Vec<String>,
        indices: Vec<u32>,
        values: Vec<WasmJson>,
        envelope: Vec<u32>,
    ) -> Vec<JsValue> {
        let result = (|| {
            let limits = self.handle.editable()?.limits();
            if chunks.len() != indices.len() + 1 {
                return Err(error("invalid-edit", "invalid exact fragment layout"));
            }
            let &[bytes, nodes] = envelope.as_slice() else {
                return Err(error("invalid-edit", "invalid batch envelope"));
            };
            let envelope = Envelope {
                bytes: bytes as usize,
                nodes: nodes as usize,
            };
            // What the edit writes must fit the document's byte limit; the
            // batch's own structure is beside it.
            let limit = limits.max_bytes.saturating_add(envelope.bytes);
            let mut size = 0usize;
            for length in chunks
                .iter()
                .map(String::len)
                .chain(indices.iter().map(|index| {
                    values
                        .get(*index as usize)
                        .map_or(usize::MAX, |value| value.value.text().len())
                }))
            {
                size = size
                    .checked_add(length)
                    .filter(|size| *size <= limit)
                    .ok_or_else(|| {
                        edit_error(EditError::Input(
                            openbindings_internal_json::backend::input_error(
                                InputErrorKind::Limit,
                                "byte-limit",
                                limits.max_bytes,
                            ),
                        ))
                    })?;
            }
            // Preflight complete UTF-8 extent before allocating the joined input.
            let mut text = String::with_capacity(size);
            for (chunk, index) in chunks.iter().zip(&indices) {
                text.push_str(chunk);
                text.push_str(values[*index as usize].value.text());
            }
            text.push_str(chunks.last().expect("fragment layout was checked"));
            self.edit_batch(&text, envelope)
        })();
        let mut output = Vec::with_capacity(values.len() + 1);
        output.push(result.err().unwrap_or(JsValue::NULL));
        for value in values {
            output.push(JsValue::from(value));
        }
        output
    }
    pub fn set(&mut self, pointer: &str, value: &WasmJson) -> Result<(), JsValue> {
        self.handle
            .edit(|document| document.set(pointer, &value.value))
    }
    pub fn remove(&mut self, pointer: &str) -> Result<(), JsValue> {
        self.handle.edit(|document| document.remove(pointer))
    }
    #[must_use]
    pub fn retain(&self) -> WasmDocument {
        Self {
            handle: DocumentHandle::new(self.handle.document().clone()),
        }
    }
    pub fn value(&self) -> WasmJson {
        WasmJson {
            value: self.handle.document().value().clone(),
        }
    }
    #[wasm_bindgen(js_name=toBytes)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.handle.document().as_bytes().into()
    }
    pub fn validate(&self) -> WasmDocumentValidation {
        let _ = self.handle.document().validate();
        WasmDocumentValidation {
            handle: DocumentHandle::new(self.handle.document().clone()),
        }
    }
    pub fn references(&self, resources: &WasmResources, cancelled: bool) -> String {
        encoded(exact::json(
            &self
                .handle
                .document()
                .references_with_resources(resources.resources.clone(), &control(cancelled)),
        ))
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
        match self.handle.document().validate() {
            Ok(v) => object!("report": exact::json(v.report()), "status": "report"),
            Err(e) => encoded(json!({"status":"version-refused", "refusal":e})),
        }
    }
    #[wasm_bindgen(js_name=takeConformant)]
    pub fn take_conformant(&self) -> Option<WasmConformantDocument> {
        if self.handle.taken.replace(true) {
            return None;
        }
        self.handle
            .document()
            .validate()
            .ok()?
            .conformant()
            .map(WasmConformantDocument::from_proof)
    }
}
/// Immutable revision owning its established conformance proof.
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
    fn from_proof(document: ConformantDocument) -> Self {
        Self {
            handle: DocumentHandle {
                revision: DocumentRevision::ConformantDocument(document),
                taken: Cell::new(false),
            },
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
            handle: DocumentHandle::new(self.handle.into_document()),
        }
    }
    #[wasm_bindgen(js_name=asDocument)]
    pub fn document(&self) -> WasmDocument {
        WasmDocument {
            handle: DocumentHandle::new(self.handle.document().clone()),
        }
    }
    pub fn value(&self) -> WasmJson {
        self.document().value()
    }
    #[wasm_bindgen(js_name=toBytes)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.document().to_bytes()
    }
    /// The admission limits this revision keeps: bytes, nodes and nesting.
    pub fn limits(&self) -> Vec<u32> {
        limit_values(self.handle.document().limits())
    }
    pub fn validate(&self) -> WasmDocumentValidation {
        self.document().validate()
    }
    /// Locate a schema location, given as exact JSON text, in this document.
    pub fn locate(&self, location: &str) -> Result<String, JsValue> {
        let proof = self.handle.proof("document-conformance")?;
        Ok(encoded(exact::json(
            &proof.locate(&schema_location(location)?),
        )))
    }
    pub fn openbindings(&self) -> Result<String, JsValue> {
        Ok(encoded(
            self.handle.proof("document-conformance")?.openbindings(),
        ))
    }
    pub fn name(&self) -> Result<String, JsValue> {
        Ok(encoded(self.handle.proof("document-conformance")?.name()))
    }
    pub fn version(&self) -> Result<String, JsValue> {
        Ok(encoded(
            self.handle.proof("document-conformance")?.version(),
        ))
    }
    pub fn description(&self) -> Result<String, JsValue> {
        Ok(encoded(
            self.handle.proof("document-conformance")?.description(),
        ))
    }
    #[wasm_bindgen(js_name=schemaNames)]
    pub fn schema_names(&self) -> Result<String, JsValue> {
        Ok(encoded(
            self.handle
                .proof("document-conformance")?
                .schemas()
                .map(|schemas| schemas.into_iter().map(|(key, _)| key).collect::<Vec<_>>()),
        ))
    }
    pub fn schema(&self, key: &str) -> Result<Option<WasmSchema>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .schema(key)
            .map(|schema| WasmSchema { schema }))
    }
    pub fn operations(&self) -> Result<Vec<WasmOperation>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .operations()
            .into_iter()
            .map(WasmOperation::from_view)
            .collect())
    }
    pub fn operation(&self, key: &str) -> Result<Option<WasmOperation>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .operation(key)
            .map(WasmOperation::from_view))
    }
    #[wasm_bindgen(js_name=resolveOperation)]
    pub fn resolve_operation(&self, name: &str) -> Result<Option<WasmOperation>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .resolve_operation(name)
            .map(WasmOperation::from_view))
    }
    pub fn bindings(&self) -> Result<Option<Vec<WasmBinding>>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .bindings()
            .map(|entries| entries.into_iter().map(WasmBinding::from_view).collect()))
    }
    pub fn binding(&self, key: &str) -> Result<Option<WasmBinding>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .binding(key)
            .map(WasmBinding::from_view))
    }
    pub fn sources(&self) -> Result<Option<Vec<WasmSource>>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .sources()
            .map(|entries| entries.into_iter().map(WasmSource::from_view).collect()))
    }
    pub fn source(&self, key: &str) -> Result<Option<WasmSource>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .source(key)
            .map(WasmSource::from_view))
    }
    pub fn dependencies(&self) -> Result<Option<Vec<WasmDependency>>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .dependencies()
            .map(|entries| entries.into_iter().map(WasmDependency::from_view).collect()))
    }
    pub fn dependency(&self, key: &str) -> Result<Option<WasmDependency>, JsValue> {
        Ok(self
            .handle
            .proof("document-conformance")?
            .dependency(key)
            .map(WasmDependency::from_view))
    }
    pub fn references(&self, resources: &WasmResources, cancelled: bool) -> String {
        self.document().references(resources, cancelled)
    }
}
impl WasmDocument {
    /// Apply an atomic array of {op, pointer, value?} edits. Values stay exact JSON.
    /// The batch is admitted under the document's limits widened by exactly its
    /// own structure: two levels (its array and a row), and the envelope's bytes
    /// and nodes. The edit itself then applies the document's limits to the
    /// edited document, as in Rust.
    fn edit_batch(&mut self, edits: &str, envelope: Envelope) -> Result<(), JsValue> {
        let limits = self.handle.editable()?.limits();
        let batch = JsonLimits {
            max_bytes: limits.max_bytes.saturating_add(envelope.bytes),
            max_nodes: limits.max_nodes.saturating_add(envelope.nodes),
            max_depth: limits.max_depth.saturating_add(2),
        };
        let input = JsonValue::parse_with_limits(edits, batch).map_err(admission_error)?;
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
                .filter(|v| v.kind() == JsonKind::String)
                .ok_or_else(|| error("invalid-pointer", "expected a pointer string"))?;
            // A pointer through a member name with an unpaired unit, as a
            // finding reports one, arrives as a JSON escape and stays exact.
            let pointer = match pointer.as_str() {
                Some(text) => Ok(text),
                None => Err(openbindings_internal_json::backend::string_pointer(pointer)
                    .ok_or_else(|| edit_error(EditError::InvalidPointer))?),
            };
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
            .map(|(pointer, value)| {
                let pointer = match pointer {
                    Ok(text) => PointerRef::from(*text),
                    Err(exact) => PointerRef::from(exact),
                };
                match value {
                    Some(value) => Edit::set_at(pointer, value.clone()),
                    None => Edit::remove_at(pointer),
                }
            })
            .collect();
        self.handle.edit(|document| document.edit(&edits))
    }
}
// The bytes and nodes of an edit batch's own structure, beside its values.
#[derive(Clone, Copy, Default)]
struct Envelope {
    bytes: usize,
    nodes: usize,
}
// An exceeded limit while admitting what an edit writes is the document's.
fn admission_error(error: InputError) -> JsValue {
    if error.kind() == InputErrorKind::Limit {
        edit_error(EditError::Input(error))
    } else {
        input_error(error)
    }
}
// Exported receivers in this family share a tagged representation. The Rust
// variant, not a JavaScript prototype, establishes which proved entry is read.
// This prevents a borrowed operation method from treating an example as a schema.
#[derive(Clone)]
enum DomainEntry {
    Operation(Operation),
    Source(Source),
    Binding(Binding),
    Dependency(Dependency),
    Example(Example),
}
#[wasm_bindgen]
#[repr(transparent)]
pub struct WasmOperation {
    entry: DomainEntry,
}
impl std::fmt::Debug for WasmOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmOperation").finish_non_exhaustive()
    }
}
impl WasmOperation {
    fn from_view(view: Operation) -> Self {
        Self {
            entry: DomainEntry::Operation(view),
        }
    }
    fn view(&self) -> Result<&Operation, JsValue> {
        match &self.entry {
            DomainEntry::Operation(view) => Ok(view),
            _ => Err(error(
                "invalid-domain-owner",
                "This method requires a operation owner.",
            )),
        }
    }
}
#[wasm_bindgen]
impl WasmOperation {
    pub fn retain(&self) -> Result<WasmOperation, JsValue> {
        Ok(Self::from_view(self.view()?.clone()))
    }
    pub fn document(&self) -> Result<WasmConformantDocument, JsValue> {
        Ok(WasmConformantDocument::from_proof(
            self.view()?.document().clone(),
        ))
    }
    pub fn value(&self) -> Result<WasmJson, JsValue> {
        Ok(WasmJson {
            value: self.view()?.value().to_owned(),
        })
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        let view = self.view()?;
        Ok(encoded(
            json!({"key":view.key(), "description":view.description(), "tags":view.tags().map(|items| items.collect::<Vec<_>>()), "aliases":view.aliases().map(|items| items.collect::<Vec<_>>()), "deprecated":view.deprecated()}),
        ))
    }
    pub fn input(&self) -> Result<Option<WasmSchema>, JsValue> {
        Ok(self.view()?.input().map(|schema| WasmSchema { schema }))
    }
    pub fn output(&self) -> Result<Option<WasmSchema>, JsValue> {
        Ok(self.view()?.output().map(|schema| WasmSchema { schema }))
    }
    pub fn bindings(&self) -> Result<Vec<WasmBinding>, JsValue> {
        Ok(self
            .view()?
            .bindings()
            .into_iter()
            .map(WasmBinding::from_view)
            .collect())
    }
    pub fn examples(&self) -> Result<Option<Vec<WasmExample>>, JsValue> {
        Ok(self
            .view()?
            .examples()
            .map(|entries| entries.into_iter().map(WasmExample::from_view).collect()))
    }
    pub fn example(&self, key: &str) -> Result<Option<WasmExample>, JsValue> {
        Ok(self.view()?.example(key).map(WasmExample::from_view))
    }
}
#[wasm_bindgen]
#[repr(transparent)]
pub struct WasmSource {
    entry: DomainEntry,
}
impl std::fmt::Debug for WasmSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmSource").finish_non_exhaustive()
    }
}
impl WasmSource {
    fn from_view(view: Source) -> Self {
        Self {
            entry: DomainEntry::Source(view),
        }
    }
    fn view(&self) -> Result<&Source, JsValue> {
        match &self.entry {
            DomainEntry::Source(view) => Ok(view),
            _ => Err(error(
                "invalid-domain-owner",
                "This method requires a source owner.",
            )),
        }
    }
}
#[wasm_bindgen]
impl WasmSource {
    pub fn retain(&self) -> Result<WasmSource, JsValue> {
        Ok(Self::from_view(self.view()?.clone()))
    }
    pub fn document(&self) -> Result<WasmConformantDocument, JsValue> {
        Ok(WasmConformantDocument::from_proof(
            self.view()?.document().clone(),
        ))
    }
    pub fn value(&self) -> Result<WasmJson, JsValue> {
        Ok(WasmJson {
            value: self.view()?.value().to_owned(),
        })
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        let view = self.view()?;
        Ok(encoded(
            json!({"key":view.key(), "kind":view.kind(), "description":view.description()}),
        ))
    }
    pub fn data(&self) -> Result<Option<WasmJson>, JsValue> {
        Ok(self.view()?.data().map(|value| WasmJson { value }))
    }
}
#[wasm_bindgen]
#[repr(transparent)]
pub struct WasmBinding {
    entry: DomainEntry,
}
impl std::fmt::Debug for WasmBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmBinding").finish_non_exhaustive()
    }
}
impl WasmBinding {
    fn from_view(view: Binding) -> Self {
        Self {
            entry: DomainEntry::Binding(view),
        }
    }
    fn view(&self) -> Result<&Binding, JsValue> {
        match &self.entry {
            DomainEntry::Binding(view) => Ok(view),
            _ => Err(error(
                "invalid-domain-owner",
                "This method requires a binding owner.",
            )),
        }
    }
}
#[wasm_bindgen]
impl WasmBinding {
    pub fn retain(&self) -> Result<WasmBinding, JsValue> {
        Ok(Self::from_view(self.view()?.clone()))
    }
    pub fn document(&self) -> Result<WasmConformantDocument, JsValue> {
        Ok(WasmConformantDocument::from_proof(
            self.view()?.document().clone(),
        ))
    }
    pub fn value(&self) -> Result<WasmJson, JsValue> {
        Ok(WasmJson {
            value: self.view()?.value().to_owned(),
        })
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        let view = self.view()?;
        Ok(encoded(
            json!({"key":view.key(), "operation":view.operation_key(), "source":view.source_key(), "description":view.description(), "preference":view.preference(), "idempotent":view.idempotent(), "deprecated":view.deprecated()}),
        ))
    }
    pub fn data(&self) -> Result<Option<WasmJson>, JsValue> {
        Ok(self.view()?.data().map(|value| WasmJson { value }))
    }
    pub fn operation(&self) -> Result<WasmOperation, JsValue> {
        Ok(WasmOperation::from_view(self.view()?.operation()))
    }
    pub fn source(&self) -> Result<WasmSource, JsValue> {
        Ok(WasmSource::from_view(self.view()?.source()))
    }
}
#[wasm_bindgen]
#[repr(transparent)]
pub struct WasmDependency {
    entry: DomainEntry,
}
impl std::fmt::Debug for WasmDependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDependency").finish_non_exhaustive()
    }
}
impl WasmDependency {
    fn from_view(view: Dependency) -> Self {
        Self {
            entry: DomainEntry::Dependency(view),
        }
    }
    fn view(&self) -> Result<&Dependency, JsValue> {
        match &self.entry {
            DomainEntry::Dependency(view) => Ok(view),
            _ => Err(error(
                "invalid-domain-owner",
                "This method requires a dependency owner.",
            )),
        }
    }
}
#[wasm_bindgen]
impl WasmDependency {
    pub fn retain(&self) -> Result<WasmDependency, JsValue> {
        Ok(Self::from_view(self.view()?.clone()))
    }
    pub fn document(&self) -> Result<WasmConformantDocument, JsValue> {
        Ok(WasmConformantDocument::from_proof(
            self.view()?.document().clone(),
        ))
    }
    pub fn value(&self) -> Result<WasmJson, JsValue> {
        Ok(WasmJson {
            value: self.view()?.value().to_owned(),
        })
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        let view = self.view()?;
        Ok(encoded(
            json!({"key":view.key(), "operation":view.operation_key(), "description":view.description(), "kinds":view.kinds().map(|items| items.collect::<Vec<_>>())}),
        ))
    }
    pub fn operation(&self) -> Result<WasmOperation, JsValue> {
        Ok(WasmOperation::from_view(self.view()?.operation()))
    }
    #[wasm_bindgen(js_name=acceptsKind)]
    pub fn accepts_kind(&self, text: &str) -> Result<bool, JsValue> {
        let view = self.view()?;
        // JSON escaping retains unpaired JavaScript units. No present scalar
        // filter can match them; an absent filter still accepts any name.
        let value = JsonValue::parse(text).map_err(input_error)?;
        let kind = value
            .view()
            .as_string()
            .ok_or_else(|| error("invalid-kind", "kind must be an encoded JSON string"))?;
        Ok(kind
            .as_str()
            .map_or_else(|| view.kinds().is_none(), |kind| view.accepts_kind(kind)))
    }
}
#[wasm_bindgen]
#[repr(transparent)]
pub struct WasmExample {
    entry: DomainEntry,
}
impl std::fmt::Debug for WasmExample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmExample").finish_non_exhaustive()
    }
}
impl WasmExample {
    fn from_view(view: Example) -> Self {
        Self {
            entry: DomainEntry::Example(view),
        }
    }
    fn view(&self) -> Result<&Example, JsValue> {
        match &self.entry {
            DomainEntry::Example(view) => Ok(view),
            _ => Err(error(
                "invalid-domain-owner",
                "This method requires a example owner.",
            )),
        }
    }
}
#[wasm_bindgen]
impl WasmExample {
    pub fn retain(&self) -> Result<WasmExample, JsValue> {
        Ok(Self::from_view(self.view()?.clone()))
    }
    pub fn document(&self) -> Result<WasmConformantDocument, JsValue> {
        Ok(WasmConformantDocument::from_proof(
            self.view()?.document().clone(),
        ))
    }
    pub fn value(&self) -> Result<WasmJson, JsValue> {
        Ok(WasmJson {
            value: self.view()?.value().to_owned(),
        })
    }
    pub fn metadata(&self) -> Result<String, JsValue> {
        let view = self.view()?;
        Ok(encoded(
            json!({"key":view.key(), "description":view.description()}),
        ))
    }
    pub fn input(&self) -> Result<Option<WasmJson>, JsValue> {
        Ok(self.view()?.input().map(|value| WasmJson { value }))
    }
    pub fn output(&self) -> Result<Option<WasmJson>, JsValue> {
        Ok(self.view()?.output().map(|value| WasmJson { value }))
    }
}
#[wasm_bindgen]
pub struct WasmResources {
    resources: ExternalSchemas,
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
            resources: ExternalSchemas::default(),
        }
    }
    pub fn add(&self, uri: &str, value: &WasmJson) -> Result<WasmResources, JsValue> {
        ExternalSchemas::new(self.resources.iter().cloned().chain([ExternalSchema {
            uri: uri.into(),
            document: value.value.clone(),
        }]))
        .map(|resources| Self { resources })
        .map_err(|e| error("resource", e))
    }
    #[must_use]
    pub fn retain(&self) -> WasmResources {
        Self {
            resources: self.resources.clone(),
        }
    }
    /// Locate a schema location, given as exact JSON text, in these resources.
    pub fn locate(&self, location: &str) -> Result<String, JsValue> {
        Ok(encoded(exact::json(
            &self.resources.locate(&schema_location(location)?),
        )))
    }
}
impl Default for WasmResources {
    fn default() -> Self {
        Self::new()
    }
}
/// Entries collected for one `ExternalSchemas::new`, so a set is admitted once
/// rather than rebuilt for each added resource.
#[wasm_bindgen]
#[derive(Default)]
pub struct WasmResourceEntries {
    resources: Vec<ExternalSchema>,
}
impl std::fmt::Debug for WasmResourceEntries {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmResourceEntries")
            .finish_non_exhaustive()
    }
}
#[wasm_bindgen]
impl WasmResourceEntries {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmResourceEntries {
        Self::default()
    }
    pub fn push(&mut self, uri: &str, value: &WasmJson) {
        self.resources.push(ExternalSchema {
            uri: uri.into(),
            document: value.value.clone(),
        });
    }
    /// Admit every collected entry as one set; the entries are taken.
    pub fn build(&mut self) -> Result<WasmResources, JsValue> {
        ExternalSchemas::new(std::mem::take(&mut self.resources))
            .map(|resources| WasmResources { resources })
            .map_err(|e| error("resource", e))
    }
}
/// One original contextual schema, retaining its proved document revision.
#[wasm_bindgen]
pub struct WasmSchema {
    schema: Schema,
}
impl std::fmt::Debug for WasmSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmSchema").finish_non_exhaustive()
    }
}
impl WasmSchema {
    fn prepare_in(
        &self,
        resources: &ExternalSchemas,
        limits: Option<String>,
        cancelled: bool,
    ) -> Result<WasmPreparation, JsValue> {
        let evaluator = evaluator(limits)?;
        Ok(WasmPreparation {
            preparation: Some(self.schema.prepare_with_control(
                &evaluator,
                resources,
                &openbindings::PreparationLimits::default(),
                &control(cancelled),
            )),
        })
    }
}
#[wasm_bindgen]
impl WasmSchema {
    pub fn json(&self) -> WasmJson {
        WasmJson {
            value: self.schema.json(),
        }
    }
    /// Prepare without supplied resources, sharing one empty set.
    pub fn prepare(
        &self,
        limits: Option<String>,
        cancelled: bool,
    ) -> Result<WasmPreparation, JsValue> {
        static EMPTY: std::sync::OnceLock<ExternalSchemas> = std::sync::OnceLock::new();
        self.prepare_in(
            EMPTY.get_or_init(ExternalSchemas::default),
            limits,
            cancelled,
        )
    }
    #[wasm_bindgen(js_name=prepareWith)]
    pub fn prepare_with(
        &self,
        resources: &WasmResources,
        limits: Option<String>,
        cancelled: bool,
    ) -> Result<WasmPreparation, JsValue> {
        self.prepare_in(&resources.resources, limits, cancelled)
    }
    #[must_use]
    pub fn retain(&self) -> WasmSchema {
        Self {
            schema: self.schema.clone(),
        }
    }
}
/// A private transfer wrapper. Dropping it drops any untaken prepared schema.
#[wasm_bindgen]
pub struct WasmPreparation {
    preparation: Option<Result<PreparedSchema<DefaultValidator>, PreparationFailure>>,
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
            Some(Ok(_)) => encoded(json!({"status":"ready"})),
            Some(Err(failure)) => object!("failure": exact::json(failure), "status": "refused"),
            None => encoded(json!({"status":"consumed"})),
        }
    }
    #[wasm_bindgen(js_name=takeSchema)]
    pub fn take_schema(&mut self) -> Option<WasmPrepared> {
        match self.preparation.take()? {
            Ok(prepared) => Some(WasmPrepared { prepared }),
            Err(_) => None,
        }
    }
}
#[wasm_bindgen]
pub struct WasmPrepared {
    prepared: PreparedSchema<DefaultValidator>,
}
impl std::fmt::Debug for WasmPrepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmPrepared").finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl WasmPrepared {
    pub fn evaluate(&self, value: &WasmJson, cancelled: bool) -> String {
        encoded(exact::json(
            &self
                .prepared
                .evaluate_with_control(&value.value, &control(cancelled)),
        ))
    }
    pub fn explain(&self, value: &WasmJson, cancelled: bool) -> String {
        encoded(exact::json(
            &self
                .prepared
                .explain_with_control(&value.value, &control(cancelled)),
        ))
    }
    #[wasm_bindgen(js_name=evaluateJson)]
    pub fn evaluate_json(&self, bytes: &[u8], cancelled: bool) -> Result<String, JsValue> {
        self.prepared
            .evaluate_json_with_control(bytes, &control(cancelled))
            .map(|outcome| encoded(exact::json(&outcome)))
            .map_err(input_error)
    }
    /// As [`Self::evaluate_json`], for text encoded into module memory directly.
    #[wasm_bindgen(js_name=evaluateJsonText)]
    pub fn evaluate_json_text(&self, text: &str, cancelled: bool) -> Result<String, JsValue> {
        self.evaluate_json(text.as_bytes(), cancelled)
    }
    #[must_use]
    pub fn retain(&self) -> WasmPrepared {
        Self {
            prepared: self.prepared.clone(),
        }
    }
}
/// Without limits, as `validate_document` admits: the default bytes and nodes, any
/// nesting. With them, as `validate_document_with_limits` does. The bindings copy
/// the bytes into module memory once; a document the validation keeps holds that
/// copy as its text.
#[wasm_bindgen(js_name=validateBytes)]
pub fn validate_bytes(bytes: Vec<u8>, limits: Option<Vec<u32>>) -> Result<String, JsValue> {
    let validation = match limits {
        None => validate_document_owned(bytes),
        Some(limits) => validate_document_owned_with_limits(bytes, admission(&limits)?),
    };
    Ok(match validation {
        Ok(a) => object!("report": exact::json(a.report()), "status": "report"),
        Err(e) => encoded(json!({"status":"version-refused","refusal":e})),
    })
}
/// As [`validate_bytes`], for text encoded into module memory directly.
#[wasm_bindgen(js_name=validateText)]
pub fn validate_text(text: String, limits: Option<Vec<u32>>) -> Result<String, JsValue> {
    validate_bytes(text.into_bytes(), limits)
}
/// The default admission limits: bytes, nodes and nesting.
#[wasm_bindgen(js_name=jsonLimits)]
pub fn json_limits() -> Vec<u32> {
    limit_values(JsonLimits::default())
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
    // The discovery client's own judgment of a complete body: one validation.
    validation: Result<DocumentValidation, VersionRefusal>,
}
impl std::fmt::Debug for WasmDiscovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDiscovery").finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl WasmDiscovery {
    /// Judge a complete body as Rust discovery does: under admission limits that
    /// admit the body bound when it exceeds the default byte limit, any nesting.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8], max_document_bytes: u32) -> WasmDiscovery {
        let max_bytes = usize::try_from(max_document_bytes).unwrap_or(usize::MAX);
        let limits = JsonLimits {
            max_bytes: max_bytes.max(JsonLimits::default().max_bytes),
            max_depth: usize::MAX,
            ..JsonLimits::default()
        };
        Self {
            validation: openbindings::validate_document_with_limits(bytes, limits),
        }
    }
    pub fn result(&self) -> String {
        match &self.validation {
            Ok(validation) => object!(
                "report": exact::json(validation.report()),
                "status": match validation.report().conclusion {
                    Conformance::Conformant => "found",
                    Conformance::NonConformant => "non-conformant",
                    Conformance::Undetermined => "undetermined",
                },
            ),
            Err(refusal) => encoded(json!({"status":"version-refused","refusal":refusal})),
        }
    }
    pub fn document(&self) -> Option<WasmDocument> {
        let document = self.validation.as_ref().ok()?.document()?;
        Some(WasmDocument {
            handle: DocumentHandle::new(document.clone()),
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

#[cfg(test)]
mod proof_mutation_tests {
    use super::*;
    #[test]
    fn proof_revision_has_no_mutable_document_even_in_an_editor_wrapper() {
        // The shared ABI layout does not grant editing authority.
        let document =
            Document::parse(r#"{"openbindings":"0.2.0","name":"certified","operations":{}}"#)
                .unwrap();
        let proof =
            WasmConformantDocument::from_proof(document.validate().unwrap().conformant().unwrap());
        let before = proof.to_bytes();
        let mut substituted = WasmDocument {
            handle: proof.handle,
        };
        assert!(substituted.handle.editable_document().is_none());
        assert_eq!(substituted.to_bytes(), before);
        let proof = WasmConformantDocument {
            handle: substituted.handle,
        };
        let mut independent = proof.document();
        independent
            .handle
            .edit(|document| {
                document.edit(&[Edit::set(
                    &["name"],
                    JsonValue::parse(r#""changed""#).unwrap(),
                )])
            })
            .unwrap();
        assert_eq!(proof.to_bytes(), before);
        let mut consumed = proof.into_inner();
        consumed
            .handle
            .edit(|document| {
                document.edit(&[Edit::set(
                    &["name"],
                    JsonValue::parse(r#""edited""#).unwrap(),
                )])
            })
            .unwrap();
    }
}

#[cfg(test)]
mod location_transfer_tests {
    use super::*;
    #[test]
    fn reports_and_locations_keep_pointers_through_unpaired_names() {
        // A JSON tree cannot hold this finding's pointer; the bridge writes
        // its exact escape, which a JavaScript host parses back exactly.
        let text = r#"{"openbindings":"0.2.0","operations":{},"x":{"\ud800":{"k":0,"k":1}}}"#;
        let report = validate_bytes(text.as_bytes().to_vec(), None).unwrap();
        assert!(report.starts_with(r#"{"report":{"#));
        assert!(report.contains(r#""pointer":"/x/\ud800/k""#), "{report}");
        let value = WasmJson::parse_text(text.into(), None).unwrap();
        let name = value
            .value
            .at("/x")
            .unwrap()
            .members()
            .unwrap()
            .next()
            .unwrap();
        let metadata = WasmJson {
            value: name.value.to_owned(),
        }
        .metadata();
        assert!(metadata.contains(r#""pointer":"/x/\ud800""#), "{metadata}");
    }
    #[test]
    fn instance_pointers_are_located_on_demand_as_exact_text() {
        let value = WasmJson::parse_text("{\"\\ud800\":[1,\n\"x\"]}".into(), None).unwrap();
        assert_eq!(
            value.locate(r#""/\ud800/1""#).unwrap(),
            r#"{"pointer":"/\ud800/1","byte_offset":14,"line":2,"byte_column":1}"#
        );
        assert_eq!(value.locate(r#""/x""#).unwrap(), "null");
    }
    #[test]
    fn schema_locations_cross_as_exact_text() {
        let uri = "https://example.test/schema";
        let resources = WasmResources::new()
            .add(
                uri,
                &WasmJson::parse_text(r#"{"$defs":{"\ud800":{"type":"string"}}}"#.into(), None)
                    .unwrap(),
            )
            .unwrap();
        let located = resources
            .locate(&format!(
                r#"{{"source_uri":"{uri}","pointer":"/$defs/\ud800/type"}}"#
            ))
            .unwrap();
        assert_eq!(
            located,
            r#"{"pointer":"/$defs/\ud800/type","byte_offset":27,"line":1,"byte_column":28}"#
        );
        assert_eq!(
            resources
                .locate(r#"{"source_uri":null,"pointer":"/$defs"}"#)
                .unwrap(),
            "null"
        );
        let document = Document::parse(
            "{\"openbindings\":\"0.2.0\",\n\"operations\":{\"op\":{\"input\":{\"type\":\"string\"}}}}",
        )
        .unwrap();
        let proof =
            WasmConformantDocument::from_proof(document.validate().unwrap().conformant().unwrap());
        assert_eq!(
            proof
                .locate(r#"{"source_uri":null,"pointer":"/operations/op/input/type"}"#)
                .unwrap(),
            r#"{"pointer":"/operations/op/input/type","byte_offset":61,"line":2,"byte_column":37}"#
        );
    }
}
