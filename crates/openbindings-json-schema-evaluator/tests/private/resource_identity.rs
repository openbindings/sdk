//! Identity admission local to each preparation's document and resource set.
//! RI numbers map the frozen 34-case policy matrix and SOL supplements; source
//! regressions do not qualify a package.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const U: &str = "https://identity.example.invalid/U";
const V: &str = "https://identity.example.invalid/V";
const W: &str = "https://identity.example.invalid/W";
const A: &str = "https://identity.example.invalid/A";
const B: &str = "https://identity.example.invalid/B";
fn value(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn document(schemas: Value, operations: Value) -> Document {
    Document::parse(
        json!({"openbindings":"0.2.0","schemas":schemas,"operations":operations}).to_string(),
    )
    .unwrap()
}
fn one(schemas: Value, schema: Value) -> Document {
    document(schemas, json!({"op":{"input":schema}}))
}
fn owner() -> Value {
    json!({"E":{"$id":U,"type":"integer","minimum":1}})
}
fn resources(rows: &[(&str, Value)]) -> ExternalSchemas {
    ExternalSchemas::new(rows.iter().map(|(uri, schema)| ExternalSchema {
        uri: (*uri).into(),
        document: value(&schema.to_string()),
    }))
    .unwrap()
}
fn conformant(doc: &Document) -> ConformantDocument {
    doc.validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
}
/// Prepare the input schema of the named operation.
fn prepare_with(
    proof: &ConformantDocument,
    supplied: &ExternalSchemas,
    operation: &str,
    evaluator: &dyn SchemaEvaluator<Validator = ProbeValidator>,
) -> Result<PreparedSchema<ProbeValidator>, PreparationFailure> {
    proof
        .resolve_operation(operation)
        .unwrap_or_else(|| panic!("declared operation {operation}"))
        .input()
        .expect("declared input")
        .prepare(evaluator, supplied)
}
fn prepare(
    proof: &ConformantDocument,
    supplied: &ExternalSchemas,
    operation: &str,
) -> Result<PreparedSchema<ProbeValidator>, PreparationFailure> {
    prepare_with(proof, supplied, operation, &Engine::Default)
}
fn ready_with(
    proof: &ConformantDocument,
    supplied: &ExternalSchemas,
    operation: &str,
    evaluator: &dyn SchemaEvaluator<Validator = ProbeValidator>,
) -> PreparedSchema<ProbeValidator> {
    match prepare_with(proof, supplied, operation, evaluator) {
        Ok(contract) => contract,
        Err(detail) => panic!("expected ready {operation}: {detail:?}"),
    }
}
fn ready(
    proof: &ConformantDocument,
    supplied: &ExternalSchemas,
    operation: &str,
) -> PreparedSchema<ProbeValidator> {
    ready_with(proof, supplied, operation, &Engine::Default)
}
fn label(result: ValueOutcome) -> &'static str {
    match result {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { .. } => "no-verdict",
    }
}
fn verdicts(contract: &PreparedSchema<ProbeValidator>, cases: &[(&str, &str)]) {
    for (text, expected) in cases {
        assert_eq!(label(contract.explain(&value(text))), *expected, "{text}");
    }
}
fn capability_only(contract: &PreparedSchema<ProbeValidator>, cases: &[&str]) {
    for text in cases {
        let value = value(text);
        for result in [contract.evaluate(&value), contract.explain(&value)] {
            assert!(
                matches!(result, ValueOutcome::NoVerdict { detail }
                if detail.reason == NoVerdictReason::UnsupportedCapability),
                "{text}"
            );
        }
    }
}

// Exercise the adapter's private projection separately from compilation.
enum Engine {
    Default,
    Projection,
}
enum ProbeValidator {
    Default(crate::DefaultValidator),
    Projection,
}
impl SchemaValidator for ProbeValidator {
    fn evaluate(
        &self,
        value: &JsonValue,
        detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        match self {
            Self::Default(validator) => validator.evaluate(value, detail, control),
            Self::Projection => ValueOutcome::Satisfies,
        }
    }
}
impl SchemaEvaluator for Engine {
    type Validator = ProbeValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        match self {
            Self::Default => DefaultEvaluator::new()
                .prepare(request, limits, control)
                .map(ProbeValidator::Default),
            Self::Projection => {
                use openbindings_internal_schema::OriginalEnvironment;
                let environment = request.environment(limits, control)?;
                environment
                    .space()
                    .program_definitions(environment.entry(), &|| control.is_cancelled())
                    .map_err(PreparationFailure::from)?;
                Ok(ProbeValidator::Projection)
            }
        }
    }
}
fn helpers_ready(doc: &Document, supplied: &ExternalSchemas, operation: &str) {
    ready_with(&conformant(doc), supplied, operation, &Engine::Projection);
}
fn refusal(
    prepared: Result<PreparedSchema<ProbeValidator>, PreparationFailure>,
) -> PreparationFailure {
    match prepared {
        Err(detail) => detail,
        other => panic!("expected preparation refusal: {other:?}"),
    }
}
fn identity_refusal(doc: &Document, supplied: &ExternalSchemas, operation: &str) {
    for evaluator in [
        &Engine::Default as &dyn SchemaEvaluator<Validator = ProbeValidator>,
        &Engine::Projection,
    ] {
        let detail = refusal(prepare_with(
            &conformant(doc),
            supplied,
            operation,
            evaluator,
        ));
        assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
        assert!(detail.origin.is_some(), "{detail:?}");
    }
}
fn reference(doc: &Document, supplied: &ExternalSchemas, pointer: &str) -> ReferenceResolution {
    doc.references_with_resources(supplied.clone(), &std::sync::atomic::AtomicBool::new(false))
        .references
        .into_iter()
        .find(|r| r.location.pointer == pointer)
        .unwrap()
        .resolution
}
fn located(
    doc: &Document,
    supplied: &ExternalSchemas,
    pointer: &str,
    target: &str,
    source: Option<&str>,
    dynamic: bool,
) {
    match reference(doc, supplied, pointer) {
        ReferenceResolution::Located {
            target: actual,
            dynamic_lookup,
        } => {
            assert_eq!(actual.pointer, target);
            assert_eq!(actual.source_uri.as_deref(), source);
            assert_eq!(dynamic_lookup, dynamic);
        }
        other => panic!("{other:?}"),
    }
}
fn unresolved_identity(doc: &Document, supplied: &ExternalSchemas, pointer: &str) {
    match reference(doc, supplied, pointer) {
        ReferenceResolution::Unresolved { detail } => {
            assert_eq!(detail.reason, ReferenceRefusalReason::IdentityConflict);
            assert!(
                matches!(
                    detail.code.as_str(),
                    "ambiguous-resource" | "resource-identity-conflict"
                ),
                "{detail:?}"
            );
        }
        other => panic!("{other:?}"),
    }
}
fn failed_location(contract: &PreparedSchema<ProbeValidator>, text: &str) -> SchemaOrigin {
    let ValueOutcome::Fails { problems, .. } = contract.explain(&value(text)) else {
        panic!("expected failure")
    };
    problems.into_iter().next().unwrap().schema
}

