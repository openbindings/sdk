# First useful result in Rust

Validate an interface document's bytes, read an operation and prepare its input
schema. Select `openbindings` and `openbindings-json-schema-evaluator` from
an exact checkout or local archives through [source replay](REPLAY.md). Packages
are unpublished; Rust needs no runtime initialization.

Run the packaged examples:

```sh
cargo run -p openbindings-json-schema-evaluator --example first_use
cargo run -p openbindings-json-schema-evaluator --example replacement
```

The first example adds exact values and verdict handling. The replacement example
keeps old work usable while a new document and resource set are installed.
This caller diagnoses an unexpected document field, corrects it and prepares input:

```rust
use openbindings::{validate_document, ExternalSchemas, PreparationLimits};
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = r#"{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"],"inputSchema":{"type":"integer"}}}}"#;
    let validation = validate_document(text)?;
    for finding in &validation.report().findings {
        // Debug quotes and escapes the pointer, including any control characters.
        let pointer = finding.location.as_ref().and_then(|at| at.pointer.as_ref());
        println!("{}/{} ({:?}) at {:?}: {}",
            finding.rule, finding.code, finding.status,
            pointer.map(ToString::to_string), finding.message);
    }
    if validation.report().findings_truncated {
        println!("More findings were omitted; rule evidence remains available.");
    }
    // Correct this known fixture's original source, not a normalized JSON value.
    let corrected = text.replace("\"inputSchema\":", "\"input\":");
    let validation = validate_document(corrected)?;
    let Some(proof) = validation.conformant() else {
        return Err(format!("document conformance: {:?}", validation.report().conclusion).into());
    };
    for operation in proof.operations() {
        let aliases: Vec<&str> = operation.aliases().into_iter().flatten().collect();
        println!("operation {} (aliases {aliases:?})", operation.key());
    }
    let operation = proof.resolve_operation("lookup").ok_or("operation is missing")?;
    let input = operation
        .input()
        .ok_or("operation has no input schema")?
        .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default(), &PreparationLimits::default())
        .map_err(|failure| failure.to_string())?;
    println!("value outcome: {:?}", input.evaluate_json("7")?);
    // The cheap check says whether a value fails; explaining it says why.
    let seven = openbindings::JsonValue::parse(r#""seven""#)?;
    if let openbindings::ValueOutcome::Fails { problems, .. } = input.explain(&seven) {
        for problem in &problems {
            // The value and the document each find their side's line and
            // column when asked; Debug of the pointer text quotes it.
            let at = seven.locate(&problem.instance).ok_or("instance location")?;
            let schema = proof.locate(&problem.schema).ok_or("schema location")?;
            println!("{} at {:?} line {} (schema {:?} line {})", problem.code,
                problem.instance.to_string(), at.line,
                problem.schema.pointer.to_string(), schema.line);
        }
    }
    Ok(())
}
```

`validate_document` returns a document report or an unsupported-version refusal.
It judges the bytes as they are: invalid UTF-8, a byte order mark or a JSON syntax
error is an OBI-01 violation in the report, located like any other finding.
A report can be conformant, non-conformant or undetermined. Only established
conformance yields a `ConformantDocument`. Its typed field reads are infallible;
raw editor reads retain the exact draft. `operations()` returns canonical `Operation`
owners in lexical key order. `key()` is the primary name and `aliases()` is optional.
`operation(key)` selects only a primary key; `resolve_operation(name)` also accepts
aliases. Both return `None` when absent. Binding/source/dependency relations retain
the same conformant snapshot, and operation bindings are unranked.

## Evaluate values

An operation's `input()` and `output()` return optional contextual `Schema` owners.
Named schemas are available through `proof.schema(key)`. `Schema::json()` retains
the exact declaration. Reading a schema performs no preparation or evaluation.

Call `schema.prepare(&evaluator, &external_schemas, &preparation_limits)` to obtain
`Result<PreparedSchema<E::Validator>, PreparationFailure>`. Resources are explicit
and immutable; the SDK never fetches them. A preparation failure judges no value,
and absence is a separate `None` before preparation. Core conformance requires no
caller-supplied evaluator. See [evaluator integration](schema-evaluators.md).

`evaluate_json(text_or_bytes)` admits exact JSON under default limits: 32 MiB,
10,000 container levels and 1,000,000 nodes. Admission failure is `Err`; `Ok`
contains `Satisfies`, `Fails`, `Undefined` or `NoVerdict`. Duplicate instance members
are admitted exactly and yield `NoVerdict` during evaluation. Number tokens retain
their exact meaning. `explain(&value)` requests diagnostics without changing the
semantic result.

Use `JsonValue::parse_with_limits` and `prepared.evaluate(&value)` for custom
admission limits or repeated checks. `JsonValue::from_serializable` checks Rust
data, including nested `JsonValue` fields, but cannot recover precision already lost by application code. It preserves
embedded number tokens and string units, and refuses duplicate object members.

Exact values can hold an unpaired UTF-16 surrogate or a number outside
`serde_json::Value`'s range. Passing one to `serde_json::json!` can panic;
conversion through a host value can also lose number precision. Compose values
with `JsonValue::from_serializable` instead:

```rust,edition2024
use openbindings::JsonValue;
use serde::Serialize;

#[derive(Serialize)]
struct Envelope { payload: JsonValue }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload = JsonValue::parse(r#"[1e9999, "\ud800"]"#)?;
    let value = JsonValue::from_serializable(&Envelope { payload })?;
    assert_eq!(value.text(), r#"{"payload":[1e9999, "\ud800"]}"#);
    // Serialize to JSON text when sending exact values over the wire.
    let bytes = serde_json::to_vec(&value)?;
    assert_eq!(JsonValue::parse(bytes)?.text(), value.text());
    Ok(())
}
```

