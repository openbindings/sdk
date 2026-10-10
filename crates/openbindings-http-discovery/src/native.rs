//! Optional reqwest adapter. Configure TLS, redirects, credentials, network
//! restrictions and deadlines on the supplied client before sharing it.
use crate::*;
use openbindings::WorkControl;

#[derive(Clone)]
/// Reusable native reqwest discovery adapter. The supplied HTTP client owns TLS, redirects, credentials, connection reuse and deadlines; no global client or response cache is installed.
pub struct Client {
    http: reqwest::Client,
    options: ClientOptions,
}
impl Client {
    /// Validate options and retain the caller-configured reqwest client. Does not perform acquisition.
    pub fn new(http: reqwest::Client, options: ClientOptions) -> Result<Self, ConfigurationError> {
        options.byte_limit()?;
        Ok(Self { http, options })
    }
    /// Run one discovery attempt with a fresh uncancelled control and the retained HTTP client.
    pub async fn discover(&self, origin: &str) -> Result<DiscoveryResult, ConfigurationError> {
        self.discover_with_control(origin, &WorkControl::new())
            .await
    }
    /// Run one cooperatively cancellable discovery attempt. Request/body work can be interrupted; synchronous assessment observes cancellation only at its boundaries.
    pub async fn discover_with_control(
        &self,
        origin: &str,
        control: &WorkControl,
    ) -> Result<DiscoveryResult, ConfigurationError> {
        self.options
            .discover_with(origin, control, |request| async move {
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
                    },
                })
            })
            .await
    }
}
fn failure(error: reqwest::Error) -> RequestFailure {
    RequestFailure {
        kind: if error.is_timeout() {
            FailureKind::Timeout
        } else {
            FailureKind::Network
        },
        message: error.without_url().to_string(),
    }
}
struct Body {
    response: reqwest::Response,
    pending: bytes::Bytes,
    offset: usize,
    may_drain: bool,
}
impl ResponseBody for Body {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize, RequestFailure> {
        if output.is_empty() {
            return Ok(0);
        }
        while self.offset == self.pending.len() {
            match self.response.chunk().await.map_err(failure)? {
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
