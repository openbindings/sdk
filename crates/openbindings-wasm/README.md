# openbindings-wasm

This crate builds the Rust/Wasm engine for `@openbindings/sdk`. The supported TypeScript API is the handwritten facade, not these ABI types. Rust applications normally use the other public crates directly. Build with wasm-bindgen 0.2.129 and the pinned wasm32 toolchain.

Version 0.2.0-alpha.1 is an unpublished development build. It requires Rust 1.88.
The full repository includes public caller examples, capabilities, release process,
license inventory and verification instructions. Package publication and application
cutover are separate actions.

Build the packaged asset with `npm run build:wasm` in `packages/typescript`.
The release profile optimizes only this private bridge crate for size and removes
the Wasm name section with wasm-bindgen. Other crates retain their release settings;
optimizing the whole Wasm dependency graph for size increases evaluation latency.
