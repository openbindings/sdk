# Preparing an SDK release candidate

The Rust packages and TypeScript facade are unpublished. Existing npm
`@openbindings/sdk` releases install the legacy implementation. This procedure
prepares and checks local artifacts; it does not publish, reserve package names,
change npm tags, or deploy documentation.

Use the toolchain in `rust-toolchain.toml`, Python 3.13 or later, Node 22 or later,
and the matching `wasm-bindgen` CLI from the workspace manifest. Install the
TypeScript development dependencies with `npm ci --ignore-scripts` in
`packages/typescript`. Run the required qualification from `RELEASING.md` on a
clean, committed candidate before selecting it for release.

Build the source-bound facade in `packages/typescript`:

```sh
npm run build:wasm
npm run build
```

From the repository root, prepare a fresh output directory:

```sh
python3 tools/package-candidate.py --output target/release-candidate
python3 tools/build-reference.py --output target/reference-candidate
```

Use a new directory for each attempt; tools preserve earlier receipts. Set
`CARGO_TARGET_DIR` to reuse a build cache and `--offline` on the packager only
when intentionally qualifying the cached Cargo graph. Keep the environment and
any offline limitation in the release evidence.

The packager verifies original source bytes in every Cargo archive, rejects stale
or dirty Wasm build identity, installs the npm tarball in a fresh project, and
runs Rust examples and the ordinary-Serde compatibility matrix exclusively from
extracted Cargo archives. Local Cargo patches resolve unpublished siblings; they
are temporary qualification infrastructure, not the eventual public quickstart.
The JSON manifest records artifacts, hashes, source identity, and a topologically
sorted publication order. Packaging success is not a substitute for the full
host, semantic, resource, and runtime qualification.

The reference builder checks Rustdoc and emitted TypeScript declarations and
creates a static versioned-reference bundle with a language index. It records
the same candidate identity. Inspect both references and their examples before
uploading that exact bundle to the existing documentation host. Its local presence
does not establish a public URL or deployment. Keep versioned reference paths
immutable; update the documentation site's current-version link only as part of
the release decision.

## Publication decision

Prepare the following concrete record before requesting the final release
decision:

- Exact commit, spec identity, package versions, artifact hashes, required CI,
  supported hosts, measured workload limits, review dispositions, and remaining
  exclusions.
- Verified crates.io account and package ownership for every required crate.
  An available name is not proof of ownership; recheck at publication time.
- Verified npm organization/package access and a deliberate legacy transition.
  Proposed first Rust-backed release: `0.2.0-alpha.1` under the `next` tag, leaving
  legacy `latest` unchanged until a separate promotion decision. This is a
  proposal, not an executed tag change.
- Intended annotated SDK tag, proposed `v0.2.0-alpha.1`, and the corresponding
  GitHub release assets and immutable documentation destination. Component
  versions remain independent of the specification and project cohorts.
- A real security contact, compatibility/support statement, and clear migration
  guidance. See `SECURITY.md`, `docs/migration.md`, and `docs/api-reference.md`.

Publish the dependency closure of the four supported Rust libraries in manifest
order. `openbindings-wasm` is an internal build crate: retain its Cargo publication
guard unless a separate consumer need is accepted. Its compiled engine is shipped
in npm. Internal dependency crates are published only because the supported Rust
libraries need them; their implementation types are not new supported APIs.

The source changes that deliberately enable publication, including corresponding
publication-guard checks, must form part of the final qualified candidate. Do not
edit manifests after recording release artifact hashes and pretend those hashes
still identify the upload. SDK tooling currently checks that publication remains
disabled; update that check narrowly during the authorized release change.

## Execution and recovery

After authorization, publish each required crate in order and verify its recorded
version and checksum before proceeding. Registry propagation can delay dependent
publication. Record each completed step and resume from the first unverified
step; never overwrite a published version or assume publication is transactional.
Resolve an incorrect already-published artifact with an explicit corrective
release decision, not by reusing its version.

Publish the qualified npm tarball with the selected prerelease tag. Verify that
the legacy tag still resolves as intended, and install the new exact version in a
fresh project. Verify the public Cargo consumers without local patches. Attach
the manifests, qualification evidence and reference bundle to the selected
annotated-tag release, then publish the documentation through the existing host's
authorized process. Confirm the actual public links and installation examples.

This component release does not migrate applications, retire legacy SDKs, or
promote an OpenBindings project cohort.
