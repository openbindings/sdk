//! All companion carrier admission through public APIs.
use openbindings::{JsonLimits, JsonRef, JsonValue};
use openbindings_context::{Context, ContextChallenge};
use openbindings_invocation::{
    BindingRequest, InvocationError, OperationAddress, OperationRequest,
};
use openbindings_kind_description as kind;
use openbindings_source_pull as pull;
use std::fmt::{Debug, Display};

/// Number of first-byte decoder selectors, also checked by stable replay.
pub const CARRIER_COUNT: usize = 26;

// A finite fixture of SDK diagnostic words and constants. Numeric coordinates
// are permitted; caller text outside that vocabulary is not. The explicit
// private marker also catches copying into otherwise plausible SDK sentences.
fn redacted(error: &(impl Display + Debug)) {
    let vocabulary = include_str!("diagnostic_words.txt");
    for text in [error.to_string(), format!("{error:?}")] {
        assert!(
            !text.contains("FUZZ_PRIVATE"),
            "diagnostic disclosed caller marker"
        );
        for word in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
            assert!(
                word.is_empty()
                    || word.chars().all(|c| c.is_ascii_digit())
                    || vocabulary.split_whitespace().any(|fixed| fixed == word),
                "diagnostic contains non-SDK text"
            );
        }
    }
}
fn context(value: &Context) {
    let _ = value.value();
    let _ = value.clone().into_value();
}
fn challenge(value: &ContextChallenge) {
    let _ = value.message();
    let _ = value.schema();
    let _ = value.value();
}
fn binding(value: &BindingRequest) {
    let _ = value.interface.value();
    let _ = &value.binding;
    if let Some(value) = &value.context {
        context(value);
    }
}
fn operation(value: &OperationRequest) {
    let _ = value.interface.value();
    match &value.address {
        OperationAddress::Operation { name, selection } => {
            let _ = (name, selection);
        }
        OperationAddress::Binding(key) => {
            let _ = key;
        }
    }
    if let Some(value) = &value.context {
        context(value);
    }
    value.admit().unwrap();
}
fn invocation_error(value: &InvocationError) {
    let _ = value.code();
    let _ = value.data();
    if let Some(value) = value.context_required() {
        challenge(&value);
    }
    redacted(value);
}
fn loss(value: &pull::PullLoss) {
    let _ = (
        value.value(),
        value.reason_code(),
        value.message(),
        value.details(),
    );
}
fn limitation(value: &pull::PullLimitation) {
    let _ = (
        value.value(),
        value.code(),
        value.message(),
        value.details(),
    );
}
fn binding_proposal(value: &pull::BindingProposal) {
    let _ = (value.value(), value.data(), value.input(), value.output());
    if let Some(v) = value.remove() {
        for _ in v {}
    }
    if let Some(v) = value.uses() {
        for _ in v {}
    }
}
fn source_proposal(value: &pull::SourceProposal) {
    let _ = (value.value(), value.data());
    if let Some(v) = value.remove() {
        for _ in v {}
    }
}
fn schema_proposal(value: &pull::SchemaProposal) {
    let _ = (value.value(), value.schema());
}
fn existing(value: &pull::ExistingEntry) {
    let _ = (
        value.value(),
        value.target(),
        value.status().as_str(),
        value.change().map(|v| v.as_str()),
        value.reason_code(),
        value.message(),
        value.details(),
    );
    if let Some(v) = value.references() {
        for _ in v {}
    }
    if let Some(proposal) = value.proposal() {
        match proposal {
            pull::Proposal::Binding(v) => binding_proposal(&v),
            pull::Proposal::Source(v) => source_proposal(&v),
            pull::Proposal::Schema(v) => schema_proposal(&v),
        }
    }
}
fn added(value: &pull::AddedEntry) {
    let _ = (
        value.value(),
        value.binding(),
        value.operation(),
        value.reused(),
    );
    if let Some(v) = value.uses() {
        for _ in v {}
    }
    if let Some(v) = value.lossy() {
        for v in v {
            loss(&v);
        }
    }
}
fn omitted(value: &pull::OmittedEntry) {
    let _ = (
        value.value(),
        value.status().as_str(),
        value.item(),
        value.existing(),
        value.offered(),
        value.identifier(),
        value.reason_code(),
        value.message(),
        value.rule(),
        value.details(),
    );
    if let Some(v) = value.also_identifies() {
        for _ in v {}
    }
    if let Some(v) = value.through() {
        for _ in v {}
    }
}
fn report(value: &pull::PullReport) {
    let _ = (value.value(), value.exhaustive(), value.is_complete());
    if let Some(v) = value.limitation() {
        limitation(&v);
    }
    for v in value.added() {
        added(&v);
    }
    if let Some(v) = value.existing() {
        for v in v {
            existing(&v);
        }
    }
    if let Some(v) = value.omitted() {
        for v in v {
            omitted(&v);
        }
    }
}
fn additions(value: &pull::PullAdditions) {
    let _ = (
        value.value(),
        value.operations(),
        value.schemas(),
        value.bindings(),
    );
}
fn result(value: &pull::PullResult) {
    let _ = value.value();
    additions(&value.additions());
    report(&value.report());
}
fn summary(value: &kind::KindSummary) {
    let _ = (value.value(), value.kind(), value.description());
    for _ in value.supports() {}
}
fn check_description(value: &kind::CheckDescription) {
    let _ = (value.value(), value.description(), value.executes());
}
fn pull_description(value: &kind::PullDescription) {
    let _ = (value.value(), value.description(), value.executes());
}
fn invoke_description(value: &kind::InvokeDescription) {
    let _ = (value.value(), value.description(), value.executes());
}
fn description(value: &kind::KindDescription) {
    let _ = (
        value.value(),
        value.kind(),
        value.description(),
        value.source_data(),
        value.binding_data(),
    );
    for _ in value.supports() {}
    if let Some(v) = value.check() {
        check_description(&v);
    }
    if let Some(v) = value.pull() {
        pull_description(&v);
    }
    if let Some(v) = value.invoke() {
        invoke_description(&v);
    }
}
fn describe_request(value: &kind::DescribeKindRequest) {
    let _ = (value.value(), value.kind());
}
fn check_request(value: &kind::CheckSourceRequest) {
    let _ = (value.value(), value.kind(), value.data());
    if let Some(v) = value.context() {
        context(&v);
    }
}
fn checked_source(value: &kind::CheckedSource) {
    let _ = (
        value.value(),
        value.kind(),
        value.data(),
        value.description(),
    );
}
fn rejection(value: &kind::CheckRejection) {
    let _ = (value.value(), value.code(), value.message(), value.data());
}

