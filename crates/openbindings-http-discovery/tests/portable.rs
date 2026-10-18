use openbindings::Conformance;
use openbindings_http_discovery::CancellationToken;
use openbindings_http_discovery::*;
use std::{cell::RefCell, rc::Rc, time::Duration};

struct Timer(tokio::time::Instant);
impl Timer {
    fn new() -> Self {
        Self(tokio::time::Instant::now())
    }
}
impl DiscoveryTimer for Timer {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    async fn sleep_until(&self, deadline: Duration) {
        tokio::time::sleep_until(self.0 + deadline).await;
    }
}
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
    async fn read(&mut self, output: &mut [u8]) -> Result<usize, RequestError> {
        if self.stall {
            std::future::pending::<()>().await;
        }
        if self.fail {
            return Err(RequestError::new(ErrorKind::Network, "incomplete response"));
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
        final_url: Some("https://example.test/.well-known/openbindings".into()),
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
async fn get(status: u16, bytes: &[u8], limit: usize) -> (DiscoveryResult, Rc<RefCell<Counts>>) {
    let counts = Rc::new(RefCell::new(Counts::default()));
    let body_counts = counts.clone();
    let result = discover(
        "HTTPS://EXAMPLE.test/",
        &ClientOptions {
            max_document_bytes: limit,
            ..ClientOptions::default()
        },
        &CancellationToken::new(),
        &Timer::new(),
        |request| {
            let body_counts = body_counts.clone();
            async move {
                assert_eq!(request.url, "https://example.test/.well-known/openbindings");
                assert_eq!(request.accept, ACCEPT);
                Ok(response(status, bytes, body_counts))
            }
        },
    )
    .await
    .unwrap();
    (result, counts)
}
#[test]
fn origins_normalize_hosts_and_reject_non_loopback_http() {
    for (origin, expected) in [
        (
            "https://example.test",
            "https://example.test/.well-known/openbindings",
        ),
        (
            "HTTP://127.0.0.1:8080/",
            "http://127.0.0.1:8080/.well-known/openbindings",
        ),
        (
            "https://[::1]:8443",
            "https://[::1]:8443/.well-known/openbindings",
        ),
        (
            "https://EXAMPLE.test:443/",
            "https://example.test/.well-known/openbindings",
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
    for origin in [AllowOrigin::Any, AllowOrigin::Omit, AllowOrigin::Null] {
        assert!(validate_allow_origin(&origin).is_ok());
    }
    for origin in [
        "https://client.example",
        "http://localhost:8080",
        "https://127.0.0.1:8443",
        "https://[2001:db8::1]:8443",
        "https://xn--bcher-kva.example",
    ] {
        assert!(
            validate_allow_origin(&AllowOrigin::Exact(origin.into())).is_ok(),
            "{origin:?}"
        );
    }
    // only spellings a browser can serialize are usable fixed CORS origins.
    for origin in [
        "",
        "*",
        "null",
        "HTTPS://A.EXAMPLE",
        "https://a.example:443",
        "file://x",
        "chrome-extension://abcdefghijklmnop",
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
        assert!(
            validate_allow_origin(&AllowOrigin::Exact(origin.into())).is_err(),
            "{origin:?}"
        );
    }
    for byte in (0..32).chain([127]) {
        assert!(
            validate_allow_origin(&AllowOrigin::Exact(format!(
                "https://client.example{}",
                char::from(byte)
            )))
            .is_err()
        );
    }
}
#[tokio::test(flavor = "current_thread")]
async fn status_is_never_collapsed_into_absence_and_every_body_is_released() {
    for status in [200, 301, 302, 303, 307, 308, 400, 401, 403, 404, 429, 500] {
        let (result, counts) = get(status, DOCUMENT, DEFAULT_MAX_DOCUMENT_BYTES).await;
        assert_eq!(result.metadata.as_ref().unwrap().status, status);
        assert_eq!(
            result.metadata.as_ref().unwrap().header("WWW-Authenticate"),
            Some(&b"Bearer realm=dummy"[..])
        );
        assert_eq!(
            result.metadata.as_ref().unwrap().final_url.as_deref(),
            Some("https://example.test/.well-known/openbindings")
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
        let (r, _) = get(200, bytes, DEFAULT_MAX_DOCUMENT_BYTES).await;
        assert!(matches!(r.outcome, DiscoveryOutcome::NonConformant { .. }));
        assert_eq!(r.body.as_deref(), Some(bytes));
        assert!(r.metadata.is_some());
    }
    let (r, _) = get(
        200,
        br#"{"openbindings":"9.0.0"}"#,
        DEFAULT_MAX_DOCUMENT_BYTES,
    )
    .await;
    assert!(matches!(r.outcome, DiscoveryOutcome::VersionRefused { .. }));
    assert!(r.body.is_some());
    let deep = format!(
        "{{\"openbindings\":\"0.2.0\",\"operations\":{{}},\"schemas\":{{\"deep\":{}{{}}{}}}}}",
        "{\"not\":".repeat(260),
        "}".repeat(260)
    );
    let (r, _) = get(200, deep.as_bytes(), DEFAULT_MAX_DOCUMENT_BYTES).await;
    // local schema checks establish conformance beyond depth 256.
    let DiscoveryOutcome::Found { document } = r.outcome else {
        panic!("expected found")
    };
    assert_eq!(
        document
            .as_document()
            .validate()
            .unwrap()
            .report()
            .conclusion,
        Conformance::Conformant
    );
    let (r, _) = get(
        200,
        br#"{"openbindings":"0.2.0","operations":{"run":{}},"x":"\ud800"}"#,
        DEFAULT_MAX_DOCUMENT_BYTES,
    )
    .await;
    // OBI-01 excludes the surrogate before later document rules apply.
    assert!(matches!(r.outcome, DiscoveryOutcome::NonConformant { .. }));
    assert!(r.body.is_some());
}
#[tokio::test(flavor = "current_thread")]
async fn size_is_decoded_bytes_with_one_sentinel_and_no_prefix_assessment() {
    for limit in [
        DEFAULT_MAX_DOCUMENT_BYTES,
        DOCUMENT.len(),
        DOCUMENT.len() - 1,
        isize::MAX as usize,
    ] {
        let (r, c) = get(200, DOCUMENT, limit).await;
        if limit == DOCUMENT.len() - 1 {
            assert!(matches!(r.outcome, DiscoveryOutcome::BodyLimit { .. }));
            assert!(r.body.is_none());
        } else {
            assert!(matches!(r.outcome, DiscoveryOutcome::Found { .. }));
        }
        let bound = if limit == 0 {
            DEFAULT_MAX_DOCUMENT_BYTES
        } else {
            limit
        };
        assert!(c.borrow().read <= bound.saturating_add(1));
        assert_eq!(c.borrow().drops, 1);
    }
    assert_eq!(ClientOptions::default().byte_limit().unwrap(), 1 << 20);
    assert!(
        ClientOptions {
            max_document_bytes: usize::MAX,
            ..Default::default()
        }
        .byte_limit()
        .is_err()
    );
    assert!(
        ClientOptions {
            max_document_bytes: 0,
            ..ClientOptions::default()
        }
        .byte_limit()
        .is_err()
    );
    let mut bytes = DOCUMENT.to_vec();
    bytes.extend(vec![b' '; 1 << 20]);
    let (r, c) = get(200, &bytes, DEFAULT_MAX_DOCUMENT_BYTES).await;
    assert!(matches!(r.outcome, DiscoveryOutcome::BodyLimit { .. }));
    assert_eq!(c.borrow().read, (1 << 20) + 1);
}
#[tokio::test(flavor = "current_thread")]
async fn cancellation_during_pending_io_wakes_without_transport_cooperation() {
    let control = CancellationToken::new();
    control.cancel();
    let r = discover(
        "https://example.test",
        &ClientOptions::default(),
        &control,
        &Timer::new(),
        |_| async {
            panic!("pre-cancelled transport was dispatched");
            #[allow(unreachable_code)]
            Ok::<HttpResponse<Body>, RequestError>(response(200, DOCUMENT, Rc::default()))
        },
    )
    .await
    .unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::Cancelled));
    assert!(r.metadata.is_none());
    let control = CancellationToken::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let options = ClientOptions::default();
    let timer = Timer::new();
    let call = discover(
        "https://example.test",
        &options,
        &control,
        &timer,
        |_| async {
            let mut r = response(200, DOCUMENT, counts.clone());
            r.body.stall = true;
            Ok(r)
        },
    );
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        control.cancel();
    };
    let (r, ()) = tokio::join!(call, cancel);
    let r = r.unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::Cancelled));
    assert!(r.metadata.is_some());
    assert_eq!(counts.borrow().drops, 1);
    let healthy = get(200, DOCUMENT, DEFAULT_MAX_DOCUMENT_BYTES).await.0;
    assert!(matches!(healthy.outcome, DiscoveryOutcome::Found { .. }));
}
#[tokio::test(flavor = "current_thread")]
async fn incomplete_body_and_transport_failure_are_distinct() {
    let options = ClientOptions::default();
    let control = CancellationToken::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let r = discover(
        "https://example.test",
        &options,
        &control,
        &Timer::new(),
        |_| async {
            let mut r = response(200, DOCUMENT, counts.clone());
            r.body.fail = true;
            r.final_url = None;
            Ok(r)
        },
    )
    .await
    .unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::BodyError { .. }));
    // A transport that cannot report a URL still dispatched the SDK's own.
    assert_eq!(
        r.metadata.unwrap().final_url.as_deref(),
        Some("https://example.test/.well-known/openbindings")
    );
    assert!(r.body.is_none());
    let r = discover::<Body, _, _, _>(
        "https://example.test",
        &options,
        &control,
        &Timer::new(),
        |_| async { Err(RequestError::new(ErrorKind::Timeout, "deadline")) },
    )
    .await
    .unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::Timeout));
    assert!(r.metadata.is_none());
}
#[test]
fn publication_copies_a_conformant_snapshot_and_serves_head_and_method_policy() {
    let mut bytes = DOCUMENT.to_vec();
    let publication = Publication::from_bytes(
        &bytes,
        PublicationOptions {
            allow_origin: AllowOrigin::Any,
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
    // Publication applies the core document boundary, including opaque content.
    let error = Publication::from_bytes(
        br#"{"openbindings":"0.2.0","operations":{},"x-value":"\ud800"}"#,
        PublicationOptions::default(),
    )
    .unwrap_err();
    let PublicationError::NonConformant(validation) = error else {
        panic!("expected OBI-01 rejection")
    };
    let report = validation.report();
    assert_eq!(report.evidence["OBI-01"], openbindings::Evidence::Violated);
    assert_eq!(
        report.evidence["OBI-02"],
        openbindings::Evidence::NotApplicable
    );
    assert_eq!(report.findings[0].rule, "OBI-01");
    assert_eq!(
        report.findings[0]
            .location
            .as_ref()
            .unwrap()
            .pointer
            .as_ref()
            .and_then(openbindings::Pointer::as_str),
        Some("/x-value")
    );
}

// WHATWG alternate IP spellings are admitted and recorded canonically.
#[test]
fn alternate_loopback_hosts_are_normalized_before_admission() {
    for origin in [
        "http://2130706433",
        "http://0x7f.1",
        "http://%31%32%37.0.0.1",
        "http://127.1",
    ] {
        assert_eq!(
            endpoint(origin).unwrap(),
            "http://127.0.0.1/.well-known/openbindings"
        );
    }
    for origin in ["http://127.255.1.2", "http://[::1]"] {
        assert!(endpoint(origin).is_ok());
    }
    for origin in [
        "http://localhost",
        "http://a.localhost",
        "http://127.attacker.test",
        "http://128.0.0.1",
        "http://[::ffff:127.0.0.1]",
        "http://example.test",
    ] {
        assert_eq!(endpoint(origin).unwrap_err().code, "origin-policy");
    }
}

// zero timeout remains a configuration refusal before I/O.
#[tokio::test(flavor = "current_thread")]
async fn zero_timeout_refuses_before_io() {
    let error = discover::<Body, _, _, _>(
        "https://example.test",
        &ClientOptions {
            timeout: AttemptTimeout::After(Duration::ZERO),
            ..Default::default()
        },
        &CancellationToken::new(),
        &Timer::new(),
        |_| async { panic!("dispatched with zero timeout") },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "invalid-timeout");
    assert_eq!(
        AttemptTimeout::Default.duration().unwrap(),
        Some(Duration::from_secs(30))
    );
    assert_eq!(AttemptTimeout::Unbounded.duration().unwrap(), None);
}

#[derive(Default)]
struct Clock {
    now: std::cell::Cell<Duration>,
    waker: RefCell<Option<std::task::Waker>>,
    started: std::cell::Cell<usize>,
    dropped: std::cell::Cell<usize>,
}
impl Clock {
    fn advance(&self, duration: Duration) {
        self.now.set(self.now.get() + duration);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }
}
struct Sleep<'a>(&'a Clock, Duration);
impl std::future::Future for Sleep<'_> {
    type Output = ();
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0.now.get() >= self.1 {
            std::task::Poll::Ready(())
        } else {
            *self.0.waker.borrow_mut() = Some(cx.waker().clone());
            std::task::Poll::Pending
        }
    }
}
impl Drop for Sleep<'_> {
    fn drop(&mut self) {
        self.0.dropped.set(self.0.dropped.get() + 1);
    }
}
impl DiscoveryTimer for Clock {
    fn now(&self) -> Duration {
        self.now.get()
    }
    fn sleep_until(&self, deadline: Duration) -> impl std::future::Future<Output = ()> {
        self.started.set(self.started.get() + 1);
        Sleep(self, deadline)
    }
}

