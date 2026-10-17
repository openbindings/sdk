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
keeps old work usable while a new document and resource set are installed.
This caller diagnoses an unexpected document field, corrects it and prepares input:

```rust
use openbindings::Document;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = r#"{"openbindings":"0.2.0","operations":{"lookup":{"inputSchema":{"type":"integer"}}}}"#;
    let document = Document::parse(text)?;
    let validation = document.validate()?;
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
    let corrected = Document::parse(corrected)?;
    let validation = corrected.validate()?;
    let Some(proof) = validation.conformant() else {
        return Err(format!("document conformance: {:?}", validation.report().conclusion).into());
    };
    let operation = proof.resolve_operation("lookup").ok_or("operation is missing")?;
    let input = operation
        .input()
        .ok_or("operation has no input contract")?
        .prepare(&DefaultEvaluator::new())?;
    println!("value outcome: {:?}", input.validate_json("7")?);
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

`validate()` returns a document report or an unsupported-version refusal.
A report can be conformant, non-conformant or undetermined. Only established
conformance yields a `Conformant<Document>`. Its typed field reads are infallible;
document editor reads remain fallible. Resolve primary names or aliases with
`resolve_operation`. A conformant lookup returns an optional operation; a document
lookup distinguishes found, missing and ambiguous.

## Validate values

A conformant operation's `input()` and `output()` return its value contracts, or
`None` when the operation declares none. `schema()` borrows the exact schema.
`prepare(&DefaultEvaluator::new())` uses the default evaluator budgets and no
supplied resources; reading a contract prepares nothing. The SDK never acquires
resources. Use `prepare_with(&evaluator, &resources)` for a shared or configured
evaluator, custom budgets, supplied resources or a custom evaluator. The evaluator
is borrowed for the call. Each check chooses its detail: `validate` is cheap and
reports only whether a value fails; `explain` on the same contract reports why.

Preparation returns `Result<PreparedContract, NoVerdict>`, so `?` propagates a
refusal with its reason, code, message and location. A missing operation and an
absent contract are `None` before preparation. Only a `Conformant<Document>` hands
out contracts; a plain `Document` operation exposes the exact schema value.
Document validation itself needs no caller-supplied evaluator. See
[evaluator integration](schema-evaluators.md).

A prepared contract's `validate_json(text_or_bytes)` admits exact JSON under the
default limits: 64 MiB, 10,000 container levels and 1,000,000 nodes. `Err` means
admission failed. `Ok` contains `Satisfies`, `Fails`, `Undefined` or `NoVerdict`.
Duplicate members are preserved during admission and yield `NoVerdict` during
validation. Number tokens retain their exact meaning.

Use `JsonValue::parse_with_limits` for custom admission limits and
`PreparedContract::validate` for retained exact values without reparsing. Use
`JsonValue::from_serializable` for ordinary Rust data. The converter preserves
integer digits but cannot recover a value already rounded by application code.
It refuses exact wrappers; keep those on the parse/retain route.

Preparation and validation refusals remain separate from an established failure.
Retain and clone a prepared contract to reuse compiled state. Every `prepare` call
compiles independently. Prepared does not imply that every value is decidable.

## Edit an opaque value

Edit the same `Document` through a mutable borrow. Generic paths use RFC 6901:
`~0` escapes a tilde and `~1` a slash. Parents must exist; `set` inserts missing
object members and `/-` appends to arrays. Use exact JSON values to preserve
number spellings and escaped UTF-16.

```rust,edition2024
use openbindings::{Document, Edit, JsonValue};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{},"x-editor":{"note":null,"id":9007199254740993}}"#,
    )?;
    let note = JsonValue::string("Reviewed")?;
    let description = JsonValue::string("Ready for review")?;
    document.edit(&[
        Edit::Set { pointer: "/x-editor/note", value: &note },
        Edit::Set { pointer: "/description", value: &description },
    ])?;
    assert_eq!(document.value().at("/x-editor/id").unwrap().text(), "9007199254740993");
    let proven = document.validate()?.conformant().ok_or("nonconformant")?;
    assert!(proven.operations().is_empty());
    let mut document = proven.into_inner();
    document.set_description("Editing again")?;
    Ok(())
}
```

Each batch splices the current bytes once and parses the result once. A failed
batch changes nothing. Untouched extension members, number tokens, member order
and whitespace remain exact; inserted members use compact punctuation. Sibling
insertions at one boundary coalesce in request order. Overlapping replacements
and removals are refused. Default byte, node and depth limits apply to the result;
byte preflight happens before replacement allocation.

Use typed parts for construction: `set_operation`, `set_binding`, `set_source`,
`set_dependency` and `set_example`. `set_operations(&operations)` commits a complete
operations map once for synthesis; the bindings, sources and dependencies maps
have corresponding setters. These setters check typed collisions and admission,
while `validate()` separately establishes document conformance.

`Conformant<Document>` has no setters or mutable borrow. `as_document()` borrows
plain reads, and `into_inner()` consumes proof to return an editable owner.
Retained views and proofs keep their earlier bytes. Findings and source positions
always describe the revision being read. The packaged
[exact-edit example](../crates/openbindings/examples/exact_edit.rs) includes
additional numeric and error cases.

## Resources, diagnostics and lifetime

`prepare_with` borrows a `&dyn SchemaEvaluator` and a `ResourceSet`;
the [replacement example](../crates/openbindings-json-schema-evaluator/examples/replacement.rs)
shows supplied resources and retained work. Schema resources are explicit and immutable. The SDK never fetches missing
references. Prepare again with a new set to change resources; prepared contracts keep
their original snapshots. An application can prepare a candidate before replacing
its active contract and keep serving old work if preparation fails.

A `ResourceSet` indexes its resource identities once when it is built, and clones
share that index. Evaluators that request the core projection reuse one resolution
space per live document revision and set, including when preparations alternate
between sets. The space without supplied resources stays with the revision; a
space with a set stays with that set and is released when the set or the revision
is dropped. A prepared contract retains at most its own document and resources,
and only when its evaluator keeps them; the default evaluator's keeps neither.

`prepare_with_control`, `validate_with_control` and `explain_with_control` accept
`WorkControl`, including
an `AtomicBool` or a companion's `CancellationToken`. Cancellation is cooperative.
Use a fresh flag or token for a retry. Cancelled preparation returns no prepared
owner; cancelled validation leaves an existing owner usable.

Document findings describe the assessed revision with exact JSON Pointers, zero-based UTF-8 byte offsets and
one-based lines and byte columns; `SourceLocation`'s `Display` writes them as
`pointer (line L, column C, byte B)`. A pointer keeps a member name's unpaired
UTF-16 unit (`Pointer::as_str` is then `None`) and is absent only for a syntax
error, which precedes any value. Quote pointers when displaying them. Truncation
leaves rule evidence intact. Use codes and evidence for logic; messages are
advisory. `validate` reports value failures without problems. `explain` is an
explicit per-call choice with library allocation and stack limits, including
possible stack overflow on deep values. An explained problem names the failing
node by its exact pointer from the value passed (`instance`) and the failing
keyword by resource and exact pointer (`schema`). Line and column come from the
text's holder when asked: `JsonValue::locate` for the value, `Conformant::locate`
and `ResourceSet::locate` for the schema. A prepared contract keeps a compact copy
of the keywords it compiled for explanations, never its document or resources, and
no positions. See the [evaluator guide](../crates/openbindings-json-schema-evaluator/README.md)
and [host support](support.md).
