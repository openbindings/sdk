//! Preparation spends a bounded amount of library compilation work for each
//! projected schema definition. Shapes whose compilation grows faster than the
//! schema itself, such as long reference chains and diamonds of resources, refuse
//! with a limit instead of running on; ordinary shapes are unaffected.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn prepare(text: &str) -> Result<PreparedContract, NoVerdict> {
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    proof
        .resolve_operation("op")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
}
fn document(schemas: &[String], input: &str) -> String {
    format!(
        r#"{{"openbindings":"0.2.0","schemas":{{{}}},"operations":{{"op":{{"input":{input}}}}}}}"#,
        schemas.join(",")
    )
}
// d0 -> d1 -> ... -> d(links), each a same-document reference; the last is an integer.
fn chain(links: usize) -> String {
    let mut schemas: Vec<_> = (0..links)
        .map(|i| format!(r##""d{i}":{{"$ref":"#/schemas/d{}"}}"##, i + 1))
        .collect();
    schemas.push(format!(r#""d{links}":{{"type":"integer"}}"#));
    document(&schemas, r##"{"$ref":"#/schemas/d0"}"##)
}
// The same chain through contained resources, one `$id` per link.
fn resource_chain(links: usize) -> String {
    let mut schemas: Vec<_> = (0..links)
        .map(|i| {
            format!(
                r#""d{i}":{{"$id":"https://chain.test/d{i}","$ref":"https://chain.test/d{}"}}"#,
                i + 1
            )
        })
        .collect();
    schemas.push(format!(
        r#""d{links}":{{"$id":"https://chain.test/d{links}","type":"integer"}}"#
    ));
    document(&schemas, r##"{"$ref":"#/schemas/d0"}"##)
}
// Each level is a contained resource whose two branches, each its own resource,
// both reference the next level: every level doubles the resource paths to it.
fn diamonds(levels: usize) -> String {
    let mut schemas = Vec::new();
    for i in 0..levels {
        schemas.push(format!(
            r#""r{i}":{{"$id":"https://diamond.test/r{i}","allOf":[{{"$ref":"https://diamond.test/a{i}"}},{{"$ref":"https://diamond.test/b{i}"}}]}}"#
        ));
        for branch in ["a", "b"] {
            schemas.push(format!(
                r#""{branch}{i}":{{"$id":"https://diamond.test/{branch}{i}","$ref":"https://diamond.test/r{}"}}"#,
                i + 1
            ));
        }
    }
    schemas.push(format!(
        r#""r{levels}":{{"$id":"https://diamond.test/r{levels}","type":"integer"}}"#
    ));
    document(&schemas, r##"{"$ref":"#/schemas/r0"}"##)
}
fn assert_work_limit(refused: NoVerdict) {
    assert_eq!(refused.reason, NoVerdictReason::LimitExceeded);
    assert_eq!(refused.code, "preparation-work-limit");
    assert!(refused.location.is_none());
}
fn decides(contract: &PreparedContract) {
    assert!(matches!(
        contract.validate(&JsonValue::parse("7").unwrap()),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        contract.validate(&JsonValue::parse("\"7\"").unwrap()),
        ValueOutcome::Fails { .. }
    ));
}

#[test]
fn long_reference_chains_refuse_at_preparation() {
    // Compiling a chain costs the library work quadratic in its length.
    assert_work_limit(prepare(&chain(4_000)).unwrap_err());
    assert_work_limit(prepare(&resource_chain(2_000)).unwrap_err());
}

#[test]
fn shorter_chains_prepare_within_the_allowance() {
    decides(&prepare(&chain(500)).unwrap());
    decides(&prepare(&resource_chain(100)).unwrap());
    // Longer chains still prepare, though evaluation cannot follow them under
    // the default evaluation limits.
    let contract = prepare(&chain(1_500)).unwrap();
    let ValueOutcome::NoVerdict { detail } = contract.validate(&JsonValue::parse("7").unwrap())
    else {
        panic!("evaluation follows no 1,500-link chain under default limits")
    };
    assert_eq!(detail.code, "evaluation-work-limit");
}

#[test]
fn resource_diamonds_refuse_before_compilation_grows_exponentially() {
    decides(&prepare(&diamonds(4)).unwrap());
    // Without the allowance, 24 levels would compile the last one 2^24 times.
    assert_work_limit(prepare(&diamonds(24)).unwrap_err());
    // Refusal leaves nothing behind; the next preparation is unaffected.
    decides(&prepare(&diamonds(4)).unwrap());
}

#[test]
fn wide_schemas_prepare_whatever_their_size() {
    // Compilation linear in the schema stays within the allowance at any size.
    let mut schemas = Vec::new();
    let mut properties = Vec::new();
    for i in 0..5_000 {
        schemas.push(format!(r#""t{i}":{{"type":"integer","minimum":{i}}}"#));
        properties.push(format!(r##""p{i}":{{"$ref":"#/schemas/t{i}"}}"##));
    }
    let text = document(
        &schemas,
        &format!(r#"{{"properties":{{{}}}}}"#, properties.join(",")),
    );
    let contract = prepare(&text).unwrap();
    assert!(matches!(
        contract.validate(&JsonValue::parse(r#"{"p0":0,"p4999":4999}"#).unwrap()),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        contract.validate(&JsonValue::parse(r#"{"p4999":0}"#).unwrap()),
        ValueOutcome::Fails { .. }
    ));
}
