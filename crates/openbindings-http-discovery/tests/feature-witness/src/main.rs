// HD-6: this is an application dependency graph, not an SDK dev-feature toggle.
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    thread,
};
#[cfg(any(feature = "sdk", feature = "compression"))]
const DOCUMENT: &[u8] = br#"{"openbindings":"0.2.0","operations":{}}"#;
const GZIP: &[u8] = &[
    31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 171, 86, 202, 47, 72, 205, 75, 202, 204, 75, 201, 204, 75,
    47, 86, 178, 82, 50, 208, 51, 210, 51, 80, 210, 1, 9, 23, 37, 150, 100, 230, 231, 1, 5, 171,
    107, 107, 1, 10, 184, 38, 208, 40, 0, 0, 0,
];
#[cfg(feature = "compression")]
const DEFLATE: &[u8] = &[
    120, 156, 171, 86, 202, 47, 72, 205, 75, 202, 204, 75, 201, 204, 75, 47, 86, 178, 82, 50, 208,
    51, 210, 51, 80, 210, 1, 9, 23, 37, 150, 100, 230, 231, 1, 5, 171, 107, 107, 1, 19, 127, 13,
    143,
];
#[cfg(feature = "compression")]
const BROTLI: &[u8] = &[
    27, 39, 0, 32, 28, 167, 197, 154, 209, 116, 28, 140, 41, 181, 136, 203, 220, 222, 130, 145,
    201, 62, 29, 22, 216, 128, 3, 230, 32, 144, 112, 64, 57, 43, 24, 80, 7, 78, 113, 103, 26, 137,
    240, 29,
];
fn server(body: &'static [u8], coding: &'static str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream);
        let mut encoding = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if line.to_ascii_lowercase().starts_with("accept-encoding:") {
                encoding = line;
            }
        }
        write!(reader.get_mut(), "HTTP/1.1 200 OK\r\nContent-Encoding: {coding}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        let _ = reader.get_mut().write_all(body);
        encoding
    });
    (origin, handle)
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (origin, worker) = server(GZIP, "gzip");
    let bytes = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(&origin)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let encoding = worker.join().unwrap();
    #[cfg(not(feature = "compression"))]
    {
        assert!(
            encoding.is_empty(),
            "SDK enabled unrelated-client Accept-Encoding: {encoding}"
        );
        assert_eq!(
            bytes.as_ref(),
            GZIP,
            "SDK enabled unrelated-client decoding"
        );
    }
    #[cfg(feature = "compression")]
    {
        assert!(encoding.contains("gzip"));
        assert_eq!(bytes.as_ref(), DOCUMENT);
    }
    #[cfg(feature = "sdk")]
    {
        use openbindings_http_discovery::*;
        let (origin, worker) = server(GZIP, "gzip");
        let client = native::Client::new(
            reqwest::Client::builder().no_proxy(),
            ClientOptions {
                max_document_bytes: DOCUMENT.len() - 1,
                ..Default::default()
            },
        )
        .unwrap();
        let r = client.discover(&origin).await.unwrap();
        worker.join().unwrap();
        #[cfg(feature = "compression")]
        assert!(
            matches!(r.outcome, DiscoveryOutcome::BodyLimit { .. }),
            "decoded-byte bound: {:?}",
            r.outcome
        );
        #[cfg(not(feature = "compression"))]
        assert!(
            matches!(
                r.outcome,
                DiscoveryOutcome::BodyError {
                    error: RequestError {
                        kind: ErrorKind::Decode,
                        ..
                    }
                }
            ),
            "unsupported coding: {:?}",
            r.outcome
        );
        #[cfg(feature = "compression")]
        {
            // HD-6: each application-enabled decoder retains the decoded-byte bound.
            for (bytes, coding) in [(GZIP, "gzip"), (DEFLATE, "deflate"), (BROTLI, "br")] {
                let (origin, worker) = server(bytes, coding);
                let r = client.discover(&origin).await.unwrap();
                worker.join().unwrap();
                assert!(
                    matches!(r.outcome, DiscoveryOutcome::BodyLimit { .. }),
                    "{coding}: {:?}",
                    r.outcome
                );
            }
            // S5b review F4 / HD-7: corrupt codecs are Network, with upstream text retained.
            for (bytes, coding) in [
                (b"corrupt gzip".as_slice(), "gzip"),
                (b"\xff\xff\xff\xff".as_slice(), "deflate"),
                (b"\xff\xff\xff\xff".as_slice(), "br"),
            ] {
                let (origin, worker) = server(bytes, coding);
                let mut response = reqwest::Client::builder()
                    .no_proxy()
                    .build()
                    .unwrap()
                    .get(&origin)
                    .send()
                    .await
                    .unwrap();
                let error = loop {
                    match response.chunk().await {
                        Err(error) => break error.without_url(),
                        Ok(Some(_)) => {}
                        Ok(None) => panic!("corrupt {coding} unexpectedly reached EOF"),
                    }
                };
                worker.join().unwrap();
                let mut upstream_messages = vec![error.to_string()];
                let mut source = std::error::Error::source(&error);
                while let Some(error) = source {
                    upstream_messages.push(error.to_string());
                    source = error.source();
                }
                assert!(
                    upstream_messages.len() > 1,
                    "no upstream source explanation"
                );
                let (origin, worker) = server(bytes, coding);
                let r = client.discover(&origin).await.unwrap();
                worker.join().unwrap();
                let DiscoveryOutcome::BodyError { error: failure } = r.outcome else {
                    panic!("corrupt {coding}: {:?}", r.outcome);
                };
                assert_eq!(failure.kind, ErrorKind::Network, "corrupt {coding}");
                for message in upstream_messages {
                    assert!(
                        failure.message.contains(&message),
                        "{coding}: upstream explanation was lost: {message:?} in {:?}",
                        failure.message
                    );
                }
                assert_eq!(r.metadata.unwrap().status, 200);
                assert!(r.body.is_none());
            }
        }
    }
}
