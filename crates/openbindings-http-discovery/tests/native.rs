#![cfg(feature = "native")]

use openbindings_http_discovery::CancellationToken;
use openbindings_http_discovery::*;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
const DOCUMENT: &[u8]=br##"{"openbindings":"0.2.0","operations":{"run":{"input":{"$ref":"#/schemas/n"}}},"schemas":{"n":{"type":"integer"}}}"##;
#[derive(Clone, Debug)]
struct Record {
    connection: usize,
    path: String,
    headers: BTreeMap<String, String>,
}
struct Server {
    origin: String,
    stopped: Arc<AtomicBool>,
    records: Arc<Mutex<Vec<Record>>>,
    redirects: Arc<Mutex<BTreeMap<String, String>>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let records = Arc::new(Mutex::new(Vec::new()));
        let redirects = Arc::new(Mutex::new(BTreeMap::new()));
        let stop = stopped.clone();
        let log = records.clone();
        let routes = redirects.clone();
        let handle = thread::spawn(move || {
            let mut children = Vec::new();
            let mut connection = 0;
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        connection += 1;
                        let stop = stop.clone();
                        let log = log.clone();
                        let routes = routes.clone();
                        children.push(thread::spawn(move || {
                            serve(socket, connection, &stop, &log, &routes)
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            for child in children {
                child.join().unwrap();
            }
        });
        Self {
            origin,
            stopped,
            records,
            redirects,
            thread: Some(handle),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn serve(
    socket: TcpStream,
    connection: usize,
    stop: &AtomicBool,
    log: &Mutex<Vec<Record>>,
    routes: &Mutex<BTreeMap<String, String>>,
) {
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let mut reader = BufReader::new(socket);
    while !stop.load(Ordering::Relaxed) {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => break,
        }
        let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
        let mut headers = BTreeMap::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).is_err() {
                return;
            }
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.to_ascii_lowercase(), v.trim().into());
            }
        }
        log.lock().unwrap().push(Record {
            connection,
            path: path.clone(),
            headers: headers.clone(),
        });
        let scenario = headers
            .get("x-case")
            .map(String::as_str)
            .unwrap_or("document");
        let mut status = 200;
        let mut body = DOCUMENT.to_vec();
        let mut extra = String::new();
        let mut stall = false;
        let mut broken = false;
        if let Some(location) = routes.lock().unwrap().get(&path).cloned() {
            status = 302;
            extra = format!("Location: {location}\r\n");
            body.clear();
        } else if path == "/done" {
        } else if let Some(code) = scenario.strip_prefix("redirect-") {
            status = code.parse().unwrap();
            let location = headers
                .get("x-location")
                .map(String::as_str)
                .unwrap_or("/done");
            extra = format!("Location: {location}\r\n");
            body.clear();
        } else {
            match scenario {
                "gzip" | "gzip-large" => {
                    if scenario == "gzip-large" {
                        body.extend(vec![b' '; 2 << 20]);
                    }
                    let mut encoder =
                        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                    encoder.write_all(&body).unwrap();
                    body = encoder.finish().unwrap();
                    extra = "Content-Encoding: gzip\r\n".into();
                }
                "downgrade-chain" => {
                    status = 302;
                    extra = format!("Location: {}\r\n", headers["x-return-to"]);
                    body.clear();
                }
                "stall" => stall = true,
                "broken" => broken = true,
                "status-stall" => {
                    status = 403;
                    stall = true;
                    body = b"short".to_vec();
                }
                "absent" => {
                    status = 404;
                    body = b"absent".to_vec();
                }
                "gated" => {
                    status = 401;
                    body = b"gated".to_vec();
                    extra = "WWW-Authenticate: Bearer realm=dummy\r\n".into();
                }
                "invalid" => body = b"{".to_vec(),
                "version" => body = br#"{"openbindings":"0.1.0"}"#.to_vec(),
                _ => {}
            }
        }
        if let Some(content_type) = headers.get("x-content-type") {
            if content_type != "missing" {
                extra.push_str(&format!("Content-Type: {content_type}\r\n"));
            }
        } else {
            extra.push_str(&format!("Content-Type: {MEDIA_TYPE}\r\n"));
        }
        let head = format!(
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\n{extra}\r\n",
            body.len() + if broken { 20 } else { 0 }
        );
        if scenario == "slow-headers" {
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(2));
            }
            break;
        }
        if reader.get_mut().write_all(head.as_bytes()).is_err() {
            break;
        }
        if stall {
            let start = Instant::now();
            while !stop.load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(2) {
                thread::sleep(Duration::from_millis(2));
            }
            break;
        }
        if reader.get_mut().write_all(&body).is_err() || broken {
            break;
        }
    }
}
fn http(
    case: &str,
    extras: &[(&str, &str)],
    redirect: reqwest::redirect::Policy,
) -> reqwest::ClientBuilder {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-case", case.parse().unwrap());
    for (k, v) in extras {
        headers.insert(
            reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
            v.parse().unwrap(),
        );
    }
    reqwest::Client::builder()
        .no_proxy()
        .default_headers(headers)
        .redirect(redirect)
        .timeout(Duration::from_secs(2))
}
fn client(case: &str) -> native::Client {
    native::Client::new(
        http(case, &[], reqwest::redirect::Policy::limited(10)),
        ClientOptions::default(),
    )
    .unwrap()
}
#[tokio::test(flavor = "current_thread")]
async fn real_http_media_types_statuses_compression_and_location_independence() {
    let server = Server::new();
    for media in [
        MEDIA_TYPE,
        "application/json",
        "application/json; charset=utf-8",
        "missing",
        "text/plain",
    ] {
        let client = native::Client::new(
            http(
                "document",
                &[("x-content-type", media)],
                reqwest::redirect::Policy::default(),
            ),
            ClientOptions::default(),
        )
        .unwrap();
        let r = client.discover(&server.origin).await.unwrap();
        let DiscoveryOutcome::Found { document } = r.outcome else {
            panic!("media {media}: {:?}", r.outcome)
        };
        assert_eq!(r.body.as_deref(), Some(DOCUMENT));
        assert_eq!(
            r.metadata.unwrap().final_url,
            Some(format!("{}{WELL_KNOWN_PATH}", server.origin))
        );
        let refs = document.as_document().references().unwrap();
        assert!(refs.complete);
        assert!(matches!(
            refs.references[0].resolution,
            openbindings::ReferenceResolution::Located { .. }
        ));
    }
    for (case, expected) in [
        ("absent", "absent"),
        ("gated", "gated"),
        ("invalid", "invalid"),
        ("version", "version"),
        ("gzip", "decode"),
        ("gzip-large", "decode"),
        ("broken", "broken"),
    ] {
        let r = client(case).discover(&server.origin).await.unwrap();
        let actual = match r.outcome {
            DiscoveryOutcome::Absent => "absent",
            DiscoveryOutcome::Gated => {
                assert_eq!(
                    r.metadata.as_ref().unwrap().header("www-authenticate"),
                    Some(&b"Bearer realm=dummy"[..])
                );
                "gated"
            }
            DiscoveryOutcome::NonConformant { .. } => "invalid",
            DiscoveryOutcome::VersionRefused { .. } => "version",
            DiscoveryOutcome::Found { .. } => "found",
            DiscoveryOutcome::BodyLimit { .. } => "limit",
            DiscoveryOutcome::BodyError { error: failure } if failure.kind == ErrorKind::Decode => {
                "decode"
            }
            // truncated reads remain Network, not Decode.
            DiscoveryOutcome::BodyError { error: failure }
                if failure.kind == ErrorKind::Network =>
            {
                "broken"
            }
            x => panic!("{x:?}"),
        };
        assert_eq!(actual, expected);
    }
    for record in server.records.lock().unwrap().iter() {
        assert_eq!(record.path, WELL_KNOWN_PATH);
        assert_eq!(
            record.headers.get("accept").map(String::as_str),
            Some(ACCEPT)
        );
    }
}
#[tokio::test(flavor = "current_thread")]
async fn redirects_all_five_statuses_honor_policy_and_strip_cross_origin_credentials() {
    let server = Server::new();
    let other = Server::new();
    for status in [301, 302, 303, 307, 308] {
        let case = format!("redirect-{status}");
        let same = http(
            &case,
            &[("authorization", "Bearer dummy")],
            reqwest::redirect::Policy::limited(4),
        );
        let r = native::Client::new(same, ClientOptions::default())
            .unwrap()
            .discover(&server.origin)
            .await
            .unwrap();
        assert!(matches!(r.outcome, DiscoveryOutcome::Found { .. }));
        assert_eq!(
            r.metadata.unwrap().final_url,
            Some(format!("{}/done", server.origin))
        );
        assert_eq!(
            server
                .records
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .headers
                .get("authorization")
                .map(String::as_str),
            Some("Bearer dummy")
        );
        let dest = format!("{}/done", other.origin);
        let cross = http(
            &case,
            &[("authorization", "Bearer dummy"), ("x-location", &dest)],
            reqwest::redirect::Policy::limited(4),
        );
        let r = native::Client::new(cross, ClientOptions::default())
            .unwrap()
            .discover(&server.origin)
            .await
            .unwrap();
        assert!(matches!(r.outcome, DiscoveryOutcome::Found { .. }));
        assert_eq!(r.metadata.unwrap().final_url, Some(dest));
        assert!(
            !other
                .records
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .headers
                .contains_key("authorization")
        );
        let manual = http(&case, &[], reqwest::redirect::Policy::none());
        let r = native::Client::with_delegated_policy(
            manual.build().unwrap(),
            ClientOptions::default(),
        )
        .unwrap()
        .discover(&server.origin)
        .await
        .unwrap();
        // F2: a delegated host that did not follow a visible Location is a policy refusal.
        assert!(matches!(
            r.outcome,
            DiscoveryOutcome::TransportError {
                error: RequestError {
                    kind: ErrorKind::RedirectPolicy,
                    ..
                }
            }
        ));
        assert_eq!(r.metadata.unwrap().status, status);
    }
    let looping = http(
        "redirect-302",
        &[("x-location", WELL_KNOWN_PATH)],
        reqwest::redirect::Policy::limited(2),
    );
    let r =
        native::Client::with_delegated_policy(looping.build().unwrap(), ClientOptions::default())
            .unwrap()
            .discover(&server.origin)
            .await
            .unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::TransportError { .. }));
}
#[tokio::test(flavor = "current_thread")]
async fn cancellation_timeout_bounded_status_cleanup_and_connection_reuse() {
    let server = Server::new();
    let client = client("stall");
    let control = CancellationToken::new();
    let (result, ()) = tokio::join!(
        client.discover_with_control(&server.origin, &control),
        async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            control.cancel();
        }
    );
    assert!(matches!(
        result.unwrap().outcome,
        DiscoveryOutcome::Cancelled
    ));
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-case", "stall".parse().unwrap());
    let timed = reqwest::Client::builder()
        .no_proxy()
        .default_headers(headers)
        .timeout(Duration::from_millis(20));
    let r = native::Client::new(timed, ClientOptions::default())
        .unwrap()
        .discover(&server.origin)
        .await
        .unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::Timeout));
    let control = CancellationToken::new();
    let start = Instant::now();
    let r = self::client("status-stall")
        .discover_with_control(&server.origin, &control)
        .await
        .unwrap();
    assert!(matches!(r.outcome, DiscoveryOutcome::Gated));
    assert!(!control.is_cancelled());
    assert!(start.elapsed() < Duration::from_millis(700));
    let reuse = self::client("absent");
    for _ in 0..2 {
        assert!(matches!(
            reuse.discover(&server.origin).await.unwrap().outcome,
            DiscoveryOutcome::Absent
        ));
    }
    let log = server.records.lock().unwrap();
    assert_eq!(log[log.len() - 1].connection, log[log.len() - 2].connection);
}

