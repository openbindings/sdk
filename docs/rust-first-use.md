# First useful result in Rust

Validate an interface document's bytes, read an operation and prepare its input
schema. Rust needs no runtime initialization. For a complete call, continue to
[document to typed result](document-to-typed-result.md), which defines its own
small adapter and handles invocation and output decoding.

In a Rust Worker service, validate documents and prepare schemas inside request
handling, following the [Worker service rule in the support table](support.md).

Packages are unpublished. In an application beside an exact SDK checkout named
`sdk`, use these dependencies (adjust the paths for extracted crate archives):

```toml
[dependencies]
openbindings = { path = "../sdk/crates/openbindings" }
openbindings-json-schema-evaluator = { path = "../sdk/crates/openbindings-json-schema-evaluator" }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.151"
```

For packaged unpublished siblings, the archive provider must also supply local
Cargo patches for the internal dependency closure; see the
[candidate procedure](release-candidate.md). [Source replay](REPLAY.md) is for
maintainers building and qualifying those artifacts.

Run the packaged examples:

```sh
cargo run -p openbindings-json-schema-evaluator --example first_use
cargo run -p openbindings-json-schema-evaluator --example replacement
```

The first example adds exact values and verdict handling. The replacement example
keeps old work usable while a new document and resource set are installed.
This caller diagnoses an unexpected document field, corrects it and prepares input.
It uses `serde_json` to render structured finding facts:

```rust
use openbindings::{validate_document, ExternalSchemas, JsonValue};
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
        if let Some(keyword) = &finding.keyword {
            println!("Schema fact: {}", serde_json::to_string(&(
                keyword, &finding.schema, &finding.details))?);
        }
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
        .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default())
        .map_err(|failure| failure.to_string())?;
    let value = JsonValue::parse("7")?;
    println!("value outcome: {:?}", input.evaluate(&value));
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
Admission limits leave every rule inconclusive with `byte-limit`, `depth-limit` or
`node-limit`, at the parser's byte offset and without a pointer. Messages describe
the reached limit. To admit more, use `validate_document_with_limits` and raise
`JsonLimits::max_bytes`, `max_depth` or `max_nodes`, respectively.
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

Call `schema.prepare(&evaluator, &external_schemas)` to obtain
`Result<PreparedSchema<E::Validator>, PreparationFailure>`. Resources are explicit
and immutable; the SDK never fetches them. A preparation failure judges no value,
and absence is a separate `None` before preparation. Core conformance requires no
caller-supplied evaluator. `prepare` uses default `PreparationLimits` and no
cancellation. Use `prepare_with_control(&evaluator, &external_schemas, &limits, control)`
for explicit preparation limits or cancellation. `max_sdk_work_units` is
`None` by default: a time bound is the caller's choice through cancellation or
`Some(work_units)`. The 512 MiB storage allowance bounds conservative reservations
for memory held at once; completed scratch is released, retained programs stay
charged. See the host-specific [250 ms suggestion](performance-boundaries.md#limits-principle-november-2026)
and [evaluator integration](schema-evaluators.md).

First admit exact JSON with `let value = JsonValue::parse(text_or_bytes)?` under
default limits: 32 MiB, 10,000 container levels and 1,000,000 nodes. Admission
failure is `Err`. Then `prepared.evaluate(&value)` returns `Satisfies`, `Fails`,
`Undefined` or `NoVerdict`. Duplicate instance members
are admitted exactly and yield `NoVerdict` during evaluation. Number tokens retain
their exact meaning. `explain(&value)` requests diagnostics without changing the
semantic result.

Use `JsonValue::parse_with_limits` and `prepared.evaluate(&value)` for custom
admission limits or repeated checks. `JsonValue::from_serializable` checks Rust
data, including nested `JsonValue` fields, but cannot recover precision already lost by application code. It preserves
embedded number tokens and string units, and refuses duplicate object members.

For a buffer that may contain several values, use
`JsonValue::parse_prefix(&buffer)`. It returns the first value and its exclusive
token-end byte offset, leaving trailing whitespace and subsequent bytes to the
host. After `JsonPrefixError::Incomplete`, retain the buffer and append at least
as many bytes as it already holds before retrying, unless EOF or a limit is
reached first. This geometric rule keeps cumulative parsing work linear. At
known EOF pass an incomplete remainder to `JsonValue::parse_with_limits`, which
accepts a final number or returns the usual refusal. Use
`parse_prefix_with_limits` for explicit `JsonLimits`. The
[sequence example](../crates/openbindings/examples/json_sequence.rs) owns bounded
reads, geometric retries and whitespace separators. Each prefix call inspects
only its value and constant lookahead, even with a large buffered remainder.

Use `JsonValue::semantic_eq(&other)` for value equality without floating-point
conversion: `1e400` and `10e399` compare as `Some(true)` despite different tokens.
The result is `Some(false)` for unequal values and `None` when either value
contains duplicate object member names, so equality is ambiguous.

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
when composition is unnecessary. Preparation errors identify their category in
`Display`; their `kind` and `origin` fields retain details for deliberate access.
Rust's `Result`-returning `main` prints structural `Debug`. The first example
converts the preparation failure to fixed display text before returning it.

## Reading errors

An SDK error's or diagnostic's `Display` and `Debug` hold fixed SDK text plus
SDK constants: rule IDs, governed codes, the supported specification line, field
and limit labels written in SDK code, limit numbers, byte offsets, lines and
columns. Document content,
caller-supplied values (URLs, URIs, names, payloads, versions), provider and
evaluator output, and anything derived from them are reachable only through
fields, accessors and serialization the caller invokes. Escaping or truncating a
value does not make it value-free.

A wrapper prints its own layer and exposes its cause through `Error::source()`,
never both, so `{:#}` and `anyhow` chains print each layer once. A native cause's
own formatting remains the provider's responsibility; apply the application's
disclosure policy before printing that chain.

A value type's own `Display` (`Pointer`, for example) is the rendering a caller
asked for, not a diagnostic, and is out of scope. A location attached to a
diagnostic is in scope: `SourceLocation`'s `Display` prints line, column and byte
only; its pointer stays reachable through `pointer()` and its public field.

Each preparation is independent. The default validator shares compiled state when
cloned; custom validators choose their own ownership and thread-safety properties.
Prepared does not mean every value is decidable. Handle all four results.

## Edit an opaque value

Parse a `Document` to edit it. `Document::parse` admits exact JSON, keeps an
invalid draft inspectable and refuses text that is not JSON as an `InputError`;
`document.validate()` assesses the admitted snapshot under its retained limits.
Default parsing and `validate_document` both use `JsonLimits::default()`, including
the 10,000-level cap. Edit
the `Document` through a shared borrow and retain the returned snapshot. Generic paths use RFC 6901:
`~0` escapes a tilde and `~1` a slash. Parents must exist; `set` inserts missing
object members and `/-` appends to arrays. Use exact JSON values to preserve
number spellings and escaped UTF-16. A pointer is escaped text or an exact
`Pointer`, so a finding's location addresses the member it reports, including a
name holding an unpaired UTF-16 unit: `document.remove(&pointer)`, or
`Edit::remove_at(&pointer)` in a batch.

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
and removals are refused. The document's retained byte, node and depth limits apply to the result;
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
UTF-8 byte offsets and one-based lines and byte columns. Quote untrusted pointers for display. OBI-02 adds the failing `keyword`, a `schema` origin in the published
fixed schema, and optional `ValueProblemDetails`: a missing required member,
expected types or exact bounds. Disallowed members use the source pointer alone.
Failed unions stay at `anyOf`/`oneOf` without speculative branch details.
Facts never copy rejected document values. The pinned 0.2 schema's largest
serialized `[keyword, schema, details]` tuple is 199 bytes, guarded by an exhaustive
schema test with no runtime fact-size cap. Reports retain at most 4096 findings
and 8 MiB of source pointers. Finding truncation leaves rule evidence intact. Use codes and evidence
for program logic and messages for explanation. The API reference lists every
[document finding code](api-reference.md#document-finding-codes) with its rule.

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