#[test]
fn resource_identity_and_edited_binding_data_keep_value_contracts() {
    let expectations: Value = serde_json::from_str(include_str!(
        "../fixtures/resource-identity/B32-expectations.json"
    ))
    .unwrap();
    let schema = value(include_str!(
        "../fixtures/resource-identity/B32-runner-result.json"
    ));
    let supplied = ExternalSchemas::new([ExternalSchema {
        uri: schema.get("$id").unwrap().as_str().unwrap().into(),
        document: schema,
    }])
    .unwrap();
    for (edited, text) in [
        (
            false,
            include_str!("../fixtures/resource-identity/B32-original-0.2.json"),
        ),
        (
            true,
            include_str!("../fixtures/resource-identity/B32-edited-0.2.json"),
        ),
    ] {
        let doc = Document::parse(text).unwrap();
        let proof = conformant(&doc);
        let evaluator = DefaultEvaluator::new();
        let local = ExternalSchemas::default();
        for case in expectations["valueCases"].as_array().unwrap() {
            let resources = if case["setup"] == "runner-result-supplied" {
                &supplied
            } else {
                &local
            };
            let operation = proof
                .resolve_operation(case["operation"].as_str().unwrap())
                .unwrap_or_else(|| panic!("{} edited={edited}: missing operation", case["id"]));
            let contract = if case["side"] == "input" {
                operation.input()
            } else {
                operation.output()
            };
            let actual = match contract.map(|c| c.prepare(&evaluator, resources)) {
                Some(Ok(contract)) => {
                    label(contract.explain(&value(case["valueText"].as_str().unwrap())))
                }
                None => "no-contract",
                Some(Err(detail)) => panic!("{} edited={edited}: {detail:?}", case["id"]),
            };
            let expected = match (edited, case["id"].as_str().unwrap()) {
                (true, "B32-V21") => "satisfies",
                (true, "B32-V24") => "fails",
                _ => case["expected"].as_str().unwrap(),
            };
            assert_eq!(actual, expected, "{} edited={edited}", case["id"]);
        }
        let example = doc
            .value()
            .at("/operations/run_job/examples/finished/output")
            .unwrap()
            .to_owned();
        let run_job = proof
            .resolve_operation("run_job")
            .expect("declared operation");
        for (resources, expected) in [
            (&supplied, "satisfies"),
            (&local, if edited { "satisfies" } else { "no-verdict" }),
        ] {
            let actual = match run_job.output().map(|c| c.prepare(&evaluator, resources)) {
                Some(Ok(contract)) => label(contract.explain(&example)),
                Some(Err(detail))
                    if matches!(
                        detail.kind,
                        PreparationFailureKind::Resolution {
                            cause: ResolutionFailureKind::ResourceUnavailable { .. }
                        }
                    ) =>
                {
                    "no-verdict"
                }
                _ => panic!("finished example setup"),
            };
            assert_eq!(actual, expected, "finished example edited={edited}");
        }
    }
}
#[test]
fn contained_resource_wins_without_rewriting_the_supplied_catalog() {
    let doc = one(owner(), json!({"$ref":U}));
    let observed = Arc::new(Mutex::new(Vec::new()));
    struct Catalog(Arc<Mutex<Vec<String>>>);
    impl SchemaEvaluator for Catalog {
        type Validator = ProbeValidator;
        fn prepare(
            &self,
            request: EvaluationRequest<'_>,
            limits: &PreparationLimits,
            control: &dyn WorkControl,
        ) -> Result<Self::Validator, PreparationFailure> {
            *self.0.lock().unwrap() = request
                .external_schemas
                .iter()
                .map(|r| r.document.text().to_owned())
                .collect();
            DefaultEvaluator::new()
                .prepare(request, limits, control)
                .map(ProbeValidator::Default)
        }
    }
    for text in [
        r#"{"$id":"https://identity.example.invalid/U","type":"integer","minimum":1}"#,
        "{ \"minimum\": 1.0, \"type\": \"integer\", \"$id\": \"https://identity.example.invalid/U\" }",
        "false",
        r#"{"type":"string"}"#,
    ] {
        let supplied = ExternalSchemas::new([ExternalSchema {
            uri: U.into(),
            document: value(text),
        }])
        .unwrap();
        let contract = ready_with(
            &conformant(&doc),
            &supplied,
            "op",
            &Catalog(observed.clone()),
        );
        verdicts(
            &contract,
            &[("2", "satisfies"), ("0", "fails"), (r#""x""#, "fails")],
        );
        assert_eq!(*observed.lock().unwrap(), vec![text]);
        assert_eq!(
            failed_location(&contract, "0"),
            SchemaOrigin::new(None, "/schemas/E/minimum".parse().unwrap())
        );
        helpers_ready(&doc, &supplied, "op");
        located(
            &doc,
            &supplied,
            "/operations/op/input/$ref",
            "/schemas/E",
            None,
            false,
        );
    }
}
#[test]
fn duplicate_contained_resource_owners_refuse_document_admission() {
    let doc = document(
        owner(),
        json!({"direct":{"input":{"$id":U,"type":"string"}},"lookup":{"input":{"$ref":U}},"pointer":{"input":{"$ref":"#/schemas/E"}}}),
    );
    let supplied = resources(&[(U, json!(true))]);
    assert_eq!(
        doc.validate().unwrap().report().evidence["OBI-13"],
        Evidence::Violated
    );
    assert!(doc.validate().unwrap().conformant().is_none());
    unresolved_identity(&doc, &supplied, "/operations/lookup/input/$ref");
    unresolved_identity(&doc, &supplied, "/operations/pointer/input/$ref");
}
#[test]
fn external_references_back_to_a_contained_resource_use_its_constraints() {
    let doc = one(owner(), json!({"$ref":A}));
    let supplied = resources(&[(A, json!({"$id":V,"$ref":U})), (U, json!(false))]);
    let contract = ready(&conformant(&doc), &supplied, "op");
    verdicts(&contract, &[("2", "satisfies"), ("0", "fails")]);
    assert_eq!(
        failed_location(&contract, "0").pointer,
        "/schemas/E/minimum"
    );
    helpers_ready(&doc, &supplied, "op");
}
#[test]
fn retrieval_aliases_cannot_bypass_canonical_resource_conflicts() {
    for equal in [false, true] {
        let loser = if equal {
            owner()["E"].clone()
        } else {
            json!({"$id":U,"type":"string"})
        };
        let supplied = resources(&[(A, loser)]);
        let doc = document(
            owner(),
            json!({"owner":{"input":{"$ref":U}},"alias":{"input":{"$ref":A}},"dynamic":{"input":{"$dynamicRef":format!("{A}#node")}}}),
        );
        verdicts(
            &ready(&conformant(&doc), &supplied, "owner"),
            &[("2", "satisfies")],
        );
        identity_refusal(&doc, &supplied, "alias");
        unresolved_identity(&doc, &supplied, "/operations/alias/input/$ref");
        let detail = refusal(prepare(&conformant(&doc), &supplied, "dynamic"));
        assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
        unresolved_identity(&doc, &supplied, "/operations/dynamic/input/$dynamicRef");
    }
    for rows in [
        vec![
            (A, json!({"$id":V,"type":"integer"})),
            (B, json!({"$id":V,"type":"string"})),
        ],
        vec![
            (B, json!({"$id":V,"type":"string"})),
            (A, json!({"$id":V,"type":"integer"})),
        ],
    ] {
        let supplied = resources(&rows);
        for uri in [A, B, V] {
            let doc = one(json!({}), json!({"$ref":uri}));
            identity_refusal(&doc, &supplied, "op");
            unresolved_identity(&doc, &supplied, "/operations/op/input/$ref");
        }
    }
}
#[test]
fn distinct_canonical_identity_keeps_retrieval_origin_and_reference_base() {
    let canonical = "https://identity.example.invalid/canonical/V";
    for retrieval in [U, A] {
        let doc = document(
            owner(),
            json!({"owner":{"input":{"$ref":U}},"canonical":{"input":{"$ref":canonical}},"alias":{"input":{"$ref":retrieval}}}),
        );
        let supplied = resources(&[
            (
                retrieval,
                json!({"$id":canonical,"$anchor":"node","$ref":"child"}),
            ),
            (
                "https://identity.example.invalid/canonical/child",
                json!({"type":"string"}),
            ),
        ]);
        let proof = conformant(&doc);
        verdicts(
            &ready(&proof, &supplied, "canonical"),
            &[(r#""x""#, "satisfies"), ("2", "fails")],
        );
        located(
            &doc,
            &supplied,
            "/operations/canonical/input/$ref",
            "",
            Some(retrieval),
            false,
        );
        helpers_ready(&doc, &supplied, "canonical");
        let target = one(owner(), json!({"$ref":format!("{canonical}#node")}));
        located(
            &target,
            &supplied,
            "/operations/op/input/$ref",
            "",
            Some(retrieval),
            false,
        );
        // A selected root's own failures keep the retrieval source, even when U lookup selects E.
        let own = resources(&[(retrieval, json!({"$id":canonical,"type":"string"}))]);
        assert_eq!(
            failed_location(&ready(&proof, &own, "canonical"), "2")
                .source_uri
                .as_deref(),
            Some(retrieval)
        );
        if retrieval == A {
            verdicts(
                &ready(&proof, &supplied, "alias"),
                &[(r#""x""#, "satisfies")],
            );
        }
    }
}
#[test]
fn invalid_and_duplicate_supplied_resources_refuse_registration() {
    for second in [
        U,
        "https://identity.example.invalid/U#",
        "https://identity.example.invalid/x/../U",
    ] {
        for text in ["true", "false"] {
            assert!(
                ExternalSchemas::new([
                    ExternalSchema {
                        uri: U.into(),
                        document: value("true")
                    },
                    ExternalSchema {
                        uri: second.into(),
                        document: value(text)
                    }
                ])
                .is_err()
            );
        }
    }
    for (uri, text) in [
        ("relative", "true"),
        ("https://identity.example.invalid/U#node", "true"),
        ("https://identity.example.invalid/%zz", "true"),
        (U, r#"{"type":"integer","type":"string"}"#),
    ] {
        assert!(
            ExternalSchemas::new([ExternalSchema {
                uri: uri.into(),
                document: value(text)
            }])
            .is_err()
        );
    }
}
#[test]
fn unused_identity_collisions_are_local_and_selected_pointer_collisions_refuse() {
    let doc = document(
        owner(),
        json!({"parent":{"input":{"$ref":V}},"child":{"input":{"$ref":format!("{V}#/$defs/child")}},"owner":{"input":{"$ref":U}}}),
    );
    let supplied = resources(&[(
        V,
        json!({"type":"string","$defs":{"child":{"$id":U,"type":"boolean"}}}),
    )]);
    verdicts(
        &ready(&conformant(&doc), &supplied, "parent"),
        &[(r#""x""#, "satisfies"), ("2", "fails")],
    );
    helpers_ready(&doc, &supplied, "parent");
    identity_refusal(&doc, &supplied, "child");
    unresolved_identity(&doc, &supplied, "/operations/child/input/$ref");
    located(
        &doc,
        &supplied,
        "/operations/owner/input/$ref",
        "/schemas/E",
        None,
        false,
    );
}
#[test]
fn direct_applicators_preserve_competing_resource_owner_refusals() {
    for (schema, is_properties) in [
        (json!({"properties":{"x":{"$id":U,"type":"string"}}}), true),
        (json!({"allOf":[{"$id":U,"type":"string"}]}), false),
    ] {
        let doc = one(owner(), json!({"$ref":V}));
        let supplied = resources(&[(V, schema)]);
        let contract = ready(&conformant(&doc), &supplied, "op");
        capability_only(&contract, &[r#"{"x":"a"}"#, r#"{"x":2}"#]);
        if is_properties {
            verdicts(
                &contract,
                &[
                    ("{}", "satisfies"),
                    ("null", "satisfies"),
                    ("2", "satisfies"),
                ],
            );
        } else {
            capability_only(&contract, &["{}", "null", "2", r#""x""#]);
        }
        // A local identity inability is never an Undefined atom, but a
        // concrete independent branch can settle validity under section 5.5.
        for (schema, expected) in [
            (json!({"anyOf":[true,{"$ref":V}]}), "satisfies"),
            (json!({"allOf":[false,{"$ref":V}]}), "fails"),
        ] {
            let masked = one(owner(), schema);
            let contract = ready(&conformant(&masked), &supplied, "op");
            for text in [r#"{"x":"a"}"#, r#"{"x":2}"#] {
                let value = value(text);
                assert_eq!(label(contract.evaluate(&value)), expected);
                assert_eq!(label(contract.explain(&value)), expected);
            }
        }
        // Static inspection locates V; it does not promise its children's
        // identities can be interpreted when an instance reaches them.
        located(
            &doc,
            &supplied,
            "/operations/op/input/$ref",
            "",
            Some(V),
            false,
        );
    }
}
#[test]
fn unique_nested_resources_survive_shadowed_lexical_ancestors() {
    let nested_id = "https://identity.example.invalid/nested/W";
    let nested = json!({"$id":U,"$defs":{"child":{"$id":"nested/W","$ref":"child"}}});
    let ext = (
        "https://identity.example.invalid/nested/child",
        json!({"type":"boolean"}),
    );
    let under_loser = resources(&[(A, nested.clone()), ext.clone()]);
    let doc = document(
        owner(),
        json!({"direct":{"input":{"$ref":nested_id}},"pointer":{"input":{"$ref":format!("{A}#/$defs/child")}}}),
    );
    let contract = ready(&conformant(&doc), &under_loser, "direct");
    verdicts(&contract, &[("true", "satisfies"), ("2", "fails")]);
    helpers_ready(&doc, &under_loser, "direct");
    located(
        &doc,
        &under_loser,
        "/operations/direct/input/$ref",
        "/$defs/child",
        Some(A),
        false,
    );
    identity_refusal(&doc, &under_loser, "pointer");
    unresolved_identity(&doc, &under_loser, "/operations/pointer/input/$ref");
    let under_healthy = resources(&[(V, json!({"$defs":{"loser":nested}})), ext]);
    let doc = document(
        owner(),
        json!({"pointer":{"input":{"$ref":format!("{V}#/$defs/loser/$defs/child")}},"loser":{"input":{"$ref":format!("{V}#/$defs/loser")}}}),
    );
    verdicts(
        &ready(&conformant(&doc), &under_healthy, "pointer"),
        &[("true", "satisfies"), ("2", "fails")],
    );
    helpers_ready(&doc, &under_healthy, "pointer");
    located(
        &doc,
        &under_healthy,
        "/operations/pointer/input/$ref",
        "/$defs/loser/$defs/child",
        Some(V),
        false,
    );
    identity_refusal(&doc, &under_healthy, "loser");
}
#[test]
fn obi13_exact_contained_host_spelling_selects_the_matching_schema() {
    let doc = one(
        json!({
            "A": {"$id":"https://CASE.example.invalid/schema", "type":"string"},
            "B": {"$id":"https://case.example.invalid/schema", "type":"integer"}
        }),
        json!({"$ref":"https://case.example.invalid/schema"}),
    );
    let validation = doc.validate().unwrap();
    assert_eq!(validation.report().evidence["OBI-13"], Evidence::Met);
    verdicts(
        &ready(&conformant(&doc), &ExternalSchemas::default(), "op"),
        &[("2", "satisfies"), (r#""x""#, "fails")],
    );
    located(
        &doc,
        &ExternalSchemas::default(),
        "/operations/op/input/$ref",
        "/schemas/B",
        None,
        false,
    );
}

#[test]
fn supplied_normalized_identity_peers_remain_ambiguous() {
    let doc = one(
        json!({}),
        json!({"$ref":"https://case.example.invalid/schema"}),
    );
    let supplied = resources(&[
        (
            "https://CASE.example.invalid/schema",
            json!({"type":"string"}),
        ),
        (
            "https://case.example.invalid/schema",
            json!({"type":"integer"}),
        ),
    ]);
    let detail = refusal(prepare(&conformant(&doc), &supplied, "op"));
    assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
    assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
}

#[test]
fn resolved_uri_aliases_preserve_exact_contained_resource_priority() {
    let child = "https://identity.example.invalid/child";
    let schemas = json!({"Parent":{"$id":"https://identity.example.invalid/dir/parent","$defs":{"Child":{"$id":"../child","type":"integer"}}}});
    for uri in [
        child,
        "https://identity.example.invalid/child#",
        "https://identity.example.invalid/x/../child",
    ] {
        let doc = one(schemas.clone(), json!({"$ref":uri}));
        let supplied = resources(&[(child, json!(false))]);
        verdicts(
            &ready(&conformant(&doc), &supplied, "op"),
            &[("2", "satisfies")],
        );
        helpers_ready(&doc, &supplied, "op");
        located(
            &doc,
            &supplied,
            "/operations/op/input/$ref",
            "/schemas/Parent/$defs/Child",
            None,
            false,
        );
    }
    for (contained, external) in [
        ("https://IDENTITY.example.invalid/U", U),
        (U, "https://identity.example.invalid/%55"),
        (
            "https://identity.example.invalid/%4a",
            "https://identity.example.invalid/%4A",
        ),
    ] {
        let doc = one(
            json!({"E":{"$id":contained,"type":"integer"}}),
            json!({"$ref":external}),
        );
        let supplied = resources(&[(external, json!({"type":"string"}))]);
        verdicts(
            &ready(&conformant(&doc), &supplied, "op"),
            &[("2", "satisfies"), (r#""x""#, "fails")],
        );
        helpers_ready(&doc, &supplied, "op");
        located(
            &doc,
            &supplied,
            "/operations/op/input/$ref",
            "/schemas/E",
            None,
            false,
        );
    }
}
#[test]
fn missing_fragments_in_the_selected_owner_do_not_use_shadowed_content() {
    let supplied = resources(&[(
        U,
        json!({"$anchor":"ordinary","$defs":{"dynamic":{"$dynamicAnchor":"dynamic"},"target":true}}),
    )]);
    for (keyword, fragment) in [
        ("$ref", "ordinary"),
        ("$dynamicRef", "dynamic"),
        ("$ref", "/$defs/target"),
    ] {
        let doc = one(owner(), json!({keyword:format!("{U}#{fragment}")}));
        let validation = doc.validate().unwrap();
        assert_eq!(validation.report().evidence["OBI-12"], Evidence::Violated);
        assert!(validation.conformant().is_none());
        assert!(matches!(
            reference(&doc, &supplied, &format!("/operations/op/input/{keyword}")),
            ReferenceResolution::Unresolved { .. }
        ));
        // The valid external-carrier reexpressions and their required Undefined
        // verdicts are retained in current_reference_obligations.
    }
}
#[test]
fn selected_anchor_kind_preserves_dynamic_dispatch_and_retained_ownership() {
    let supplied = resources(&[(A, json!({"$id":U,"$dynamicAnchor":"node","type":"boolean"}))]);
    for dynamic in [false, true] {
        let keyword = if dynamic { "$dynamicAnchor" } else { "$anchor" };
        let doc = one(
            json!({"E":{"$id":U,keyword:"node","type":"integer"},"Outer":{"$id":"https://identity.example.invalid/outer","$defs":{"outerNode":{"$dynamicAnchor":"node","type":"string"}},"$dynamicRef":format!("{U}#node")}}),
            json!({"$ref":"https://identity.example.invalid/outer"}),
        );
        located(
            &doc,
            &supplied,
            "/schemas/Outer/$dynamicRef",
            "/schemas/E",
            None,
            dynamic,
        );
        let proof = conformant(&doc);
        let contract = ready(&proof, &supplied, "op");
        drop(proof);
        drop(doc);
        let expected = if dynamic {
            [("2", "fails"), (r#""x""#, "satisfies")]
        } else {
            [("2", "satisfies"), (r#""x""#, "fails")]
        };
        verdicts(&contract, &expected);
        let location = failed_location(&contract, if dynamic { "2" } else { r#""x""# });
        assert_eq!(
            location.pointer,
            if dynamic {
                "/schemas/Outer/$defs/outerNode/type"
            } else {
                "/schemas/E/type"
            }
        );
    }
}
fn assert_opaque_namespace(doc: &Document, supplied: &ExternalSchemas, operation: &str, uri: &str) {
    let proof = conformant(doc);
    let input = proof.resolve_operation(operation).unwrap().input().unwrap();
    let environment = input
        .environment(
            supplied,
            &PreparationLimits::default(),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
    let failure = environment.resolve(&environment.root(), uri).unwrap_err();
    assert_eq!(
        failure.kind,
        ResolutionFailureKind::UnsupportedCapability {
            capability: "custom-profile-namespace".into(),
        }
    );
    // Keep the precise query refusal while allowing Plan to retain it for
    // value-dependent analysis. It must not invent a target or an absence.
    capability_only(
        &ready(&proof, supplied, operation),
        &["null", "2", r#""x""#],
    );
}

#[test]
fn selected_original_resource_controls_dialect_and_namespace_certification() {
    let supplied = resources(&[(
        A,
        json!({"$id":U,"$schema":"https://identity.example.invalid/custom","$defs":{"Child":{"$id":W,"type":"integer"}}}),
    )]);
    let doc = document(
        owner(),
        json!({"owner":{"input":{"$ref":U}},"alias":{"input":{"$ref":A}},"nested":{"input":{"$ref":W}}}),
    );
    verdicts(
        &ready(&conformant(&doc), &supplied, "owner"),
        &[("2", "satisfies")],
    );
    helpers_ready(&doc, &supplied, "owner");
    identity_refusal(&doc, &supplied, "alias");
    // Unknown required vocabulary can introduce competing names. It does not
    // prove that W is absent, even though no actual indexed child claims it.
    assert_opaque_namespace(&doc, &supplied, "nested", W);
    // Identical child bodies retain distinct resolved bases and inherited dialects.
    let child = json!({"$id":"child","type":"integer"});
    let supplied = resources(&[
        (
            "https://identity.example.invalid/old/root",
            json!({"$schema":"https://identity.example.invalid/custom","$defs":{"Child":child}}),
        ),
        (
            "https://identity.example.invalid/new/root",
            json!({"$defs":{"Child":child}}),
        ),
    ]);
    let doc = document(
        json!({}),
        json!({"old":{"input":{"$ref":"https://identity.example.invalid/old/root"}},"fragment":{"input":{"$ref":"https://identity.example.invalid/old/root#/$defs/Child"}},"new":{"input":{"$ref":"https://identity.example.invalid/new/child"}}}),
    );
    for (operation, uri) in [
        ("old", "https://identity.example.invalid/old/root"),
        (
            "fragment",
            "https://identity.example.invalid/old/root#/$defs/Child",
        ),
        ("new", "https://identity.example.invalid/new/child"),
    ] {
        assert_opaque_namespace(&doc, &supplied, operation, uri);
    }
    // Retain the independent base/inheritance control with two fully known
    // profiles; the opaque fixture above intentionally cannot prove it.
    let supplied = resources(&[
        (
            "https://identity.example.invalid/old/root",
            json!({"$schema":"https://json-schema.org/draft/2019-09/schema","$defs":{"Child":child}}),
        ),
        (
            "https://identity.example.invalid/new/root",
            json!({"$defs":{"Child":child}}),
        ),
    ]);
    let doc = document(
        json!({}),
        json!({"old":{"input":{"$ref":"https://identity.example.invalid/old/child"}},"new":{"input":{"$ref":"https://identity.example.invalid/new/child"}}}),
    );
    let proof = conformant(&doc);
    for (operation, dialect) in [
        ("old", "https://json-schema.org/draft/2019-09/schema"),
        ("new", "https://json-schema.org/draft/2020-12/schema"),
    ] {
        verdicts(
            &ready(&proof, &supplied, operation),
            &[("2", "satisfies"), (r#""x""#, "fails")],
        );
        let input = proof.resolve_operation(operation).unwrap().input().unwrap();
        let environment = input
            .environment(
                &supplied,
                &PreparationLimits::default(),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        let child = environment
            .resolve(
                &environment.root(),
                &format!("https://identity.example.invalid/{operation}/child"),
            )
            .unwrap()
            .schema;
        assert_eq!(child.dialect(), Some(dialect));
    }
}

#[test]
fn user_claims_to_standard_schema_names_cannot_replace_trusted_assessment() {
    for uri in [
        "https://json-schema.org/draft/2020-12/schema",
        "https://openbindings.com/schema/openbindings-0.2.json",
    ] {
        let doc = one(
            json!({"E":{"$id":uri,"type":"integer"},"Malformed":{"type":17}}),
            json!({"$ref":uri}),
        );
        let supplied = resources(&[(uri, json!(true))]);
        let validation = doc.validate().unwrap();
        assert_eq!(validation.report().evidence["OBI-10"], Evidence::Violated);
        assert!(validation.conformant().is_none());
        let valid = one(
            json!({"E":{"$id":uri,"type":"integer"}}),
            json!({"$ref":uri}),
        );
        verdicts(
            &ready(&conformant(&valid), &supplied, "op"),
            &[("2", "satisfies"), (r#""x""#, "fails")],
        );
        helpers_ready(&valid, &supplied, "op");
        assert_eq!(
            doc.validate().unwrap().report().evidence["OBI-10"],
            Evidence::Violated
        );
        let malformed=Document::parse(json!({"openbindings":"0.2.0","operations":{},"unexpected":true,"schemas":{"E":{"$id":uri}}}).to_string()).unwrap();
        assert_eq!(
            malformed.validate().unwrap().report().evidence["OBI-02"],
            Evidence::Violated
        );
    }
    // Without a contained owner, supplied claims on a packaged identity conflict.
    let standard = "https://json-schema.org/draft/2020-12/schema";
    let doc = one(json!({}), json!({"$ref":standard}));
    assert_eq!(
        refusal(prepare(
            &conformant(&doc),
            &resources(&[(standard, json!(false))]),
            "op"
        ))
        .kind,
        PreparationFailureKind::InvalidExternalSchema
    );
}
#[test]
fn supplied_retrieval_and_canonical_names_are_equal_priority_claims() {
    let supplied = resources(&[
        (A, json!({"$id":V,"type":"string"})),
        (V, json!({"$id":W,"type":"integer"})),
    ]);
    for uri in [A, V] {
        let doc = one(json!({}), json!({"$ref":uri}));
        identity_refusal(&doc, &supplied, "op");
        unresolved_identity(&doc, &supplied, "/operations/op/input/$ref");
    }
    let doc = one(json!({}), json!({"$ref":W}));
    verdicts(
        &ready(&conformant(&doc), &supplied, "op"),
        &[("2", "satisfies"), (r#""x""#, "fails")],
    );
    helpers_ready(&doc, &supplied, "op");
    located(
        &doc,
        &supplied,
        "/operations/op/input/$ref",
        "",
        Some(V),
        false,
    );
}
#[test]
fn preparations_and_retained_schema_owners_keep_independent_resource_snapshots() {
    let supplied = resources(&[(U, json!({"type":"string"}))]);
    let old = one(json!({}), json!({"$ref":U}));
    let d1 = one(owner(), json!({"$ref":U}));
    let d2 = one(json!({"E":{"$id":U,"type":"boolean"}}), json!({"$ref":U}));
    let old_proof = conformant(&old);
    let old_owner = ready(&old_proof, &supplied, "op");
    let p1 = conformant(&d1);
    let first = ready(&p1, &supplied, "op");
    let p2 = conformant(&d2);
    let second = ready(&p2, &supplied, "op");
    drop((old_proof, p1, p2, old, d1, d2));
    verdicts(&old_owner, &[(r#""x""#, "satisfies"), ("2", "fails")]);
    verdicts(&first, &[("2", "satisfies"), (r#""x""#, "fails")]);
    verdicts(&second, &[("true", "satisfies"), ("2", "fails")]);
    assert_eq!(
        failed_location(&old_owner, "2").source_uri.as_deref(),
        Some(U)
    );
    assert_eq!(failed_location(&first, r#""x""#).source_uri, None);
    let doc = document(
        json!({}),
        json!({"op":{"input":{"$ref":V}},"other":{"input":true}}),
    );
    let mut retained = Vec::new();
    for kind in ["integer", "string"] {
        let proof = conformant(&doc);
        let snapshot = resources(&[(V, json!({"type":kind}))]);
        retained.push(ready(&proof, &snapshot, "op"));
        ready(&proof, &snapshot, "other");
        ready(&proof, &snapshot, "op");
        drop((proof, snapshot));
    }
    drop(doc);
    verdicts(&retained[0], &[("2", "satisfies"), (r#""x""#, "fails")]);
    verdicts(&retained[1], &[("2", "fails"), (r#""x""#, "satisfies")]);
    assert_eq!(
        failed_location(&retained[0], r#""x""#)
            .source_uri
            .as_deref(),
        Some(V)
    );
    assert_eq!(
        failed_location(&retained[1], "2").source_uri.as_deref(),
        Some(V)
    );
}
#[test]
fn identity_conflicts_do_not_poison_healthy_preparations_or_mimic_missing_content() {
    let supplied = resources(&[(A, json!({"$id":U,"type":"string"}))]);
    let doc = document(
        owner(),
        json!({"healthy":{"input":{"$ref":U}},"conflict":{"input":{"$ref":A}},"mixed":{"input":{"allOf":[{"$ref":"https://identity.example.invalid/missing"},{"$ref":A}]}}}),
    );
    let proof = conformant(&doc);
    for _ in 0..3 {
        verdicts(&ready(&proof, &supplied, "healthy"), &[("2", "satisfies")]);
        identity_refusal(&doc, &supplied, "conflict");
    }
    // The original projection reports its first unresolved reference. The
    // semantic fallback continues inventory accounting and retains the reached
    // identity conflict as an input failure, rather than inventing a carrier.
    let default = refusal(prepare_with(
        &conformant(&doc),
        &supplied,
        "mixed",
        &Engine::Default,
    ));
    assert_eq!(default.kind, PreparationFailureKind::InvalidExternalSchema);
    let projected = refusal(prepare_with(
        &conformant(&doc),
        &supplied,
        "mixed",
        &Engine::Projection,
    ));
    assert!(matches!(
        projected.kind,
        PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::ResourceUnavailable { .. }
        }
    ));
}
#[test]
fn precancelled_identity_queries_allow_healthy_independent_retries() {
    let doc = one(owner(), json!({"$ref":U}));
    let supplied = resources(&[(U, json!(false))]);
    let proof = conformant(&doc);
    let operation = proof.resolve_operation("op").expect("declared operation");
    let control = std::sync::atomic::AtomicBool::new(false);
    control.store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        refusal(
            operation
                .input()
                .expect("declared input")
                .prepare_with_control(
                    &Engine::Default,
                    &supplied,
                    &openbindings::PreparationLimits::default(),
                    &control
                )
        )
        .kind,
        PreparationFailureKind::Cancelled
    );
    assert!(
        !doc.references_with_resources(supplied.clone(), &control)
            .complete
    );
    verdicts(&ready(&proof, &supplied, "op"), &[("2", "satisfies")]);
}
#[test]
fn oversized_supplied_closures_refuse_without_poisoning_healthy_entries() {
    let deep = format!("{}true{}", r#"{"items":"#.repeat(258), "}".repeat(258));
    let supplied = ExternalSchemas::new([
        ExternalSchema {
            uri: A.into(),
            document: value(&deep),
        },
        ExternalSchema {
            uri: U.into(),
            document: value("false"),
        },
    ])
    .unwrap();
    let doc = document(
        owner(),
        json!({"healthy":{"input":{"$ref":U}},"deep":{"input":{"$ref":A}}}),
    );
    for evaluator in [
        &Engine::Default as &dyn SchemaEvaluator<Validator = ProbeValidator>,
        &Engine::Projection,
    ] {
        let proof = conformant(&doc);
        ready_with(&proof, &supplied, "healthy", evaluator);
        assert!(matches!(
            refusal(prepare_with(&proof, &supplied, "deep", evaluator)).kind,
            PreparationFailureKind::Limit { .. }
        ));
        ready_with(&proof, &supplied, "healthy", evaluator);
    }
}
#[test]
fn identity_refusal_location_is_optional_and_measured_before_copy() {
    for size in [64 * 1024 - 256, 64 * 1024] {
        let key = "x".repeat(size);
        let supplied = resources(&[(V, json!({"properties":{&key:{"$id":U}}}))]);
        let doc = one(owner(), json!({"$ref":V}));
        // Preserve the original fixture: an absent property needs no child
        // interpretation, whereas a present property must not use a loser.
        let contract = ready(&conformant(&doc), &supplied, "op");
        verdicts(&contract, &[("{}", "satisfies")]);
        capability_only(&contract, &[&json!({&key:1}).to_string()]);

        // A real static reference conflict still fails preparation. Place its
        // referrer at the same long original coordinate to keep the exact
        // optional-origin boundary independent of projection/evaluation timing.
        let supplied = resources(&[(
            V,
            json!({
                "properties":{&key:{"$ref":format!("{V}#/$defs/collision")}},
                "$defs":{"collision":{"$id":U}}
            }),
        )]);
        let detail = refusal(prepare(&conformant(&doc), &supplied, "op"));
        assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
        // Source URI bytes share the allowance with the full original pointer.
        if let Some(origin) = detail.origin {
            assert_eq!(origin.source_uri.as_deref(), Some(V));
            assert_eq!(origin.pointer, format!("/properties/{key}"));
        }
    }
}
#[test]
fn invalid_or_unsupported_selected_owners_do_not_use_shadowed_content() {
    for (schema, rule) in [
        (
            json!({"$id":U,"$schema":"https://identity.example.invalid/custom","type":"integer"}),
            "OBI-09",
        ),
        (json!({"$id":U,"type":17}), "OBI-10"),
    ] {
        let doc = one(json!({"E":schema}), json!({"$ref":U}));
        let supplied = resources(&[(U, json!(true))]);
        let validation = doc.validate().unwrap();
        assert_eq!(validation.report().evidence[rule], Evidence::Violated);
        assert!(validation.conformant().is_none());
        located(
            &doc,
            &supplied,
            "/operations/op/input/$ref",
            "/schemas/E",
            None,
            false,
        );
    }
}
#[test]
fn anonymous_document_resources_do_not_acquire_a_generated_identity() {
    for uri in [
        "https://identity.example.invalid/acquired-obi#/schemas/E",
        "https://sdk-program.openbindings.invalid/r0#/schemas/E",
        "https://sdk-bounds.openbindings.invalid/0/r0#/schemas/E",
    ] {
        let doc = one(json!({"E":{"type":"integer"}}), json!({"$ref":uri}));
        let supplied = ExternalSchemas::default();
        assert!(matches!(
            refusal(prepare_with(
                &conformant(&doc),
                &supplied,
                "op",
                &Engine::Projection
            ))
            .kind,
            PreparationFailureKind::Resolution {
                cause: ResolutionFailureKind::ResourceUnavailable { .. }
            }
        ));
        assert!(matches!(
            reference(&doc, &supplied, "/operations/op/input/$ref"),
            ReferenceResolution::Unresolved {
                detail: ReferenceRefusal {
                    reason: ReferenceRefusalReason::ResourceUnavailable,
                    ..
                }
            }
        ));
    }
}

#[test]
fn unused_resource_peer_conflicts_do_not_contaminate_other_selections() {
    for (schemas, supplied) in [
        (
            json!({"E":{"$id":U,"type":"integer"},"Duplicate":{"$id":U,"type":"string"}}),
            ExternalSchemas::default(),
        ),
        (
            json!({}),
            resources(&[(A, json!({"$id":U})), (B, json!({"$id":U}))]),
        ),
    ] {
        let contained_conflict = schemas.get("Duplicate").is_some();
        let doc = document(
            schemas,
            json!({"healthy":{"input":{"type":"boolean"}},"conflict":{"input":{"$ref":U}}}),
        );
        if contained_conflict {
            let validation = doc.validate().unwrap();
            assert_eq!(validation.report().evidence["OBI-13"], Evidence::Violated);
            assert!(validation.conformant().is_none());
            unresolved_identity(&doc, &supplied, "/operations/conflict/input/$ref");
            continue;
        }
        let proof = conformant(&doc);
        for _ in 0..3 {
            verdicts(
                &ready(&proof, &supplied, "healthy"),
                &[("true", "satisfies"), ("2", "fails")],
            );
            let detail = refusal(prepare(&proof, &supplied, "conflict"));
            assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
            assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
        }
        helpers_ready(&doc, &supplied, "healthy");
    }
}

#[test]
fn nested_resources_do_not_inherit_shadowed_parent_dynamic_scope() {
    let supplied = resources(&[(
        A,
        json!({"$id":U,"$dynamicAnchor":"node","type":"string","$defs":{"Child":{"$id":W,"$dynamicRef":"#node","$defs":{"Node":{"$dynamicAnchor":"node","type":"integer"}}}}}),
    )]);
    let doc = one(owner(), json!({"$ref":W}));
    let contract = ready(&conformant(&doc), &supplied, "op");
    verdicts(&contract, &[("2", "satisfies"), (r#""x""#, "fails")]);
    assert_eq!(
        failed_location(&contract, r#""x""#),
        SchemaOrigin::new(
            Some(A.into()),
            "/$defs/Child/$defs/Node/type".parse().unwrap()
        )
    );
}

#[test]
fn contained_resource_priority_preserves_both_value_verdicts() {
    let uri = "https://ex.invalid/shared";
    let doc = one(
        json!({"S":{"$id":uri,"type":"integer"}}),
        json!({"$ref":uri}),
    );
    let supplied = resources(&[(uri, json!({"type":"string"}))]);
    let contract = ready(&conformant(&doc), &supplied, "op");
    verdicts(&contract, &[("1", "satisfies"), (r#""x""#, "fails")]);
    assert_eq!(
        failed_location(&contract, r#""x""#),
        SchemaOrigin::new(None, "/schemas/S/type".parse().unwrap())
    );
    helpers_ready(&doc, &supplied, "op");
}

#[test]
fn collision_policy_matrix_used_and_unused() {
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Origin {
        Contained,
        Supplied,
        NestedSupplied,
        Standard,
    }
    use Origin::*;
    let standard = "https://json-schema.org/draft/2020-12/meta/validation";
    for left in [Contained, Supplied, NestedSupplied, Standard] {
        for right in [Contained, Supplied, NestedSupplied, Standard] {
            let origins = [left, right];
            let uri = if origins.contains(&Standard) {
                standard
            } else {
                U
            };
            let mut schemas = json!({});
            let mut supplied = Vec::new();
            for (i, origin) in origins.iter().enumerate() {
                let claim = json!({"$id":uri,"type":"integer"});
                match origin {
                    Contained => schemas[format!("owner{i}")] = claim,
                    Supplied => supplied.push(([A, B][i], claim)),
                    NestedSupplied => supplied.push(([A, B][i], json!({"$defs":{"nested":claim}}))),
                    Standard => (),
                }
            }
            let supplied = resources(&supplied);
            let doc = document(
                schemas,
                json!({
                    "used":{"input":{"$ref":uri}},
                    "unused":{"input":{"type":"boolean"}}
                }),
            );
            if origins == [Contained, Contained] {
                let validation = doc.validate().unwrap();
                assert_eq!(validation.report().evidence["OBI-13"], Evidence::Violated);
                assert!(validation.conformant().is_none());
                unresolved_identity(&doc, &supplied, "/operations/used/input/$ref");
                continue;
            }
            let proof = conformant(&doc);
            verdicts(
                &ready(&proof, &supplied, "unused"),
                &[("true", "satisfies"), ("2", "fails")],
            );
            helpers_ready(&doc, &supplied, "unused");
            let contained = origins.iter().filter(|&&o| o == Contained).count();
            if contained == 1 {
                verdicts(
                    &ready(&proof, &supplied, "used"),
                    &[("2", "satisfies"), (r#""x""#, "fails")],
                );
                helpers_ready(&doc, &supplied, "used");
            } else if origins == [Standard, Standard] {
                // Both references designate the one packaged carrier.
                verdicts(
                    &ready(&proof, &supplied, "used"),
                    &[("{}", "satisfies"), ("2", "fails")],
                );
            } else {
                for engine in [
                    &Engine::Default as &dyn SchemaEvaluator<Validator = ProbeValidator>,
                    &Engine::Projection,
                ] {
                    let detail =
                        refusal(prepare_with(&conformant(&doc), &supplied, "used", engine));
                    assert_eq!(
                        detail.kind,
                        PreparationFailureKind::InvalidExternalSchema,
                        "{left:?}/{right:?}"
                    );
                    assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
                }
            }
        }
    }
}

#[test]
fn packaged_retrieval_aliases_conflict_even_with_a_different_declared_id() {
    for suffix in [
        "schema",
        "meta/core",
        "meta/applicator",
        "meta/unevaluated",
        "meta/validation",
        "meta/meta-data",
        "meta/format-annotation",
        "meta/content",
    ] {
        let uri = format!("https://json-schema.org/draft/2020-12/{suffix}");
        for body in [json!(true), json!({"$id":U})] {
            let supplied = resources(&[(&uri, body)]);
            let doc = document(
                json!({}),
                json!({"used":{"input":{"$ref":uri}},"unused":{"input":true}}),
            );
            let proof = conformant(&doc);
            assert_eq!(
                refusal(prepare(&proof, &supplied, "used")).kind,
                PreparationFailureKind::InvalidExternalSchema,
                "{uri}"
            );
            verdicts(
                &ready(&proof, &supplied, "unused"),
                &[("null", "satisfies")],
            );
        }
    }
}

#[test]
fn standard_identity_claims_are_ambiguous_not_completions() {
    let standard = "https://json-schema.org/draft/2020-12/meta/validation";
    // A supplied claim to a packaged meta-schema identity is ambiguous with
    // it; without the claim the reference to `U` is missing.
    // A value whose verdict depends on `U` under each keyword: one the other
    // branch fails under `anyOf`, and one it accepts under `allOf`.
    for (keyword, replacement, depends) in [("anyOf", false, "-1"), ("allOf", true, "1")] {
        let doc = one(
            json!({}),
            json!({keyword:[{"$ref":U},{"$ref":format!("{standard}#/$defs/nonNegativeInteger")}]}),
        );
        // Without the claim `U` is missing: a value whose verdict depends on
        // it gives no verdict.
        let contract = prepare(&conformant(&doc), &ExternalSchemas::default(), "op").unwrap();
        assert!(matches!(
            contract.evaluate(&value(depends)),
            ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::ResourceUnavailable
        ));
        let claim = json!({"$id":standard,"$defs":{"nonNegativeInteger":replacement}});
        for supplied in [
            resources(&[(U, json!({"not":!replacement,"$defs":{"x":claim}}))]),
            resources(&[(U, json!(replacement)), (A, claim.clone())]),
            resources(&[(U, json!(replacement)), (A, json!({"$defs":{"x":claim}}))]),
            resources(&[(U, json!(replacement)), (standard, claim)]),
        ] {
            let detail = refusal(prepare(&conformant(&doc), &supplied, "op"));
            assert_eq!(
                detail.kind,
                PreparationFailureKind::InvalidExternalSchema,
                "{keyword}"
            );
            assert_eq!(detail.kind, PreparationFailureKind::InvalidExternalSchema);
        }
    }
}
