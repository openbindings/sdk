# CI, fixture integrity, and package repair review

**PASS for the 22 files pinned in [source-hashes.json](source-hashes.json).** Numeric implementation, new recovery documents, whole-workspace verification, and clean-source package-candidate qualification are separate gates. No production files were edited by the reviewer.

## R-CI1 — P2: removing a ledger removed its coverage

The initial verifier discovered only README files. A known interface fixture directory with a file but no README therefore returned success and reported zero checked files. [missing-ledger-before.json](missing-ledger-before.json) preserves this independent reproduction. This was a pre-existing discovery weakness, not a defect in the two-column regular expression, but it undermined the newly added ledgers' continued coverage.

**Resolved:** the final verifier explicitly requires all six current interface fixture ledgers and still discovers additional ledgers. Removing any required README now fails; an empty repository fails; an extra discovered ledger with invalid coverage still fails. Core/evaluator input gates and SDK-owned test data are explicitly distinguished rather than indiscriminately imposing interface provenance on every fixtures folder. The final source and self-test retain byte drift, extra-file, missing-file, duplicate-row, and both table-format controls.

## Fixture and attribution integrity

The new parser accepts exactly the supported two-column path/digest and three-column path/owner/digest row presentations. SHA-256 remains 64 lowercase hexadecimal digits; repeated recorded paths fail, and every file under a discovered ledger directory must have a matching row. Path traversal and absolute row paths do not match the inventory and fail. The implementation does not skip mismatched bytes to accommodate table format.

All **32** ledger-controlled files match their recorded bytes. Independent lookup against interfaces pin `5815fb106bddd2c85ff4e93e0077570b3f6b9e74` finds exact byte-identical publication files for all **21 newly labelled pinned inputs**. SDK design controls and source receipts are separately labelled. [pinned-fixture-audit.json](pinned-fixture-audit.json) records the original source paths and hashes. Comparison's original manifest, 14 structural identity fixtures, and exact-value pack are unchanged; no directional coverage or general equivalence claim was added.

Each of context, kind-description, schema-comparison, and source-pull receives a LICENSE byte-identical to the root Apache license and a NOTICE byte-identical to current core. The seven existing NOTICE changes replace only the obsolete specification SHA with `c1419c9fc3ae5a00c2b83da710c11bf67dab7934`. Existing author and Go-origin attribution in the root/TypeScript notices remains intact. This is a package-content/integrity review, not a new legal interpretation.

## Lockfile and observation policy

The benchmark lock gains **24** registry packages and removes none. Every new package's name, version, registry source, and checksum exactly matches the root lock. Existing package identities and checksums are unchanged. Only two existing dependency lists change: jsonschema gains its already enabled IDNA dependency, and the SDK evaluator gains its already present regex-automata edge. [lock-audit.json](lock-audit.json) records the comparison. Locked offline Cargo metadata succeeds. The root lock and comparison policy were not changed by these repairs.

The preparation-refusal counter now uses a true schema with `schema_resources = 0`. This is a deliberate, typed preparation limit and judges no instance. Its two refusal observations remain counted separately from no-verdict. The original missing local target input and value are preserved in a new strict Undefined control, also requiring two observations and no preparation-refusal count. Because this schema is supplied externally by the benchmark adapter, it is not an invalid contained-reference admission witness. Pinned OpenBindings §5.5/§7.4 and JSON Schema's missing-target semantics support the required Undefined result. The prior preparation-refusal expectation was stale capability characterization. No expected semantic answer was weakened to generic inability and no timing/memory threshold changed.

## Independent checks

- Final digest verifier plus its self-tests: **32 files pass**.
- Independent digest matrix: **23 positive/negative controls pass**, including six removed ledgers, empty repository, both table formats, duplicate/drift/unlisted/missing/path/four-column cases.
- All **nine** observation tests pass against the actual benchmark executable, including wrong-expectation rejection and the two corrected outcome/refusal controls.
- All **48** benchmark policy tests pass, preserving enforcement, advisory timing, memory thresholds, and false-control rejection.
- Locked offline benchmark metadata succeeds; exact additions and fixture/attribution equality were checked independently.

The first policy run omitted the repository's Rust environment and failed because `rustc` was absent from PATH. Its log is preserved as `policy-driver-environment-failure.log`; rerunning with `/tmp/ob-sdk-env.sh` passes without source or expectation changes. A shell inventory glob for a nonexistent interface NOTICE also returned a shell error; it was not a build/test result. No SDK failure was hidden by either correction.

**Remaining gate:** a clean committed source/archive package-candidate run must establish that the added files are delivered. This bounded source review does not claim that still-pending run or the complete workspace/host campaign has passed.
