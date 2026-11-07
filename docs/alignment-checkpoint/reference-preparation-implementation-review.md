# Independent implementation review: bounded reference preparation

**PASS for this bounded increment. No unresolved implementation finding.** The four production files match the initial review snapshot, including the parent's `enqueue_dynamic` tightening; see `final-production-hashes.json`. Tests added during review are recorded separately in `final-test-hashes.json`. The base is `98ab4753f7f78ae1d5f381717477d2b6dd83d26d`. I did not edit production.

This closes the accounting/read-sharing implementation reviewed in `../design/review.md` and restores the two unchanged OE-05 sibling-reference jobs under aggregate default preparation limits. It does not qualify the outgoing compiler, full pattern VM-once preparation, cross-phase syntax-fact sharing, or the complete original-engine performance/default-switch gate.

## Source review

The fixed local extent optimization is exactly the six audited edition/keyword pairs and only string operands. It subtracts disjoint original token extents with the existing checked helper, leaving one token header. Wrong kinds and nonqualified fields keep their prior extent. The actual validator receives the unchanged original value. Draft 7's scalar-presence shortcut, including its rejection of lone surrogates, is unchanged. The modern inactive `$recursiveRef` syntax constraint is included. Actual custom-meta evaluation does not use this shortcut. The complete resource/root snapshot remains unchanged; new tests bind the six constraints and production nonasserting-format options to the certificate.

`Queries` holds one immutable borrowed SchemaSpace for its entire lifetime. Its key uses original resource ID, profile-presence and the exact decoded scalar reference string. It borrows text only during construction. The only shared owner is `Arc<Result<Target, QueryFailure>>`; permanent prepared entries retain original node and ReferenceKind identity. Cache hits therefore cannot replace semantic effect identity, dynamic context or referring origins.

The 1,024-key policy is an insertion cap: a late hit remains a hit, while new misses run the fully reserved original query and are not retained in the temporary cache. No cap-specific refusal, eviction, altered resolver or second namespace was introduced. Per-keyword candidate discovery, target queuing, resource counting and original-location construction remain after the lookup. Explicit `use_name`, `enqueue_dynamic` and missing-URI charges cover work that hits no longer obtain from the resolver envelope.

The storage extraction retains the existing two-vector Path behavior and full-new-backing schedule. Its borrowed Ledger does not retain a control or reset an attempt. Preparation overflow/allocation failure marks the enclosing allocation attempt; Graph retains its existing evaluation failure mapping. Hash matches still require equality, collision traversal is charged, and rehash changes links only after capacity/work admission. A failed insertion does not publish its key. The whole temporary table drops at construction exit; only result Arcs needed by original prepared entries survive.

## Independent execution evidence

The copied-source bridge forces Plan preparation from the original document/environment. It neither changes the root/dialect nor routes ordinary cases through the outgoing compiled validator. Dependency artifact identities and copied production sources are retained.

| Lane | Independent result |
| --- | --- |
| Required original evaluator corpus | 1,546/1,546, no preparation/configuration or semantic failures |
| Existing numeric/large workload controls | 62/62, including unchanged 20,000/20,001 array and 50,000-property jobs |
| Reference context/effect controls | 19/19 in Outcome and Explain |
| Fixed read-shape and bounded checker controls | 210/210; all exact/one-below work and allocation boundaries pass |
| Fixed-check cancellation/retry | 390 observed boundaries, plus nested ignored-error controls |
| Cold fixed compiler lanes | Draft 7, 2019-09 and 2020-12: one-below refusal, cancellation, retry and warm behavior pass |
| Cached versus uncached original queries | 744 comparisons over varied holders/references; includes a stopped profile |
| Temporary-cache cancellation/retry | 33 observed boundaries; exact shared caps and tighter nested refusal pass |
| Public original preparation cancellation/retry | 240 observed boundaries across repeated success, dynamic dispatch and missing content |
| Backing/collision controls | Five standalone groups, including an 8,000-operation collision/LIFO/clear model, failed growth and actual allocation measurement |
| Existing copied implementation tests | Five query groups and two storage groups pass |

