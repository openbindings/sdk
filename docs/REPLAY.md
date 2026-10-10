# Source replay

Start with a clean clone or extracted source archive. Rust 1.99.0 is the MSRV and
qualification compiler; install rustfmt, Clippy and wasm32-unknown-unknown for that
toolchain. Install wasm-bindgen-cli 0.2.129 with its locked dependencies. Use Node
22.19.0/npm 10.9.3 and Python 3.13 or later. First acquisition needs public network
access. Subsequent Cargo commands can use the resulting cache offline.

```sh
cargo install wasm-bindgen-cli --version 0.2.129 --locked
python3 tools/verify.py
cd packages/typescript
npm ci --ignore-scripts
npx playwright-core install --with-deps chromium webkit
cd ../..
python3 tools/verify.py --browser
```

`WASM_BINDGEN` may identify a task-local wasm-bindgen executable;
`CHROMIUM_EXECUTABLE` may identify an installed Chromium binary, and
`PLAYWRIGHT_BROWSERS_PATH` may select a task-local browser directory. The scripts do
not require these overrides or any original workstation path. Wasm source hashing
works without Git. Generated JS, Wasm and observations are ignored build outputs.

The independent Python judge decodes the pinned specification fixtures and rejects
missing/duplicate/unexpected/wrong results; the Rust/JS observers only exercise
public APIs. Both browsers run 435 core cases and 1,566 evaluator cases. The reusable
Rust evaluator kit additionally supplies its own exact-case refusal declarations
and source-location checks. A test filter or ignored TLS test is not a full pass.

The accompanying migration evidence includes additional isolated TLS/CORS/redirect
fixtures, local workerd, fresh Cargo/npm archive consumers, source mutations,
finite robustness campaigns, dependency checks and frozen equivalent-work
benchmarks. Its REPLAY.md describes these qualification commands and source pins.
The repository CI is a maintainable regression baseline, not a claim that every
migration campaign runs on every push or that unexecuted OS jobs passed.

For local npm packaging, build Wasm then JS and run `npm pack --ignore-scripts` in
`packages/typescript`. A stale generated Wasm identity makes `npm run build` fail.
Install the tarball in a fresh Node project; initialize using the documented
included `.wasm` export. No Rust compiler is required for package consumption.

Cargo siblings are unpublished, so `cargo package --no-verify` alone is insufficient.
The [candidate procedure](release-candidate.md) archives the workspace and internal
dependencies, then runs fresh consumers whose registry-version dependencies are
patched only to extracted archives. Normalized manifests have no checkout paths.
Keep the recorded locks, commands, and temporary patches until the packages exist
in an approved registry.

For the active acquisition-boundary check on a POSIX host, allow temporary loopback
listeners and named pipes and use a fresh output directory:

```sh
cargo build --locked --manifest-path tools/no-io/Cargo.toml
python3 tools/verify-no-io.py --binary tools/no-io/target/debug/sdk-no-io \
  --output target/no-io-receipt
```

If `CARGO_TARGET_DIR` is set, use its `debug/sdk-no-io` binary instead. This isolated
consumer enables the private dependency's HTTP/file retrieval features. Direct
dependency positive controls must hit both traps; SDK document, schema reference,
and explicit-resource paths must not. Record the build command, source, feature
graph, and binary hash together. This is a regression control for the named paths,
not a universal proof of absence of I/O or a new public dependency API.

The [controlled runtime harness](../tools/runtime-qualification/README.md) measures
declared consumer jobs with frozen inputs and budgets. Its deterministic `check`
mode is separate from exclusive-window timing; do not treat shared CI timings as
qualification measurements.
