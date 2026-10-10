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

For local consumption, build Cargo archives or an npm tarball from the intended
commit. Test a fresh Cargo consumer with registry-version dependencies patched to
the extracted archives, or install the npm tarball into a fresh project. Cargo
archive manifests contain no development path dependencies. Neither route requires
publishing these packages.

Use [the candidate procedure](docs/release-candidate.md) to generate and verify
fresh archives and a versioned local reference bundle. The public
[support policy](docs/support.md) distinguishes qualified hosts, prerelease
compatibility, and reporting channels.

## SDK release readiness evidence

Source landing, package publication, consumer migration and legacy SDK retirement
are separate decisions. Record the candidate commit, toolchain, artifact hashes,
commands/statuses and any explicit capability exclusions for each applicable row.
A passing row is scoped evidence, not a whole-project quality or speed ranking.

| Gate | Required evidence for this SDK |
| --- | --- |
| Normative semantics | Frozen applied spec identity; complete OBI evidence and version-refusal checks; original-source diagnostic checks; closed semantic partitions preserved. |
| Consumer data compatibility | Public Rust/Serde and TypeScript ordinary/exact-value consumers, including large/precise numbers, duplicate names and explicit admission refusals; owned dependency profile evidence. |
| Evaluator capability | Default evaluator and custom-evaluator contract checks; permitted refusals named separately from verdicts; no implicit retrieval; limits and conservative-cycle behavior reported truthfully. |
| API reference and teaching | `tools/verify-api-reference.py`, negative reference controls, compiled guide/examples, and rendered Rustdoc/TypeScript inspection; explicit proof/draft, ownership, units and cancellation contracts. See [reference verification](docs/api-reference.md). |
| Host and lifecycle behavior | Native Rust, actual Chromium/WebKit, Node and workerd jobs where supported; retained-contract replacement, independent owner cleanup, real editor event flow and asynchronous discovery cancellation. |
| Diagnostics and bounded work | Stable codes, original byte/pointer coordinates, safe default messages, visible finding/problem truncation, adversarial bounded diagnostic generation and owner-release receipts. |
| Package reproducibility | Fresh nonsymlink Cargo/npm installs from exact archives; compiler/runtime source identity, archive hashes, license/notices, selected dependency policy and required CI results. |
| Performance and retention | Predeclared equivalent caller jobs with cold/setup/retained boundaries, repeated measurements and truthful entry/byte/RSS units. Historical Go comparisons do not establish current absolute readiness or a pure-TypeScript speedup. |
| Release authorization | Explicit registry/package ownership and version-transition approval, intended version changes and annotated-tag prerequisites; publication guards remain until that decision. |

Historical cohorts, retired Go procedures and peer rankings are not automatic SDK
release gates. When a consumer migration is requested, qualify that concrete
consumer against the exact released artifact in its own work.