// the default host timer wakes never-ready headers/body, and cancellation wins a tie.
#[tokio::test(flavor = "current_thread")]
async fn whole_attempt_timer_wakes_pending_io_and_cancellation_wins_ties() {
    for body_phase in [false, true] {
        for cancel in [false, true] {
            let clock = Clock::default();
            let control = CancellationToken::new();
            let counts = Rc::new(RefCell::new(Counts::default()));
            let options = ClientOptions::default();
            let call = discover(
                "https://example.test",
                &options,
                &control,
                &clock,
                |_| async {
                    if !body_phase {
                        std::future::pending::<()>().await;
                    }
                    let mut r = response(200, DOCUMENT, counts.clone());
                    r.body.stall = true;
                    Ok(r)
                },
            );
            let advance = async {
                tokio::task::yield_now().await;
                clock.advance(Duration::from_secs(30));
                if cancel {
                    control.cancel();
                }
            };
            let (r, ()) = tokio::join!(call, advance);
            let r = r.unwrap();
            if cancel {
                assert!(matches!(r.outcome, DiscoveryOutcome::Cancelled));
            } else {
                assert!(matches!(r.outcome, DiscoveryOutcome::Timeout));
            }
            assert_eq!(r.metadata.is_some(), body_phase);
            assert!(r.body.is_none());
            assert_eq!(counts.borrow().drops, usize::from(body_phase));
            assert_eq!(clock.started.get(), 1);
            assert_eq!(clock.dropped.get(), 1);
        }
    }
}

