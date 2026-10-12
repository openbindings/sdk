# Synthesis workload measurements

Local macOS arm64 run, 2026-10-11, Rust 1.99.0, release profile with thin LTO and one
codegen unit. Seven samples per size/phase; CSV records minimum, median, maximum,
unit count, input bytes and output document bytes. CPU model was unavailable to
the sandbox. These are local synthetic qualification measurements, not a service
or browser latency guarantee. No allocations or RSS measurements were taken.

Replay from root:

```sh
python3 tools/verify-interface-synthesis.py --performance
```

The external fixture's `examples/measure.rs` is the executable. Its receipt and
raw stdout/stderr are in `target/interface-synthesis-qualification/source/`.
Pass `--archives` to measure extracted packages instead. Native provider calls are
counted: one derivation per complete call, no retry. Exact source owners are cloned
per source during authoring, never per coverage entry; correspondence memoizes
source-index/key pairs. Cancellation/drop ownership has maintained tests.

`baseline.csv` preserves the first correct candidate before optimization, using
core semantics at SDK base `63d5808f6e9bbcc27b9a442dec787ad500518ace`. `indexed.csv`
uses the same workload after replacing repeated primary-key object scans in core
assessment with borrowed key sets. No schema or conformance policy changed.

At 10,000 units (178,901 source-content bytes; 1,194,591 document bytes):

| Phase | Baseline median | Indexed median |
| --- | ---: | ---: |
| Provider derivation | 20.64 ms | 19.83 ms |
| Complete checked synthesis | 300.62 ms | 39.49 ms |
| Coverage with cached assessment | 9.49 ms | 9.16 ms |
| Cold document assessment | 269.94 ms | 10.66 ms |
| Exact envelope encoding | 21.36 ms | 21.21 ms |

“Coverage with cached assessment” includes admission, a constant-time provider
handoff, cached proof lookup and source/coverage correspondence. Candidate cloning
and first assessment happen outside that timer. It is the closest public-consumer
measurement of standalone checks; no extra public validation API was added solely
for a benchmark. Cold assessment excludes parsing; provider derivation includes
building/parsing its output; the complete call includes both. Encoding separately
includes exact envelope serialization and its bounded reparse.

Cached assessment is below the useful timer resolution in some samples (0–125 ns);
zero means unresolved by the timer, not zero work. The first size-zero complete
call also initializes fixed normative schemas (2.3–3.0 ms max); the median hides
that first-process cost, so the full spread is retained. Inputs are generated before
timers. The sequential phases and local load can affect comparisons.

Guardrails for this fixture: investigate a repeatable >30× median increase for a
10× increase from 100→1,000 or 1,000→10,000 units, excluding sub-resolution cached
assessment. On this same host/profile, investigate 10,000-unit complete synthesis
>100 ms, cold assessment >30 ms, or cached-coverage checks >25 ms. These are generous
regression investigation thresholds, not cross-machine CI deadlines. The original
cold assessment grew ~70× and triggered investigation; indexed assessment grows
~9.3× and complete synthesis ~9.2× between 1,000 and 10,000 units. Re-establish
absolute baselines on another host before making a performance claim.

The companion adds no new production dependency versions: it reuses core, Serde
and the existing RawValue-aware Serde JSON bridge. It does not pull inspection,
an HTTP transport, an async runtime or the optional evaluator into its production
dependency graph. Tokio and the evaluator are qualification-only dependencies.
