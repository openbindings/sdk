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
impl Exact for Finding {
    fn exact(&self) -> String {
        #[derive(serde::Serialize)]
        struct Fields<'a> {
            rule: &'a str,
            status: Evidence,
            code: &'a str,
            location: Box<RawValue>,
            message: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            keyword: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            schema: Option<Box<RawValue>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            details: Option<&'a ValueProblemDetails>,
        }
        encoded(Fields {
            rule: self.rule,
            status: self.status,
            code: self.code,
            location: json(&self.location),
            message: &self.message,
            keyword: self.keyword,
            schema: self.schema.as_ref().map(json),
            details: self.details.as_ref(),
        })
    }
}
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
    "location": json(&v.location), "resource_root": json(&v.resource_root), "keyword": &v.keyword,
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
            // Facts already contain exact scalar strings, including original
            // JSON tokens. Serialize them as strings, never as parsed numbers
            // or enum values; only the pointer fields need UTF-16 encoding.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn agrees(value: &(impl Exact + serde::Serialize)) {
        assert_eq!(value.exact(), serde_json::to_string(value).unwrap());
    }

    #[test]
    fn every_record_matches_serde_with_populated_optional_fields() {
        let value = JsonValue::parse(r#"{"a/~\"\n😀":1}"#).unwrap();
        let location = value.get("a/~\"\n😀").unwrap().location();
        agrees(&location.pointer);
        agrees(&location);
        let origin = SchemaOrigin::new(
            Some("https://example.test/schema".into()),
            location.pointer.clone().unwrap(),
        );
        agrees(&origin);

        let mut report = validate_document(r#"{"openbindings":"0.2.0","operations":[]}"#)
            .unwrap()
            .report()
            .clone();
        assert!(!report.findings.is_empty());
        report.findings[0].location = Some(location);
        agrees(&report.findings[0]);
        agrees(&report);
        for text in [
            "{}",
            r#"{"openbindings":"0.2.0","operations":{"op":{"input":null}}}"#,
            "[0,]",
        ] {
            let validation = validate_document(text).unwrap();
            for finding in &validation.report().findings {
                agrees(finding);
            }
            agrees(validation.report());
        }

        let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/missing"}}}}"#).unwrap();
        let mut references = document.references();
        let ReferenceResolution::Unresolved { detail } = &references.references[0].resolution
        else {
            panic!("missing resource")
        };
        assert!(detail.location.is_some());
        assert!(detail.resource_uri.is_some());
        agrees(detail);
        agrees(&references.references[0]);
        agrees(&references.references[0].resolution);
        references.limitation = Some(detail.clone());
        references.complete = false;
        agrees(&references);
        references.references[0].resolution = ReferenceResolution::Located {
            target: origin.clone(),
            dynamic_lookup: true,
        };
        agrees(&references.references[0].resolution);
        agrees(&references.references[0]);

        let refusal = NoVerdict::new(NoVerdictReason::LimitExceeded, "limit", "bounded work")
            .with_location(origin.clone());
        let undefined =
            UndefinedResult::new("undefined", "undefined assertion").with_location(origin.clone());
        agrees(&refusal);
        agrees(&undefined);
        for kind in [
            PreparationFailureKind::Resolution {
                cause: ResolutionFailureKind::InvalidExternalSchema,
            },
            PreparationFailureKind::UnsupportedCapability {
                capability: "capability".into(),
            },
            PreparationFailureKind::Limit {
                resource: "work".into(),
            },
            PreparationFailureKind::Cancelled,
            PreparationFailureKind::InvalidExternalSchema,
            PreparationFailureKind::Implementation {
                message: "implementation".into(),
            },
        ] {
            agrees(&PreparationFailure::new(kind).with_origin(origin.clone()));
        }
        let mut problem = ValueProblem::new(
            "/instance".parse().unwrap(),
            origin,
            "type",
            "assertion failed",
        );
        agrees(&problem);
        assert!(!problem.exact().contains("\"details\""));
        for details in [
            ValueProblemDetails::Type {
                expected: vec!["integer".into(), "null".into()],
            },
            ValueProblemDetails::Required {
                member: "a/\"\n😀".into(),
            },
            ValueProblemDetails::NumericBound {
                bound: "1E+1000".into(),
            },
            ValueProblemDetails::SizeBound {
                bound: "2.00e+0".into(),
            },
            ValueProblemDetails::Enum {
                choices: vec![
                    "null".into(),
                    r#"{ "a": 1E+400 }"#.into(),
                    r#""\ud800""#.into(),
                    "-0.00".into(),
                    "9007199254740993".into(),
                ],
            },
        ] {
            problem.details = Some(details);
            agrees(&problem);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&problem.exact()).unwrap()["details"],
                serde_json::to_value(&problem.details).unwrap(),
            );
            agrees(&ValueOutcome::Fails {
                message: "failure".into(),
                problems: vec![problem.clone()],
                problems_complete: true,
            });
        }
        agrees(&ValueOutcome::Satisfies);
        agrees(&ValueOutcome::Undefined { detail: undefined });
        agrees(&ValueOutcome::NoVerdict { detail: refusal });
    }
}