// hop admission is before dispatch, and redirects do not reset the deadline.
#[tokio::test(flavor = "current_thread")]
async fn portable_redirects_obey_statuses_relative_location_limit_and_total_time() {
    for status in [301, 302, 303, 307, 308] {
        let clock = Clock::default();
        let calls = std::cell::Cell::new(0);
        let r = discover(
            "https://example.test",
            &ClientOptions::default(),
            &CancellationToken::new(),
            &clock,
            |request| {
                let n = calls.get();
                calls.set(n + 1);
                async move {
                    let mut r =
                        response(if n == 0 { status } else { 200 }, DOCUMENT, Rc::default());
                    r.final_url = Some(request.url.clone());
                    if n == 0 {
                        r.headers.push(Header::new("location", "../done"));
                    } else {
                        assert_eq!(request.url, "https://example.test/done");
                    }
                    Ok(r)
                }
            },
        )
        .await
        .unwrap();
        assert!(matches!(r.outcome, DiscoveryOutcome::Found { .. }));
        assert_eq!(calls.get(), 2);
    }
    for location in [
        "http://127.0.0.1/next",
        "ftp://example.test/next",
        "https://user:pass@example.test/next",
        "http://example.test/next",
        "http://[",
    ] {
        let calls = std::cell::Cell::new(0);
        let r = discover(
            "https://example.test",
            &ClientOptions::default(),
            &CancellationToken::new(),
            &Clock::default(),
            |request| {
                calls.set(calls.get() + 1);
                async move {
                    let mut r = response(302, DOCUMENT, Rc::default());
                    r.final_url = Some(request.url);
                    r.headers.push(Header::new("location", location));
                    Ok(r)
                }
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            r.outcome,
            DiscoveryOutcome::TransportError {
                error: RequestError {
                    kind: ErrorKind::RedirectPolicy,
                    ..
                }
            }
        ));
        assert_eq!(calls.get(), 1, "downgrade was dispatched");
    }
    for slow in [false, true] {
        let clock = Clock::default();
        let calls = std::cell::Cell::new(0);
        let r = discover(
            "https://example.test",
            &ClientOptions::default(),
            &CancellationToken::new(),
            &clock,
            |request| {
                calls.set(calls.get() + 1);
                let clock = &clock;
                async move {
                    if slow {
                        clock.advance(Duration::from_secs(11));
                    }
                    let mut r = response(302, DOCUMENT, Rc::default());
                    r.final_url = Some(request.url);
                    r.headers.push(Header::new("location", "/loop"));
                    Ok(r)
                }
            },
        )
        .await
        .unwrap();
        if slow {
            assert!(matches!(r.outcome, DiscoveryOutcome::Timeout));
            assert_eq!(calls.get(), 3);
        } else {
            assert!(matches!(
                r.outcome,
                DiscoveryOutcome::TransportError {
                    error: RequestError {
                        kind: ErrorKind::RedirectPolicy,
                        ..
                    }
                }
            ));
            assert_eq!(calls.get(), MAX_REDIRECTS + 1);
        }
    }
}

