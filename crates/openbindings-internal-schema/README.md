# Private schema embedding adapter

Implementation support for OpenBindings §7 projection, resource policy and original provenance. No bounds planner or paired evaluation is compiled into this package. Its Rust surface is an implementation seam, not a product API. Resolution performs no acquisition.


Document findings are independent of the value diagnostics mode. OBI-02 and OBI-10
use the library's validity API only, with the fixed message
`value violates the fixed normative schema`, their rule, and original source
coordinates. OBI-02 follows failing object members, member names and array elements
through the pinned schema's compiled validator map and local pointer references.
It reports each deepest failing position under the findings cap. Missing members,
container constraints and unsupported descent constructs stay at their container;
an empty walk falls back to the root. OBI-10 reports the schema node checked by its
localized walk. Location work shares the existing work budget; stopping retains
the deepest established failing node and marks findings truncated. Locations and
missing detail never change validity or rule evidence. No document opt-in, size
gate, library error construction or active diagnostic patch is involved.

Migration: document findings keep source locations, lose keyword-specific wording,
and can now contain several positions. OBI-10 uses the checked node rather than a
keyword within it. Use rule, code and evidence for logic, not message text or
finding count.
