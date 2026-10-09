use openbindings::{Conformance, WorkControl};
use openbindings_http_discovery::*;
use std::{cell::RefCell, rc::Rc, time::Duration};

const DOCUMENT: &[u8] = br#" {"openbindings":"0.2.0","operations":{"run":{}}} "#;
#[derive(Default)]
struct Counts {
    read: usize,
    drops: usize,
    cleanup: usize,
}
struct Body {
    bytes: Vec<u8>,
    offset: usize,
    fail: bool,
    stall: bool,
    counts: Rc<RefCell<Counts>>,
}
impl Drop for Body {
    fn drop(&mut self) {
        self.counts.borrow_mut().drops += 1;
    }
}
impl ResponseBody for Body {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize, RequestFailure> {
        if self.stall {
            std::future::pending::<()>().await;
        }
        if self.fail {
            return Err(RequestFailure {
                kind: FailureKind::Network,
                message: "incomplete response".into(),
            });
        }
        let n = output.len().min(self.bytes.len() - self.offset).min(17);
        output[..n].copy_from_slice(&self.bytes[self.offset..self.offset + n]);
        self.offset += n;
        self.counts.borrow_mut().read += n;
        Ok(n)
    }
    async fn discard_status(&mut self) {
        self.counts.borrow_mut().cleanup += 1;
    }
}
fn response(status: u16, bytes: &[u8], counts: Rc<RefCell<Counts>>) -> HttpResponse<Body> {
    HttpResponse {
        status,
        final_url: Some("https://redirect.test/location".into()),
        headers: vec![
            Header::new("www-authenticate", "Bearer realm=dummy"),
            Header::new("content-type", "text/plain"),
        ],
        body: Body {
            bytes: bytes.into(),
            offset: 0,
            fail: false,
            stall: false,
            counts,
        },
    }
}
async fn get(status: u16, bytes: &[u8], limit: i64) -> (DiscoveryResult, Rc<RefCell<Counts>>) {
    let counts = Rc::new(RefCell::new(Counts::default()));
    let body_counts = counts.clone();
    let result = ClientOptions {
        max_document_bytes: limit,
    }
    .discover_with(
        "HTTP://EXAMPLE.test/",
        &WorkControl::new(),
        |request| async move {
            assert_eq!(request.url, "http://EXAMPLE.test/.well-known/openbindings");
            assert_eq!(request.accept, ACCEPT);
            Ok(response(status, bytes, body_counts))
        },
    )
    .await
    .unwrap();
    (result, counts)
}
#[test]
fn origins_preserve_case_and_reject_resource_urls() {
    for (origin, expected) in [
        (
            "https://example.test",
            "https://example.test/.well-known/openbindings",
        ),
        (
            "HTTP://localhost:8080/",
            "http://localhost:8080/.well-known/openbindings",
        ),
        (
            "https://[::1]:8443",
            "https://[::1]:8443/.well-known/openbindings",
        ),
        (
            "https://EXAMPLE.test:443/",
            "https://EXAMPLE.test:443/.well-known/openbindings",
        ),
    ] {
        assert_eq!(endpoint(origin).unwrap(), expected);
    }
    for origin in [
        "",
        "example.test",
        "//example.test",
        "ftp://example.test",
        "https:",
        "https:///",
        "https://user:pass@example.test",
        "https://example.test/api",
        "https://example.test/%2f",
        "https://example.test?q=x",
        "https://example.test?",
        "https://example.test#part",
        "https://example.test#",
        "https://example.test:wrong",
        "https://exa mple.test",
        "https://%",
        "https://example.test:65536",
    ] {
        assert!(endpoint(origin).is_err(), "{origin:?}");
    }
}
#[test]
fn fixed_cors_origin_matches_the_published_syntax_policy() {
    for origin in [
        "",
        "*",
        "null",
        "https://client.example",
        "http://localhost:8080",
        "https://127.0.0.1:8443",
        "https://[2001:db8::1]:8443",
        "https://xn--bcher-kva.example",
        "chrome-extension://abcdefghijklmnop",
    ] {
        assert!(validate_allow_origin(origin).is_ok(), "{origin:?}");
    }
    for origin in [
        "https://client.example/",
        "https://client.example/path",
        "https://a.example, https://b.example",
        "https://a.example,b.example",
        "https://a.example https://b.example",
        "https://user:pass@client.example",
        "https://client.example?",
        "https://client.example?q=x",
        "https://client.example#",
        "https://client.example#part",
        " https://client.example",
        "https://client.example ",
        " *",
        "null ",
        "client.example",
        "//client.example",
        "https:",
        "https:///",
        "https://*.example",
        "https://clïent.example",
        "https://%63lient.example",
        "https://<client.example>",
        "https://client>example",
        "https://\"client.example\"",
        "https://client.example:",
        "https://client.example:wrong",
        "https://client.example:65536",
        "https://[invalid]",
        "https://[::1]extra",
        "https://::1",
        "https://[fe80::1%25en0]",
        "https://client.example\\path",
    ] {
        assert!(validate_allow_origin(origin).is_err(), "{origin:?}");
    }
    for byte in (0..32).chain([127]) {
        assert!(
            validate_allow_origin(&format!("https://client.example{}", char::from(byte))).is_err()
        );
    }
}
#[tokio::test(flavor = "current_thread")]
async fn status_is_never_collapsed_into_absence_and_every_body_is_released() {
    for status in [200, 301, 302, 303, 307, 308, 400, 401, 403, 404, 429, 500] {
        let (result, counts) = get(status, DOCUMENT, 0).await;
        assert_eq!(result.metadata.as_ref().unwrap().status, status);
        assert_eq!(
            result.metadata.as_ref().unwrap().header("WWW-Authenticate"),
            Some(&b"Bearer realm=dummy"[..])
        );
        assert_eq!(
            result.metadata.as_ref().unwrap().final_url.as_deref(),
            Some("https://redirect.test/location")
        );
        assert_eq!(counts.borrow().drops, 1);
        match status {
            200 => {
                assert!(matches!(result.outcome, DiscoveryOutcome::Found { .. }));
                assert_eq!(result.body.as_deref(), Some(DOCUMENT));
            }
            404 => assert!(matches!(result.outcome, DiscoveryOutcome::Absent)),
            401 | 403 => assert!(matches!(result.outcome, DiscoveryOutcome::Gated)),
            _ => assert!(matches!(result.outcome, DiscoveryOutcome::HttpStatus)),
        }
        if status != 200 {
            assert!(result.body.is_none());
            assert_eq!(counts.borrow().read, 0);
            assert_eq!(counts.borrow().cleanup, 1);
        }
    }
}
#[tokio::test(flavor = "current_thread")]
async fn complete_bad_body_and_version_priority_survive_with_metadata() {
    for bytes in [
        &b"{"[..],
        br#"{"openbindings":"0.2.0"}"#,
        br#"{"openbindings":"0.2.0","operations":{"run":{"input":{"pattern":"["}}}}}"#,
    ] {
        let (r, _) = get(200, bytes, 0).await;
        assert!(matches!(r.outcome, DiscoveryOutcome::NonConformant { .. }));
        assert_eq!(r.body.as_deref(), Some(bytes));
        assert!(r.metadata.is_some());
    }
    let (r, _) = get(200, br#"{"openbindings":"9.0.0"}"#, 0).await;
    assert!(matches!(r.outcome, DiscoveryOutcome::VersionRefused { .. }));
    assert!(r.body.is_some());
    let deep = format!(
        "{{\"openbindings\":\"0.2.0\",\"operations\":{{}},\"schemas\":{{\"deep\":{}{{}}{}}}}}",
        "{\"not\":".repeat(260),
        "}".repeat(260)
    );
    let (r, _) = get(200, deep.as_bytes(), 0).await;
    let DiscoveryOutcome::Undetermined { assessment } = r.outcome else {
        panic!("expected undetermined")
    };
    assert_eq!(assessment.report().conclusion, Conformance::Undetermined);
    let (r, _) = get(
        200,
        br#"{"openbindings":"0.2.0","operations":{"run":{}},"x":"\ud800"}"#,
        0,
    )
    .await;
    assert!(matches!(r.outcome, DiscoveryOutcome::Undetermined { .. }));
    assert!(r.body.is_some());
}
#[tokio::test(flavor = "current_thread")]
async fn size_is_decoded_bytes_with_one_sentinel_and_no_prefix_assessment() {
    for limit in [
        0,
        DOCUMENT.len() as i64,
        DOCUMENT.len() as i64 - 1,
        i64::MAX,
    ] {
        let (r, c) = get(200, DOCUMENT, limit).await;
        if limit == DOCUMENT.len() as i64 - 1 {
            assert!(matches!(r.outcome, DiscoveryOutcome::BodyLimit { .. }));
            assert!(r.body.is_none());
        } else {
            assert!(matches!(r.outcome, DiscoveryOutcome::Found { .. }));
        }
        let bound = if limit == 0 {
            DEFAULT_MAX_DOCUMENT_BYTES
        } else {
            limit as usize
        };
        assert!(c.borrow().read <= bound.saturating_add(1));
        assert_eq!(c.borrow().drops, 1);
    }
    assert_eq!(ClientOptions::default().byte_limit().unwrap(), 1 << 20);
    assert!(
        ClientOptions {
            max_document_bytes: -1
        }
        .byte_limit()
        .is_err()
    );
    let mut bytes = DOCUMENT.to_vec();
    bytes.extend(vec![b' '; 1 << 20]);
    let (r, c) = get(200, &bytes, 0).await;
    assert!(matches!(r.outcome, DiscoveryOutcome::BodyLimit { .. }));
    assert_eq!(c.borrow().read, (1 << 20) + 1);
}
#[tokio::test(flavor = "current_thread")]
async fn cancellation_during_pending_io_wakes_without_transport_cooperation() {
    let control = WorkControl::new();
    control.cancel();
    let r = ClientOptions::default()
        .discover_with("https://example.test", &control, |_| async {
            panic!("pre-cancelled transport was dispatched");
            #[allow(unreachable_code)]
            Ok::<HttpResponse<Body>, RequestFailure>(response(200, DOCUMENT, Rc::default()))
        })
        .await
        .unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::Cancelled));
    assert!(r.metadata.is_none());
    let control = WorkControl::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let options = ClientOptions::default();
    let call = options.discover_with("https://example.test", &control, |_| async {
        let mut r = response(200, DOCUMENT, counts.clone());
        r.body.stall = true;
        Ok(r)
    });
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        control.cancel();
    };
    let (r, ()) = tokio::join!(call, cancel);
    let r = r.unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::Cancelled));
    assert!(r.metadata.is_some());
    assert_eq!(counts.borrow().drops, 1);
    let healthy = get(200, DOCUMENT, 0).await.0;
    assert!(matches!(healthy.outcome, DiscoveryOutcome::Found { .. }));
}
#[tokio::test(flavor = "current_thread")]
async fn incomplete_body_transport_failure_and_missing_final_url_are_distinct() {
    let options = ClientOptions::default();
    let control = WorkControl::new();
    let counts = Rc::default();
    let r = options
        .discover_with("https://example.test", &control, |_| async {
            let mut r = response(200, DOCUMENT, counts);
            r.body.fail = true;
            r.final_url = None;
            Ok(r)
        })
        .await
        .unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::BodyFailure { .. }));
    assert!(r.metadata.unwrap().final_url.is_none());
    assert!(r.body.is_none());
    let r = options
        .discover_with::<Body, _, _>("https://example.test", &control, |_| async {
            Err(RequestFailure {
                kind: FailureKind::Timeout,
                message: "deadline".into(),
            })
        })
        .await
        .unwrap();
    assert!(matches!(
        r.outcome,
        DiscoveryOutcome::TransportFailure {
            failure: RequestFailure {
                kind: FailureKind::Timeout,
                ..
            }
        }
    ));
    assert!(r.metadata.is_none());
}
#[test]
fn publication_copies_a_conformant_snapshot_and_serves_head_and_method_policy() {
    let mut bytes = DOCUMENT.to_vec();
    let publication = Publication::from_bytes(
        &bytes,
        PublicationOptions {
            allow_origin: "*".into(),
        },
    )
    .unwrap();
    bytes.fill(b'x');
    let get = publication.respond("GET", WELL_KNOWN_PATH);
    let head = publication.respond("HEAD", WELL_KNOWN_PATH);
    assert_eq!(get.body.as_ref(), DOCUMENT);
    assert!(head.body.is_empty());
    assert_eq!(get.headers, head.headers);
    assert_eq!(head.status, 200);
    assert!(
        get.headers
            .contains(&Header::new("content-type", MEDIA_TYPE))
    );
    assert!(
        get.headers
            .contains(&Header::new("access-control-allow-origin", "*"))
    );
    for method in ["POST", "PUT", "OPTIONS", "DELETE", "get"] {
        let response = publication.respond(method, WELL_KNOWN_PATH);
        assert_eq!(response.status, 405);
        assert!(
            response
                .headers
                .contains(&Header::new("allow", "GET, HEAD"))
        );
    }
    assert_eq!(publication.respond("GET", "/other").status, 404);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..100 {
                    assert_eq!(
                        publication.respond("GET", WELL_KNOWN_PATH).body.as_ref(),
                        DOCUMENT
                    );
                }
            });
        }
    });
}
#[test]
fn publication_refuses_every_unestablished_conformance_state() {
    assert!(matches!(
        Publication::from_bytes(b"{}", PublicationOptions::default()),
        Err(PublicationError::NonConformant(_))
    ));
    assert!(matches!(
        Publication::from_bytes(
            br#"{"openbindings":"8.0.0"}"#,
            PublicationOptions::default()
        ),
        Err(PublicationError::VersionRefused(_))
    ));
    assert!(matches!(
        Publication::from_bytes(
            br#"{"openbindings":"0.2.0","operations":{},"x-value":"\ud800"}"#,
            PublicationOptions::default()
        ),
        Err(PublicationError::Undetermined(_))
    ));
}
