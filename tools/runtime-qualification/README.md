# Controlled runtime qualification

This is a public-API caller harness for C1–C5 of the SDK adoption plan. It changes no SDK APIs. `protocol.json` is the frozen local assisted-pilot budget and sampling contract, including product rationale, host scope, noise handling, and the single permitted inconclusive rerun. Passing baseline timings do not imply acceptable performance on every workload or host.

## Inputs and reproducibility

Use Node22+, Python3, Rust1.99.0, an unpacked `npm pack` archive, its original `.tgz`, and installed Chromium/WebKit plus their matching Playwright driver. The baseline used Playwright1.61.0 with Chromium revision1228/WebKit2311. Use real workerd and its matching `workerd.capnp`; no Node compatibility flag or mock workerd is used. All paths are supplied by the caller. Historical design directories are not runtime dependencies of these scripts.

```sh
python3 tools/runtime-qualification/fixtures.py /tmp/sdk-fixtures
cargo build --release --locked --manifest-path tools/runtime-qualification/native/Cargo.toml
export PLAYWRIGHT_MODULE=/absolute/path/to/playwright-core/index.mjs
export WORKERD_BIN=/absolute/path/to/workerd
export WORKERD_CAPNP=/absolute/path/to/workerd.capnp
python3 tools/runtime-qualification/compare.py --self-test
python3 tools/runtime-qualification/campaign.py \
  --package /absolute/unpacked/package --archive /absolute/sdk.tgz \
  --fixtures /tmp/sdk-fixtures --source /absolute/sdk \
  --native tools/runtime-qualification/native/target/release/sdk-runtime-qualification \
  --regression tools/runtime-qualification/native/target/release/runtime-regression \
  --output /tmp/sdk-check --mode check
```

`check` executes assertions with harness timers disabled. It is suitable for deterministic correctness validation, not a latency gate. Optional `BROWSER_EXECUTABLE` selects an installed browser binary; normally the matching Playwright driver resolves its own binaries. Scripts require loopback listening and browser/workerd subprocess permissions.

Before measuring, compile everything and reserve an exclusive local window with every active SDK worker. Run the same command with `--mode measure --quiet-window OWNER-CONFIRMED-ID` and a fresh output directory. Nothing silently overwrites an earlier campaign. The process snapshot catches active compilers, but cannot establish host isolation by itself. No latency gate belongs in shared CI without demonstrated stability.

```sh
python3 tools/runtime-qualification/compare.py /tmp/baseline > /tmp/baseline-budget-report.json
python3 tools/runtime-qualification/compare.py /tmp/candidate /tmp/baseline > /tmp/candidate-comparison.json
```

A candidate uses its own exact archive, source, and separately built native binaries. Keep the same fixtures/protocol/host/runtime versions; record any amended harness and invalidate affected evidence. For the historical twelve regression jobs, additionally use `paired-regression.py` with both preserved native binaries so the same quiet campaign alternates baseline/candidate order across jobs.

## Work and observations

Three tiers use8/128/900,000 integer-array entries; the representative and near tiers have16 operations. The near document also has a900,000-entry extension array:90% of the default1,000,000-node admission limit, not a claim of approaching every independent limit. Its valid value reaches the default evaluator work limit and returns `no-verdict`. Report that as refusal latency. Do not raise limits or count it as successful validation throughput. Small and representative values satisfy. Invalid values establish failure, with256 retained problems/incomplete in the near case. The supplemental32KiB-key/257-property witness preserves diagnostic output amplification from the earlier investigation; changed diagnostic caps change output volume and cannot justify an equivalent-output speedup claim.

The serial runners distinguish parse, assess, context+preparation, exact value admission, first validation, retained validation, invalid validation, JSON serialization, and cleanup. `complete` sums the document parse/assessment/context+prepare/value admission/first validation stages and excludes report serialization and cleanup. Throughput is single-caller retained-call throughput, not an HTTP server capacity claim. Seven batch averages give medians and descriptive tails only. File reads, fixture transport to the harness, and assertion/observation construction are outside stage timers.

Cold Node trials use fresh processes. Cold browser trials use fresh browser processes and real module Workers. They distinguish module import, local asset read/fetch, Wasm compilation, and instantiation+initialization. These are local cached-filesystem/loopback measurements, not internet transfer measurements. Each asset records raw/gzip9/brotli11 bytes and a separately labeled10Mbps+100ms transfer model. Native cold observations include the complete small valid/invalid harness workflow plus process startup. workerd imports a native Wasm module; initialization happens inside `fetch`. workerd's internal clock does not establish CPU timing, so its cost observations use external loopback request durations. They include HTTP overhead and are not deployed Cloudflare latency or quota evidence.

The browser harness URL-resolves the delivered Worker example's bare SDK import (the bundler's normal job), then runs the delivered `WorkerOwner`/`createWorkerView` logic unchanged. It tests latest-edit suppression, malformed-edit recovery, one-shot editor roundtrips, and a10ms main-page heartbeat after Worker initialization. Browser Worker computation does not run on the main thread. Unicode editor-coordinate mapping is a separate consumer-example gate; these baseline Workers do not claim to implement it.

The lifetime control performs1,000 retained validations and200 alternating document/resource replacements, checks released owners at20/50/100/200 replacements, and checks old retained contracts after parent release and preparation-cache eviction. All complete paths are warmed before recording the comparison count; eight fixed schema arenas are expected on this baseline. Arena counts are not handle counts, allocated-byte counts, RSS, or Wasm memory capacity. Node process RSS/heap/external bytes and native peak RSS are observations only; the public facade does not expose Worker/Wasm capacity. A stable owner count is not a total-memory or no-leak proof.

C4 includes discovered-document inspection and deterministic disposal. Node uses an injected Fetch Response; browsers use actual loopback HTTP. The near case explicitly raises HTTP decoded-body admission to64MiB so its1.8MiB document can reach the parser. No WAN claim follows. Native C4 timing remains unqualified; native archive-consumer discovery correctness is a separate gate.

The six byte-identical historical fixtures regenerate to their preserved SHA256 identities, retaining the original twelve jobs and exact15% OR +10µs healthy/contract / +1ms diagnostic allowances. Both adoption variants already have the8MiB document pointer cap, so the observer expects it on both sides. Raw/gzip Wasm growth retains the original15% allowance. These thresholds do not turn the old jobs into a general speed ranking.

## Evidence contract

Each campaign records source commit/status, archive/native/harness/protocol/fixture hashes, host information, commands, stdout/stderr/exit receipts, all samples, verdict/count/completeness/wire-size observations, cleanup checkpoints, and limitations. Missing or incompatible inputs fail; noisy metrics remain inconclusive. At most one complete rerun may follow a declared inconclusive campaign, preserving both. Comparisons reject differing fixture/protocol/host identities and browser/workerd versions. The owner still reviews semantic differences and final candidate identity.

This harness measures macOS native, Node22, named Chromium/WebKit desktop Workers, and local workerd. Linux/Windows native CI remains required; their timings are unmeasured here. Firefox, other JS hosts, mobile/second-device behavior, arbitrary custom evaluators, and deployed Cloudflare are unqualified. No broad performance grade is generated.
