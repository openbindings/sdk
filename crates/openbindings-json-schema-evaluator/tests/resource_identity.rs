//! Context-local identity admission. RI numbers map the frozen 34-case policy
//! matrix and SOL supplements; source regressions do not qualify a package.
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
fn document(schemas: Value, operations: Value) -> ParsedDocument {
    ParsedDocument::parse(
        json!({"openbindings":"0.2.0","schemas":schemas,"operations":operations}).to_string(),
    )
    .unwrap()
}
fn one(schemas: Value, schema: Value) -> ParsedDocument {
    document(schemas, json!({"op":{"input":schema}}))
}
fn owner() -> Value {
    json!({"E":{"$id":U,"type":"integer","minimum":1}})
}
fn resources(rows: &[(&str, Value)]) -> ResourceSet {
    ResourceSet::new(rows.iter().map(|(uri, schema)| SchemaResource {
        uri: (*uri).into(),
        document: value(&schema.to_string()),
    }))
    .unwrap()
}
fn context(doc: &ParsedDocument, supplied: &ResourceSet) -> ValueContracts {
    doc.value_contracts(Arc::new(DefaultEvaluator::new()), supplied.clone())
        .unwrap()
}
fn ready(ctx: &ValueContracts, operation: &str) -> PreparedContract {
    match ctx.prepare(operation, Side::Input) {
        ContractPreparation::Ready(contract) => contract,
        other => panic!("expected ready {operation}: {other:?}"),
    }
}
fn label(result: ValueOutcome) -> &'static str {
    match result {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::NoVerdict { .. } => "no-verdict",
    }
}
fn verdicts(contract: &PreparedContract, cases: &[(&str, &str)]) {
    for (text, expected) in cases {
        assert_eq!(label(contract.validate(&value(text))), *expected, "{text}");
    }
}
// Exercise both public projection helpers independently of the default adapter's
// strict-first/fallback choice. Ready here only reports successful projection.
struct Project(bool);
struct Projected;
impl PreparedSchema for Projected {
    fn validate(&self, _: &JsonValue, _: &WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
impl SchemaEvaluator for Project {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        if self.0 {
            request.evaluation_bounds(control)?;
        } else {
            request.evaluation_program(control)?;
        }
        Ok(Arc::new(Projected))
    }
}
fn helpers_ready(doc: &ParsedDocument, supplied: &ResourceSet, operation: &str) {
    for bounds in [false, true] {
        let ctx = doc
            .value_contracts(Arc::new(Project(bounds)), supplied.clone())
            .unwrap();
        ready(&ctx, operation);
    }
}
fn refusal(prepared: ContractPreparation) -> NoVerdict {
    match prepared {
        ContractPreparation::NoVerdict { detail } => detail,
        other => panic!("expected setup refusal: {other:?}"),
    }
}
fn identity_refusal(doc: &ParsedDocument, supplied: &ResourceSet, operation: &str) {
    let mut engines: Vec<Arc<dyn SchemaEvaluator>> = vec![
        Arc::new(DefaultEvaluator::new()),
        Arc::new(Project(false)),
        Arc::new(Project(true)),
    ];
    for evaluator in engines.drain(..) {
        let detail = refusal(
            doc.value_contracts(evaluator, supplied.clone())
                .unwrap()
                .prepare(operation, Side::Input),
        );
        assert_eq!(
            detail.reason,
            NoVerdictReason::ConservativePreparation,
            "{detail:?}"
        );
        assert!(
            matches!(
                detail.code.as_str(),
                "ambiguous-resource" | "resource-identity-conflict"
            ),
            "{detail:?}"
        );
        assert!(detail.location.is_some(), "{detail:?}");
    }
}
fn reference(doc: &ParsedDocument, supplied: &ResourceSet, pointer: &str) -> ReferenceResolution {
    doc.references_with_resources(supplied.clone(), &WorkControl::new())
        .unwrap()
        .references
        .into_iter()
        .find(|r| r.location.pointer == pointer)
        .unwrap()
        .resolution
}
fn located(
    doc: &ParsedDocument,
    supplied: &ResourceSet,
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
            assert_eq!(actual.resource.as_deref(), source);
            assert_eq!(dynamic_lookup, dynamic);
        }
        other => panic!("{other:?}"),
    }
}
fn unresolved_identity(doc: &ParsedDocument, supplied: &ResourceSet, pointer: &str) {
    match reference(doc, supplied, pointer) {
        ReferenceResolution::Unresolved { detail } => {
            assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
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
fn failed_location(contract: &PreparedContract, text: &str) -> SchemaLocation {
    let ValueOutcome::Fails { problems, .. } = contract.validate(&value(text)) else {
        panic!("expected failure")
    };
    problems
        .into_iter()
        .find_map(|p| p.schema_location)
        .unwrap()
}

#[test]
fn ri01_03_exact_b32_original_and_edited_resources() {
    let expectations: Value = serde_json::from_str(include_str!(
        "fixtures/resource-identity/B32-expectations.json"
    ))
    .unwrap();
    let schema = value(include_str!(
        "fixtures/resource-identity/B32-runner-result.json"
    ));
    let supplied = ResourceSet::new([SchemaResource {
        uri: schema.get("$id").unwrap().as_str().unwrap().into(),
        document: schema,
    }])
    .unwrap();
    for (edited, text) in [
        (
            false,
            include_str!("fixtures/resource-identity/B32-original.json"),
        ),
        (
            true,
            include_str!("fixtures/resource-identity/B32-edited.json"),
        ),
    ] {
        let doc = ParsedDocument::parse(text).unwrap();
        let local = context(&doc, &ResourceSet::default());
        let external = context(&doc, &supplied);
        for case in expectations["valueCases"].as_array().unwrap() {
            let ctx = if case["setup"] == "runner-result-supplied" {
                &external
            } else {
                &local
            };
            let side = if case["side"] == "input" {
                Side::Input
            } else {
                Side::Output
            };
            let result = ctx.prepare(case["operation"].as_str().unwrap(), side);
            let actual = match result {
                ContractPreparation::Ready(contract) => {
                    label(contract.validate(&value(case["valueText"].as_str().unwrap())))
                }
                ContractPreparation::NoContract => "no-contract",
                other => panic!("{} edited={edited}: {other:?}", case["id"]),
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
        for (ctx, expected) in [
            (&external, "satisfies"),
            (&local, if edited { "satisfies" } else { "no-verdict" }),
        ] {
            let ContractPreparation::Ready(contract) = ctx.prepare("run_job", Side::Output) else {
                panic!("finished example setup")
            };
            assert_eq!(
                label(contract.validate(&example)),
                expected,
                "finished example edited={edited}"
            );
        }
    }
}
#[test]
fn ri04_05_contained_authority_ignores_supplied_body_and_keeps_raw_catalog() {
    let doc = one(owner(), json!({"$ref":U}));
    let observed = Arc::new(Mutex::new(Vec::new()));
    struct Catalog(Arc<Mutex<Vec<String>>>);
    impl SchemaEvaluator for Catalog {
        fn prepare(
            &self,
            request: &SchemaRequest,
            control: &WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            *self.0.lock().unwrap() = request
                .supplied_resources()
                .iter()
                .map(|r| r.document.text().to_owned())
                .collect();
            DefaultEvaluator::new().prepare(request, control)
        }
    }
    for text in [
        r#"{"$id":"https://identity.example.invalid/U","type":"integer","minimum":1}"#,
        "{ \"minimum\": 1.0, \"type\": \"integer\", \"$id\": \"https://identity.example.invalid/U\" }",
        "false",
        r#"{"type":"string"}"#,
    ] {
        let supplied = ResourceSet::new([SchemaResource {
            uri: U.into(),
            document: value(text),
        }])
        .unwrap();
        let ctx = doc
            .value_contracts(Arc::new(Catalog(observed.clone())), supplied.clone())
            .unwrap();
        let contract = ready(&ctx, "op");
        verdicts(
            &contract,
            &[("2", "satisfies"), ("0", "fails"), (r#""x""#, "fails")],
        );
        assert_eq!(*observed.lock().unwrap(), vec![text]);
        assert_eq!(
            failed_location(&contract, "0"),
            SchemaLocation {
                resource: None,
                pointer: "/schemas/E/minimum".into()
            }
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
fn ri06_duplicate_contained_owners_refuse_lookup_and_direct_entry() {
    let doc = document(
        owner(),
        json!({"direct":{"input":{"$id":U,"type":"string"}},"lookup":{"input":{"$ref":U}},"pointer":{"input":{"$ref":"#/schemas/E"}}}),
    );
    let supplied = resources(&[(U, json!(true))]);
    assert_eq!(
        doc.assess().unwrap().report().evidence["OBI-13"],
        Evidence::Violated
    );
    identity_refusal(&doc, &supplied, "direct");
    identity_refusal(&doc, &supplied, "lookup");
    unresolved_identity(&doc, &supplied, "/operations/lookup/input/$ref");
    identity_refusal(&doc, &supplied, "pointer");
    unresolved_identity(&doc, &supplied, "/operations/pointer/input/$ref");
}
#[test]
fn ri07_external_references_back_use_contained_target() {
    let doc = one(owner(), json!({"$ref":A}));
    let supplied = resources(&[(A, json!({"$id":V,"$ref":U})), (U, json!(false))]);
    let contract = ready(&context(&doc, &supplied), "op");
    verdicts(&contract, &[("2", "satisfies"), ("0", "fails")]);
    assert_eq!(
        failed_location(&contract, "0").pointer,
        "/schemas/E/minimum"
    );
    helpers_ready(&doc, &supplied, "op");
}
#[test]
fn ri08_12_aliases_cannot_bypass_canonical_conflicts() {
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
            &ready(&context(&doc, &supplied), "owner"),
            &[("2", "satisfies")],
        );
        identity_refusal(&doc, &supplied, "alias");
        unresolved_identity(&doc, &supplied, "/operations/alias/input/$ref");
        let detail = refusal(context(&doc, &supplied).prepare("dynamic", Side::Input));
        assert_eq!(detail.code, "resource-identity-conflict");
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
fn ri09_10_distinct_canonical_identity_preserves_alias_provenance_and_base() {
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
        let ctx = context(&doc, &supplied);
        verdicts(
            &ready(&ctx, "canonical"),
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
            failed_location(&ready(&context(&doc, &own), "canonical"), "2")
                .resource
                .as_deref(),
            Some(retrieval)
        );
        if retrieval == A {
            verdicts(&ready(&ctx, "alias"), &[(r#""x""#, "satisfies")]);
        }
    }
}
#[test]
fn ri11_31_invalid_resource_configuration_is_not_hidden_by_precedence() {
    for second in [
        U,
        "https://identity.example.invalid/U#",
        "https://identity.example.invalid/x/../U",
    ] {
        for text in ["true", "false"] {
            assert!(
                ResourceSet::new([
                    SchemaResource {
                        uri: U.into(),
                        document: value("true")
                    },
                    SchemaResource {
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
            ResourceSet::new([SchemaResource {
                uri: uri.into(),
                document: value(text)
            }])
            .is_err()
        );
    }
}
#[test]
fn ri13_15_unused_collisions_are_isolated_but_pointer_targets_refuse() {
    let doc = document(
        owner(),
        json!({"parent":{"input":{"$ref":V}},"child":{"input":{"$ref":format!("{V}#/$defs/child")}},"owner":{"input":{"$ref":U}}}),
    );
    let supplied = resources(&[(
        V,
        json!({"type":"string","$defs":{"child":{"$id":U,"type":"boolean"}}}),
    )]);
    verdicts(
        &ready(&context(&doc, &supplied), "parent"),
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
fn ri14_direct_applicators_refuse_competing_carriers() {
    for schema in [
        json!({"properties":{"x":{"$id":U,"type":"string"}}}),
        json!({"allOf":[{"$id":U,"type":"string"}]}),
    ] {
        let doc = one(owner(), json!({"$ref":V}));
        let supplied = resources(&[(V, schema)]);
        identity_refusal(&doc, &supplied, "op");
        // Static inspection locates V; it does not promise V's closure is preparable.
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
fn ri16_17_unique_nested_resources_survive_losing_lexical_ancestors() {
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
    let contract = ready(&context(&doc, &under_loser), "direct");
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
        &ready(&context(&doc, &under_healthy), "pointer"),
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
fn ri18_19_existing_uri_resolution_and_spelling_comparison_are_preserved() {
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
            &ready(&context(&doc, &supplied), "op"),
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
            &ready(&context(&doc, &supplied), "op"),
            &[("2", "fails"), (r#""x""#, "satisfies")],
        );
        helpers_ready(&doc, &supplied, "op");
        located(
            &doc,
            &supplied,
            "/operations/op/input/$ref",
            "",
            Some(external),
            false,
        );
    }
}
#[test]
fn ri20_34_missing_winner_fragments_never_fall_back_to_loser() {
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
        let detail = refusal(context(&doc, &supplied).prepare("op", Side::Input));
        assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
        assert!(
            matches!(detail.code.as_str(), "anchor-missing" | "non-schema-target"),
            "{detail:?}"
        );
        assert!(matches!(
            reference(&doc, &supplied, &format!("/operations/op/input/{keyword}")),
            ReferenceResolution::Unresolved { .. }
        ));
        if keyword == "$ref" {
            let ctx = doc
                .value_contracts(Arc::new(Project(true)), supplied.clone())
                .unwrap();
            assert_eq!(refusal(ctx.prepare("op", Side::Input)).code, detail.code);
        }
    }
}
#[test]
fn ri21_sol02_winning_anchor_kind_controls_dynamic_dispatch_and_retained_owner() {
    let supplied = resources(&[(A, json!({"$id":U,"$dynamicAnchor":"node","type":"boolean"}))]);
    for dynamic in [false, true] {
        let keyword = if dynamic { "$dynamicAnchor" } else { "$anchor" };
        let doc = one(
            json!({"E":{"$id":U,keyword:"node","type":"integer"},"Outer":{"$dynamicAnchor":"node","type":"string"}}),
            json!({"$dynamicRef":format!("{U}#node")}),
        );
        located(
            &doc,
            &supplied,
            "/operations/op/input/$dynamicRef",
            "/schemas/E",
            None,
            dynamic,
        );
        let ctx = context(&doc, &supplied);
        let contract = ready(&ctx, "op");
        drop(ctx);
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
                "/schemas/Outer/type"
            } else {
                "/schemas/E/type"
            }
        );
    }
}
#[test]
fn ri22_23_original_dialect_follows_selected_resource() {
    let supplied = resources(&[(
        A,
        json!({"$id":U,"$schema":"https://identity.example.invalid/custom","$defs":{"Child":{"$id":W,"type":"integer"}}}),
    )]);
    let doc = document(
        owner(),
        json!({"owner":{"input":{"$ref":U}},"alias":{"input":{"$ref":A}},"nested":{"input":{"$ref":W}}}),
    );
    verdicts(
        &ready(&context(&doc, &supplied), "owner"),
        &[("2", "satisfies")],
    );
    helpers_ready(&doc, &supplied, "owner");
    identity_refusal(&doc, &supplied, "alias");
    for engine in [
        Arc::new(DefaultEvaluator::new()) as Arc<dyn SchemaEvaluator>,
        Arc::new(Project(false)),
        Arc::new(Project(true)),
    ] {
        let ctx = doc.value_contracts(engine, supplied.clone()).unwrap();
        let detail = refusal(ctx.prepare("nested", Side::Input));
        assert_eq!(detail.reason, NoVerdictReason::UnsupportedCapability);
        assert_eq!(detail.code, "schema-dialect");
        assert_eq!(
            detail.location.unwrap(),
            SchemaLocation {
                resource: Some(A.into()),
                pointer: "/$defs/Child".into()
            }
        );
    }
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
        json!({"old":{"input":{"$ref":"https://identity.example.invalid/old/child"}},"new":{"input":{"$ref":"https://identity.example.invalid/new/child"}}}),
    );
    let ctx = context(&doc, &supplied);
    assert_eq!(
        refusal(ctx.prepare("old", Side::Input)).reason,
        NoVerdictReason::UnsupportedCapability
    );
    verdicts(
        &ready(&ctx, "new"),
        &[("2", "satisfies"), (r#""x""#, "fails")],
    );
}
#[test]
fn ri24_user_standard_names_do_not_replace_trusted_assessment() {
    for uri in [
        "https://json-schema.org/draft/2020-12/schema",
        "https://openbindings.com/schema/openbindings-0.2.json",
    ] {
        let doc = one(
            json!({"E":{"$id":uri,"type":"integer"},"Malformed":{"type":17}}),
            json!({"$ref":uri}),
        );
        let supplied = resources(&[(uri, json!(true))]);
        verdicts(
            &ready(&context(&doc, &supplied), "op"),
            &[("2", "satisfies"), (r#""x""#, "fails")],
        );
        helpers_ready(&doc, &supplied, "op");
        assert_eq!(
            doc.assess().unwrap().report().evidence["OBI-10"],
            Evidence::Violated
        );
        let malformed=ParsedDocument::parse(json!({"openbindings":"0.2.0","operations":{},"unexpected":true,"schemas":{"E":{"$id":uri}}}).to_string()).unwrap();
        assert_eq!(
            malformed.assess().unwrap().report().evidence["OBI-02"],
            Evidence::Violated
        );
    }
    // A supplied name also precedes the packaged fallback when no contained ID owns it.
    let standard = "https://json-schema.org/draft/2020-12/schema";
    let doc = one(json!({}), json!({"$ref":standard}));
    verdicts(
        &ready(
            &context(&doc, &resources(&[(standard, json!(false))])),
            "op",
        ),
        &[("2", "fails")],
    );
}
#[test]
fn sol01_supplied_alias_and_canonical_names_are_strict_peers() {
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
        &ready(&context(&doc, &supplied), "op"),
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
fn ri25_27_contexts_and_retained_owners_are_independent() {
    let supplied = resources(&[(U, json!({"type":"string"}))]);
    let old = one(json!({}), json!({"$ref":U}));
    let d1 = one(owner(), json!({"$ref":U}));
    let d2 = one(json!({"E":{"$id":U,"type":"boolean"}}), json!({"$ref":U}));
    let old_ctx = context(&old, &supplied);
    let old_owner = ready(&old_ctx, "op");
    let c1 = context(&d1, &supplied);
    let first = ready(&c1, "op");
    let c2 = context(&d2, &supplied);
    let second = ready(&c2, "op");
    drop((old_ctx, c1, c2, old, d1, d2));
    verdicts(&old_owner, &[(r#""x""#, "satisfies"), ("2", "fails")]);
    verdicts(&first, &[("2", "satisfies"), (r#""x""#, "fails")]);
    verdicts(&second, &[("true", "satisfies"), ("2", "fails")]);
    assert_eq!(
        failed_location(&old_owner, "2").resource.as_deref(),
        Some(U)
    );
    assert_eq!(failed_location(&first, r#""x""#).resource, None);
    let doc = document(
        json!({}),
        json!({"op":{"input":{"$ref":V}},"evict":{"input":true}}),
    );
    let mut retained = Vec::new();
    for kind in ["integer", "string"] {
        let ctx = doc
            .value_contracts_with_options(
                Arc::new(DefaultEvaluator::new()),
                resources(&[(V, json!({"type":kind}))]),
                ValueContractOptions { cache_capacity: 1 },
            )
            .unwrap();
        retained.push(ready(&ctx, "op"));
        ready(&ctx, "evict");
        ready(&ctx, "op");
        drop(ctx);
    }
    drop(doc);
    verdicts(&retained[0], &[("2", "satisfies"), (r#""x""#, "fails")]);
    verdicts(&retained[1], &[("2", "fails"), (r#""x""#, "satisfies")]);
    assert_eq!(
        failed_location(&retained[0], r#""x""#).resource.as_deref(),
        Some(V)
    );
    assert_eq!(
        failed_location(&retained[1], "2").resource.as_deref(),
        Some(V)
    );
}
#[test]
fn ri28_29_conflicts_do_not_poison_healthy_preparation_or_become_holes() {
    let supplied = resources(&[(A, json!({"$id":U,"type":"string"}))]);
    let doc = document(
        owner(),
        json!({"healthy":{"input":{"$ref":U}},"conflict":{"input":{"$ref":A}},"mixed":{"input":{"allOf":[{"$ref":"https://identity.example.invalid/missing"},{"$ref":A}]}}}),
    );
    let ctx = context(&doc, &supplied);
    for _ in 0..3 {
        verdicts(&ready(&ctx, "healthy"), &[("2", "satisfies")]);
        identity_refusal(&doc, &supplied, "conflict");
    }
    // Strict preparation first sees a real absent resource; bounds subsequently
    // must propagate the conflict rather than creating an incomplete owner.
    for engine in [
        Arc::new(DefaultEvaluator::new()) as Arc<dyn SchemaEvaluator>,
        Arc::new(Project(true)),
    ] {
        let ctx = doc.value_contracts(engine, supplied.clone()).unwrap();
        let detail = refusal(ctx.prepare("mixed", Side::Input));
        assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
        assert_eq!(detail.code, "resource-identity-conflict");
    }
}
#[test]
fn ri30_precancellation_and_fresh_retry_preserve_identity_policy() {
    let doc = one(owner(), json!({"$ref":U}));
    let supplied = resources(&[(U, json!(false))]);
    let ctx = context(&doc, &supplied);
    let control = WorkControl::new();
    control.cancel();
    assert_eq!(
        refusal(ctx.prepare_with_control("op", Side::Input, &control)).reason,
        NoVerdictReason::Cancelled
    );
    assert!(
        !doc.references_with_resources(supplied, &control)
            .unwrap()
            .complete
    );
    verdicts(&ready(&ctx, "op"), &[("2", "satisfies")]);
}
#[test]
fn ri30_oversized_supplied_closure_keeps_limits_and_healthy_catalog_entries() {
    let deep = format!("{}true{}", r#"{"items":"#.repeat(258), "}".repeat(258));
    let supplied = ResourceSet::new([
        SchemaResource {
            uri: A.into(),
            document: value(&deep),
        },
        SchemaResource {
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
        Arc::new(DefaultEvaluator::new()) as Arc<dyn SchemaEvaluator>,
        Arc::new(Project(false)),
        Arc::new(Project(true)),
    ] {
        let ctx = doc.value_contracts(evaluator, supplied.clone()).unwrap();
        ready(&ctx, "healthy");
        assert_eq!(
            refusal(ctx.prepare("deep", Side::Input)).reason,
            NoVerdictReason::LimitExceeded
        );
        ready(&ctx, "healthy");
    }
}
#[test]
fn identity_refusal_location_is_optional_and_measured_before_copy() {
    for size in [64 * 1024 - 256, 64 * 1024] {
        let key = "x".repeat(size);
        let supplied = resources(&[(V, json!({"properties":{key:{"$id":U}}}))]);
        let doc = one(owner(), json!({"$ref":V}));
        for engine in [
            Arc::new(DefaultEvaluator::new()) as Arc<dyn SchemaEvaluator>,
            Arc::new(Project(false)),
            Arc::new(Project(true)),
        ] {
            let detail = refusal(
                doc.value_contracts(engine, supplied.clone())
                    .unwrap()
                    .prepare("op", Side::Input),
            );
            assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
            assert_eq!(detail.code, "resource-identity-conflict");
            // Source URI bytes share the same allowance as the full pointer.
            assert_eq!(detail.location.is_some(), size + 12 + V.len() <= 64 * 1024);
        }
    }
}
#[test]
fn ri32_selected_unsupported_or_invalid_owner_never_falls_back() {
    for (schema, reason) in [
        (
            json!({"$id":U,"$schema":"https://identity.example.invalid/custom","type":"integer"}),
            NoVerdictReason::UnsupportedCapability,
        ),
        (
            json!({"$id":U,"type":17}),
            NoVerdictReason::ConservativePreparation,
        ),
    ] {
        let doc = one(json!({"E":schema}), json!({"$ref":U}));
        let supplied = resources(&[(U, json!(true))]);
        for engine in [
            Arc::new(DefaultEvaluator::new()) as Arc<dyn SchemaEvaluator>,
            Arc::new(Project(false)),
            Arc::new(Project(true)),
        ] {
            assert_eq!(
                refusal(
                    doc.value_contracts(engine, supplied.clone())
                        .unwrap()
                        .prepare("op", Side::Input)
                )
                .reason,
                reason
            );
        }
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
fn ri33_anonymous_document_has_no_acquisition_or_generated_identity() {
    for uri in [
        "https://identity.example.invalid/acquired-obi#/schemas/E",
        "https://sdk-program.openbindings.invalid/r0#/schemas/E",
        "https://sdk-bounds.openbindings.invalid/0/r0#/schemas/E",
    ] {
        let doc = one(json!({"E":{"type":"integer"}}), json!({"$ref":uri}));
        let supplied = ResourceSet::default();
        let strict = doc
            .value_contracts(Arc::new(Project(false)), supplied.clone())
            .unwrap();
        assert_eq!(
            refusal(strict.prepare("op", Side::Input)).reason,
            NoVerdictReason::ResourceUnavailable
        );
        assert!(matches!(
            reference(&doc, &supplied, "/operations/op/input/$ref"),
            ReferenceResolution::Unresolved {
                detail: NoVerdict {
                    reason: NoVerdictReason::ResourceUnavailable,
                    ..
                }
            }
        ));
    }
}

#[test]
fn ri28_unused_contained_and_supplied_peer_conflicts_are_local() {
    for (schemas, supplied) in [
        (
            json!({"E":{"$id":U,"type":"integer"},"Duplicate":{"$id":U,"type":"string"}}),
            ResourceSet::default(),
        ),
        (
            json!({}),
            resources(&[(A, json!({"$id":U})), (B, json!({"$id":U}))]),
        ),
    ] {
        let doc = document(
            schemas,
            json!({"healthy":{"input":{"type":"boolean"}},"conflict":{"input":{"$ref":U}}}),
        );
        let ctx = context(&doc, &supplied);
        for _ in 0..3 {
            verdicts(
                &ready(&ctx, "healthy"),
                &[("true", "satisfies"), ("2", "fails")],
            );
            let detail = refusal(ctx.prepare("conflict", Side::Input));
            assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
            assert_eq!(detail.code, "ambiguous-resource");
        }
        helpers_ready(&doc, &supplied, "healthy");
    }
}

#[test]
fn ri16_nested_resource_does_not_invent_losing_parent_dynamic_scope() {
    let supplied = resources(&[(
        A,
        json!({"$id":U,"$dynamicAnchor":"node","type":"string","$defs":{"Child":{"$id":W,"$dynamicRef":"#node","$defs":{"Node":{"$dynamicAnchor":"node","type":"integer"}}}}}),
    )]);
    let doc = one(owner(), json!({"$ref":W}));
    let contract = ready(&context(&doc, &supplied), "op");
    verdicts(&contract, &[("2", "satisfies"), (r#""x""#, "fails")]);
    assert_eq!(
        failed_location(&contract, r#""x""#),
        SchemaLocation {
            resource: Some(A.into()),
            pointer: "/$defs/Child/$defs/Node/type".into()
        }
    );
}

#[test]
fn inherited_conservative_policy_case_has_both_value_controls() {
    let uri = "https://ex.invalid/shared";
    let doc = one(
        json!({"S":{"$id":uri,"type":"integer"}}),
        json!({"$ref":uri}),
    );
    let supplied = resources(&[(uri, json!({"type":"string"}))]);
    let contract = ready(&context(&doc, &supplied), "op");
    verdicts(&contract, &[("1", "satisfies"), (r#""x""#, "fails")]);
    assert_eq!(
        failed_location(&contract, r#""x""#),
        SchemaLocation {
            resource: None,
            pointer: "/schemas/S/type".into()
        }
    );
    helpers_ready(&doc, &supplied, "op");
}
