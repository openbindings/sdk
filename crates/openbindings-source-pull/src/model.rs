use crate::carrier::{self, CarrierError};
use openbindings::{JsonKind, JsonObject, JsonStringRef, JsonValue};
use std::fmt;

macro_rules! carrier {
    ($name:ident,$doc:literal) => {
        #[doc=$doc]
        #[derive(Clone)]
        pub struct $name(JsonObject);
        impl $name {
            /// Borrow the complete admitted object, including allowed metadata.
            pub fn value(&self) -> &JsonObject {
                &self.0
            }
            /// Retain exact original JSON without reconstruction or serialization.
            pub fn to_json(&self) -> JsonObject {
                self.0.clone()
            }
        }
        impl TryFrom<JsonValue> for $name {
            type Error = CarrierError;
            fn try_from(value: JsonValue) -> Result<Self, CarrierError> {
                Self::from_json(value)
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }
    };
}
macro_rules! finite {
    ($name:ident,$doc:literal,$field:literal,$($variant:ident=>$text:literal),+ $(,)?) => {
        #[doc=$doc]
        #[derive(Clone,Copy,Debug,PartialEq,Eq)]
        pub enum $name { $(#[doc=$text] $variant),+ }
        impl $name {
            fn read(value:openbindings::JsonRef<'_>)->Result<Self,CarrierError> {
                match value.as_str() { $(Some($text)=>Ok(Self::$variant),)+ _=>Err(CarrierError::FieldType($field)) }
            }
            /// Return the published carrier spelling.
            pub fn as_str(self)->&'static str { match self { $(Self::$variant=>$text),+ } }
        }
    };
}
finite!(ExistingStatus,"The producer's observation of an existing entry.","status",Current=>"current",Stale=>"stale",Unmatched=>"unmatched");
finite!(Change,"The kind's judgment of a stale proposal's effect, not an instruction to apply it.","change",Breaking=>"breaking",Dangerous=>"dangerous",NonBreaking=>"non-breaking");
finite!(OmittedStatus,"Why a source offering was omitted from this pull.","status",Excluded=>"excluded",Invalid=>"invalid",ImplementationUnsupported=>"implementation-unsupported",Conflict=>"conflict");

macro_rules! required_text {
    ($method:ident,$field:literal,$doc:literal) => {
        #[doc=$doc]
        pub fn $method(&self) -> JsonStringRef<'_> {
            self.0
                .value()
                .get($field)
                .and_then(|v| v.as_string())
                .expect("admitted field")
        }
    };
}
macro_rules! optional_text {
    ($method:ident,$field:literal,$doc:literal) => {
        #[doc=$doc]
        pub fn $method(&self) -> Option<JsonStringRef<'_>> {
            self.0.value().get($field).and_then(|v| v.as_string())
        }
    };
}
macro_rules! optional_value {
    ($method:ident,$field:literal,$doc:literal) => {
        #[doc=$doc]
        pub fn $method(&self) -> Option<JsonValue> {
            self.0.value().get($field).map(|v| v.to_owned())
        }
    };
}
macro_rules! uses_accessor {
    () => {
        /// Iterate exact dependency labels in authored order; absence differs from an empty list.
        pub fn uses(&self) -> Option<impl ExactSizeIterator<Item = JsonStringRef<'_>>> {
            self.0.value().get("uses").map(|values| {
                values
                    .elements()
                    .expect("admitted uses")
                    .map(|v| v.as_string().expect("admitted label"))
            })
        }
    };
}
carrier!(
    PullLoss,
    "A producer's disclosed fidelity loss on an added binding."
);
impl PullLoss {
    /// Admit the open loss carrier without verifying its evidence.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        carrier::string(carrier::field(&object, "reasonCode")?, "reasonCode", true)?;
        carrier::string(carrier::field(&object, "message")?, "message", false)?;
        carrier::details(&object)?;
        Ok(Self(object))
    }
    required_text!(reason_code, "reasonCode", "Borrow the exact reason code.");
    required_text!(
        message,
        "message",
        "Borrow the readable explanation, which may be empty."
    );
    optional_value!(
        details,
        "details",
        "Retain optional kind-specific evidence."
    );
}
carrier!(
    PullLimitation,
    "A producer's explanation of why its source inventory is not exhaustive."
);
impl PullLimitation {
    /// Admit the open limitation carrier; code and message must be nonempty.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        for name in ["code", "message"] {
            carrier::string(carrier::field(&object, name)?, name, true)?;
        }
        carrier::details(&object)?;
        Ok(Self(object))
    }
    required_text!(code, "code", "Borrow the exact limitation code.");
    required_text!(
        message,
        "message",
        "Borrow the nonempty limitation explanation."
    );
    optional_value!(
        details,
        "details",
        "Retain optional kind-specific limitation evidence."
    );
}
carrier!(
    BindingProposal,
    "Proposed binding data and/or operation schemas; author-owned fields are excluded."
);
impl BindingProposal {
    /// Admit an open proposal carrying data, input or output. Known author-owned
    /// fields are forbidden; retained unknown metadata gains no edit authority.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        if carrier::forbidden(
            &object,
            &[
                "operation",
                "source",
                "description",
                "preference",
                "idempotent",
                "deprecated",
                "aliases",
                "examples",
                "tags",
            ],
        ) {
            return Err(CarrierError::ProposalAuthority);
        }
        if !["data", "input", "output"]
            .iter()
            .any(|name| object.value().get(name).is_some())
        {
            return Err(CarrierError::EmptyBindingProposal);
        }
        for name in ["input", "output"] {
            if let Some(schema) = object.value().get(name) {
                carrier::schema(schema, name)?;
            }
        }
        carrier::uses(&object)?;
        Ok(Self(object))
    }
    optional_value!(
        data,
        "data",
        "Retain proposed binding data, distinguishing absence from null."
    );
    optional_value!(
        input,
        "input",
        "Retain an optional proposed input schema without proving schema validity."
    );
    optional_value!(
        output,
        "output",
        "Retain an optional proposed output schema without proving schema validity."
    );
    uses_accessor!();
}
carrier!(
    SourceProposal,
    "Proposed replacement source data; changing the kind or description is not authorized."
);
impl SourceProposal {
    /// Admit required data, including null, and retain other allowed metadata.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        carrier::field(&object, "data")?;
        if carrier::forbidden(&object, &["kind", "description"]) {
            return Err(CarrierError::ProposalAuthority);
        }
        Ok(Self(object))
    }
    /// Retain the required proposed source data exactly.
    pub fn data(&self) -> JsonValue {
        self.0
            .value()
            .get("data")
            .expect("admitted data")
            .to_owned()
    }
}
/// A proposal's target determines what its payload means. Schema contents stay opaque.
#[derive(Clone, Debug)]
pub enum Proposal {
    /// Only binding data, operation input/output and named additions may be applied.
    Binding(BindingProposal),
    /// The exact whole schema, including a Boolean or empty object.
    Schema(JsonValue),
    /// Only source data may be replaced.
    Source(SourceProposal),
}
/// One role-specific existing-entry key, borrowed without normalization.
#[derive(Clone, Copy)]
pub enum ExistingTarget<'a> {
    /// An existing binding naming the selected source.
    Binding(JsonStringRef<'a>),
    /// An existing schema offered under its current key.
    Schema(JsonStringRef<'a>),
    /// The selected source itself.
    Source(JsonStringRef<'a>),
}
impl fmt::Debug for ExistingTarget<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Binding(_) => "ExistingTarget::Binding { .. }",
            Self::Schema(_) => "ExistingTarget::Schema { .. }",
            Self::Source(_) => "ExistingTarget::Source { .. }",
        })
    }
}
carrier!(
    ExistingEntry,
    "A current, stale or unmatched observation of one existing document entry."
);
impl ExistingEntry {
    /// Admit the target/status/proposal relationships, retaining open metadata.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        let mut target = None;
        for name in ["binding", "schema", "source"] {
            if let Some(value) = object.value().get(name) {
                carrier::string(value, name, true)?;
                if target.replace(name).is_some() {
                    return Err(CarrierError::ExistingTarget);
                }
            }
        }
        let target = target.ok_or(CarrierError::ExistingTarget)?;
        let status = ExistingStatus::read(carrier::field(&object, "status")?)?;
        match status {
            ExistingStatus::Current => {
                if carrier::forbidden(
                    &object,
                    &["proposal", "change", "reasonCode", "message", "details"],
                ) {
                    return Err(CarrierError::StatusFields);
                }
            }
            ExistingStatus::Unmatched => {
                if carrier::forbidden(&object, &["proposal", "change"]) {
                    return Err(CarrierError::StatusFields);
                }
            }
            ExistingStatus::Stale => {
                Change::read(carrier::field(&object, "change")?)?;
                let proposal = carrier::field(&object, "proposal")?;
                match target {
                    "binding" => {
                        BindingProposal::from_json(proposal.to_owned())?;
                    }
                    "source" => {
                        SourceProposal::from_json(proposal.to_owned())?;
                    }
                    _ => carrier::schema(proposal, "proposal")?,
                }
            }
        }
        carrier::optional_string(&object, "reasonCode", true)?;
        carrier::optional_string(&object, "message", false)?;
        carrier::details(&object)?;
        Ok(Self(object))
    }
    /// Read the exact role and key this observation addresses.
    pub fn target(&self) -> ExistingTarget<'_> {
        for (name, role) in [("binding", 0), ("schema", 1), ("source", 2)] {
            if let Some(value) = self.0.value().get(name) {
                let key = value.as_string().expect("admitted key");
                return match role {
                    0 => ExistingTarget::Binding(key),
                    1 => ExistingTarget::Schema(key),
                    _ => ExistingTarget::Source(key),
                };
            }
        }
        unreachable!("admitted target")
    }
    /// Read the producer's observation status.
    pub fn status(&self) -> ExistingStatus {
        ExistingStatus::read(self.0.value().get("status").expect("status"))
            .expect("admitted status")
    }
    /// Read the kind's change judgment, present only for a stale entry.
    pub fn change(&self) -> Option<Change> {
        self.0
            .value()
            .get("change")
            .map(|v| Change::read(v).expect("admitted change"))
    }
    /// Retain a role-specific proposal without applying it or interpreting schema members as report fields.
    pub fn proposal(&self) -> Option<Proposal> {
        let value = self.0.value().get("proposal")?.to_owned();
        Some(match self.target() {
            ExistingTarget::Binding(_) => {
                Proposal::Binding(BindingProposal(value.as_object().expect("proposal")))
            }
            ExistingTarget::Schema(_) => Proposal::Schema(value),
            ExistingTarget::Source(_) => {
                Proposal::Source(SourceProposal(value.as_object().expect("proposal")))
            }
        })
    }
    optional_text!(
        reason_code,
        "reasonCode",
        "Borrow optional kind-specific reason code."
    );
    optional_text!(
        message,
        "message",
        "Borrow the optional explanation, preserving absent versus empty."
    );
    optional_value!(
        details,
        "details",
        "Retain optional evidence without giving it address semantics."
    );
}
carrier!(
    AddedEntry,
    "The report entry for one newly added binding and its operation."
);
impl AddedEntry {
    /// Admit one added report entry; correspondence with additions is a client check.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        for name in ["binding", "operation"] {
            carrier::string(carrier::field(&object, name)?, name, true)?;
        }
        if let Some(value) = object.value().get("reused")
            && value.as_bool() != Some(true)
        {
            return Err(CarrierError::FieldType("reused"));
        }
        carrier::uses(&object)?;
        if let Some(value) = object.value().get("lossy") {
            PullLoss::from_json(value.to_owned())?;
        }
        Ok(Self(object))
    }
    required_text!(binding, "binding", "Borrow the added binding key.");
    required_text!(
        operation,
        "operation",
        "Borrow the operation key; aliases do not address this field."
    );
    /// Whether the producer claims reuse of an existing operation.
    pub fn reused(&self) -> bool {
        self.0.value().get("reused").is_some()
    }
    /// Retain optional disclosed loss without treating it as proof of fidelity.
    pub fn lossy(&self) -> Option<PullLoss> {
        self.0
            .value()
            .get("lossy")
            .map(|v| PullLoss(v.to_owned().as_object().expect("loss")))
    }
    uses_accessor!();
}
carrier!(
    OmittedEntry,
    "An offered source item omitted under a rule, invalidity, implementation gap or conflict."
);
impl OmittedEntry {
    /// Admit an open omission carrier; excluded requires rule, and other statuses
    /// may retain one. Details do not become a reusable item address.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        let status = OmittedStatus::read(carrier::field(&object, "status")?)?;
        carrier::string(carrier::field(&object, "reasonCode")?, "reasonCode", true)?;
        carrier::string(carrier::field(&object, "message")?, "message", false)?;
        if status == OmittedStatus::Excluded {
            carrier::field(&object, "rule")?;
        }
        carrier::optional_string(&object, "rule", true)?;
        carrier::details(&object)?;
        Ok(Self(object))
    }
    /// Read the reason category, distinct from a source-check rejection.
    pub fn status(&self) -> OmittedStatus {
        OmittedStatus::read(self.0.value().get("status").expect("status")).expect("admitted status")
    }
    required_text!(
        reason_code,
        "reasonCode",
        "Borrow the stable kind/implementation reason code."
    );
    required_text!(
        message,
        "message",
        "Borrow the readable explanation, which may be empty."
    );
    optional_text!(
        rule,
        "rule",
        "Borrow the rule evidence required for an excluded item."
    );
    optional_value!(
        details,
        "details",
        "Retain kind-specific evidence; it is not an item address."
    );
}
carrier!(
    PullReport,
    "The producer's report of this pull, including explicit gaps and proposals."
);
impl PullReport {
    /// Admit report shape and its exhaustive/limitation relationship.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        let exhaustive = carrier::field(&object, "exhaustive")?
            .as_bool()
            .ok_or(CarrierError::FieldType("exhaustive"))?;
        if !exhaustive {
            carrier::field(&object, "limitation")?;
        }
        if let Some(value) = object.value().get("limitation") {
            PullLimitation::from_json(value.to_owned())?;
        }
        for entry in carrier::field(&object, "added")?
            .elements()
            .ok_or(CarrierError::FieldType("added"))?
        {
            AddedEntry::from_json(entry.to_owned())?;
        }
        if let Some(value) = object.value().get("existing") {
            for entry in value
                .elements()
                .ok_or(CarrierError::FieldType("existing"))?
            {
                ExistingEntry::from_json(entry.to_owned())?;
            }
        }
        if let Some(value) = object.value().get("omitted") {
            for entry in value.elements().ok_or(CarrierError::FieldType("omitted"))? {
                OmittedEntry::from_json(entry.to_owned())?;
            }
        }
        Ok(Self(object))
    }
    /// Whether the producer says it read the whole source.
    pub fn exhaustive(&self) -> bool {
        self.0
            .value()
            .get("exhaustive")
            .and_then(|v| v.as_bool())
            .expect("exhaustive")
    }
    /// Retain an optional limitation; it may also accompany an exhaustive report.
    pub fn limitation(&self) -> Option<PullLimitation> {
        self.0
            .value()
            .get("limitation")
            .map(|v| PullLimitation(v.to_owned().as_object().expect("limitation")))
    }
    /// Iterate added binding claims in the producer's original order.
    pub fn added(&self) -> impl ExactSizeIterator<Item = AddedEntry> {
        self.0
            .value()
            .get("added")
            .expect("added")
            .elements()
            .expect("array")
            .map(|v| AddedEntry(v.to_owned().as_object().expect("entry")))
    }
    /// Iterate optional existing observations; absence stays distinct from empty.
    pub fn existing(&self) -> Option<impl ExactSizeIterator<Item = ExistingEntry>> {
        self.0.value().get("existing").map(|v| {
            v.elements()
                .expect("array")
                .map(|v| ExistingEntry(v.to_owned().as_object().expect("entry")))
        })
    }
    /// Iterate optional omitted offerings; absence stays distinct from empty.
    pub fn omitted(&self) -> Option<impl ExactSizeIterator<Item = OmittedEntry>> {
        self.0.value().get("omitted").map(|v| {
            v.elements()
                .expect("array")
                .map(|v| OmittedEntry(v.to_owned().as_object().expect("entry")))
        })
    }
    /// Apply exactly the report condition: exhaustive and no implementation gap.
    /// This checks the report's claim, not what the producer actually read.
    pub fn is_complete(&self) -> bool {
        self.exhaustive()
            && self.omitted().is_none_or(|mut entries| {
                entries.all(|entry| entry.status() != OmittedStatus::ImplementationUnsupported)
            })
    }
}
carrier!(
    PullAdditions,
    "New operations, schemas and bindings, retained separately from the original document."
);
impl PullAdditions {
    /// Admit open additions carriers. New-key and core conformance relationships
    /// require the original document and are checked by the client.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        for name in ["operations", "schemas", "bindings"] {
            if let Some(value) = object.value().get(name) {
                let map = carrier::object(value.to_owned())?;
                for member in map.value().view().members().expect("map") {
                    match name {
                        "operations" => {
                            if member.value.kind() != JsonKind::Object {
                                return Err(CarrierError::FieldType("operations"));
                            }
                        }
                        "schemas" => carrier::schema(member.value, "schemas")?,
                        _ => carrier::binding(member.value.to_owned())?,
                    }
                }
            }
        }
        Ok(Self(object))
    }
    /// Retain new operation carriers; these are not canonical core Operation proofs.
    pub fn operations(&self) -> Option<JsonObject> {
        self.0
            .value()
            .get("operations")
            .map(|v| v.to_owned().as_object().expect("map"))
    }
    /// Retain new exact Boolean/object schema entries.
    pub fn schemas(&self) -> Option<JsonObject> {
        self.0
            .value()
            .get("schemas")
            .map(|v| v.to_owned().as_object().expect("map"))
    }
    /// Retain new binding carriers without interpreting their data.
    pub fn bindings(&self) -> Option<JsonObject> {
        self.0
            .value()
            .get("bindings")
            .map(|v| v.to_owned().as_object().expect("map"))
    }
}
carrier!(
    PullResult,
    "Exact additions and report for a single source pull; no edit is performed by admission."
);
impl PullResult {
    /// Admit the open result and its carrier relationships. This alone does not
    /// establish consistency with an input document or a producer's truthfulness.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = carrier::object(value)?;
        PullAdditions::from_json(carrier::field(&object, "additions")?.to_owned())?;
        PullReport::from_json(carrier::field(&object, "report")?.to_owned())?;
        Ok(Self(object))
    }
    /// Retain additions independently of this result owner.
    pub fn additions(&self) -> PullAdditions {
        PullAdditions(
            self.0
                .value()
                .get("additions")
                .expect("additions")
                .to_owned()
                .as_object()
                .expect("object"),
        )
    }
    /// Retain the complete report independently of this result owner.
    pub fn report(&self) -> PullReport {
        PullReport(
            self.0
                .value()
                .get("report")
                .expect("report")
                .to_owned()
                .as_object()
                .expect("object"),
        )
    }
}
