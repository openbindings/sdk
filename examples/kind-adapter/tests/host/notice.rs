//! A second fictional kind: one fixed notice, with source checking but no pull.
use kind_adapter::AdapterError;
use openbindings::JsonValue;
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, ContextChallenge, InvocationError,
    InvocationSupport, ProviderIo,
};
use openbindings_kind_description::{
    CheckFailure, CheckSourceRequest, CheckedSource, DescribeKindRequest, KindDescription,
    KindDescriptor, KindSummary, SourceChecker,
};
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
};

pub const KIND: &str = "example.notice@1";
pub struct Notice;
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

fn object<const N: usize>(fields: [(&str, JsonValue); N]) -> JsonValue {
    JsonValue::from_serializable(&std::collections::BTreeMap::from(fields)).unwrap()
}
fn supports() -> JsonValue {
    JsonValue::from_serializable(&[
        openbindings_invocation::INVOKE_BINDING,
        openbindings_kind_description::CHECK_SOURCE,
    ])
    .unwrap()
}

impl KindDescriptor for Notice {
    type Error = AdapterError;
    type List<'a> = Ready<Result<Vec<KindSummary>, Self::Error>>;
    type Describe<'a> = Ready<Result<KindDescription, Self::Error>>;
    fn handle_list_kinds(&self, _: CancellationToken) -> Self::List<'_> {
        ready(Ok(vec![
            KindSummary::from_json(object([
                ("kind", JsonValue::string(KIND).unwrap()),
                ("supports", supports()),
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
            ("kind", JsonValue::string(KIND).unwrap()),
            ("supports", supports()),
            (
                "description",
                JsonValue::string("One local notice.").unwrap(),
            ),
            (
                "invoke",
                json(r#"{"description":"Returns one fixed notice.","executes":false}"#),
            ),
            (
                "check",
                json(r#"{"description":"Checks a fixed local source.","executes":false}"#),
            ),
        ]))
        .unwrap()))
    }
}
impl SourceChecker for Notice {
    type Error = AdapterError;
    type Check<'a> = Ready<Result<CheckedSource, CheckFailure<Self::Error>>>;
    fn handle_check_source(
        &self,
        request: CheckSourceRequest,
        _: CancellationToken,
    ) -> Self::Check<'_> {
        if request.kind().as_str() != Some(KIND) || request.data().is_some() {
            return ready(Err(CheckFailure::Operational(AdapterError(
                "unsupported source",
            ))));
        }
        // Context never enters the returned entry.
        ready(Ok(CheckedSource::from_json(json(
            r#"{"kind":"example.notice@1"}"#,
        ))
        .unwrap()))
    }
}
impl InvocationSupport for Notice {
    fn can_invoke(&self, kind: &str) -> bool {
        kind == KIND
    }
}
impl BindingInvoker for Notice {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            admit(&request)?;
            io.close_input();
            io.send(JsonValue::integer(1)).await
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        ready(admit(&request).map(|()| None))
    }
}
fn admit(request: &BindingRequest) -> Result<(), InvocationError> {
    let binding = request
        .interface
        .binding(&request.binding)
        .ok_or_else(InvocationError::binding_not_found)?;
    if binding.source().kind() != KIND
        || binding.source().data().is_some()
        || binding.data().is_some()
    {
        return Err(InvocationError::new("example.notice.invalid-data", None).unwrap());
    }
    Ok(())
}
