//! Native discovery with SDK redirect policy and, when the application sets a
//! timeout, one Tokio attempt timer. Application builders retain TLS,
//! credentials, proxy, resolver and decoding choices.
//!
//! Discovery must run on a Tokio runtime with timers enabled (`enable_time` or
//! `enable_all`), as reqwest's own timeouts require; without them Tokio panics
//! when the SDK creates a timer: the attempt timer when
//! [`ClientOptions::timeout`] is set, and the bounded wait that drains a small
//! non-200 body for connection reuse. Tokio offers no way to detect a disabled
//! timer driver before that point, so the SDK cannot return an error instead.
use crate::CancellationToken;
use crate::*;

#[derive(Clone)]
/// Reusable reqwest discovery adapter. No global client or response cache is installed.
pub struct Client {
    http: reqwest::Client,
    options: ClientOptions,
}
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Build the application builder after installing the SDK redirect policy last.
    /// HTTP is allowed only for literal loopback; every HTTPS-to-HTTP hop is
    /// refused before dispatch, and at most ten redirects are followed. A foreign-origin
    /// hop followed by a same-origin hop is refused to prevent upstream header replay.
    ///
    /// Redirects keep reqwest's execution, so the policy sees only the URL that
    /// tower-http's WHATWG join resolved. A Location that is not an RFC 3986
    /// URI-reference is recovered, as RFC 9110 section 10.2.2 permits: a space is
    /// percent-encoded and a backslash is read as a slash, so `/tar get` is requested
    /// as `/tar%20get`. Portable [`crate::discover`] refuses such a Location before a
    /// second request; both admit the URL actually requested at every hop.
    ///
    /// The application scopes credentials across origins. On a cross-origin hop
    /// reqwest removes only `Authorization`, `Cookie`, `Proxy-Authorization` and
    /// `WWW-Authenticate`; another credential header, such as an `x-api-key` in
    /// the builder's default headers, reaches the redirect target. Carry such a
    /// credential in `Authorization`, or refuse foreign-origin hops in a client
    /// passed to [`Self::with_delegated_policy`].
    ///
    /// # Errors
    /// Returns a configuration error for invalid byte or timeout limits, or if the configured HTTP client cannot be built.
    pub fn new(
        builder: reqwest::ClientBuilder,
        options: ClientOptions,
    ) -> Result<Self, ConfigurationError> {
        validate_options(options)?;
        let http = builder
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() > MAX_REDIRECTS {
                    return attempt.error("discovery redirect limit exceeded");
                }
                if let Some(previous) = attempt.previous().last()
                    && let Err(message) = admit_redirect(previous, attempt.url())
                {
                    return attempt.error(message);
                }
                if let (Some(first), Some(previous)) =
                    (attempt.previous().first(), attempt.previous().last())
                    && credential_replay_hop(first, previous, attempt.url())
                {
                    return attempt
                        .error("redirect would replay original credentials to a foreign origin");
                }
                attempt.follow()
            }))
            .build()
            .map_err(|_| ConfigurationError {
                code: "http-client-build",
                message: "could not build the configured discovery HTTP client",
            })?;
        Ok(Self { http, options })
    }
    /// Retain an already-built client through explicit policy delegation.
    /// The application MUST enforce the same per-hop admission and ten-redirect
    /// limit before dispatch. Applications needing a foreign-origin hop followed by
    /// a same-origin hop must own credential policy to prevent original-header replay
    /// by tower-http 0.6.11 / reqwest 0.13.5. A final URL cannot prove hop safety.
    /// TLS, resolved-address, proxy, credential and decoding restrictions remain
    /// the application's responsibility on both construction paths. The SDK
    /// still owns the optional whole-attempt timer and the caller cancellation race.
    ///
    /// # Errors
    /// Returns a configuration error for invalid byte or timeout limits.
    pub fn with_delegated_policy(
        http: reqwest::Client,
        options: ClientOptions,
    ) -> Result<Self, ConfigurationError> {
        validate_options(options)?;
        Ok(Self { http, options })
    }
    /// Run one discovery attempt with a fresh uncancelled control and the retained HTTP client.
    ///
    /// # Errors
    /// Returns a configuration error for an invalid origin, byte limit, or timeout. HTTP, transport, body, and document failures are represented in the returned discovery outcome.
    pub async fn discover(&self, origin: &str) -> Result<DiscoveryResult, ConfigurationError> {
        self.discover_with_control(origin, &CancellationToken::new())
            .await
    }
    /// Run one cooperatively cancellable discovery attempt. Request/body work can be interrupted; synchronous validation observes cancellation only at its boundaries.
    ///
    /// # Errors
    /// Returns a configuration error for an invalid origin, byte limit, or timeout. HTTP, transport, body, and document failures are represented in the returned discovery outcome.
    pub async fn discover_with_control(
        &self,
        origin: &str,
        control: &CancellationToken,
    ) -> Result<DiscoveryResult, ConfigurationError> {
        validate_options(self.options)?;
        let timer = Timer(tokio::time::Instant::now());
        self.options
            .discover_using(
                origin,
                control,
                &timer,
                |request| async move {
                    let response = self
                        .http
                        .get(request.url)
                        .header(reqwest::header::ACCEPT, request.accept)
                        .send()
                        .await
                        .map_err(failure)?;
                    let status = response.status().as_u16();
                    let final_url = Some(response.url().as_str().into());
                    let headers = response
                        .headers()
                        .iter()
                        .map(|(k, v)| Header::new(k.as_str(), v.as_bytes()))
                        .collect();
                    let unsupported_coding = response
                        .headers()
                        .get_all(reqwest::header::CONTENT_ENCODING)
                        .iter()
                        .any(|value| {
                            value.to_str().map_or(true, |value| {
                                value
                                    .split(',')
                                    .any(|coding| !coding.trim().eq_ignore_ascii_case("identity"))
                            })
                        });
                    let may_drain = matches!(
                        response.version(),
                        reqwest::Version::HTTP_09
                            | reqwest::Version::HTTP_10
                            | reqwest::Version::HTTP_11
                    ) && response.content_length().is_none_or(|n| n <= 2048);
                    Ok(HttpResponse {
                        status,
                        final_url,
                        headers,
                        body: Body {
                            response,
                            pending: bytes::Bytes::new(),
                            offset: 0,
                            may_drain,
                            unsupported_coding,
                        },
                    })
                },
                false,
            )
            .await
    }
}
fn failure(error: reqwest::Error) -> RequestError {
    RequestError {
        kind: if error.is_timeout() {
            ErrorKind::Timeout
        } else if error.is_redirect() {
            ErrorKind::RedirectPolicy
        } else if error.is_builder() {
            ErrorKind::Other
        } else {
            ErrorKind::Network
        },
        message: error.without_url().to_string(),
    }
}
fn body_failure(error: reqwest::Error) -> RequestError {
    RequestError {
        kind: if error.is_timeout() {
            ErrorKind::Timeout
        } else {
            ErrorKind::Network
        },
        message: message_with_sources(&error.without_url()),
    }
}
fn same_origin(a: &url::Url, b: &url::Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}
fn credential_replay_hop(first: &url::Url, previous: &url::Url, next: &url::Url) -> bool {
    // F1: tower-http 0.6.11 follow_redirect/mod.rs:327 restores the original headers
    // for each hop. reqwest 0.13.5 redirect.rs:239 strips credentials only relative
    // to the immediately previous origin. A -> B -> B therefore restores A's secrets.
    same_origin(previous, next) && !same_origin(first, previous)
}
fn message_with_sources(error: &(dyn std::error::Error + 'static)) -> String {
    // reqwest exposes no documented codec diagnosis for chunk errors.
    // Retain upstream Display/source explanations for humans without interpreting them.
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }
    message
}
struct Body {
    response: reqwest::Response,
    pending: bytes::Bytes,
    offset: usize,
    may_drain: bool,
    unsupported_coding: bool,
}
impl ResponseBody for Body {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize, RequestError> {
        if self.unsupported_coding {
            return Err(RequestError {
                kind: ErrorKind::Decode,
                message: "host did not decode the response content coding".into(),
            });
        }
        if output.is_empty() {
            return Ok(0);
        }
        while self.offset == self.pending.len() {
            match self.response.chunk().await.map_err(body_failure)? {
                None => return Ok(0),
                Some(chunk) => {
                    self.pending = chunk;
                    self.offset = 0;
                }
            }
        }
        let count = output.len().min(self.pending.len() - self.offset);
        output[..count].copy_from_slice(&self.pending[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
    async fn discard_status(&mut self) {
        if !self.may_drain {
            return;
        }
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), async {
            let mut buffer = [0; 2048];
            let mut remaining = buffer.len();
            while remaining > 0 {
                match self.read(&mut buffer[..remaining]).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => remaining -= n,
                }
            }
        })
        .await;
    }
}

