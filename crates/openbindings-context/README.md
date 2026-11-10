# OpenBindings context

Exact context objects and `{message, schema}` challenges shared by the optional
interface companions. This applies the [context document at the pinned interface
revision](https://github.com/openbindings/interfaces/blob/67f707c31aecbdf0551385e369d8024097db6278/context/README.md).

```rust
use openbindings::JsonValue;
use openbindings_context::{Context, ContextChallenge};

let context = Context::try_from(JsonValue::parse(r#"{"apiKey":"provided-for-this-attempt"}"#)?)?;
let challenge = ContextChallenge::try_from(JsonValue::parse(
    r#"{"message":"An API key is required.","schema":{"type":"object","properties":{"apiKey":{"type":"string","writeOnly":true}},"required":["apiKey"]}}"#,
)?)?;
assert_eq!(challenge.message().as_str(), Some("An API key is required."));
assert!(context.value().member("apiKey")?.is_some());
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Context` checks object shape and leaves every member opaque. `ContextChallenge`
checks the closed outer carrier, a nonempty message and an object-form schema.
It preserves the exact schema, including the `oauth2` acquisition annotation,
without evaluating it. An object-form schema is not a schema-validity proof.
The message accessor preserves JSON strings that cannot be represented by `&str`.

Clones share immutable JSON storage. Debug and error formatting omit producer and
caller data. Acquiring context, deciding what to disclose, combining sources,
storage and retry policy belong to the application. This package performs no I/O
and depends only on the core's exact JSON API. It adds no OBI document fields.

Borrow an admitted carrier's exact object with `value()`; use `value().clone()`
when an owned object is needed. On interface carriers, `to_json` means bounded
encoding, as on invocation errors and requests.