The 19 inputs are the exact independent design controls. They cover both static/dynamic keyword orders, a dynamic candidate otherwise hidden in `$defs`, different relative bases, exact case/default-port contained identities, repeated coherent defects/missing content and equivalent decoded JSON string spellings. The private query comparison uses actual uncached `SchemaSpace::query` for each holder, including malformed, missing, pointer and anchor queries; it does not infer correctness from a final verdict alone.

The private cap control performs 1,026 distinct queries, retains exactly 1,024 entries, confirms an earlier result still shares its Arc, confirms repeated post-cap misses do not share an Arc, and verifies the temporary cache releases its ownership. Public warm preparation tests enforce the measured exact work threshold and one below, zero-allocation refusal, and a clean subsequent retry. Existing owner-drop checks continue to evaluate successfully after the original caller handles are released.

## Frozen OE-05 and allocation observations

The five frozen inputs were copied verbatim from the prior iterative-design receipt. All retain their expected results: depth success, explicit small-stack Limit, two repeated-reference successes and reference fan-in success. The previous repeated-reference preparation refusals remain in the earlier receipts.

The table below measures the complete public original preparation call, including environment retrieval/construction and Plan creation under one aggregate default Attempt. Parsing/document admission and ExternalSchemas registration happen before that call, as in the existing consumer boundary. “First environment” does not mean every process-global fixed compiler is cold; independent fresh-process fixed-compiler lanes are retained separately.

| Frozen job | First-environment work | Warm preparation work | First preparation requested bytes | Incremental peak bytes |
| --- | ---: | ---: | ---: | ---: |
| 1,000 repeated 256-byte payloads | 1,463,229 | 1,417,629 | 599,134 | 298,003 |
| 5,000 repeated 1,024-byte payloads | 14,892,669 | 14,749,917 | 2,625,638 | 1,150,652 |

Both jobs remain below the unchanged 20,000,000 work-unit and 512 MiB preparation defaults. Warm temporary construction releases all call-local allocations after dropping the newly prepared owner. The first call also retains the completed original environment/Plan ownership, which is expected and is reported separately from temporary memory. Evaluation of these repeated aliases still incurs temporary alias-path allocations; this review does not call that a completed latency optimization.

For a 1,024-entry Path with the probe's concrete key/value layout, measured requested backing was **98,112 bytes**, exactly equal to its ledger charge; incremental peak was **49,152 bytes**, and retention after drop was zero. This tests the generic schedule; production key/value sizes remain target-derived. The 1,026-query production cache sequence charged 1,065,120 total bytes including resolver/result envelopes and 481,680 work units. These are requested-allocation observations, not allocator-internal or RSS claims.

All 210 warm fixed checks stayed within their extent reservation. Qualified reference-string checks allocated no heap during validation in the measured lane. Eight nonqualified `$id`/`$anchor` lone-surrogate cases each requested eight bytes, which remain covered by their unchanged full-extent policy. Fresh-process cold 2020 validation requested 1,930,103 bytes with 1,106,923-byte incremental peak under its unchanged 16 MiB startup reservation; the other edition results are retained in their own JSON files.

## Reproducibility and limits

`setup.py`, `run-controls.py`, `private-tests.py` and `preparation-probe.py` retain build commands, inputs, source copies and outputs. `final-summary.json` collects the main gate counts. `final-production-hashes.json` is separate from the moving test hashes. The storage probe uses a minimal Graph stand-in solely to inspect the real generic Path's requested backing and collision mechanics; it is not used as evidence for evaluator semantics or preparation Control behavior. Those are exercised by the actual copied evaluator and actual Attempt.

Three independent harness errors were preserved and corrected without changing production or required answers: a missing root `dialect` import in the helper wrapper; omitted private test fixtures plus a Target comparison/type-inference mistake; and an allocation observation retaining its own 745-byte JSON report across the drop check. The corrected drop check releases that report before measuring. None was an SDK finding.

The parent separately reports current/minimum-Rust, strict lint, fresh Wasm, Node/package and browser qualification, including the new reference cases. I did not rerun those complete host lanes; this receipt's independent executions use current native Rust and the copied-source original bridge. There is no new full latency baseline claim. Required outcomes, resource defaults, original source identity and the existing performance gates were not weakened.
