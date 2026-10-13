# Bounded performance repair

The first registered run is preserved in `cpu-run1/`. Thirty-nine rows passed and one failed: the first process pair of OpenAPI 3.0.4 AND POST (64 KiB), operation layer, measured 8,109.22 ns incremental adapter cost against a 6,581.76 ns ceiling. Later pairs passing does not erase that failure. No external disturbance is asserted and no gate, sample count, duration, workload or pass formula is relaxed.

Independent Astra examination identified redundant per-call work: normalizing every supplied origin twice and building a source-derived alternative inventory even on successful calls. The repair normalizes every grant once while still validating all grants, and lazily builds the sorted/deduplicated inventory only for challenges. Explicit selection and first-equivalent-alternative behavior remain unchanged. A matching-then-malformed origin regression guards against short-circuiting validation.

The review also found a measurement-procedure gap: warm-up did not assert wire/effect results for the precise benchmark inputs. Each condition now verifies exact target/body, expected credential header counts/values, and exactly one dispatch before timing. Inactive instrumentation costs remain inside both paths. The revised measurement source is byte-identical for the closed snapshot and candidate.

This is a changed candidate and corrected harness, not a repeat of unchanged timing until green. One new complete registered measurement is authorized by the focused-repair rule. Its outcome must be reported independently from run 1. The broad/source compatibility run preceded this repair; it used the previous qualified snapshot. Its anonymous/source-only execution paths are unaffected by these auth-context-only changes. The stricter final-run unsecured comparisons separately recheck the final candidate against the closed SDK.

See `astra-performance-repair.md` for the independent code review and `closed-snapshot.json` for all 621 checked baseline source files. Qualification compiler/lint failures are retained as development diagnostics; they are not runtime pass evidence.