Conversion keeps its 128-container ceiling and the selected byte/node limits,
including inside exact fields. Parse or clone an already complete exact value
when composition is unnecessary. Preparation errors have readable `Display`
text; Rust's `Result`-returning `main` prints `Debug`, which deliberately hides
these details. The first example converts the preparation failure to display
text before returning it.

Each preparation is independent. The default validator shares compiled state when
cloned; custom validators choose their own ownership and thread-safety properties.
Prepared does not mean every value is decidable. Handle all four results.

## Edit an opaque value

Parse a `Document` to edit it. `Document::parse` admits exact JSON, keeps an
invalid draft inspectable and refuses text that is not JSON as an `InputError`;
`document.validate()` returns the same validation as `validate_document`. Edit
the `Document` through a shared borrow and retain the returned snapshot. Generic paths use RFC 6901:
`~0` escapes a tilde and `~1` a slash. Parents must exist; `set` inserts missing
object members and `/-` appends to arrays. Use exact JSON values to preserve
number spellings and escaped UTF-16. A pointer is escaped text or an exact
`Pointer`, so a finding's location addresses the member it reports, including a
name holding an unpaired UTF-16 unit: `document.remove(&pointer)`, or
`Edit::Remove { pointer: (&pointer).into() }` in a batch.

```rust,edition2024
use openbindings::{Document, Edit, JsonValue};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{},"x-editor":{"note":null,"id":9007199254740993}}"#,
    )?;
    let note = JsonValue::string("Reviewed")?;
    let description = JsonValue::string("Ready for review")?;
    let changed = document.edit(&[
        Edit::set(&["x-editor", "note"], note),
        Edit::set(&["description"], description),
    ])?;
    assert_eq!(document.value().at("/x-editor/id").unwrap().text(), "9007199254740993");
    let proven = changed.validate()?.conformant().ok_or("nonconformant")?;
    assert!(proven.operations().is_empty());
    let document = proven.document().set("/description", &JsonValue::string("Editing again")?)?;
    assert_eq!(document.value().get("description").unwrap().as_str(), Some("Editing again"));
    Ok(())
}
```

Each batch splices the current bytes once and parses the result once. A failed
batch changes nothing. Untouched extension members, number tokens, member order
and whitespace remain exact; inserted members use compact punctuation. Sibling
insertions at one boundary coalesce in request order. Adjacent removals, up to
every member of a container, share their separators. Overlapping replacements
and removals are refused. Default byte, node and depth limits apply to the result;
byte preflight happens before replacement allocation.

Construct operations, bindings, sources, dependencies and examples through exact
JSON path edits. Set complete maps when namespaces are absent; edits do not create
missing parents. Validate the resulting snapshot separately. Canonical domain
entries come from that proof, not from detached authoring structures. Use
`openbindings::is_valid_name(name)` to check a proposed entry key or alias before
editing; validating the document still checks namespace collisions and relationships.

`ConformantDocument` has no setters or mutable borrow. `document()` borrows
plain reads, and `into_inner()` consumes proof to return the immutable document snapshot.
Retained views and proofs keep their earlier bytes. Findings and source positions
always describe the revision being read. The packaged
[exact-edit example](../crates/openbindings/examples/exact_edit.rs) includes
additional numeric and error cases.

## Resources, diagnostics and lifetime

External schemas are explicit immutable inputs. Preparing against a replacement
set produces independent work; existing prepared schemas keep their original
meaning. A prepared schema owns whatever its validator needs and outlives the
caller handles. Core imposes no universal retention or thread-safety strategy.

`prepare_with_control`, `evaluate_with_control` and `explain_with_control` accept
`WorkControl`, including an `AtomicBool` or a companion's cancellation token.
Cancellation is cooperative. A cancelled preparation returns no validator; an
interrupted evaluation leaves an existing validator available for another call.

Document findings refer to the assessed revision using exact pointers, zero-based
UTF-8 byte offsets and one-based lines and byte columns. Quote untrusted pointers
for display. Finding truncation leaves rule evidence intact. Use codes and evidence
for program logic and messages for explanation.

Value problems name the failing instance and original schema coordinate.
`JsonValue::locate`, `ConformantDocument::locate` and `ExternalSchemas::locate` compute
line and byte positions on demand. The default evaluator's `explain` reports
established failing assertions at these locations, with at most 100 problems. `problems_complete` is true only when
all reportable failures were collected; a limit, missing witness or interrupted
diagnostic work leaves it false without changing the failure verdict. `allOf`
collects failures from its failing branches. Failed `anyOf`, `oneOf` and `not`
report the applicator itself, without treating their branch failures as independent
problems. `details` includes missing required member names, expected types,
exact numeric and size bounds, and complete enum choices from the original
schema. Bounds and choices are original JSON token strings. Missing members
with tied locations keep the schema's `required` array order. Keywords without
a matching variant, such as `const`, `pattern`, `multipleOf` and
`additionalProperties`, have no details; pattern text stays out. Facts that
cannot fit the diagnostic allowance or be represented exactly are omitted and
leave `problems_complete` false. Enum lists are never partial.

Messages remain fixed SDK text, and `Debug` redacts schema facts. Rejected
instance values are never copied into facts or messages. Serializing whole
problems into a public response sends schema facts. An app with a private
contract should forward only `instance` and `code`.

Serialized problem and schema-origin pointers replace unpaired UTF-16 units with
U+FFFD so they can be ordinary JSON strings. Keep the original `Pointer` for exact
lookup or editing; the serialized spelling cannot identify such names exactly.
See the [API reference](api-reference.md),
[evaluator guide](../crates/openbindings-json-schema-evaluator/README.md) and
[host support](support.md).
