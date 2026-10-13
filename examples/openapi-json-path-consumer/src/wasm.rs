use super::*;
use dynamic_openapi_client::{Cancellation, HostCapabilities, Outcome, PreparedRequest};
use openbindings::{DocumentBuilder, WorkControl};
use std::{cell::Cell, rc::Rc, task::Poll};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
#[wasm_bindgen]
pub async fn exact_roundtrip(
    kind: &str,
    description: &str,
    input: &str,
    response: &str,
) -> Result<String, JsValue> {
    let saved = author(kind, description).await;
    let output = response.to_owned();
    let calls = Rc::new(Cell::new(0));
    let recorded = calls.clone();
    let provider =
        OpenApi::default().with_transport(move |request: PreparedRequest, cancel: Cancellation| {
            let output = output.clone();
            let recorded = recorded.clone();
            async move {
                // A genuine JS suspension with an Rc host proves a local, non-Send seam.
                JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL))
                    .await
                    .unwrap();
                request
                    .invoke(
                        cancel,
                        &HostCapabilities::programmable(),
                        move |_, _| async move {
                            recorded.set(recorded.get() + 1);
                            reply(&output)
                        },
                    )
                    .await
            }
        });
    let facade = invoker(&saved, provider);
    let parsed = ParsedDocument::parse(&saved).unwrap();
    let draft = DocumentBuilder::from_json(parsed.value()).unwrap();
    let name = draft.operations.keys().next().unwrap();
    let result = call(&facade, operation::Request::operation(name), input)
        .await
        .map_err(|e| JsValue::from_str(e.code()))?;
    assert_eq!(calls.get(), 1);
    Ok(result.unwrap())
}
#[wasm_bindgen]
pub fn logical_owners() -> String {
    let (sdk, client) = owners();
    format!("{sdk},{client}")
}
#[wasm_bindgen]
pub async fn pending_cleanup(kind: &str, description: &str, abandon: bool) -> Result<(), JsValue> {
    struct Guard(Rc<Cell<usize>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(self.0.get() - 1);
        }
    }
    let saved = author(kind, description).await;
    let live = Rc::new(Cell::new(0));
    let registered = live.clone();
    let provider = OpenApi::default().with_transport(
        move |_request: PreparedRequest, _cancel: Cancellation| {
            let registered = registered.clone();
            async move {
                registered.set(registered.get() + 1);
                let _guard = Guard(registered);
                let promise = js_sys::Promise::new(&mut |_resolve, _reject| {});
                let _ = JsFuture::from(promise).await;
                std::future::pending::<Outcome>().await
            }
        },
    );
    let facade = invoker(&saved, provider);
    let parsed = ParsedDocument::parse(&saved).unwrap();
    let draft = DocumentBuilder::from_json(parsed.value()).unwrap();
    let name = draft.operations.keys().next().unwrap();
    let control = WorkControl::new();
    let mut job = facade.invoke_with_control(operation::Request::operation(name), &control);
    job.write(JsonValue::parse(INPUT).unwrap()).await.unwrap();
    job.close();
    {
        let mut next = std::pin::pin!(job.next());
        std::future::poll_fn(|cx| {
            assert!(next.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    }
    assert_eq!(live.get(), 1);
    if !abandon {
        control.cancel();
        assert_eq!(
            job.next().await.unwrap().unwrap_err().code(),
            "ERR_CANCELLED"
        );
    }
    drop(job);
    assert_eq!(live.get(), 0);
    Ok(())
}
