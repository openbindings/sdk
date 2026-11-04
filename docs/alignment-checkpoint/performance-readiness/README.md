# Performance qualification checkpoint

Candidate SDK source is `df06588`, compared with the pristine pinned `18a66957`
baseline. Both archives use Rust 1.99, identical timed drivers and their authentic
SDK dependency closures. The independent review found a harness readiness bug:
non-cancellation preparation errors counted as ready. Both adapters now require
successful preparation; four fresh binaries report verified source digests. The
current executable has ten passing protocol tests, including refusal rejection.

All 42 request workloads remain. Their independently adjudicated expectations leave
27 workloads / 82 modes strictly comparable and15 workloads / 19 modes observation-only.
Changed diagnostics, document metadata and retired policy outcomes are never used
as equivalent-work performance comparisons. No baseline workloads were removed.

The pilot has 101 rows and zero outcome errors. It flags 53 rows for one or more
memory metrics. Several small cases reflect a larger retained baseline; large
literal equality, uniqueness and repeated-reference paths have substantive extra
allocations. These findings remain open. Two pairs / one batch / .03 seconds with other
validation active are diagnostic sampling, not a timing qualification. No new
performance residual has been accepted.

A separate diagnostic-memory replay exposed default expanded-pattern preparation
capacity regressions. The original four-pattern and 380 repeated-pattern positives
remain required. Explicitly funded observation lanes isolate later limits but do
not replace those default assertions. The proposed negative expectations were
rejected and the original probe was restored. Four counted names, 380 long literal
patterns and 100 ambiguous searches retain their original positive outcomes.

See pilot-summary.json for bounded allocation evidence and the copied independent
protocol/fixture/dependency audits. The coordinator retains complete raw command,
comparison, build and archive receipts. This checkpoint is not a whole-SDK pass.
