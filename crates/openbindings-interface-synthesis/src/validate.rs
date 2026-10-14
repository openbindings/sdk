use crate::*;
use openbindings::{AUTHORING_VERSION, JsonKind, JsonRef, Source, VersionDecision, check_version};
use std::collections::{BTreeMap, HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

fn issue(code: &str, path: impl Into<String>, message: impl Into<String>) -> SynthesisFinding {
    SynthesisFinding {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}
fn ptr(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
fn index(value: Option<JsonRef<'_>>) -> Option<BTreeMap<&str, JsonRef<'_>>> {
    value
        .and_then(|v| v.members())
        .into_iter()
        .flatten()
        .map(|m| Some((m.name.as_str()?, m.value)))
        .collect()
}

// A nonrecursive structural hash ignores formatting and ordinary member order.
// Numbers contribute only their kind tag; core equality resolves hash collisions.
#[derive(PartialEq, Eq, Hash)]
enum Token<'a> {
    Tag(u8),
    Length(usize),
    Text(&'a str),
    Unpaired(Vec<u16>),
}
enum Work<'a> {
    Value(JsonRef<'a>),
    Token(Token<'a>),
}
fn tokens(value: JsonRef<'_>) -> impl Iterator<Item = Token<'_>> {
    let mut stack = vec![Work::Value(value)];
    std::iter::from_fn(move || {
        let work = stack.pop()?;
        let Work::Value(v) = work else {
            let Work::Token(t) = work else { unreachable!() };
            return Some(t);
        };
        let tag = match v.kind() {
            JsonKind::Null => 0,
            JsonKind::Boolean => {
                stack.push(Work::Token(Token::Tag(u8::from(v.as_bool().unwrap()))));
                1
            }
            JsonKind::Number => 2,
            JsonKind::String => {
                stack.push(Work::Token(match v.as_str() {
                    Some(s) => Token::Text(s),
                    None => Token::Unpaired(v.utf16_units().unwrap()),
                }));
                3
            }
            JsonKind::Array => {
                let children: Vec<_> = v.elements().unwrap().collect();
                for c in children.into_iter().rev() {
                    stack.push(Work::Value(c));
                }
                stack.push(Work::Token(Token::Length(v.len().unwrap())));
                4
            }
            JsonKind::Object => {
                let mut members: Vec<_> = v.members().unwrap().collect();
                members.sort_by(|a, b| match (a.name.as_str(), b.name.as_str()) {
                    (Some(a), Some(b)) => a.cmp(b),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a.name.utf16_units().cmp(&b.name.utf16_units()),
                });
                for m in members.into_iter().rev() {
                    stack.push(Work::Value(m.value));
                    stack.push(Work::Value(m.name));
                }
                stack.push(Work::Token(Token::Length(v.len().unwrap())));
                5
            }
        };
        Some(Token::Tag(tag))
    })
}
fn same_json(a: JsonRef<'_>, b: JsonRef<'_>) -> bool {
    // RO-9: ambiguous duplicate-member values cannot establish preservation.
    // Retain owners cheaply and defer exact decimal and UTF-16 equality to core.
    a.to_owned().semantic_eq(&b.to_owned()) == Some(true)
}

fn hash_value(value: Option<JsonRef<'_>>, h: &mut DefaultHasher) {
    value.is_some().hash(h);
    if let Some(v) = value {
        for t in tokens(v) {
            t.hash(h)
        }
    }
}
fn source_hash(s: &Source) -> u64 {
    let mut h = DefaultHasher::new();
    s.kind.hash(&mut h);
    s.description.as_deref().hash(&mut h);
    hash_value(s.content.as_ref().map(|v| v.view()), &mut h);
    s.additional_fields.len().hash(&mut h);
    for (k, v) in &s.additional_fields {
        k.hash(&mut h);
        hash_value(Some(v.view()), &mut h);
    }
    h.finish()
}
fn output_hash(v: JsonRef<'_>) -> u64 {
    let mut h = DefaultHasher::new();
    v.get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .hash(&mut h);
    v.get("description").and_then(|v| v.as_str()).hash(&mut h);
    hash_value(v.get("content"), &mut h);
    let ext: BTreeMap<_, _> = v
        .members()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let k = m.name.as_str()?;
            (!["kind", "content", "description"].contains(&k)).then_some((k, m.value))
        })
        .collect();
    ext.len().hash(&mut h);
    for (k, v) in ext {
        k.hash(&mut h);
        hash_value(Some(v), &mut h);
    }
    h.finish()
}
fn source_matches(a: &Source, b: JsonRef<'_>) -> bool {
    if b.get("kind").and_then(|v| v.as_str()) != Some(a.kind.as_str())
        || b.get("description").and_then(|v| v.as_str()) != a.description.as_deref()
    {
        return false;
    }
    if b.len()
        != Some(
            1 + usize::from(a.description.is_some())
                + usize::from(a.content.is_some())
                + a.additional_fields.len(),
        )
    {
        return false;
    }
    match (a.content.as_ref(), b.get("content")) {
        (None, None) => {}
        (Some(a), Some(b)) if same_json(a.view(), b) => {}
        _ => return false,
    }
    let Some(fields) = index(Some(b)) else {
        return false;
    };
    a.additional_fields.iter().all(|(k, v)| {
        fields
            .get(k.as_str())
            .is_some_and(|b| same_json(v.view(), *b))
    })
}
fn same_source(a: &Source, b: &Source) -> bool {
    a.kind == b.kind
        && a.description == b.description
        && a.additional_fields.len() == b.additional_fields.len()
        && match (&a.content, &b.content) {
            (None, None) => true,
            (Some(a), Some(b)) => same_json(a.view(), b.view()),
            _ => false,
        }
        && a.additional_fields.iter().all(|(k, v)| {
            b.additional_fields
                .get(k)
                .is_some_and(|b| same_json(v.view(), b.view()))
        })
}
pub(crate) fn admit(request: &SynthesisRequest<'_>) -> Result<(), SynthesisError> {
    let invalid = |message: &str| {
        SynthesisError::simple(SynthesisErrorKind::InvalidInput, "invalid-input", message)
    };
    let declared = request.openbindings_version.unwrap_or(AUTHORING_VERSION);
    match check_version(declared) {
        VersionDecision::Supported => {}
        VersionDecision::Unsupported => return Err(SynthesisError::unsupported_version(declared)),
        VersionDecision::Malformed => {
            return Err(SynthesisError::simple(
                SynthesisErrorKind::InvalidInput,
                "malformed-version",
                "target version is not valid SemVer",
            ));
        }
    }
    let mut names: HashMap<&str, &Source> = HashMap::new();
    for input in request.sources {
        if input
            .name
            .is_some_and(|name| !openbindings::valid_name(name))
        {
            return Err(SynthesisError::simple(
                SynthesisErrorKind::InvalidInput,
                "invalid-source-name",
                "supplied source name violates core key grammar",
            ));
        }
        if input.source.kind.is_empty()
            || input
                .source
                .additional_fields
                .keys()
                .any(|k| !k.starts_with("x-"))
        {
            return Err(invalid(
                "source carrier has an empty kind or a non-extension additional member",
            ));
        }
        if let Some(name) = input.name
            && let Some(prior) = names.insert(name, input.source)
            && !same_source(prior, input.source)
        {
            return Err(invalid("conflicting explicit source name"));
        }
    }
    Ok(())
}
fn no_shadow(
    fields: &BTreeMap<String, openbindings::JsonValue>,
    known: &[&str],
    path: &str,
) -> Result<(), SynthesisFinding> {
    if let Some(k) = fields.keys().find(|k| known.contains(&k.as_str())) {
        return Err(issue(
            "shadowed-field",
            format!("{path}/{}", ptr(k)),
            "additional evidence shadows a typed field",
        ));
    }
    Ok(())
}
fn check(
    request: &SynthesisRequest<'_>,
    candidate: &SynthesisCandidate,
) -> Result<Vec<String>, SynthesisFinding> {
    let root = candidate.document.value().view();
    for (k, wanted) in [
        (
            "openbindings",
            Some(request.openbindings_version.unwrap_or(AUTHORING_VERSION)),
        ),
        ("name", request.name),
        ("version", request.version),
        ("description", request.description),
    ] {
        if let Some(wanted) = wanted
            && root.get(k).and_then(|v| v.as_str()) != Some(wanted)
        {
            return Err(issue(
                "request-override",
                format!("/interface/{k}"),
                "provider did not honor a requested override",
            ));
        }
    }
    let sources = index(root.get("sources")).ok_or_else(|| {
        issue(
            "unrepresentable-key",
            "/interface/sources",
            "emitted keys must be Unicode scalar strings",
        )
    })?;
    let bindings = index(root.get("bindings")).ok_or_else(|| {
        issue(
            "unrepresentable-key",
            "/interface/bindings",
            "emitted keys must be Unicode scalar strings",
        )
    })?;
    let dependencies = index(root.get("dependencies")).ok_or_else(|| {
        issue(
            "unrepresentable-key",
            "/interface/dependencies",
            "emitted keys must be Unicode scalar strings",
        )
    })?;
    let operations = index(root.get("operations")).ok_or_else(|| {
        issue(
            "unrepresentable-key",
            "/interface/operations",
            "emitted keys must be Unicode scalar strings",
        )
    })?;
    let mut input_hashes = HashMap::<u64, Vec<usize>>::new();
    let mut keys = Vec::with_capacity(request.sources.len());
    for (i, input) in request.sources.iter().enumerate() {
        let key = source_hash(input.source);
        keys.push(key);
        input_hashes.entry(key).or_default().push(i);
    }
    let mut output_hashes = HashMap::<u64, Vec<&str>>::new();
    for (key, source) in &sources {
        let hash = output_hash(*source);
        if !input_hashes.get(&hash).is_some_and(|ids| {
            ids.iter()
                .any(|i| source_matches(request.sources[*i].source, *source))
        }) {
            return Err(issue(
                "source-preservation",
                format!("/interface/sources/{}", ptr(key)),
                "emitted source does not match any supplied source",
            ));
        }
        output_hashes.entry(hash).or_default().push(key);
    }
    let mut source_keys = Vec::with_capacity(request.sources.len());
    for (i, input) in request.sources.iter().enumerate() {
        let matched_key = if let Some(name) = input.name {
            sources
                .get(name)
                .filter(|s| source_matches(input.source, **s))
                .map(|_| name)
        } else {
            output_hashes.get(&keys[i]).and_then(|names| {
                // Output keys were indexed in sorted order. Choose a stable
                // representative without imposing a naming policy on providers.
                names
                    .iter()
                    .copied()
                    .find(|n| source_matches(input.source, sources[n]))
            })
        };
        let Some(key) = matched_key else {
            return Err(issue(
                "source-preservation",
                format!("/request/sources/{i}"),
                "supplied source was dropped, renamed or changed",
            ));
        };
        source_keys.push(key.to_owned());
    }
    if request.sources.is_empty()
        && (!operations.is_empty()
            || !bindings.is_empty()
            || !dependencies.is_empty()
            || !candidate.coverage.entries.is_empty()
            || !candidate.coverage.exhaustive)
    {
        return Err(issue(
            "source-free",
            "/interface",
            "source-free synthesis requires an empty exhaustive skeleton",
        ));
    }
    let c = &candidate.coverage;
    no_shadow(
        &c.additional_fields,
        &[
            "sourceKeys",
            "entries",
            "exhaustive",
            "fullyRepresented",
            "limitation",
        ],
        "/coverage",
    )?;
    if !c.exhaustive && c.limitation.is_none() {
        return Err(issue(
            "missing-limitation",
            "/coverage/limitation",
            "non-exhaustive coverage requires limitation evidence",
        ));
    }
    if let Some(l) = &c.limitation {
        if l.code.is_empty() || l.message.is_empty() {
            return Err(issue(
                "empty-limitation",
                "/coverage/limitation",
                "limitation code and message must be nonempty",
            ));
        }
        no_shadow(
            &l.additional_fields,
            &["code", "message", "details"],
            "/coverage/limitation",
        )?;
    }
    let mut identities = HashSet::new();
    let mut source_pairs = HashSet::new();
    for (i, e) in c.entries.iter().enumerate() {
        let path = format!("/coverage/entries/{i}");
        let fail = |code: &str, field: &str, msg: &str| issue(code, format!("{path}/{field}"), msg);
        no_shadow(
            &e.additional_fields,
            &[
                "sourceIndex",
                "sourceRef",
                "scope",
                "status",
                "sourceKey",
                "operationKey",
                "bindingKey",
                "dependencyKey",
                "reasonCode",
                "rule",
                "message",
                "requirements",
                "details",
            ],
            &path,
        )?;
        let input = request.sources.get(e.source_index).ok_or_else(|| {
            fail(
                "source-index",
                "sourceIndex",
                "sourceIndex is outside the supplied inputs",
            )
        })?;
        if e.source_ref.is_empty() {
            return Err(fail(
                "empty-identity",
                "sourceRef",
                "sourceRef must be nonempty",
            ));
        }
        let projection = (e.scope == CoverageScope::Projection).then_some((
            &e.source_key,
            &e.operation_key,
            &e.binding_key,
        ));
        if !identities.insert((e.source_index, e.scope, &e.source_ref, projection)) {
            return Err(fail(
                "duplicate-unit",
                "sourceRef",
                "duplicate coverage unit identity",
            ));
        }
        if e.source_key.as_ref().is_some_and(String::is_empty) {
            return Err(fail(
                "empty-source-key",
                "sourceKey",
                "sourceKey must be nonempty when present",
            ));
        }
        if e.reason_code.as_ref().is_some_and(String::is_empty)
            || e.rule.as_ref().is_some_and(String::is_empty)
            || e.requirements
                .as_ref()
                .is_some_and(|r| r.iter().any(String::is_empty))
        {
            return Err(fail(
                "empty-evidence",
                "reasonCode",
                "reason codes, rules and runtime requirement names must be nonempty",
            ));
        }
        if e.status != CoverageStatus::Represented
            && (e.reason_code.is_none() || e.message.is_none())
        {
            return Err(fail(
                "missing-reason",
                "reasonCode",
                "non-represented disposition requires reasonCode and message",
            ));
        }
        if e.status == CoverageStatus::Excluded && e.rule.is_none() {
            return Err(fail(
                "missing-rule",
                "rule",
                "exclusion requires a revision-identifiable kind rule",
            ));
        }
        let emitted = matches!(
            e.status,
            CoverageStatus::Represented | CoverageStatus::Lossy
        );
        if e.scope == CoverageScope::Dependency {
            if e.source_key.is_some() || e.operation_key.is_some() || e.binding_key.is_some() {
                return Err(fail(
                    "dependency-path",
                    "bindingKey",
                    "dependency must not carry operation or binding identity or source key",
                ));
            }
            if e.status == CoverageStatus::Represented && e.reason_code.is_some() {
                return Err(fail(
                    "dependency-reason",
                    "reasonCode",
                    "represented dependency must not carry a reason code",
                ));
            }
            if emitted {
                if let Some(key) = &e.dependency_key
                    && !dependencies.contains_key(key.as_str())
                {
                    return Err(fail(
                        "dependency-correspondence",
                        "dependencyKey",
                        "dependency key is missing from output",
                    ));
                }
            } else if e.dependency_key.is_some() {
                return Err(fail(
                    "dependency-key",
                    "dependencyKey",
                    "non-emitted dependency must not carry dependencyKey",
                ));
            }
        } else {
            if e.dependency_key.is_some() {
                return Err(fail(
                    "dependency-key",
                    "dependencyKey",
                    "other scopes must not carry dependencyKey",
                ));
            }
            if emitted {
                let op = e.operation_key.as_deref().ok_or_else(|| {
                    fail(
                        "missing-path",
                        "operationKey",
                        "represented/lossy path requires operationKey",
                    )
                })?;
                let binding = e.binding_key.as_deref().ok_or_else(|| {
                    fail(
                        "missing-path",
                        "bindingKey",
                        "represented/lossy path requires bindingKey",
                    )
                })?;
                let source = e
                    .source_key
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        fail(
                            "missing-path",
                            "sourceKey",
                            "represented/lossy path requires nonempty sourceKey",
                        )
                    })?;
                let b = bindings.get(binding).ok_or_else(|| {
                    fail(
                        "binding-correspondence",
                        "bindingKey",
                        "no matching binding",
                    )
                })?;
                if !operations.contains_key(op)
                    || b.get("operation").and_then(|v| v.as_str()) != Some(op)
                    || b.get("source").and_then(|v| v.as_str()) != Some(source)
                {
                    return Err(fail(
                        "binding-correspondence",
                        "bindingKey",
                        "binding does not match coverage operation/source",
                    ));
                }
                if source_pairs.insert((e.source_index, source))
                    && (!sources
                        .get(source)
                        .is_some_and(|v| source_matches(input.source, *v))
                        || input.name.is_some_and(|n| n != source))
                {
                    return Err(fail(
                        "source-correspondence",
                        "sourceKey",
                        "coverage path does not match the indexed input source",
                    ));
                }
            }
        }
    }
    Ok(source_keys)
}
pub(crate) fn validate(
    request: &SynthesisRequest<'_>,
    candidate: SynthesisCandidate,
) -> Result<Synthesis, SynthesisError> {
    let validation = match candidate.document.validate() {
        Ok(a) => a,
        Err(v) => return Err(SynthesisError::invalid(candidate, None, None, Some(v))),
    };
    let Some(document) = validation.validated() else {
        return Err(SynthesisError::invalid(
            candidate,
            Some(validation),
            None,
            None,
        ));
    };
    let source_keys = match check(request, &candidate) {
        Ok(keys) => keys,
        Err(f) => {
            return Err(SynthesisError::invalid(
                candidate,
                Some(validation),
                Some(f),
                None,
            ));
        }
    };
    Ok(Synthesis {
        document,
        coverage: SynthesisCoverage {
            draft: candidate.coverage,
            source_keys,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unpaired_string_comparison_never_aliases_decoded_text() {
        let parse = |s: &str| openbindings::JsonValue::parse(s).unwrap();
        let a = parse(r#""\uD800""#);
        let b = parse(r#""\ud800""#);
        let impostor = parse(r#""\"\\uD800\"""#);
        assert!(same_json(a.view(), b.view()));
        assert!(!same_json(a.view(), impostor.view()));
        let a = parse(r#"{"\uD800":1,"a":2}"#);
        let b = parse(r#"{"a":2,"\ud800":1}"#);
        assert!(same_json(a.view(), b.view()));
    }
}

#[cfg(test)]
mod numeric_tests {
    use super::*;
    use openbindings::JsonValue;

    // RO-9: numeric spelling does not affect source hash buckets. Comparison
    // within each bucket remains core's exact value equality.
    #[test]
    fn ro9_values_differing_only_in_number_spelling_hash_equally() {
        let left = JsonValue::parse(r#"{"n":[1,-0,{"large":9007199254740993}]}"#).unwrap();
        let right = JsonValue::parse(r#"{"n":[1.0,0e0,{"large":9007199254740993e0}]}"#).unwrap();
        let hash = |value: &JsonValue| {
            let mut h = DefaultHasher::new();
            hash_value(Some(value.view()), &mut h);
            h.finish()
        };
        assert_eq!(hash(&left), hash(&right));
        assert!(same_json(left.view(), right.view()));
    }

    // RO-9: equal values must hash equally. Distinct values may collide, but
    // core equality and checked synthesis must still distinguish their sources.
    #[test]
    fn ro9_decimal_equality_and_hashes_are_congruent() {
        for (left, right, equal) in [
            ("1", "1.0", true),
            ("1.0", "1e0", true),
            ("-0", "0", true),
            ("1e999999999999999999999", "10e999999999999999999998", true),
            (
                "1e-999999999999999999999",
                "0.1e-999999999999999999998",
                true,
            ),
            ("9007199254740993", "9007199254740992", false),
            (
                r#"{"n":[1,-0,{"a":1e0}]}"#,
                r#"{"n":[1.0,0,{"a":1}]}"#,
                true,
            ),
        ] {
            let a = JsonValue::parse(left).unwrap();
            let b = JsonValue::parse(right).unwrap();
            assert_eq!(same_json(a.view(), b.view()), equal);
            // Independent equality oracle from the document model.
            assert_eq!(a.semantic_eq(&b), Some(equal));
            let hash = |v: &JsonValue| {
                let mut h = DefaultHasher::new();
                hash_value(Some(v.view()), &mut h);
                h.finish()
            };
            if equal {
                assert_eq!(hash(&a), hash(&b));
            } else {
                assert_eq!(a.semantic_eq(&b), Some(false));
                let input = Source {
                    kind: "example.numeric-source".into(),
                    content: Some(a),
                    ..Default::default()
                };
                let mut draft = openbindings::Document::new();
                draft.sources = Some(BTreeMap::from([(
                    "api".into(),
                    Source {
                        content: Some(b),
                        ..input.clone()
                    },
                )]));
                let candidate = SynthesisCandidate {
                    document: draft.build().unwrap(),
                    coverage: CoverageDraft {
                        exhaustive: true,
                        ..Default::default()
                    },
                };
                for name in [Some("api"), None] {
                    let sources = [SynthesisSource {
                        name,
                        source: &input,
                    }];
                    let request = SynthesisRequest {
                        sources: &sources,
                        ..Default::default()
                    };
                    let error = validate(&request, candidate.clone()).unwrap_err();
                    assert_eq!(error.kind(), SynthesisErrorKind::InvalidResult);
                    assert_eq!(
                        error.rejected().unwrap().finding.as_ref().unwrap().code,
                        "source-preservation"
                    );
                }
            }
        }
    }
}
