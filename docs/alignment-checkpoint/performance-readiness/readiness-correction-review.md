# R-PERF1 correction: bounded closure

The parent changed `Prepared::is_ready` to `self.0.is_ok()` in both explicit
matched adapters and the current benchmark adapter. Independent source inspection
confirms all three use the actual preparation result. The SDK archive sources,
request payloads, side-specific expected outcomes and timed phase boundaries are
unchanged by this correction.

`readiness-correction-controls.json` records 16 executions of the actual four
fresh binaries: baseline/candidate, ordinary/allocation instrumentation, each in
compile/context mode with a positive `true` schema and the same schema under
`schema_resources: 0`. All eight positives counted one prepared owner. All eight
negative controls failed at the timed `is_ready` assertion; no refusal was counted
as successful preparation. The empty expected object in these deliberately
negative harness controls isolates the readiness guard after initial observation;
it does not replace or relax any of the real workload expectations.

`readiness-provenance.json` independently recomputes the build.rs FNV input-tree
digests and compares them with each actual binary's `--provenance` response and
the refreshed parent `instrument-pair.json`. All four match. Their actual SHA-256
binary hashes also match the parent metadata. The timing/allocation binaries share
the expected source identity per side; adapters intentionally differ across SDK
versions.

R-PERF1 is closed for this exact paired build. This addendum supersedes only the
open readiness paragraph in `protocol-review.md`; it does not replace the initial
failing evidence or grant a performance PASS. The parent's short overlapping-build
pilot is advisory, and its allocation flags remain real open G6 findings. No
residuals, mode exclusions, expectations, generic comparison gate or measurement
threshold were waived by this review.
