use crate::{SynthesisError, SynthesisErrorKind};
use openbindings::{JsonLimits, JsonValue};
use serde::Serialize;
pub(crate) fn encode<T: Serialize>(
    value: &T,
    limits: JsonLimits,
) -> Result<JsonValue, SynthesisError> {
    struct Output {
        bytes: Vec<u8>,
        max: usize,
    }
    impl std::io::Write for Output {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if b.len() > self.max.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("synthesis JSON byte limit exceeded"));
            }
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let error = |e: String| SynthesisError::simple(SynthesisErrorKind::Encoding, "encoding", e);
    let mut out = Output {
        bytes: Vec::new(),
        max: limits.max_bytes,
    };
    serde_json_compat::to_writer(&mut out, value).map_err(|e| error(e.to_string()))?;
    JsonValue::parse_with_limits(out.bytes, limits).map_err(|e| error(e.to_string()))
}
