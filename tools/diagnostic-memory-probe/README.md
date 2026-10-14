# Pinned-schema first-error memory regression

Run `python3 tools/verify-diagnostics.py` after sourcing the offline program environment.
The standalone crate isolates allocator instrumentation from the safe SDK workspace.
It warms both fixed validators, parses outside the measured region, then measures
peak live allocation above baseline for one `fixed_schema::check` and drops its
error/finding before the next check. The 4,096 finding cap remains in force.

The same cases run with 1,000 and 100,000 array elements. They cover the document
schema's JSONSchema union, an opaque literal plus a failing sibling, localized
meta-schema children, opaque meta-schema literals, and the meta-schema type union.
A 256 KiB ceiling detects growth rather than treating input-proportional copies as
an acceptable pinned-schema guarantee. An unpinned two-branch items/anyOf negative
control must exceed the ceiling and 16 times input bytes; its validity call is
measured separately. Numbers are allocator peaks, not RSS or retained-string sizes.

S8a currently exposes amplification in the existing pins. The regression is
intentionally failing pending Claude's correction of the accepted premise; it must
not be relaxed or presented as a passing qualification gate. No vendored source is
changed by this tool.
