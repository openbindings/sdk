# Numeric cache policy clarification

The implementer selected 256 bytes per eager token, 65,536 aggregate eager source bytes, and 1,024 original-node records. This is accepted as a private bounded optional-cache policy. Each record has at most 13 active predicate slots (five numeric, six sizes, two non-orphan contains). Reserve record/map/vector and invalid-marker ownership under that complete bound, not only token bytes.

The earlier 64-byte/1,024-operand figures in the independent design review were illustrative policy recommendations, not normative or soundness requirements. Count-first selection does not reorder runtime evaluation. Raw fallback is selected before optional charges; no failed charge may be caught to continue. Schema-dependent comparison allowances remain because the retained Bound allocation witness remains applicable. No defaults increase; large unused and orphan operands remain default-admission controls.

This confirms design only. Implementation, exact/one-over policy boundaries, ownership, cancellation/retry, and allocation qualification remain required. See the final current-public-documentation-review receipt for the contemporaneous design-file hash.
