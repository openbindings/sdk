# Fuzz harness

This is a standalone, unpublished cargo-fuzz crate. No fuzz runtime is added to
SDK dependencies. The five `fuzz_targets/` binaries call ordinary functions in
`src/lib.rs`. `seeds/Cargo.toml` compiles that identical file on stable without
resolving libfuzzer-sys (or its arbitrary dependency). Its lockfile is independent
of the workspace. `tools/verify.py` replays every committed corpus file.

```sh
cargo test --locked --manifest-path fuzz/seeds/Cargo.toml --test seed_corpus
rustup toolchain install nightly-2026-11-01
cargo +nightly-2026-11-01 install cargo-fuzz --version 0.13.1 --locked
cargo +nightly-2026-11-01 fuzz run lanes -- -max_total_time=120 -max_len=16384
```

These commands follow the [cargo-fuzz guide](https://rust-fuzz.github.io/book/cargo-fuzz/guide.html).
Nightly and cargo-fuzz are only needed for coverage-guided runs. Resolve the fuzz
manifest outside an offline sandbox with `cargo generate-lockfile --manifest-path
fuzz/Cargo.toml` if libfuzzer-sys is not cached. The stable replay remains usable.

| Target | Input | Assertions |
| --- | --- | --- |
| `document` | Arbitrary document bytes | Owned and borrowed parse/validation agree; serialization preserves value and conformance/version admission |
| `json_value` | Arbitrary JSON bytes | Owned and borrowed parse agree; exact round trip preserves text and semantic equality, or ambiguity for duplicate names |
| `decimal` | JSON `[number, divisor, optional third number]` | Equality, ordering, transitivity, sign reversal, equivalent spellings, division identities, integer remainder oracle |
| `pattern` | JSON `[pattern, string]`, or raw UTF-8 pattern | Prepare invokes the ECMAScript reader; verdicts agree across lanes and across string pattern and propertyNames |
| `lanes` | JSON `[schema, value]` | Default-limit evaluate and explain agree whenever both establish a verdict |

Inputs retain raw number tokens. Decimal receives only lexically valid numbers;
zero divisors are excluded from the arithmetic API's precondition. Unsupported
patterns, invalid schemas, and evaluations without verdicts are valid outcomes.
The stable replay requires at least one successful input per target, so it cannot
pass with a corpus that exercises only rejection. All targets cap input at 16 KiB;
this is a throughput choice, not evidence about larger inputs. Schema preparation
and evaluation use default limits, including the compiled lane's admission checks.
Route counters are private to unit tests; the generated unit differential guards
compiled coverage, while the external fuzz harness cannot count those entries.
Both lanes share some parsing and arithmetic, so agreement is not an independent
specification oracle.

Crashes, timeouts and OOM inputs go to `fuzz/artifacts/<target>/`. Reproduce with
`cargo +nightly-2026-11-01 fuzz run <target> <artifact-path>` and minimize with
`cargo +nightly-2026-11-01 fuzz tmin <target> <artifact-path>`. After diagnosis,
retain a named `.json` or `.bin` regression input in `fuzz/corpus/<target>/` for
stable CI replay. Hash-named generated corpus entries and artifacts are ignored.

The separate `Fuzz` workflow runs weekly and on manual dispatch. It runs every
target even after a preceding target fails, and uploads artifacts on failure.
Five sequential two-minute campaigns cost 10 runner minutes of fuzzing, plus
installation and compilation (estimate 15 to 20 minutes total on a cold runner,
30-minute job cap). This estimate has not been measured on GitHub Actions.
