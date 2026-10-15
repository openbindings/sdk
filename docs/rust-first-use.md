# First useful result in Rust

Parse an interface document, validate it, read an operation and prepare its input
value contract. Select `openbindings` and `openbindings-json-schema-evaluator` from
an exact checkout or local archives through [source replay](REPLAY.md). Packages
are unpublished; Rust needs no runtime initialization.

Run the packaged examples:

```sh
cargo run -p openbindings-json-schema-evaluator --example first_use
cargo run -p openbindings-json-schema-evaluator --example replacement
```

The first example adds exact values and verdict handling. The replacement example
keeps old work usable while a new document and resource context are installed.
This caller diagnoses an unexpected document field, corrects it and prepares input:

```rust
use openbindings::{ValueContractPreparation, ParsedDocument, ResourceSet, Direction};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = r#"{"openbindings":"0.2.0","operations":{"lookup":{"inputSchema":{"type":"integer"}}}}"#;
    let document = ParsedDocument::parse(text)?;
    let validation = document.validate()?;
    for finding in &validation.report().findings {
        // Debug quotes and escapes the pointer, including any control characters.
        let pointer = finding.location.as_ref().and_then(|at| at.pointer.as_deref());
        println!("{}/{} ({:?}) at {:?}: {}",
            finding.rule, finding.code, finding.status, pointer, finding.message);
    }
    if validation.report().findings_truncated {
        println!("More findings were omitted; rule evidence remains available.");
    }
    // Correct this known fixture's original source, not a normalized JSON value.
    let corrected = text.replace("\"inputSchema\":", "\"input\":");
    let corrected = ParsedDocument::parse(corrected)?;
    let validation = corrected.validate()?;
    let Some(proof) = validation.validated() else {
        return Err(format!("document conformance: {:?}", validation.report().conclusion).into());
    };
    let context = proof.value_contracts(
        Arc::new(DefaultEvaluator::new()), ResourceSet::default());
    match context.prepare("lookup", Direction::Input) {
        ValueContractPreparation::Ready(_input) => { /* retain input for value validation */ }
        other => return Err(format!("input setup was refused: {other:?}").into()),
    }
    Ok(())
}
```

`validate()` returns a document report or an unsupported-version refusal.
A report can be conformant, non-conformant or undetermined. Only established
conformance yields a `ValidatedDocument`. Its typed field reads are infallible;
parsed editor reads remain fallible. Resolve primary names or aliases with
`resolve_operation`. A validated lookup returns an optional operation; a parsed
lookup distinguishes found, missing and ambiguous.

## Validate values

Create a context with `validated.value_contracts(evaluator, resources)`, then
`prepare(name, Direction::Input)` or `Direction::Output`. Match ready,
no-value-contract, operation-missing or no-verdict before validating a value.
The retained ambiguity branch is unreachable from conformant documents.
Document validation itself needs no caller-supplied evaluator; operation value
validation always does. See [evaluator integration](schema-evaluators.md).

Use `JsonValue::from_serializable` for ordinary Rust data and `JsonValue::parse`
for exact JSON text. The ordinary converter preserves integer digits but cannot
recover a value already rounded by application code. It refuses exact wrappers;
keep those on the parse/retain route.

A ready contract's `validate` returns `Satisfies`, `Fails` or `NoVerdict`.
Preparation and validation refusals remain separate from an established failure.
Retain and clone a ready contract to reuse compiled state. Every `prepare` call
compiles independently. Ready does not imply that every value is decidable.

## Edit an opaque value

Use `to_authoring()` to obtain a typed draft, change its fields, then build and
validate a new snapshot. To change a member inside a schema, content or extension
object, compose the changed object from retained exact children:

```rust,edition2024
use openbindings::{JsonValue, ParsedDocument};
use std::collections::BTreeMap;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn replace_exact_object_member(
    object: &JsonValue,
    key: &str,
    replacement: &JsonValue,
) -> Result<JsonValue> {
    if object.has_duplicate_names() {
        return Err("this recipe requires unique member names".into());
    }
    let members = object.view().members().ok_or("expected an exact object")?;
    let mut composed = BTreeMap::new();
    for member in members {
        let name = member
            .name
            .as_str()
            .ok_or("member name is not UTF-8 representable")?;
        composed.insert(name, member.value.to_owned());
    }
    let target = composed
        .get_mut(key)
        .ok_or("replacement member is absent")?;
    *target = replacement.clone();
    // JsonValue's Serde representation carries raw JSON tokens. Do not convert
    // these exact values through ordinary serde_json::Value or floating point.
    Ok(JsonValue::parse(serde_json::to_vec(&composed)?)?)
}

fn main() -> Result<()> {
    let parsed = ParsedDocument::parse(
        r#"{"openbindings":"0.2.0","operations":{},"x-editor":{"note":null,"id":9007199254740993}}"#,
    )?;
    let mut draft = parsed.to_authoring()?;
    let original = draft.additional_fields.get("x-editor").ok_or("missing extension")?;
    let replacement = JsonValue::string("Reviewed")?;
    let edited = replace_exact_object_member(original, "note", &replacement)?;
    draft.additional_fields.insert("x-editor".into(), edited);
    let revised = draft.build()?;
    assert!(revised.validate()?.validated().is_some());
    assert_eq!(revised.value().at("/x-editor/id").unwrap().text(), "9007199254740993");
    Ok(())
}
```

This is application code using existing APIs. It replaces one existing member,
requires unique names throughout the subtree and UTF-8-representable immediate
names, and refuses a missing member. Exact siblings retain numeric tokens.
Formatting, name escaping and order may change. The whole changed object is
serialized and parsed; `build()` also serializes the document. Typed fields are
re-encoded, and operation renaming leaves reference updates to the caller.
The packaged [exact-edit example](../crates/openbindings/examples/exact_edit.rs)
includes additional numeric and error cases.

## Resources, diagnostics and lifetime

Schema resources are explicit and immutable. The SDK never fetches missing
references. Construct a new context to change resources; retained contracts keep
their original snapshots. An application can prepare a candidate before replacing
its active contract and keep serving old work if preparation fails.

`prepare_with_control` and `validate_with_control` accept `WorkControl`, including
an `AtomicBool` or a companion's `CancellationToken`. Cancellation is cooperative.
Use a fresh flag or token for a retry. Cancelled preparation returns no ready
owner; cancelled validation leaves an existing owner usable.

Document findings have original JSON Pointers, zero-based UTF-8 byte offsets and
one-based lines and byte columns. Quote pointers when displaying them. Truncation
leaves rule evidence intact. Use codes and evidence for logic; messages are
advisory. Value failures use coarse diagnostics by default. Upstream diagnostics
are an explicit choice with library allocation and stack limits, including possible
stack overflow on deep values. See the [evaluator guide](../crates/openbindings-json-schema-evaluator/README.md)
and [host support](support.md).
