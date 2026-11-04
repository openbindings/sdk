# Independent generated identifier-range review

**PASS for the bounded correction; R-PP3 is resolved.** The generated static ranges remove the unreserved cold general-regex construction without changing identifier membership. The full AST/HIR/NFA compiler allocation/work certificate remains open.

Production, generator, lock, license, checkout and integration identities are recorded in `source-hashes.json`. Production was not edited. The runtime copy used for independent probes is byte-equal to the reviewed source.

`identifier::start/part` now perform the same ASCII fast paths and binary search over sorted inclusive scalar ranges. There is no runtime parser, initializer, mutable search cache or retained global owner. The original group-name grammar retains dollar/underscore/join-control additions, raw/escaped decoding, duplicate detection and invalid/lone-unit rejection. The no-flags regex-format parser also uses this same membership API; its language policy is unchanged. The tables contain 677/793 ranges, so each membership lookup is logarithmic with a small fixed comparison bound. This certifies this membership component, not every width-dependent operation elsewhere in the reader.

The explicit regex-syntax `=0.8.11` dependency and developer generator use the same exact package identity/checksum as the existing root lock. `--check` reproduces the table from the public HIR property API and compares exact bytes; `--write` is explicit and never invoked by a production build. The required `tools/verify.py` path runs the locked checker. The copied Unicode license is byte-identical to the pinned dependency's Unicode-table license.

## Finding corrected during review

The first implementation omitted a checkout rule for the byte-compared generated source. Independent Git `core.autocrlf=true` smudge changed all 1,479 line endings, from 33,205 to 34,684 bytes, which would fail the required Windows checker. The explicit ranges.rs `-text` rule now preserves all 33,205 original bytes. Before/after hashes are retained. The checker was not weakened to normalize altered bytes.

## Independent evidence

* **2,224,128 scalar comparisons:** every Unicode scalar for both production membership functions versus the prior anchored regex matchers, with zero differences.
* **16 private tests pass**, including the production table/grammar controls and independent exhaustive/concurrent controls. Eight concurrent callers perform 800 reads with identical supported outcomes.
* The unchanged cold/warm reader inputs now request **184/184 bytes** for `(?<é>x)`, **185/185** for `(?<xé>x)`, **186/186** for `(?<éé>x)` and **183/183** for the ASCII control. Every read releases all measured storage. The former first/two-property cold requests were 798,632 / 1,617,239 bytes with retained engine globals.
* The actual locked generator check passes. An isolated compilation of the unchanged generator passes **eight controls**: exact source, changed range rejection, CRLF rejection, missing source rejection, explicit regeneration, regenerated check, no implicit write and unknown-argument rejection. The generated output and dependency license remain exact.
* A fresh forced-original evaluator copy passes **1,546/1,546 required cases**, with no configuration failures, and **62 inherited controls in both modes**, including unchanged large array/late-invalid/object outcomes and budgets.

The coordinator's native/minimum-Rust and actual host gates are broader evidence, separate from these independent controls. No verdict oracle, pattern support policy, caller default or required cap was enlarged or relaxed. This closes the named-group cold-construction defect only; it does not turn the remaining general VM construction schedule into an approved certificate.
