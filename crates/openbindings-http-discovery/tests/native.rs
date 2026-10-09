#![cfg(feature = "native")]
use openbindings::WorkControl;
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
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let records = Arc::new(Mutex::new(Vec::new()));
        let stop = stopped.clone();
        let log = records.clone();
        let handle = thread::spawn(move || {
            let mut children = Vec::new();
            let mut connection = 0;
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        connection += 1;
                        let stop = stop.clone();
                        let log = log.clone();
                        children.push(thread::spawn(move || {
                            serve(socket, connection, &stop, &log)
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
fn serve(socket: TcpStream, connection: usize, stop: &AtomicBool, log: &Mutex<Vec<Record>>) {
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
        if path == "/done" {
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
) -> reqwest::Client {
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
        .build()
        .unwrap()
}
fn client(case: &str) -> native::Client {
    native::Client::new(
        http(case, &[], reqwest::redirect::Policy::limited(10)),
        ClientOptions::default(),
    )
    .unwrap()
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "run by the isolated TLS fixture harness with a temporary test CA"]
async fn tls_uses_caller_trust_roots_without_changing_system_trust() {
    let origin = std::env::var("OB_HTTP_TEST_ORIGIN").expect("TLS harness origin");
    let ca = std::fs::read(std::env::var("OB_HTTP_TEST_CA").expect("TLS harness CA path")).unwrap();
    let ordinary = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let untrusted = native::Client::new(ordinary, ClientOptions::default())
        .unwrap()
        .discover(&origin)
        .await
        .unwrap();
    assert!(matches!(
        untrusted.outcome,
        DiscoveryOutcome::TransportFailure { .. }
    ));
    assert!(untrusted.metadata.is_none());
    let certificate = reqwest::Certificate::from_pem(&ca).unwrap();
    let trusted = reqwest::Client::builder()
        .no_proxy()
        .tls_certs_only([certificate])
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let result = native::Client::new(trusted, ClientOptions::default())
        .unwrap()
        .discover(&origin)
        .await
        .unwrap();
    assert!(matches!(result.outcome, DiscoveryOutcome::Found { .. }));
    assert!(
        result
            .metadata
            .unwrap()
            .final_url
            .unwrap()
            .starts_with("https://")
    );
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
        let refs = document.parsed().references().unwrap();
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
        ("gzip", "found"),
        ("gzip-large", "limit"),
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
            DiscoveryOutcome::BodyFailure { .. } => "broken",
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
        let r = native::Client::new(manual, ClientOptions::default())
            .unwrap()
            .discover(&server.origin)
            .await
            .unwrap();
        assert!(matches!(r.outcome, DiscoveryOutcome::HttpStatus));
        assert_eq!(r.metadata.unwrap().status, status);
    }
    let looping = http(
        "redirect-302",
        &[("x-location", WELL_KNOWN_PATH)],
        reqwest::redirect::Policy::limited(2),
    );
    let r = native::Client::new(looping, ClientOptions::default())
        .unwrap()
        .discover(&server.origin)
        .await
        .unwrap();
    assert!(matches!(
        r.outcome,
        DiscoveryOutcome::TransportFailure { .. }
    ));
}
#[tokio::test(flavor = "current_thread")]
async fn cancellation_timeout_bounded_status_cleanup_and_connection_reuse() {
    let server = Server::new();
    let client = client("stall");
    let control = WorkControl::new();
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
        .timeout(Duration::from_millis(20))
        .build()
        .unwrap();
    let r = native::Client::new(timed, ClientOptions::default())
        .unwrap()
        .discover(&server.origin)
        .await
        .unwrap();
    assert!(matches!(
        r.outcome,
        DiscoveryOutcome::BodyFailure {
            failure: RequestFailure {
                kind: FailureKind::Timeout,
                ..
            }
        }
    ));
    let control = WorkControl::new();
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