// the SDK follows portable hops itself, so it reports the URL it resolved
// even when the transport reports none or another spelling of it.
#[tokio::test(flavor = "current_thread")]
async fn portable_discovery_reports_the_url_it_resolved() {
    for (reported, redirect) in [
        (None, false),
        (None, true),
        (Some("HTTPS://B.TEST:443/moved"), true),
    ] {
        let calls = std::cell::Cell::new(0);
        let r = discover(
            "https://a.test",
            &ClientOptions::default(),
            &CancellationToken::new(),
            &Clock::default(),
            |request| {
                let n = calls.get();
                calls.set(n + 1);
                async move {
                    let mut r = response(
                        if redirect && n == 0 { 302 } else { 200 },
                        DOCUMENT,
                        Rc::default(),
                    );
                    r.final_url = None;
                    if redirect && n == 0 {
                        r.headers
                            .push(Header::new("location", "https://b.test/moved"));
                    } else if redirect {
                        assert_eq!(request.url, "https://b.test/moved");
                        r.final_url = reported.map(Into::into);
                    }
                    Ok(r)
                }
            },
        )
        .await
        .unwrap();
        assert!(matches!(r.outcome, DiscoveryOutcome::Found { .. }));
        let metadata = r.metadata.unwrap();
        assert_eq!(
            metadata.requested_url,
            "https://a.test/.well-known/openbindings"
        );
        assert_eq!(
            metadata.final_url.as_deref(),
            Some(if redirect {
                "https://b.test/moved"
            } else {
                "https://a.test/.well-known/openbindings"
            })
        );
    }
}

