# Direct exact pointer construction for original explanations

Serialization update (2026-11-05): the original design below predates the safety
fix. Stored pointer units and lookup remain exact. Default Rust serialization
replaces unpaired units with U+FFFD; the private Wasm metadata encoder preserves
exact JavaScript pointers. See [the API guide](../api-reference.md).

The Explain review found that an exact pointer through an unpaired UTF-16 name
allocates temporary decoded segment vectors and speculative scalar text. Its
128-segment witness requests 6,443 bytes for a 258-byte retained pointer. The
implemented direct writer now requests only 258 bytes for that witness. Parent
native/MSRV/actual-host gates and independent implementation/resource review pass.
This qualifies the construction helper; the diagnostic caller's bounds and
ancestry checks still need implementation. The original design follows.

Keep the existing scalar path's measured backward writer. When that path discovers
an unpaired name between the selected node and selected ancestor root, measure the
escaped UTF-16 unit count directly from borrowed arena names. Allocate the final
unit buffer once at that exact length. Walk the same ancestor edges backward,
write index digits backward, and fill each escaped name into its measured final
segment. Read scalar names with encode_utf16 and unpaired names with the arena's
existing nonallocating unit iterator. No ancestor vector, decoded-name vector or
numeric String is needed. Checked arithmetic and caller precharges precede bounded
diagnostic materialization; the separate layout/ancestry helper must not confuse
the retained instance root with the source's absolute root.

Add a private constructor for an already escaped buffer known to contain an
unpaired unit. It directly selects Pointer's Units representation, avoiding the
current speculative String::from_utf16 allocation. Debug assertions check the
escaped syntax and the unpaired invariant without allocating. Scalar paths,
including a subtree below an unpaired ancestor outside its relative path, still
use Text. This is an implementation mechanism, not a new specification concept
or a new public API.

Preserve exact lookup and serialization for empty/root pointers, empty member
names, multi-digit indices, escaped tilde/slash, paired astral names, lone high/low
units and mixed paths. The chosen root must be an ancestor in the same arena before
using the unchecked internal ancestor walker. Measure actual requested allocation
and cleanup against the independent before probe at depths 8/32/64/128, including
retained subtrees. If Vec-to-box conversion requests replacement backing, include
it in the construction bound; do not report final live size as cumulative requests.
Run the core exact-pointer tests and actual native/Wasm lookup and serialization
gates. This correction alone does not implement witness propagation or diagnostics.
