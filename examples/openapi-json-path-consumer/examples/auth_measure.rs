//! Registered, interleaved CPU comparisons. Manufactured credentials only.
use dynamic_openapi_client as client;
use openapi_json_path_consumer::{reply, source};
use openbindings::{Binding, DocumentBuilder, JsonValue, Operation, ParsedDocument};
use openbindings_invocation::{BindingRequest, BindingValue, binding, operation};
use openbindings_openapi::OpenApi;
use std::{
    cell::Cell,
    collections::BTreeMap,
    future::Future,
    hint::black_box,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
struct Oracle {
    active: Cell<bool>,
    calls: Cell<usize>,
    headers: Vec<(&'static str, String)>,
    body: Option<String>,
}
impl Oracle {
    fn record(&self, wire: &client::DispatchRequest) {
        if !self.active.get() {
            return;
        }
        self.calls.set(self.calls.get() + 1);
        assert_eq!(wire.target, "https://measure.invalid/p0");
        assert_eq!(
            wire.body.as_deref(),
            self.body.as_deref().map(str::as_bytes)
        );
        for name in ["authorization", "x-key"] {
            let actual: Vec<_> = wire
                .headers
                .iter()
                .filter(|h| h.name().eq_ignore_ascii_case(name))
                .collect();
            let expected = self.headers.iter().find(|(n, _)| *n == name);
            assert_eq!(actual.len(), usize::from(expected.is_some()));
            if let Some((_, value)) = expected {
                assert_eq!(actual[0].value(), value.as_bytes());
            }
        }
    }
    fn finish(&self) {
        if self.active.replace(false) {
            assert_eq!(self.calls.get(), 1);
        }
    }
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        _ => panic!("ready host"),
    }
}
fn sample(f: &mut dyn FnMut()) -> f64 {
    f();
    let start = Instant::now();
    let mut n = 0;
    while start.elapsed() < Duration::from_millis(50) {
        f();
        n += 1;
    }
    start.elapsed().as_nanos() as f64 / n as f64
}
fn description(version: &str, method: &str, auth: &str, n: usize) -> String {
    let schemes = serde_json::json!({"Token":{"type":"http","scheme":"bearer"},"Key":{"type":"apiKey","in":"header","name":"X-Key"}});
    let security = match auth {
        "bearer" => serde_json::json!([{"Token":[]}]),
        "key" => serde_json::json!([{"Key":[]}]),
        "and" => serde_json::json!([{"Token":[],"Key":[]}]),
        _ => serde_json::json!([]),
    };
    let paths:BTreeMap<_,_>=(0..n).map(|i|{let mut op=serde_json::json!({"responses":{"200":{"description":"JSON","content":{"application/json":{}}}}});if method=="post"{op["requestBody"]=serde_json::json!({"content":{"application/json":{}}});}(format!("/p{i}"),serde_json::json!({method:op}))}).collect();
    serde_json::json!({"openapi":version,"info":{"title":"Auth CPU","version":"1"},"servers":[{"url":"https://measure.invalid"}],"security":security,"components":{"securitySchemes":schemes},"paths":paths}).to_string()
}
type Measurement = (&'static str, Box<dyn FnMut()>);
fn main() {
    let anon_only = std::env::args().any(|a| a == "anonymous-only");
    let reverse = std::env::args().any(|a| a == "reverse");
    let mut rows = vec![];
    for version in ["3.0.4", "3.1.2"] {
        let kind = if version == "3.0.4" {
            openbindings_openapi::EXPERIMENTAL_KIND_30
        } else {
            openbindings_openapi::EXPERIMENTAL_KIND
        };
        for (workload, method, bytes, token_bytes, mechanism) in [
            ("bearer-get", "get", 1024, 64, "bearer"),
            ("key-post", "post", 1024, 64, "key"),
            ("and-post", "post", 65536, 64, "and"),
            ("large-token", "get", 1024, 4096, "bearer"),
            ("anonymous-get-large", "get", 65536, 0, "none"),
            ("anonymous-post-large", "post", 65536, 0, "none"),
        ] {
            for layer in ["binding", "operation"] {
                let mut functions: Vec<Measurement> = vec![];
                let mut context_bytes = 0;
                for mode in [
                    "direct-anonymous",
                    "direct-auth",
                    "adapter-anonymous",
                    "adapter-auth",
                ] {
                    let authenticated = mode.ends_with("-auth") && mechanism != "none";
                    if anon_only && !mode.ends_with("anonymous") {
                        continue;
                    }
                    let text = description(
                        version,
                        method,
                        if authenticated { mechanism } else { "none" },
                        1,
                    );
                    let src = source(kind, &text);
                    let doc =
                        client::Document::parse(text.as_bytes(), None, client::Limits::default())
                            .unwrap();
                    let op = doc.operations().next().unwrap();
                    let body = format!("\"{}\"", "x".repeat(bytes - 2));
                    let envelope = if method == "post" {
                        format!("{{\"body\":{body}}}")
                    } else {
                        "{}".into()
                    };
                    let input = JsonValue::parse(&envelope).unwrap();
                    let body = std::sync::Arc::new(body);
                    let mut credentials = serde_json::Map::new();
                    let mut origins = serde_json::Map::new();
                    let mut names = vec![];
                    if authenticated {
                        for name in match mechanism {
                            "and" => vec!["Key", "Token"],
                            "key" => vec!["Key"],
                            _ => vec!["Token"],
                        } {
                            names.push(name);
                            credentials
                                .insert(name.into(), serde_json::json!("a".repeat(token_bytes)));
                            origins.insert(
                                name.into(),
                                serde_json::json!(["https://measure.invalid"]),
                            );
                        }
                    }
                    let context=JsonValue::parse(serde_json::json!({"credentials":credentials,"configuration":{format!("{kind}.security"):{"schemes":names,"origins":origins}}}).to_string()).unwrap();
                    if authenticated {
                        context_bytes = context.bytes().len();
                    }
                    let oracle = Rc::new(Oracle {
                        active: Cell::new(true),
                        calls: Cell::new(0),
                        headers: names
                            .iter()
                            .map(|name| {
                                if *name == "Token" {
                                    (
                                        "authorization",
                                        format!("Bearer {}", "a".repeat(token_bytes)),
                                    )
                                } else {
                                    ("x-key", "a".repeat(token_bytes))
                                }
                            })
                            .collect(),
                        body: (method == "post").then(|| body.to_string()),
                    });
                    if mode.starts_with("direct") {
                        functions.push((
                            mode,
                            Box::new(move || {
                                let mut supplied = client::Input::default();
                                supplied.selection.server = Some(0);
                                if let Some(body) = input.get("body") {
                                    supplied.body = client::Body::Json(
                                        client::ExactJson::parse(
                                            body.text(),
                                            client::Limits::default(),
                                        )
                                        .unwrap()
                                        .root(),
                                    );
                                }
                                if authenticated {
                                    supplied.selection.security = Some(0);
                                    for name in &names {
                                        let raw = context
                                            .get("credentials")
                                            .unwrap()
                                            .get(name)
                                            .unwrap()
                                            .as_str()
                                            .unwrap();
                                        supplied.credentials.insert(
                                            (*name).into(),
                                            client::Credential {
                                                value: if *name == "Token" {
                                                    client::CredentialValue::Bearer(raw.into())
                                                } else {
                                                    client::CredentialValue::ApiKey(raw.into())
                                                },
                                                origins: vec!["https://measure.invalid".into()],
                                            },
                                        );
                                    }
                                }
                                let req = op.prepare(&supplied).unwrap();
                                let outcome = ready(req.invoke(
                                    client::Cancellation::default(),
                                    &client::HostCapabilities::programmable(),
                                    |wire, _| {
                                        oracle.record(&wire);
                                        async { reply(&body) }
                                    },
                                ));
                                let response = outcome.response.unwrap();
                                let exact = response.json().unwrap();
                                black_box(JsonValue::parse(exact.source()).unwrap());
                                oracle.finish();
                            }),
                        ));
                    } else {
                        let host_oracle = oracle.clone();
                        let p = OpenApi::default().with_transport(
                            move |request: client::PreparedRequest,
                                  cancel: client::Cancellation| {
                                let body = body.clone();
                                let host_oracle = host_oracle.clone();
                                async move {
                                    request
                                        .invoke(
                                            cancel,
                                            &client::HostCapabilities::programmable(),
                                            |wire, _| {
                                                host_oracle.record(&wire);
                                                async { reply(&body) }
                                            },
                                        )
                                        .await
                                }
                            },
                        );
                        if layer == "binding" {
                            let facade = binding::Invoker::new(p);
                            let binding = BindingValue {
                                content: Some(
                                    JsonValue::parse(
                                        serde_json::json!({"path":"/p0","method":method})
                                            .to_string(),
                                    )
                                    .unwrap(),
                                ),
                                ..Default::default()
                            };
                            functions.push((
                                mode,
                                Box::new(move || {
                                    let mut call = facade.invoke(BindingRequest {
                                        source: &src,
                                        binding: &binding,
                                        context: Some(&context),
                                    });
                                    ready(call.write(input.clone())).unwrap();
                                    call.close();
                                    black_box(ready(call.next()).unwrap().unwrap());
                                    assert!(ready(call.next()).is_none());
                                    oracle.finish();
                                }),
                            ));
                        } else {
                            let mut draft = DocumentBuilder::new();
                            draft.sources = Some(BTreeMap::from([("api".into(), src)]));
                            draft.operations.insert("call".into(), Operation::default());
                            draft.bindings = Some(BTreeMap::from([(
                                "call".into(),
                                Binding {
                                    operation: "call".into(),
                                    source: "api".into(),
                                    content: Some(
                                        JsonValue::parse(
                                            serde_json::json!({"path":"/p0","method":method})
                                                .to_string(),
                                        )
                                        .unwrap(),
                                    ),
                                    ..Default::default()
                                },
                            )]));
                            let facade = operation::Invoker::new(
                                ParsedDocument::from_json(draft.to_json().unwrap()),
                                p,
                            );
                            functions.push((
                                mode,
                                Box::new(move || {
                                    let mut call = facade.invoke(
                                        operation::Request::operation("call")
                                            .with_context(&context),
                                    );
                                    ready(call.write(input.clone())).unwrap();
                                    call.close();
                                    black_box(ready(call.next()).unwrap().unwrap());
                                    assert!(ready(call.next()).is_none());
                                    oracle.finish();
                                }),
                            ));
                        }
                    }
                }
                let mut samples: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
                for round in 0..7 {
                    if (round % 2 == 1) ^ reverse {
                        functions.reverse();
                    }
                    for (mode, f) in &mut functions {
                        samples.entry(mode).or_default().push(sample(&mut **f));
                    }
                    if (round % 2 == 1) ^ reverse {
                        functions.reverse();
                    }
                }
                rows.push(serde_json::json!({"version":version,"workload":workload,"layer":layer,"bytes":bytes,"token_bytes":token_bytes,"context_bytes":context_bytes,"samples_ns":samples}));
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"rows":rows,"anonymous_only":anon_only})
    );
}
