# Narrow integer predicate: bounded design approval

The proposed direct predicate is sound and appropriately small. Approve it for implementation review with the existing admission sequence unchanged. No build, test, benchmark or SDK edit was performed for this design review. Source identities are pinned in `source-hashes.json`; `controls.json` freezes 52 independent exact-token answers and nine control groups.

`typed("integer")` already checks that the instance is a Number before calling `number(...).is_integer()`. That Number has passed JSON token syntax admission. Successful `text.parse::<i64>()` therefore proves an exact signed integer, including `-0`; there is no binary floating-point conversion or safe-integer restriction. Every such token also fits the existing inline Decimal path: at most 19 significant digits, zero normalized to integer zero, and no written fractional/exponent component. Returning true removes the redundant Decimal construction and classification without changing its answer.

Keep this exact sequence in the new helper:

1. Borrow the original admitted number token.
2. Call `graph.work(text.len().saturating_mul(8).saturating_add(1))?` before the first parse, exactly as today.
3. Parse i64 once. On success, return true immediately.
4. On failure, execute the unchanged `graph.charge(text.len().saturating_mul(16).saturating_add(256))?`, then return `Decimal::parse(text).is_integer()`.

Do not call `number()` again after failed i64 parsing: that would duplicate work, parsing and checks. Do not add a post-parse control check. The old Decimal constructor/classifier contains no Graph or callback boundary, so removing it for the admitted subset leaves the complete externally cooperative sequence unchanged. Preserve overflow/saturation behavior, shared preparation charging and first-cause propagation. A low work allowance or cancellation still wins before even a token as small as `0` returns true. Fallback reservation applies even when a fractional/exponent spelling ultimately uses inline Decimal storage.

The typed caller keeps its existing Number short-circuit, type-name decoding and error behavior. Wrong instance kinds remain false without number work. Unknown/non-scalar type names and wrong operand shapes keep their current local inability or standard-meta admission behavior. The type-union loop remains eager: matching an earlier `integer` must not skip a later invalid name. No other primitive, const/enum/unique comparison, count reading, numeric bound, effect identity, diagnostic origin or original source owner changes.

Prefer the small direct predicate to a new record/cache/closure abstraction. The two short preflight formulas will be duplicated with `number()`; a nearby comment should identify the intentional shared admission policy, and differential tests should enforce identical charge/check traces. A helper solely to avoid those few lines is not necessary for correctness and may add another runtime boundary to the hot path. Reconsider extraction only if the policy grows or measurements justify it; do not change that architecture in this increment.

The optimization remains deliberately narrower than recognizing every plain integer token. `9223372036854775808`, longer plain integers, `1.0`, `1e0`, `1.5e1`, huge positive/negative exponents and zero with a huge exponent all retain Decimal fallback and its exact reservation policy. Their mathematical answers are frozen independently. Existing backend integer classification has a broader plain-token shortcut, but that does not authorize silently broadening this evaluator change or dropping its current charges.

Qualification should compare the old direct `number(...).is_integer()` path and new predicate on the same admitted value with actual ordered work/storage/control traces, exact/one-below limits, earlier spent allowance and every observed interruption. Reuse complete public Outcome/Explain observations and locations. Check actual zero-allocation i64 behavior and unchanged fallback cleanup. Current/minimum builds and selected unchanged array/object/scalar workloads then establish whether this removal helps. The prior ordinary/forced keyword-hint experiments were discarded and remain separate evidence. This design does not accept a performance residual or close G6.