struct Timer(tokio::time::Instant);
impl DiscoveryTimer for Timer {
    fn now(&self) -> std::time::Duration {
        self.0.elapsed()
    }
    async fn sleep_until(&self, deadline: std::time::Duration) {
        tokio::time::sleep_until(self.0 + deadline).await;
    }
}

fn validate_options(options: ClientOptions) -> Result<(), ConfigurationError> {
    options.byte_limit()?;
    if let Some(duration) = options.attempt_timeout()?
        && tokio::time::Instant::now().checked_add(duration).is_none()
    {
        return Err(ConfigurationError {
            code: "invalid-timeout",
            message: "attempt deadline exceeds the native clock range",
        });
    }
    Ok(())
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn credential_replay_guard_uses_reqwest_origin_comparison() {
        // path changes do not authorize original headers on B.
        let a = url::Url::parse("https://a.test/start").unwrap();
        let b = url::Url::parse("https://b.test/first").unwrap();
        for (previous, next, refused) in [
            (&b, "https://b.test:443/final", true),
            (&b, "https://b.test:444/final", false),
            (&b, "http://b.test/final", false),
            (&b, "https://a.test/final", false),
            (&a, "https://a.test/next", false),
            (&a, "https://b.test/first", false),
        ] {
            assert_eq!(
                credential_replay_hop(&a, previous, &url::Url::parse(next).unwrap()),
                refused,
                "{next}"
            );
        }
    }
    #[test]
    fn upstream_body_messages_are_retained_without_interpretation() {
        // arbitrary upstream wording survives for human diagnosis.
        #[derive(Debug)]
        struct Frame(std::io::Error);
        impl std::fmt::Display for Frame {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("response frame failed")
            }
        }
        impl std::error::Error for Frame {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let detail = "arbitrary upstream explanation";
        let error = Frame(std::io::Error::other(detail));
        let message = message_with_sources(&error);
        assert!(message.contains(&error.to_string()));
        assert!(message.contains(detail));
    }
}
