# Kind adapter example

The [adapter author guide](../../docs/writing-a-kind.md) builds this external
crate in stages. `example.memory@1` is a fictional in-memory protocol: source
data is exactly `{"channel":"local"}`, binding data is exactly
`{"mode":"echo"}`, and each input is returned unchanged until input closes.
It has no transport, network, threads or file access. Its fixed inventory offers
`echo` without input or output schemas.

`Memory` implements binding invocation, advisory preflight, kind description and
source pull. It does not offer source checking. The pull handles repeated calls,
alias reuse, Boolean-schema conflicts and removal proposals. Object schemas
produce explicit comparison gaps. It fails with a native
error when its generated binding key is occupied. This bounded demonstration
uses assertions for construction of its own JSON; an adapter accepting arbitrary
external content should return admission failures through its native error type.

From the SDK root, run:

```sh
python3 tools/verify-kind-adapter.py --wasm
```

Like the other external examples, the manifest uses version dependencies and a
separate workspace. The verifier copies it to `target/`, supplies local patches
for the unpublished SDK crates, and uses its committed lockfile offline. It
verifies the selected dependency identities against the SDK lockfile. It also
checks that all guide Rust blocks match the compiled source, runs tests and
strict Clippy, and builds the library for `wasm32-unknown-unknown`. Tokio is only
a native test dependency.

The Wasm check builds the library; it does not instantiate it in a browser or
Workers host. Host transport and host imports need separate integration tests.
