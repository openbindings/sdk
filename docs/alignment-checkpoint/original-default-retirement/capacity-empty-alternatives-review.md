# Bare empty-alternation capacity correction

Bounded source/native PASS, separate from the scalar/fixed-meta correction. The existing actual-Wasm `empty-alternatives-10000` positive was not among the explicitly superseded structural attacks. An empty alternative matches every string, so both original `""` and `"x"` expectations remain S. A new local Size refusal was a capability regression to fix, not a new semantic oracle.

The former general AST bound charged `3*alternatives + 2`. In the guarded normalized subset, a Summary equal to its default except `alternatives` proves the input is empty or consists solely of `|` separators. This condition also excludes flags, depth, classes/ranges, repeats, assertions, literals and numeric spellings.

Pinned regex-syntax0.8.11 `push_or_add_alternation` appends each completed empty Concat as one Empty. `pop_group_end` appends the final Empty and constructs one Alternation. For n>=1 separators, actual AST nodes are exactly n+2. Empty input contains one Empty, conservatively bounded by2. No concatenation node or nested group is retained in this case. The production fast branch uses checked addition and otherwise retains the general bound. All normalized/profile/aggregate/translation/NFA checks and the16,384 AST ceiling remain.

Reviewed controls cover0/1/2/10,000/16,382 separators, parser AST equality, node shape/count,16,383 refusal before parser entry, and exclusions for escaped separator/group/literal/anchor/class/repeat. The independent actual public API probe adds36 observations: six sizes × empty/string/non-string × Outcome/Explain. All string cases through16,382 satisfy;16,383 returns local `LimitExceeded/pattern-size-limit`; a non-string does not demand the limited pattern and satisfies. Exact hashes, command and output are retained.

This is a corrected conservative bound, not a raised cap or altered match semantics. Fresh parent Wasm/browser qualification remains separately required. No new production edits were made by this reviewer.
