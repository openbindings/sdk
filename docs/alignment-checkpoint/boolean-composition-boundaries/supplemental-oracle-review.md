# Supplemental Boolean workloads — static oracle review

The three proposed semantic expectations are sound under the current standard profile and absent external resource. This review inspected the actual input file and current contains/conclusion code; it ran no builds, tests or measurements and changed no SDK source.

| Job | Independent reasoning | Required outcome |
| --- | --- | --- |
| `bc-repeat64-range-impossible` | All 64 semantically equal zero values reach the same original referenced predicate in the same scope. They therefore share its truth choice. The count is either 0 or 64, neither within 1..32. | Fails |
| `bc-repeat64-minimum` | External schema `true` satisfies the minimum; external schema `false` fails it. Unavailable content is not a known Undefined result. | NoVerdict, ResourceUnavailable |
| `bc-distinct16-range` | External schema `{"enum":[0]}` gives one match and satisfies 1..8. External `false` gives zero, and external `true` gives 16, both failing. Missing content leaves the result unsettled. | NoVerdict, ResourceUnavailable |

`unevaluatedItems:true` preserves these validity results while demanding annotation processing: whether an item is already evaluated or the true child is applied, its remaining constraint succeeds. It does not break the equal-value correlation or make unavailable content a known undefined assertion.

The current status-only `no-verdict` expectations are weaker than this reasoning: they could accept a new LimitExceeded or UnsupportedCapability refusal and conceal a capacity regression in the supplemental measurement. Tighten the two unsettled jobs to ResourceUnavailable through the adapter's supported expected-outcome shape. Exact message wording or optional locations need not be frozen for this purpose.

These are supplemental jobs. They do not replace any original G6 request, expectation, exclusion or comparison, and this preflight grants no performance acceptance.
