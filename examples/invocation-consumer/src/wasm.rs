use super::Catalog;
use openbindings::{Document, ExternalSchemas, JsonValue};
use openbindings_invocation::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_source_pull::{PullLimits, PullRequest, SourcePullerExt};
use source_pull_consumer::Fixed;
use std::future::Future;
use wasm_bindgen::prelude::*;

fn failure(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Exercise actual kind descriptions for the host's independent Unicode controls.
#[wasm_bindgen(js_name = describeLocalKinds)]
pub async fn describe_local_kinds() -> Result<String, JsValue> {
    use openbindings_kind_description::{DescribeKindRequest, KindDescriptorExt};
    let client = source_pull_consumer::KindCatalog::default();
    let listed = client
        .list_kinds(CancellationToken::new())
        .await
        .map_err(failure)?;
    let mut descriptions = Vec::new();
    for kind in listed {
        let request = DescribeKindRequest::from_json(
            JsonValue::parse(format!(
                r#"{{"kind":{}}}"#,
                kind.value().value().get("kind").unwrap().text()
            ))
            .map_err(failure)?,
        )
        .map_err(failure)?;
        let description = client
            .describe_kind(request, CancellationToken::new())
            .await
            .map_err(failure)?;
        descriptions.push(description.value().value().text().to_owned());
    }
    Ok(format!("[{}]", descriptions.join(",")))
}
async fn exchange<F: Future<Output = Result<(), InvocationError>>>(
    call: Invocation<F>,
    value: JsonValue,
) -> Result<String, JsValue> {
    let (mut input, mut output) = call.split();
    let sending = async {
        let result = input.send(value).await;
        input.close();
        result
    };
    let receiving = async {
        let mut value = None;
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Output(output) => value = Some(output),
                OutputEvent::Complete => {
                    return value
                        .map(|v: JsonValue| v.text().to_owned())
                        .ok_or_else(|| JsValue::from_str("missing output"));
                }
                OutputEvent::Error(error) => return Err(failure(error)),
                _ => {}
            }
        }
        Err(JsValue::from_str("missing terminal"))
    };
    let (sent, received) = futures_util::future::join(sending, receiving).await;
    let received = received?;
    sent.map_err(failure)?;
    Ok(received)
}

/// The host explicitly chooses whether to apply operation value schemas.
#[wasm_bindgen(js_name = pullAdoptAndEcho)]
pub async fn pull_adopt_and_echo(text: String, operation: bool) -> Result<String, JsValue> {
    let original = Document::parse(r#"{"openbindings":"0.2.0","operations":{},"sources":{"fixed":{"kind":"example.fixed@1"}}}"#).map_err(failure)?.validate().map_err(failure)?.conformant().unwrap();
    let request = PullRequest::new(original, "fixed");
    let result = (Fixed)
        .pull_source(request.clone(), CancellationToken::new())
        .await
        .map_err(failure)?;
    let adopted = result
        .validate_for(&request, &PullLimits::default(), &CancellationToken::new())
        .map_err(failure)?;
    let value = JsonValue::parse(&text).map_err(failure)?;
    if operation {
        let client = DefaultOperationInvoker::new(
            Catalog::default(),
            DefaultEvaluator::new(),
            ExternalSchemas::default(),
        );
        exchange(
            client
                .invoke(
                    OperationRequest::operation(adopted, "echo"),
                    CancellationToken::new(),
                )
                .map_err(failure)?,
            value,
        )
        .await
    } else {
        let client = Catalog::default();
        exchange(
            client
                .invoke(
                    BindingRequest::new(adopted, "echo.fixed"),
                    CancellationToken::new(),
                )
                .map_err(failure)?,
            value,
        )
        .await
    }
}