// generate a private CA at test time and run a real TLS fixture.
struct TlsServer {
    origin: String,
    root: std::path::PathBuf,
    child: std::process::Child,
}
impl TlsServer {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "obi-tls-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut child = std::process::Command::new("python3")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tls_fixture.py"))
            .arg(&root)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut origin = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut origin)
            .unwrap();
        let fixture = Self {
            origin: origin.trim().into(),
            root,
            child,
        };
        assert!(
            fixture.origin.starts_with("https://"),
            "TLS fixture failed to bind or generate its private CA"
        );
        fixture
    }
    fn builder(&self) -> reqwest::ClientBuilder {
        let ca = reqwest::Certificate::from_pem(&std::fs::read(self.root.join("ca.pem")).unwrap())
            .unwrap();
        reqwest::Client::builder().no_proxy().tls_certs_only([ca])
    }
}
impl Drop for TlsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn tls_uses_private_ca_and_downgrade_chain_is_refused_before_plaintext_dispatch() {
    // a final HTTPS target cannot excuse the intervening HTTP hop.
    let tls = TlsServer::new();
    let plain = Server::new();
    let ordinary = native::Client::new(
        reqwest::Client::builder().no_proxy(),
        ClientOptions::default(),
    )
    .unwrap();
    let untrusted = ordinary.discover(&tls.origin).await.unwrap();
    assert!(matches!(
        untrusted.outcome,
        DiscoveryOutcome::TransportError { .. }
    ));
    let trusted = native::Client::new(tls.builder(), ClientOptions::default()).unwrap();
    assert!(matches!(
        trusted.discover(&tls.origin).await.unwrap().outcome,
        DiscoveryOutcome::Found { .. }
    ));
    // an HTTP literal-loopback request may upgrade to the trusted TLS fixture.
    let mut upgrade_headers = reqwest::header::HeaderMap::new();
    upgrade_headers.insert("x-case", "redirect-302".parse().unwrap());
    upgrade_headers.insert(
        "x-location",
        format!("{}/done", tls.origin).parse().unwrap(),
    );
    let upgrade = native::Client::new(
        tls.builder().default_headers(upgrade_headers),
        ClientOptions::default(),
    )
    .unwrap();
    let upgraded = upgrade.discover(&plain.origin).await.unwrap();
    assert!(matches!(upgraded.outcome, DiscoveryOutcome::Found { .. }));
    assert_eq!(
        upgraded.metadata.unwrap().final_url,
        Some(format!("{}/done", tls.origin))
    );
    plain.records.lock().unwrap().clear();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-case", "downgrade-chain".parse().unwrap());
    headers.insert(
        "x-location",
        format!("{}{WELL_KNOWN_PATH}", plain.origin)
            .parse()
            .unwrap(),
    );
    headers.insert(
        "x-return-to",
        format!("{}/done", tls.origin).parse().unwrap(),
    );
    let client = native::Client::new(
        tls.builder().default_headers(headers),
        ClientOptions::default(),
    )
    .unwrap();
    let r = client.discover(&tls.origin).await.unwrap();
    assert!(matches!(
        r.outcome,
        DiscoveryOutcome::TransportError {
            error: RequestError {
                kind: ErrorKind::RedirectPolicy,
                ..
            }
        }
    ));
    assert!(
        plain.records.lock().unwrap().is_empty(),
        "plaintext downgrade was dispatched"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn safe_native_constructor_follows_five_statuses_relative_locations_and_ten_hops() {
    // SDK policy is installed last, overriding a builder's old redirect policy.
    let server = Server::new();
    for status in [301, 302, 303, 307, 308] {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-case", format!("redirect-{status}").parse().unwrap());
        headers.insert("x-location", "../done".parse().unwrap());
        let client = native::Client::new(
            reqwest::Client::builder()
                .no_proxy()
                .default_headers(headers)
                .redirect(reqwest::redirect::Policy::none()),
            ClientOptions::default(),
        )
        .unwrap();
        let r = client.discover(&server.origin).await.unwrap();
        assert!(matches!(r.outcome, DiscoveryOutcome::Found { .. }));
        assert_eq!(
            r.metadata.unwrap().final_url,
            Some(format!("{}/done", server.origin))
        );
    }
    server.records.lock().unwrap().clear();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-case", "redirect-302".parse().unwrap());
    headers.insert("x-location", WELL_KNOWN_PATH.parse().unwrap());
    let client = native::Client::new(
        reqwest::Client::builder()
            .no_proxy()
            .default_headers(headers),
        ClientOptions::default(),
    )
    .unwrap();
    let r = client.discover(&server.origin).await.unwrap();
    assert!(matches!(
        r.outcome,
        DiscoveryOutcome::TransportError {
            error: RequestError {
                kind: ErrorKind::RedirectPolicy,
                ..
            }
        }
    ));
    assert_eq!(server.records.lock().unwrap().len(), MAX_REDIRECTS + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn normalized_loopback_http_and_non_loopback_admission() {
    // metadata agrees with reqwest's actual requested URL.
    let server = Server::new();
    let client = native::Client::new(
        reqwest::Client::builder().no_proxy(),
        ClientOptions::default(),
    )
    .unwrap();
    for host in ["127.0.0.1", "2130706433", "0x7f.1", "%31%32%37.0.0.1"] {
        let origin = server.origin.replace("127.0.0.1", host);
        let r = client.discover(&origin).await.unwrap();
        assert!(matches!(r.outcome, DiscoveryOutcome::Found { .. }));
        let metadata = r.metadata.unwrap();
        assert_eq!(
            metadata.requested_url,
            format!("{}{WELL_KNOWN_PATH}", server.origin)
        );
        assert_eq!(
            metadata.final_url.as_deref(),
            Some(metadata.requested_url.as_str())
        );
    }
    for origin in [
        "http://example.test",
        "http://localhost",
        "http://127.attacker.test",
    ] {
        assert_eq!(
            client.discover(origin).await.unwrap_err().code,
            "origin-policy"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn sdk_deadline_covers_slow_headers_and_slow_body() {
    // no caller/client timeout is needed, and metadata survives body expiry.
    let server = Server::new();
    for case in ["slow-headers", "stall"] {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-case", case.parse().unwrap());
        let client = native::Client::new(
            reqwest::Client::builder()
                .no_proxy()
                .default_headers(headers),
            ClientOptions {
                timeout: AttemptTimeout::After(Duration::from_millis(40)),
                ..Default::default()
            },
        )
        .unwrap();
        let r = client.discover(&server.origin).await.unwrap();
        assert!(matches!(r.outcome, DiscoveryOutcome::Timeout));
        assert_eq!(r.metadata.is_some(), case == "stall");
        assert!(r.body.is_none());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn native_pre_cancelled_attempt_has_no_dispatch_and_overrides_are_explicit() {
    // cancellation wins before dispatch, including an otherwise expiring attempt.
    let control = CancellationToken::new();
    control.cancel();
    let client = native::Client::new(
        reqwest::Client::builder().no_proxy(),
        ClientOptions {
            timeout: AttemptTimeout::After(Duration::from_nanos(1)),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        client
            .discover_with_control("https://example.test", &control)
            .await
            .unwrap()
            .outcome,
        DiscoveryOutcome::Cancelled
    ));
    assert!(
        native::Client::new(
            reqwest::Client::builder(),
            ClientOptions {
                timeout: AttemptTimeout::After(Duration::ZERO),
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn native_builder_retains_application_resolver_policy() {
    // application address filtering remains effective with SDK redirects installed.
    struct DenyResolver(Arc<std::sync::atomic::AtomicUsize>);
    impl reqwest::dns::Resolve for DenyResolver {
        fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
            self.0.fetch_add(1, Ordering::Relaxed);
            Box::pin(async {
                Err(std::io::Error::other("application resolver refused address").into())
            })
        }
    }
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let client = native::Client::new(
        reqwest::Client::builder()
            .no_proxy()
            .dns_resolver(Arc::new(DenyResolver(calls.clone()))),
        ClientOptions::default(),
    )
    .unwrap();
    let r = client.discover("https://discovery.test").await.unwrap();
    assert!(matches!(
        r.outcome,
        DiscoveryOutcome::TransportError {
            error: RequestError {
                kind: ErrorKind::Network,
                ..
            }
        }
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn native_cancellation_wins_when_timer_and_caller_are_ready_together() {
    // hold resolution pending, then make both causes ready before the next poll.
    struct PendingResolver;
    impl reqwest::dns::Resolve for PendingResolver {
        fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
            Box::pin(std::future::pending())
        }
    }
    let client = native::Client::new(
        reqwest::Client::builder()
            .no_proxy()
            .dns_resolver(Arc::new(PendingResolver)),
        ClientOptions {
            timeout: AttemptTimeout::After(Duration::from_millis(50)),
            ..Default::default()
        },
    )
    .unwrap();
    let control = CancellationToken::new();
    let (result, ()) = tokio::join!(
        client.discover_with_control("https://discovery.test", &control),
        async {
            tokio::task::yield_now().await;
            // Simulate a suspended host; the timer cannot preempt synchronous work.
            std::thread::sleep(Duration::from_millis(75));
            control.cancel();
        }
    );
    assert!(matches!(
        result.unwrap().outcome,
        DiscoveryOutcome::Cancelled
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn credential_origin_chains_refuse_only_cross_origin_then_same_origin() {
    // never replay A's default credentials on B -> B.
    let a = Server::new();
    let b = Server::new();
    for chain in ["A-B-B", "A-B", "A-A-B", "A-B-A"] {
        a.redirects.lock().unwrap().clear();
        b.redirects.lock().unwrap().clear();
        a.records.lock().unwrap().clear();
        b.records.lock().unwrap().clear();
        let initial_target = if chain == "A-A-B" {
            format!("{}/first", a.origin)
        } else if chain == "A-B" {
            format!("{}/done", b.origin)
        } else {
            format!("{}/first", b.origin)
        };
        a.redirects
            .lock()
            .unwrap()
            .insert(WELL_KNOWN_PATH.into(), initial_target);
        match chain {
            "A-B-B" => {
                b.redirects
                    .lock()
                    .unwrap()
                    .insert("/first".into(), format!("{}/done", b.origin));
            }
            "A-A-B" => {
                a.redirects
                    .lock()
                    .unwrap()
                    .insert("/first".into(), format!("{}/done", b.origin));
            }
            "A-B-A" => {
                b.redirects
                    .lock()
                    .unwrap()
                    .insert("/first".into(), format!("{}/done", a.origin));
            }
            _ => {}
        }
        let client = native::Client::new(
            http(
                "document",
                &[
                    ("authorization", "Bearer source-secret"),
                    ("cookie", "source-secret=1"),
                    ("proxy-authorization", "Basic source-secret"),
                ],
                reqwest::redirect::Policy::default(),
            ),
            ClientOptions::default(),
        )
        .unwrap();
        let r = client.discover(&a.origin).await.unwrap();
        let b_log = b.records.lock().unwrap();
        assert_eq!(b_log.len(), 1, "{chain}: a second request reached B");
        for header in ["authorization", "cookie", "proxy-authorization"] {
            assert!(
                !b_log[0].headers.contains_key(header),
                "{chain}: B received {header}"
            );
        }
        if chain == "A-B-B" {
            assert!(matches!(
                r.outcome,
                DiscoveryOutcome::TransportError {
                    error: RequestError {
                        kind: ErrorKind::RedirectPolicy,
                        ..
                    }
                }
            ));
        } else {
            assert!(
                matches!(r.outcome, DiscoveryOutcome::Found { .. }),
                "{chain}: {:?}",
                r.outcome
            );
            let a_log = a.records.lock().unwrap();
            assert_eq!(
                a_log[0].headers.get("authorization").map(String::as_str),
                Some("Bearer source-secret")
            );
            if chain == "A-A-B" {
                assert!(a_log[1].headers.contains_key("authorization"));
            }
            if chain == "A-B-A" {
                assert!(!a_log[1].headers.contains_key("authorization"));
            }
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_native_location_is_a_typed_policy_refusal() {
    // tower-http can return this response without consulting the policy.
    let server = Server::new();
    for status in [301, 302, 303, 307, 308] {
        let client = native::Client::new(
            http(
                &format!("redirect-{status}"),
                &[("x-location", "http://[")],
                reqwest::redirect::Policy::default(),
            ),
            ClientOptions::default(),
        )
        .unwrap();
        let r = client.discover(&server.origin).await.unwrap();
        assert!(matches!(
            r.outcome,
            DiscoveryOutcome::TransportError {
                error: RequestError {
                    kind: ErrorKind::RedirectPolicy,
                    ..
                }
            }
        ));
        assert_eq!(r.metadata.unwrap().status, status);
        assert!(r.body.is_none());
    }
    assert_eq!(server.records.lock().unwrap().len(), 5);
}

#[tokio::test(flavor = "current_thread")]
async fn truncated_native_body_is_network_without_coding_evidence() {
    // retain upstream explanations without interpreting wording.
    let server = Server::new();
    let mut response = http("broken", &[], reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!("{}{WELL_KNOWN_PATH}", server.origin))
        .send()
        .await
        .unwrap();
    let error = loop {
        match response.chunk().await {
            Err(error) => break error.without_url(),
            Ok(Some(_)) => {}
            Ok(None) => panic!("truncated body unexpectedly reached EOF"),
        }
    };
    let r = client("broken").discover(&server.origin).await.unwrap();
    let DiscoveryOutcome::BodyError { error: failure } = r.outcome else {
        panic!("{:?}", r.outcome);
    };
    assert_eq!(failure.kind, ErrorKind::Network);
    assert!(failure.message.contains(&error.to_string()));
    let mut source = std::error::Error::source(&error);
    assert!(source.is_some(), "no upstream source explanation");
    while let Some(error) = source {
        assert!(failure.message.contains(&error.to_string()));
        source = error.source();
    }
    assert_eq!(r.metadata.unwrap().status, 200);
    assert!(r.body.is_none());
}
