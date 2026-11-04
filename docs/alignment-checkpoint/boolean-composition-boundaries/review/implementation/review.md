# Boolean composition boundary implementation review

**Bounded implementation PASS**, with the permanent-test coverage finding R-BC1 now corrected. This reviews the four-file experiment over `0142eaa56ca2c8fd50716e943aeef05931bee19b`; runtime-probed snapshots/hashes are in `source/` and `source-hashes.json`; final test-only correction pins are in `final-source/` and `final-source-hashes.json`. No SDK source was edited by this review. Performance selection and G6 remain open.

## Source assessment

The implementation matches the approved policy. Symmetric AND/OR annihilator, identity and equal-formula cases return through `immediate`; only an enclosing preparation control receives `charge_work(0)`. The operation does not traverse BDD nodes or allocate. Nontrivial `apply`, its cache/recursion/terminal visits, not/Xor, effect identities, selected child execution and shared error mapping are unchanged. All four identified loops now tick unconditionally before the affected iteration work: completion annotation gating, property merge, item merge and uncertain contains-distribution slots.

No new data field, owner, cache, layout, capacity, default or semantic oracle was introduced. Existing memory charges remain. The exact SDK work and cooperative callback boundaries intentionally change: 88 of the compared public records have different callback counts. This is the new approved accounting policy, not an assertion that the old trace was preserved.

## Independent qualification

Private copies compiled against the existing current and minimum-version dependency sets; exact compiler commands are retained. Each version passed 66 distinct selected tests, including five new independent groups and inherited ownership, Boolean, count, execution, exact operand, effect, custom-profile, preparation-budget, recursive interruption and Explain controls. Two preexisting release-only frame controls remained ignored; this review did not relabel them as run or repeat unchanged physical-stack qualification.

- All **106 complete public observations** match the 0142 baseline, including exact diagnostics/locations and all existing eager local-limit inputs. Comparison deliberately excludes callback counts and the differently indexed cancellation traces.
- **798 recomputed public cancellation stops per version** pass, with healthy same-owner retries and the established distinction between pre-verdict cancellation and post-verdict incomplete Explain. Existing recursive and Explain suites add their own observed-checkpoint, cumulative-budget and enclosing-Attempt controls.
- The four loops were instrumented only in copied sources immediately before their actual new ticks. At widths 1 and 64, all **260 target-iteration cancellations per version** stop at the selected iteration, not at a later final check. Funded widths 1/64/4096 pass; exact and one-below work/storage thresholds behave correctly. Failed count updates do not publish their temporary distribution; completion failures leave completed memo/facts unpublished. Clean retries pass.
- **1,280 independent annotation truth assertions per version** cover property/item merges and completed-state gates with FALSE, TRUE, an Undefined formula and an independent Inability formula, every two-variable valuation, absent/previous entries, and a lone-surrogate member name.
- Direct algebra controls verify zero ordinary polls and unchanged graph work/storage/node/cache state for terminal results. Genuine nontrivial composition matches direct unchanged apply on cold and cached calls. Actual nested Attempt tests distinguish a healthy zero remainder from spent work, spent reservation, ignored narrower-child failure, overflow and cancellation, including their actual distinct codes.

`coverage.json` maps all 12 frozen groups to evidence. `comparison.json` retains complete-observation comparisons and the changed count inventory. The instrumented loop widths are synthetic resource controls, not benchmark inputs or performance evidence. No new allocator-request or universal cancellation-latency guarantee is claimed: this slice changes no storage representation, and actual matched allocation observations belong to the parent's separate measurement lane.

## R-BC1 — permanent test coverage only

`trivial_composition_preserves_shared_attempt_exhaustion` currently uses `Attempt::run(0, 0)` for both branches. In its purported allocation branch, `charge_allocation(1)` calls `Attempt::check` first and exhausts work before reaching allocation accounting. Thus both branches currently cover work exhaustion. Production behavior is correct under the independent funded-work/zero-reservation, nested and overflow probes. Give the permanent allocation branch enough work and assert `preparation-sdk-reservation-bytes`; retain a distinct zero-work branch asserting `preparation-sdk-work-units`. This correction does not change the timed production logic.

The final test-only correction gives the allocation branch eight work units and zero reservation, keeps the work branch at zero, and asserts distinct codes on the first refusal, both fast returns and outer completion. `test-correction.patch` preserves the exact delta. Static comparison confirms the entire production region of decision.rs and all other reviewed files are byte-identical to the runtime-probed source. `final-source-hashes.json` and `final-source/` pin the repaired source; no runtime replay was necessary for this test-only change. Parent owns its focused test/lint rerun. R-BC1 is closed.

No implementation finding blocks the bounded experiment. Keep it only if the separate selected compile/evaluation measurements, including annotation/uncertainty work, justify retention. This receipt makes no G6, baseline-recovery or whole-SDK acceptance claim.