// observable hidden following is a contract failure, never document invalidity.
#[tokio::test(flavor = "current_thread")]
async fn portable_hidden_redirect_is_a_typed_policy_refusal() {
    let r = discover(
        "https://example.test",
        &ClientOptions::default(),
        &CancellationToken::new(),
        &Timer::new(),
        |_| async {
            let mut r = response(200, DOCUMENT, Rc::default());
            r.final_url = Some("https://other.test/final".into());
            Ok(r)
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        r.outcome,
        DiscoveryOutcome::TransportError {
            error: RequestError {
                kind: ErrorKind::RedirectPolicy,
                ..
            }
        }
    ));
    assert!(r.body.is_none());
}

// permissive CORS is the publication default; omission remains explicit.
#[test]
fn publication_cors_default_and_explicit_omission() {
    for allow_origin in ["*", ""] {
        let publication = Publication::from_bytes(
            DOCUMENT,
            if allow_origin == "*" {
                PublicationOptions::default()
            } else {
                PublicationOptions {
                    allow_origin: AllowOrigin::Omit,
                }
            },
        )
        .unwrap();
        assert_eq!(
            publication
                .respond("GET", WELL_KNOWN_PATH)
                .headers
                .iter()
                .any(|h| h.name == "access-control-allow-origin"),
            !allow_origin.is_empty()
        );
    }
}

