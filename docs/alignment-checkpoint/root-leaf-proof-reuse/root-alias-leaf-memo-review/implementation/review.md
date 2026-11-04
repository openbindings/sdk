# Root alias-leaf memo — bounded implementation review

**PASS on the reviewed four-file source over f62f47757ba6682bea316492056428a665a7bea4. No production finding.** Exact original source snapshots, hashes and diff are retained. The reviewer edited only isolated copies and receipts. This approves correctness/resource behavior of the experiment, not performance retention or G6.

## Implementation

The preparation pass checks each original query entry, updates only an existing descriptor, and relies on `note_reference_target`'s certified-leaf guard. The two bits saturate at two and cannot overlap keyword, Original or leaf flags. Shared query Arcs still count as multiple original edges. The final shared check precedes publication; no extra map/record/target cloning is introduced.

Execution leaves the first/direct leaf path untouched. Only a final post-alias shared certified leaf at the actual root occurrence uses the existing memo. Visit retains exact final node, root instance ID and content-keyed Context. Lookup and publication retain their context work; hits use State.copied; misses cache only a completed cut-independent State after the existing copy and 256-byte insertion reservation. All errors propagate. Alias traversal, dynamic selection, child dispatch, original effect/cause identity and witness propagation remain unchanged.

## Independent evidence

Current Rust 1.99 and minimum Rust 1.88 each passed **70 distinct selected tests**, covering the new private probes and reused public ownership, scope, exact value, effect, custom-profile, preparation-budget, execution, recursion and Explain controls. Two preexisting release-only frame tests remain ignored; they are not counted as executed. The final probe-only additions add actual selected-meta and prior shared-attempt hit controls without changing the reviewed production bytes.

- At 2/64/5000 direct references, the target leaf executes once per pass, with N−1 hits and one completed leaf entry. At 5000 this is 1 body/4999 hits. Outcome and Explain's diagnostic pass have separate memo owners. Funded S/F and original type-witness outputs remain correct.
- Direct/single-wrapper and linear-chain jobs create no new leaf entries. Two queries to an indirect singleton alias leave the terminal leaf unmarked and keep ordinary behavior. Multiple original query entries sharing one Arc still mark the direct target.
- A retained root subtree with a nonzero arena ID caches correctly after its original owner is dropped. A marked-target 50001-item array, both valid and late-invalid, retains zero leaf entries. Property-name/value tokens also remain excluded. Existing 20k/late-invalid/50k-object and bounded composite jobs pass unchanged defaults; existing 1000/5000 reference-preparation controls pass.
- Equal-content Contexts reuse one entry; distinct Contexts produce separate entries. An actual undefined/inability effect made cut-dependent prevents new publication. Known invalid-pattern U, unsupported-pattern NoVerdict and local size Limit remain distinct; errors publish no State. Existing dynamic/recursive/eager controls retain their outcomes.
- **106 complete public records match f62 exactly**, including diagnostics and locations; their existing **798 observed cancellation stops** pass per version. The new root-memo jobs add **794 exhaustive hit/miss stops** per version with clean retries and correct verdict-first Explain handling. The metadata pass adds ten targeted each-entry/final-publication interruptions.
- Exact/one-below new work and reservation thresholds pass. Actual nested shared work/reservation failure and cancellation stop a populated memo hit. A real selected custom-meta job records one miss/one hit through this same path. No new allowance is created.

`coverage.json` maps the 13 original and 8 addendum control groups, identifies superseded coarse-filter expectations, and distinguishes runtime evidence from static proofs. No assertion equates the new job's callback totals to baseline or treats an optimization miss as a refusal.

## Layout and qualification limits

Native Run grows 976→984 bytes on both tested Rust versions; State 64, Fields 8 and Frame 256 are unchanged. Actual current wasm32 layout reports Run 600→600, State 32, Fields 8 and Frame 136: existing padding absorbs the root ID. Wasm sizes were obtained by exactly four intentional compile-time array-size diagnostics per side, with the dependency fingerprint retained. These diagnostics are a measurement method, not failed semantic builds. This review did not run a new Wasm semantic replay or physical-stack claim.

Private instrumentation counts leaf bodies/hits/entries and metadata progress; it is not benchmark or allocator evidence. Existing SDK reservations are preserved rather than presented as exact physical memory. The parent owns uninstrumented allocation/timing selection and broader final host qualification. No source correction or oracle adjustment is requested.
