//! The fictional example.memory@1 protocol echoes exact JSON on a local channel.

// guide:setup:start
use openbindings::JsonValue;
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, InvocationError, InvocationSupport,
    PreflightOutcome, ProviderIo,
};
use std::{
    collections::BTreeMap,
    fmt,
    future::{Future, Ready, ready},
    pin::Pin,
};

pub const KIND: &str = "example.memory@1";

#[derive(Default)]
pub struct Memory;

#[derive(Debug)]
pub struct AdapterError(pub &'static str);
impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for AdapterError {}

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).expect("valid example JSON")
}
fn object<const N: usize>(fields: [(&str, JsonValue); N]) -> JsonValue {
    // The checked serializer preserves nested exact values, including wide numbers.
    JsonValue::from_serializable(&BTreeMap::from(fields)).expect("bounded example object")
}
fn string(text: &str) -> JsonValue {
    JsonValue::string(text).expect("scalar string")
}
fn matches(data: Option<JsonValue>, expected: &str) -> bool {
    data.is_some_and(|value| value.semantic_eq(&json(expected)) == Some(true))
}
fn source_ok(source: &openbindings::Source) -> bool {
    source.kind() == KIND && matches(source.data(), r#"{"channel":"local"}"#)
}
// guide:setup:end

// guide:invoke:start
fn admit(request: &BindingRequest) -> Result<(), InvocationError> {
    let binding = request
        .interface
        .binding(&request.binding)
        .ok_or_else(|| InvocationError::new("ERR_BINDING_NOT_FOUND", None).unwrap())?;
    if !source_ok(&binding.source()) || !matches(binding.data(), r#"{"mode":"echo"}"#) {
        // This kind defines a fixed code and never copies rejected data into errors.
        return Err(InvocationError::new("example.memory.invalid-data", None).unwrap());
    }
    Ok(())
}
impl BindingInvoker for Memory {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;

    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            admit(&request)?;
            while let Some(value) = io.next().await {
                io.send(value).await?;
            }
            Ok(())
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        ready(admit(&request).map(|()| PreflightOutcome::NoPrediction))
    }
}
impl InvocationSupport for Memory {
    fn can_invoke(&self, kind: &str) -> bool {
        kind == KIND
    }
}
// guide:invoke:end

// guide:describe:start
use openbindings_kind_description::{
    DescribeKindRequest, KindDescription, KindDescriptor, KindSummary,
};

const SUPPORTS: &str = r#"[
    "openbindings.binding-invoker.invokeBinding",
    "openbindings.source-puller.pullSource"
]"#;

impl KindDescriptor for Memory {
    type Error = AdapterError;
    type List<'a> = Ready<Result<Vec<KindSummary>, Self::Error>>;
    type Describe<'a> = Ready<Result<KindDescription, Self::Error>>;

