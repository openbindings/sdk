//! A fixed schema offering, its comparison context, and its checked report.
use openbindings::{Document, JsonValue};
use openbindings_schema_comparison::{
    ComparisonContext, ComparisonLimits, identical, identical_with_references,
};
use openbindings_source_pull::{
    CancellationToken, PullFailure, PullRequest, PullResult, SourcePuller, SourcePullerExt,
};
use std::{
    convert::Infallible,
    future::{Ready, ready},
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

// The report is fixed so the example can concentrate on its labels. A real
// puller constructs these entries from its observation and comparison results.
struct SchemaReport(PullResult);
impl SourcePuller for SchemaReport {
    type Error = Infallible;
    type Pull<'a> = Ready<Result<PullResult, PullFailure<Self::Error>>>;
    fn handle_pull_source(&self, _: PullRequest, _: CancellationToken) -> Self::Pull<'_> {
        ready(Ok(self.0.clone()))
    }
}

#[tokio::test]
async fn schemas_and_reference_conflict_pass_the_checked_pull() {
    let original = Document::parse(
        r##"{
        "openbindings":"0.2.0",
        "operations":{"getUser":{"output":{"$ref":"#/schemas/User"}}},
        "schemas":{
            "Label":{"type":"string","description":"Author's annotation"},
            "User":{"type":"integer"}
        },
        "sources":{"s":{"kind":"example.schema-report@1"}}
    }"##,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();

    // Keep the offering's own OBI schema positions, without validating a
    // temporary Document or borrowing the destination's schema definitions.
    let offered_root = json(
        r##"{
        "openbindings":"0.2.0",
        "operations":{"getUser":{"output":{"$ref":"#/schemas/User"}}},
        "schemas":{"Label":{"type":"string"},"User":{"type":"string"}}
    }"##,
    );
    let offered = ComparisonContext::new(offered_root);
    let before = ComparisonContext::new(original.document().value().clone());
    let limits = ComparisonLimits::default();
    for (path, equal) in [
        ("/schemas/Label", true),
        ("/schemas/User", false),
        ("/operations/getUser/output", false),
    ] {
        let old_schema = before.document().at(path).unwrap().to_owned();
        let offered_schema = offered.document().at(path).unwrap().to_owned();
        if equal {
            // Exact JSON equality would incorrectly call Label stale.
            assert_eq!(old_schema.semantic_eq(&offered_schema), Some(false));
        }
        let old = before.schema(old_schema);
        let new = offered.schema(offered_schema);
        assert_eq!(identical(Some(&old), Some(&new), &limits), Ok(equal));
    }
    // An existing entry accounts for User's drift, so a same-spelled reference
    // to it stays a leaf when judging existing entries (not operation reuse).
    let path = "/operations/getUser/output";
    let old = before.schema(before.document().at(path).unwrap().to_owned());
    let new = offered.schema(offered.document().at(path).unwrap().to_owned());
    assert_eq!(
        identical_with_references(
            Some(&old),
            Some(&new),
            &|reference| reference.spelling() == "#/schemas/User",
            &limits
        ),
        Ok(true)
    );

    let report = PullResult::from_json(json(
        r##"{
        "additions":{},
        "report":{"exhaustive":true,"added":[],
            "existing":[
                {"schema":"Label","status":"current"},
                {"schema":"User","status":"stale","change":"breaking",
                 "proposal":{"schema":{"type":"string"}}},
                {"source":"s","status":"current"}
            ],
            "omitted":[{
                "status":"conflict","reasonCode":"schema-drift",
                "message":"The offered operation differs through a schema.",
                "item":"operations/getUser","existing":"operations/getUser",
                "offered":{"output":{"$ref":"#/schemas/User"}},
                "through":["schemas/User"]
            }]
        }
    }"##,
    ))
    .unwrap();
    let (checked, prospective) = SchemaReport(report)
        .pull_source(
            PullRequest::new(original.clone(), "s"),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(checked.report().is_complete());
    // Checking does not apply the stale proposal.
    assert_eq!(
        prospective
            .value()
            .at("/schemas/User/type")
            .unwrap()
            .as_str(),
        Some("integer")
    );
}