// Closed invocation encoders may reorder envelope fields, so compare every
// carried value independently, then require exact encoding to be idempotent.
fn same_fields(original: JsonRef<'_>, encoded: JsonRef<'_>) {
    assert_eq!(
        original.members().unwrap().len(),
        encoded.members().unwrap().len()
    );
    for field in original.members().unwrap() {
        let again = encoded.get(field.name.as_str().unwrap()).unwrap();
        if field.value.to_owned().has_duplicate_names() {
            assert_eq!(field.value.text(), again.text());
        } else {
            assert_eq!(
                field.value.to_owned().semantic_eq(&again.to_owned()),
                Some(true)
            );
        }
        // Opaque nested owners must keep numeric spellings and string escapes.
        if field.name.as_str() != Some("selection")
            && (field.value.members().is_some() || field.value.elements().is_some())
        {
            assert_eq!(field.value.text(), again.text());
        }
    }
}

/// First byte selects one of every public companion decoder. The remainder is
/// exact JSON, including malformed text, duplicate names and shape boundaries.
pub fn carriers(data: &[u8]) -> bool {
    if data.len() > crate::MAX_INPUT || data.is_empty() {
        return false;
    }
    // Bound stack use in the harness, independently of production defaults.
    let Ok(input) = JsonValue::parse_with_limits(
        &data[1..],
        JsonLimits {
            max_depth: 64,
            ..JsonLimits::default()
        },
    ) else {
        return false;
    };
    macro_rules! opaque {
        ($ty:ty, $visit:ident) => {{
            match <$ty>::from_json(input.clone()) {
                Ok(value) => {
                    $visit(&value);
                    let encoded =
                        JsonValue::parse(serde_json::to_vec(value.value().value()).unwrap())
                            .unwrap();
                    let again = <$ty>::from_json(encoded).unwrap();
                    $visit(&again);
                    assert_eq!(value.value().value().text(), input.text());
                    assert_eq!(again.value().value().text(), input.text());
                    true
                }
                Err(error) => {
                    redacted(&error);
                    false
                }
            }
        }};
    }
    macro_rules! invocation {
        ($ty:ty, $visit:ident) => {{
            match <$ty>::from_json(&input) {
                Ok(value) => {
                    $visit(&value);
                    let encoded = value.to_json().unwrap();
                    same_fields(input.view(), encoded.view());
                    let again = <$ty>::from_json(&encoded).unwrap();
                    $visit(&again);
                    assert_eq!(again.to_json().unwrap().text(), encoded.text());
                    assert_eq!(
                        value
                            .to_json_with_limits(JsonLimits::default())
                            .unwrap()
                            .text(),
                        encoded.text()
                    );
                    if let Err(error) = value.to_json_with_limits(JsonLimits {
                        max_bytes: 0,
                        ..JsonLimits::default()
                    }) {
                        redacted(&error);
                    }
                    true
                }
                Err(error) => {
                    redacted(&error);
                    false
                }
            }
        }};
    }
    match usize::from(data[0]) % CARRIER_COUNT {
        0 => match Context::try_from(input.clone()) {
            Ok(value) => {
                context(&value);
                let again = Context::new(value.clone().into_value()).unwrap();
                context(&again);
                assert_eq!(again.value().value().text(), input.text());
                true
            }
            Err(error) => {
                redacted(&error);
                false
            }
        },
        1 => match ContextChallenge::try_from(input.clone()) {
            Ok(value) => {
                challenge(&value);
                let again = ContextChallenge::from_json(value.value().clone()).unwrap();
                challenge(&again);
                assert_eq!(again.value().value().text(), input.text());
                true
            }
            Err(error) => {
                redacted(&error);
                false
            }
        },
        2 => invocation!(BindingRequest, binding),
        3 => invocation!(OperationRequest, operation),
        4 => invocation!(InvocationError, invocation_error),
        5 => match pull::PullRequest::from_json(&input) {
            Ok(value) => {
                let _ = (value.interface(), value.source());
                if let Some(v) = value.context() {
                    context(v);
                }
                true
            }
            Err(error) => {
                redacted(&error);
                false
            }
        },
        6 => opaque!(pull::PullLoss, loss),
        7 => opaque!(pull::PullLimitation, limitation),
        8 => opaque!(pull::BindingProposal, binding_proposal),
        9 => opaque!(pull::SourceProposal, source_proposal),
        10 => opaque!(pull::SchemaProposal, schema_proposal),
        11 => opaque!(pull::ExistingEntry, existing),
        12 => opaque!(pull::AddedEntry, added),
        13 => opaque!(pull::OmittedEntry, omitted),
        14 => opaque!(pull::PullReport, report),
        15 => opaque!(pull::PullAdditions, additions),
        16 => opaque!(pull::PullResult, result),
        17 => opaque!(kind::KindSummary, summary),
        18 => opaque!(kind::CheckDescription, check_description),
        19 => opaque!(kind::PullDescription, pull_description),
        20 => opaque!(kind::InvokeDescription, invoke_description),
        21 => opaque!(kind::KindDescription, description),
        22 => opaque!(kind::DescribeKindRequest, describe_request),
        23 => opaque!(kind::CheckSourceRequest, check_request),
        24 => opaque!(kind::CheckedSource, checked_source),
        25 => opaque!(kind::CheckRejection, rejection),
        _ => unreachable!(),
    }
}