    fn handle_list_kinds(&self, _: CancellationToken) -> Self::List<'_> {
        ready(Ok(vec![
            KindSummary::from_json(object([
                ("kind", string(KIND)),
                ("supports", json(SUPPORTS)),
            ]))
            .unwrap(),
        ]))
    }
    fn handle_describe_kind(
        &self,
        request: DescribeKindRequest,
        _: CancellationToken,
    ) -> Self::Describe<'_> {
        if request.kind().as_str() != Some(KIND) {
            return ready(Err(AdapterError("unsupported kind")));
        }
        ready(Ok(KindDescription::from_json(object([
            ("kind", string(KIND)),
            ("supports", json(SUPPORTS)),
            ("description", string("Echo exact JSON through a local channel.")),
            ("invoke", json(r#"{"description":"Echoes each input until input closes.","executes":false}"#)),
            ("pull", json(r#"{"description":"Reads a fixed local inventory.","executes":false}"#)),
            ("sourceData", json(r#"{"type":"object","properties":{"channel":{"const":"local"}},"required":["channel"],"additionalProperties":false}"#)),
            ("bindingData", json(r#"{"type":"object","properties":{"mode":{"const":"echo"}},"required":["mode"],"additionalProperties":false}"#)),
        ])).unwrap()))
    }
}
// guide:describe:end

// guide:pull:start
use openbindings_source_pull::{PullFailure, PullRequest, PullResult, SourcePuller};

impl SourcePuller for Memory {
    type Error = AdapterError;
    type Pull<'a> = Ready<Result<PullResult, PullFailure<Self::Error>>>;

    fn handle_pull_source(&self, request: PullRequest, _: CancellationToken) -> Self::Pull<'_> {
        ready(pull(&request).map_err(PullFailure::Operational))
    }
}

fn comparable(operation: &openbindings::Operation) -> bool {
    // Full profile normalization can fail even when the offered slot is absent.
    // This small adapter compares only absent or Boolean schemas.
    operation
        .input()
        .into_iter()
        .chain(operation.output())
        .all(|schema| schema.json().view().as_bool().is_some())
}

fn pull(request: &PullRequest) -> Result<PullResult, AdapterError> {
    let interface = request.interface();
    let source = interface
        .source(request.source())
        .ok_or(AdapterError("missing source"))?;
    if !source_ok(&source) {
        return Err(AdapterError("unsupported kind or source data"));
    }
    let mut existing = Vec::new();
    let mut bound = false;
    for binding in interface.bindings().into_iter().flatten() {
        if binding.source_key() != request.source() {
            continue;
        }
        let recognized = matches(binding.data(), r#"{"mode":"echo"}"#);
        bound |= recognized;
        let operation = binding.operation();
        let mut remove = Vec::new();
        if operation.input().is_some() {
            remove.push(string("input"));
        }
        if operation.output().is_some() {
            remove.push(string("output"));
        }
        let mut entry = BTreeMap::from([("binding", string(binding.key()))]);
        let status = if !recognized {
            "unmatched"
        } else if !comparable(&operation) {
            entry.insert("reasonCode", string("example.memory.object-schema"));
            entry.insert(
                "message",
                string("This adapter does not compare object schemas."),
            );
            "unassessed"
        } else if remove.is_empty() {
            "current"
        } else {
            // This fixed protocol offers no schemas. Removal is an explicit proposal.
            entry.insert("change", string("dangerous"));
            entry.insert(
                "proposal",
                object([("remove", JsonValue::from_serializable(&remove).unwrap())]),
            );
            "stale"
        };
        entry.insert("status", string(status));
        existing.push(JsonValue::from_serializable(&entry).unwrap());
    }
    let mut additions = json("{}");
    let mut added = Vec::new();
    let mut omitted = Vec::new();
    if !bound {
        let operation = interface.resolve_operation("echo");
        if let Some(operation) = &operation
            && !comparable(operation)
        {
            omitted.push(object([
                ("status", string("implementation-unsupported")),
                ("reasonCode", string("example.memory.object-schema")),
                (
                    "message",
                    string("This adapter does not compare object schemas."),
                ),
                ("item", string("operations/echo")),
                (
                    "existing",
                    string(&format!("operations/{}", operation.key())),
                ),
            ]));
        } else if let Some(operation) = &operation
            && (operation.input().is_some() || operation.output().is_some())
        {
            // Absent schemas differ from present schemas, even a Boolean true schema.
            omitted.push(object([
                ("status", string("conflict")),
                ("reasonCode", string("example.memory.schema-conflict")),
                (
                    "message",
                    string("The existing operation declares schemas."),
                ),
                ("item", string("operations/echo")),
                (
                    "existing",
                    string(&format!("operations/{}", operation.key())),
                ),
                ("offered", json("{}")),
            ]));
        } else {
            let key = format!("echo.{}", request.source());
            if interface.binding(&key).is_some() {
                return Err(AdapterError("generated binding key is occupied"));
            }
            let operation_key = operation.as_ref().map_or("echo", |value| value.key());
            let binding = object([
                ("operation", string(operation_key)),
                ("source", string(request.source())),
                ("data", json(r#"{"mode":"echo"}"#)),
            ]);
            let bindings = object([(&key, binding)]);
            additions = if operation.is_some() {
                object([("bindings", bindings)])
            } else {
                object([
                    ("operations", json(r#"{"echo":{}}"#)),
                    ("bindings", bindings),
                ])
            };
            let mut entry = BTreeMap::from([
                ("binding", string(&key)),
                ("operation", string(operation_key)),
            ]);
            if operation.is_some() {
                entry.insert("reused", json("true"));
            }
            added.push(JsonValue::from_serializable(&entry).unwrap());
        }
    }
    PullResult::from_json(object([
        ("additions", additions),
        (
            "report",
            object([
                ("exhaustive", json("true")),
                ("added", JsonValue::from_serializable(&added).unwrap()),
                ("existing", JsonValue::from_serializable(&existing).unwrap()),
                ("omitted", JsonValue::from_serializable(&omitted).unwrap()),
            ]),
        ),
    ]))
    .map_err(|_| AdapterError("invalid generated pull result"))
}
// guide:pull:end
