# OpenBindings Wasm bridge

Builds the engine included in `@openbindings/sdk`. TypeScript applications use the
handwritten facade and its bundled Wasm asset. Start with the
[TypeScript example](https://github.com/openbindings/sdk/blob/main/packages/typescript/README.md).

The bridge ABI is private. Rust applications use the public Rust crates. This
unpublished build package requires Rust 1.88; its generated bindings require
wasm-bindgen 0.2.129. Run `npm run build:wasm` in `packages/typescript` to rebuild
the packaged asset. See [build instructions](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md)
and [size measurements](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md).

The facade's `evaluateJson` and `explainJson` admit exact text or bytes inside
Wasm and release the temporary input in the same call. The latter collects the
same bounded diagnostics as `explain`. Registration limit errors cross the bridge
with fixed text and `details.resource`, using preparation's resource identities.
