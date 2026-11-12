use crate::{Context, RequestError};
use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_context::ContextShapeError;
use std::{fmt, sync::Arc};

/// One explicit source of an original, immutable conformant document.
/// Clones retain the document, selected key and opaque context without copying them.
#[derive(Clone)]
pub struct PullRequest {
    interface: ConformantDocument,
    source: Arc<str>,
    context: Option<Context>,
}
impl PullRequest {
    /// Retain an original document and an exact source key. The checked caller
    /// establishes that the source exists before constructing provider work.
    pub fn new(interface: ConformantDocument, source: impl Into<String>) -> Self {
        Self {
            interface,
            source: Arc::from(source.into()),
            context: None,
        }
    }
    /// Supply this attempt's opaque context, without combining or interpreting it.
    #[must_use]
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
    /// Borrow the complete original conformant document.
    pub fn interface(&self) -> &ConformantDocument {
        &self.interface
    }
    /// Borrow the selected source key, never an alias or a URI.
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Borrow separately supplied context; absence differs from an empty object.
    pub fn context(&self) -> Option<&Context> {
        self.context.as_ref()
    }
    /// Read the closed published request and require core document conformance.
    /// Parsing and this admission precede the caller's handoff limits; callers
    /// accepting bytes should first use explicit exact-JSON admission limits.
    /// A missing selected source is refused by the checked caller before dispatch.
    ///
    /// # Errors
    /// Returns [`RequestError::Shape`] for a malformed closed carrier, including
    /// `"duplicate-member"` for a repeated outer name or for context in which any
    /// object, at any depth, repeats a decoded member name. Document admission
    /// failures are [`RequestError::Version`] and [`RequestError::Interface`].
    pub fn from_json(value: &JsonValue) -> Result<Self, RequestError> {
        let mut seen = [false; 3];
        for member in value
            .view()
            .members()
            .ok_or(RequestError::Shape("object"))?
        {
            let index = match member.name.as_str() {
                Some("interface") => 0,
                Some("source") => 1,
                Some("context") => 2,
                _ => return Err(RequestError::Shape("unknown-member")),
            };
            if std::mem::replace(&mut seen[index], true) {
                return Err(RequestError::Shape("duplicate-member"));
            }
        }
        let source = value
            .get("source")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or(RequestError::Shape("source"))?;
        let original = value
            .get("interface")
            .ok_or(RequestError::Shape("interface"))?;
        let document = Document::from_json(original.to_owned());
        let validation = document.validate().map_err(RequestError::Version)?;
        let interface = validation
            .conformant()
            .ok_or(RequestError::Interface(validation))?;
        let context = value
            .get("context")
            .map(|v| {
                Context::try_from(v.to_owned()).map_err(|error| match error {
                    ContextShapeError::DuplicateMember => RequestError::Shape("duplicate-member"),
                    _ => RequestError::Shape("context"),
                })
            })
            .transpose()?;
        Ok(Self {
            interface,
            source: source.into(),
            context,
        })
    }
}
impl fmt::Debug for PullRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PullRequest")
            .field("context_present", &self.context.is_some())
            .finish_non_exhaustive()
    }
}
