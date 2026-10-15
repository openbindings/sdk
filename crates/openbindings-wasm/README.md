# OpenBindings Wasm bridge

Builds the engine included in `@openbindings/sdk`. TypeScript applications use the
handwritten facade and its bundled Wasm asset. Start with the
[TypeScript example](https://github.com/openbindings/sdk/blob/main/packages/typescript/README.md).

The bridge ABI is private. Rust applications use the public Rust crates. This
unpublished build package requires Rust 1.88; its generated bindings require
wasm-bindgen 0.2.129. Run `npm run build:wasm` in `packages/typescript` to rebuild
the packaged asset. See [build instructions](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md)
and [size measurements](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md).
