//! A schema reached through an external URI follows the dialect JSON Schema
//! assigns it (specification §5.2; JSON Schema Core §8.1.1): a supplied
//! resource's own `$schema` names its dialect, an embedded resource without one
//! takes its enclosing resource's, and a resource that declares none is read as
//! 2020-12. The default evaluator reads only 2020-12, so a value whose
//! evaluation needs a schema of another dialect gets no verdict rather than a
//! 2020-12 reading of that schema.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

const RESOURCE: &str = "https://dialects.test/r";

fn prepare(contract: &str, resource: &str) -> Result<PreparedValueContract, NoVerdict> {
    let text =
        format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{contract}}}}}}}"#);
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant");
    let resources = ResourceSet::new([SchemaResource {
        uri: RESOURCE.into(),
        document: JsonValue::parse(resource).unwrap(),
    }])
    .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    operation
        .input()
        .unwrap()
        .prepare_with(&DefaultEvaluator::new(), &resources)
}

/// `S`atisfies, `F`ails, or `D` for a refusal naming the unread dialect.
fn outcome(contract: &str, resource: &str, value: &str) -> char {
    let dialect = |detail: &NoVerdict| {
        assert_eq!(
            (detail.reason, detail.code.as_str()),
            (NoVerdictReason::UnsupportedCapability, "schema-dialect"),
            "{resource}"
        );
        'D'
    };
    let prepared = match prepare(contract, resource) {
        Ok(prepared) => prepared,
        Err(detail) => return dialect(&detail),
    };
    match prepared.validate(&JsonValue::parse(value).unwrap()) {
        ValueOutcome::Satisfies => 'S',
        ValueOutcome::Fails { .. } => 'F',
        ValueOutcome::NoVerdict { detail } => dialect(&detail),
        other => panic!("{other:?}"),
    }
}

#[test]
fn supplied_resources_are_read_in_the_dialect_they_declare() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}"}}"#);
    // `prefixItems` is a 2020-12 keyword: read as 2020-12, `[1]` fails it; read
    // as 2019-09, the keyword is unknown and `[1]` would satisfy.
    let prefix = r#""prefixItems":[{"type":"string"}]"#;
    for (resource, expected) in [
        (format!("{{{prefix}}}"), ['S', 'F']),
        (
            format!(r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema",{prefix}}}"#),
            ['S', 'F'],
        ),
        (
            format!(r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema#",{prefix}}}"#),
            ['S', 'F'],
        ),
        (
            format!(r#"{{"$schema":"https://json-schema.org/draft/2019-09/schema",{prefix}}}"#),
            ['D', 'D'],
        ),
        // Draft-07's array form of `items`, which 2020-12 has no reading for.
        (
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","items":[{"type":"string"}]}"#
                .to_owned(),
            ['D', 'D'],
        ),
    ] {
        let got = [r#"["a"]"#, "[1]"].map(|value| outcome(&contract, &resource, value));
        assert_eq!(got, expected, "{resource}");
    }
}

#[test]
fn embedded_resources_take_their_enclosing_resources_dialect_unless_they_declare_one() {
    // A 2020-12 resource embedding a 2019-09 one, and a 2019-09 resource
    // embedding one that declares nothing: the embedded schema is unread in both.
    let contract = format!(r#"{{"$ref":"{RESOURCE}#/$defs/inner"}}"#);
    let inner = r#""$id":"https://dialects.test/inner","prefixItems":[{"type":"string"}]"#;
    let nested_2019 = format!(
        r#"{{"$defs":{{"inner":{{"$schema":"https://json-schema.org/draft/2019-09/schema",{inner}}}}}}}"#
    );
    let enclosing_2019 = format!(
        r#"{{"$schema":"https://json-schema.org/draft/2019-09/schema","$defs":{{"inner":{{{inner}}}}}}}"#
    );
    let enclosing_2020 = format!(r#"{{"$defs":{{"inner":{{{inner}}}}}}}"#);
    for (resource, expected) in [
        (nested_2019, ['D', 'D']),
        (enclosing_2019, ['D', 'D']),
        (enclosing_2020, ['S', 'F']),
    ] {
        let got = [r#"["a"]"#, "[1]"].map(|value| outcome(&contract, &resource, value));
        assert_eq!(got, expected, "{resource}");
    }
}
