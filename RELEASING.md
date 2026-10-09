# Release process

The canonical repository is `openbindings/sdk`; source changes integrate on `main`
through reviewed squash-merged pull requests. The current packages are unpublished.
`publish = false` intentionally prevents accidental Cargo publication. Source
landing and local qualification do not publish a package, deploy a consumer,
retire another SDK, or establish whole-project release readiness.

Before a separately authorized release:

1. Confirm repository ownership, integration branch and package names against
   current `openbindings/project` policy. Resolve the existing npm package's
   ownership/version transition explicitly; do not overwrite its history.
2. Close independent product-quality review and any material release findings.
   Review the bounded migration report, permitted capability limits and exact
   specification identity; do not infer whole-project/spec release readiness.
3. Select component versions independently from specification versions. Before
   1.0, minor releases may break APIs; patch releases contain compatible fixes.
   Document breaking changes under Changed/Removed in CHANGELOG.md.
4. Update workspace/npm/sibling versions together, locks and dependency notices.
   Retain upstream source/patch identities. Re-run native, actual browser, Node,
   workerd, fresh package, no-I/O and performance gates on the final source.
5. Publish internal dependencies and companion crates in dependency order only
   after destination/package ownership is approved. Remove `publish = false`
   deliberately for the intended packages. Test registry consumers. Generate the
   npm Wasm asset from the same source and verify its recorded source identity.
6. Use the project's current annotated-tag/release process. Record exact artifact
   hashes and supported specification identity. Application migration, existing
   SDK retirement and verified-cohort promotion require separate decisions.

For local consumption now, use the accompanying delivery's extracted Cargo archive
consumer (registry-version dependencies patched to adjacent archive sources) or
install its npm tarball. Cargo archive manifests contain no development path
dependencies. Neither route requires publishing these packages.