// committed HTTP status survives expiry in a noncooperating cleanup future.
#[tokio::test(flavor = "current_thread")]
async fn status_cleanup_is_bounded_by_remaining_attempt_time() {
    struct StatusBody(Rc<std::cell::Cell<bool>>);
    impl Drop for StatusBody {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    impl ResponseBody for StatusBody {
        async fn read(&mut self, _: &mut [u8]) -> Result<usize, RequestError> {
            panic!("status body read as document")
        }
        async fn discard_status(&mut self) {
            std::future::pending::<()>().await;
        }
    }
    let clock = Clock::default();
    let dropped = Rc::new(std::cell::Cell::new(false));
    let options = ClientOptions::default();
    let control = CancellationToken::new();
    let call = discover(
        "https://example.test",
        &options,
        &control,
        &clock,
        |_| async {
            Ok(HttpResponse {
                status: 404,
                final_url: None,
                headers: vec![],
                body: StatusBody(dropped.clone()),
            })
        },
    );
    let (result, ()) = tokio::join!(call, async {
        tokio::task::yield_now().await;
        clock.advance(Duration::from_secs(30));
    });
    assert!(matches!(result.unwrap().outcome, DiscoveryOutcome::Absent));
    assert!(dropped.get());
    assert_eq!(clock.started.get(), 1);
    assert_eq!(clock.dropped.get(), 1);
}

// monotonic boundary checks discard an validation completed after expiry.
#[tokio::test(flavor = "current_thread")]
async fn validation_boundary_expiry_retains_complete_bytes_and_no_success() {
    struct BoundaryClock(std::cell::Cell<usize>);
    impl DiscoveryTimer for BoundaryClock {
        fn now(&self) -> Duration {
            let call = self.0.get() + 1;
            self.0.set(call);
            if call >= 10 {
                Duration::from_secs(30)
            } else {
                Duration::ZERO
            }
        }
        async fn sleep_until(&self, _: Duration) {
            std::future::pending::<()>().await;
        }
    }
    struct Complete(bool);
    impl ResponseBody for Complete {
        async fn read(&mut self, output: &mut [u8]) -> Result<usize, RequestError> {
            if self.0 {
                return Ok(0);
            }
            self.0 = true;
            output[..DOCUMENT.len()].copy_from_slice(DOCUMENT);
            Ok(DOCUMENT.len())
        }
    }
    let clock = BoundaryClock(std::cell::Cell::new(0));
    let result = discover(
        "https://example.test",
        &ClientOptions::default(),
        &CancellationToken::new(),
        &clock,
        |_| async {
            Ok(HttpResponse {
                status: 200,
                final_url: None,
                headers: vec![],
                body: Complete(false),
            })
        },
    )
    .await
    .unwrap();
    assert!(matches!(result.outcome, DiscoveryOutcome::Timeout));
    assert_eq!(
        clock.0.get(),
        10,
        "expiry must be observed after validation"
    );
    assert_eq!(result.body.as_deref(), Some(DOCUMENT));
    assert_eq!(result.metadata.unwrap().status, 200);
}

// EOF proves completeness before the terminal boundary check.
#[tokio::test(flavor = "current_thread")]
async fn eof_boundary_retains_complete_body_on_timeout_or_cancellation() {
    struct EofBody<'a> {
        read: bool,
        eof: bool,
        cancel: bool,
        clock: &'a Clock,
        control: &'a CancellationToken,
    }
    impl ResponseBody for EofBody<'_> {
        async fn read(&mut self, output: &mut [u8]) -> Result<usize, RequestError> {
            if !self.read {
                self.read = true;
                output[..DOCUMENT.len()].copy_from_slice(DOCUMENT);
                return Ok(DOCUMENT.len());
            }
            if self.cancel {
                self.control.cancel();
            } else {
                self.clock.advance(Duration::from_secs(30));
            }
            if self.eof {
                Ok(0)
            } else {
                output[0] = b' ';
                Ok(1)
            }
        }
    }
    for eof in [false, true] {
        for cancel in [false, true] {
            let clock = Clock::default();
            let control = CancellationToken::new();
            let r = discover(
                "https://example.test",
                &ClientOptions::default(),
                &control,
                &clock,
                |_| async {
                    Ok(HttpResponse {
                        status: 200,
                        final_url: None,
                        headers: vec![],
                        body: EofBody {
                            read: false,
                            eof,
                            cancel,
                            clock: &clock,
                            control: &control,
                        },
                    })
                },
            )
            .await
            .unwrap();
            if cancel {
                assert!(matches!(r.outcome, DiscoveryOutcome::Cancelled));
            } else {
                assert!(matches!(r.outcome, DiscoveryOutcome::Timeout));
            }
            assert_eq!(r.body.as_deref(), if eof { Some(DOCUMENT) } else { None });
            assert_eq!(r.metadata.unwrap().status, 200);
        }
    }
}
#[test]
fn transports_report_failures_with_the_request_error_constructor() {
    let error = RequestError::new(ErrorKind::RedirectPolicy, "hop refused");
    assert_eq!(error.kind, ErrorKind::RedirectPolicy);
    assert_eq!(error.message, "hop refused");
    assert_eq!(error.to_string(), "hop refused");
}
