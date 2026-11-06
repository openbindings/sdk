//! Private metadata encoding for JavaScript's exact UTF-16 pointers.
//! Ordinary Rust serialization deliberately replaces unpaired pointer units.
use super::encoded;
use openbindings::*;
use serde_json::value::RawValue;

pub(super) fn json<T: Exact + ?Sized>(value: &T) -> Box<RawValue> {
    RawValue::from_string(value.exact()).expect("exact metadata is JSON")
}

pub(super) trait Exact {
    fn exact(&self) -> String;
}
impl<T: Exact + ?Sized> Exact for &T {
    fn exact(&self) -> String {
        (*self).exact()
    }
}
impl<T: Exact> Exact for Option<T> {
    fn exact(&self) -> String {
        self.as_ref().map_or_else(|| "null".into(), Exact::exact)
    }
}
impl<T: Exact> Exact for Vec<T> {
    fn exact(&self) -> String {
        encoded(self.iter().map(json).collect::<Vec<_>>())
    }
}
impl Exact for Pointer {
    fn exact(&self) -> String {
        let mut token = String::from("\"");
        for unit in char::decode_utf16(self.utf16_units()) {
            match unit {
                Ok(c) => {
                    let quoted = encoded(c.to_string());
                    token.push_str(&quoted[1..quoted.len() - 1]);
                }
                Err(unit) => {
                    use std::fmt::Write;
                    let _ = write!(token, "\\u{:04x}", unit.unpaired_surrogate());
                }
            }
        }
        token.push('"');
        token
    }
}

macro_rules! record {
    ($kind:ty, $v:ident, {$($key:literal: $field:expr),* $(,)?}) => {
        impl Exact for $kind {
            fn exact(&self) -> String {
                let $v = self;
                object!($($key: $field),*)
            }
        }
    };
}
record!(SourceLocation, v, {
    "pointer": json(&v.pointer), "byte_offset": v.byte_offset,
    "line": v.line, "byte_column": v.byte_column,
});
record!(SchemaOrigin, v, {
    "source_uri": &v.source_uri, "pointer": json(&v.pointer),
});
record!(Finding, v, {
    "rule": v.rule, "status": v.status, "code": v.code,
    "location": json(&v.location), "message": &v.message,
});
record!(ValidationReport, v, {
    "release": v.release, "revision": v.revision, "conclusion": v.conclusion,
    "evidence": &v.evidence, "findings": json(&v.findings),
    "findings_truncated": v.findings_truncated,
});
record!(ReferenceRefusal, v, {
    "reason": v.reason, "code": &v.code, "message": &v.message,
    "location": json(&v.location), "resource_uri": &v.resource_uri,
});
record!(Reference, v, {
    "location": json(&v.location), "keyword": &v.keyword,
    "spelling": &v.spelling, "resolution": json(&v.resolution),
});
record!(ReferenceReport, v, {
    "references": json(&v.references), "complete": v.complete,
    "limitation": json(&v.limitation),
});
record!(NoVerdict, v, {
    "reason": v.reason, "code": &v.code, "message": &v.message,
    "location": json(&v.location),
});
record!(UndefinedResult, v, {
    "code": &v.code, "message": &v.message, "location": json(&v.location),
});
impl Exact for ReferenceResolution {
    fn exact(&self) -> String {
        match self {
            Self::Located {
                target,
                dynamic_lookup,
            } => object!(
                "status": "located", "target": json(target), "dynamic_lookup": dynamic_lookup,
            ),
            Self::Unresolved { detail } => object!("status": "unresolved", "detail": json(detail)),
        }
    }
}
impl Exact for PreparationFailure {
    fn exact(&self) -> String {
        #[derive(serde::Serialize)]
        struct Fields<'a> {
            #[serde(flatten)]
            kind: &'a PreparationFailureKind,
            origin: Box<RawValue>,
        }
        encoded(Fields {
            kind: &self.kind,
            origin: json(&self.origin),
        })
    }
}
impl Exact for ValueProblem {
    fn exact(&self) -> String {
        #[derive(serde::Serialize)]
        struct Fields<'a> {
            instance: Box<RawValue>,
            schema: Box<RawValue>,
            code: &'a str,
            message: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            details: &'a Option<ValueProblemDetails>,
        }
        encoded(Fields {
            instance: json(&self.instance),
            schema: json(&self.schema),
            code: &self.code,
            message: &self.message,
            details: &self.details,
        })
    }
}
impl Exact for ValueOutcome {
    fn exact(&self) -> String {
        match self {
            Self::Satisfies => object!("status": "satisfies"),
            Self::Fails {
                message,
                problems,
                problems_complete,
            } => object!(
                "status": "fails", "message": message, "problems": json(problems),
                "problems_complete": problems_complete,
            ),
            Self::Undefined { detail } => object!("status": "undefined", "detail": json(detail)),
            Self::NoVerdict { detail } => object!("status": "no-verdict", "detail": json(detail)),
        }
    }
}
