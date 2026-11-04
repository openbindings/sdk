# Metadata work and storage schedule

Status: source-derived proposal for independent review, inactive implementation.
These units bound the named source operations and word transfers; they are not
CPU instruction, wall-time or machine-stack measurements. No default-budget pass
is claimed by this local schedule.

The current representation has one walk-owned additive collector. Per-child state
is only Shape plus descendant a/r/b. n/u/k, all eleven work counters, allocation
site totals, prefix call totals and maximum heights are added once at their local
constructor, never copied/merged at every ancestor. This preserves the independent
recursive oracle's root results. Zero/exact-one removal still visits and charges
its original descendants before simplification.

Each iteration does one node dispatch or accepts one completed child. Neither
helper loops over children, class members or ranges. `Frame::next` fetches one
borrowed child. Flat class count is the existing slice length; only its possible
singleton is examined. The closed guarded AST establishes the class grammar.
The only input-dependent storage is the ancestor frame vector; each frame owns
references and scalars, no child vectors, strings, ASTs or control.

| Charged batch | Work units | Source obligations |
| --- | ---: | --- |
| Entry/root completion | 3 × words(Summary) + 2 × words(Subtree) + 32 | Totals initialization, return payload and caller extraction; root completed-slot extraction and Shape/a/r/b copy; fixed cursor/control/vector ownership. |
| Node dispatch | 96 | Count one AST node; classify its variant; initialize one fixed Subtree/frame fold or inspect one leaf; fixed scalar encoding/width facts. |
| Child acceptance | 128 | Transfer one completed Subtree; three checked lineage adds, width/height folds, one root-kind branch, concat lexical-run handling or alternate frontier/range/eligibility facts. Unary finishing occurs here instead of merging additive totals. |
| Unary completion | 32 | Return one already completed scalar subtree and dispatch to its parent. |
| Concat completion | 128 | Checked local input/visit/literal site contributions and allocation formula; zero/literal/single-child/root shape selection. |
| Alternate completion | 512 | Checked calls/frontier/range/rebuild formulas, attempted-equality charges, optional sort/range terms, local totals additions and resulting root/height. Each formula has fixed arity; no sort or HIR work runs here. |
| Final phase schedules | 1,024 | Compute and checked-sum 21 work and seven allocation sites, including width conversion and fixed-array transfers. |
| Frame ownership | 3 × ceil(sizeof(Frame)/word) per pushed frame | Push, pop/completion transfer and eventual scalar cleanup; paid before push. Node dispatch covers initial fixed-field construction. |
| Frame growth | initialized length × ceil(sizeof(Frame)/word) | Copy only initialized frames during exact geometric backing growth. |

Publication into Inventoried follows the final 1,024-unit batch; its fixed
Summary/Parsed/scalar transfer belongs to that batch. Earlier entry/root moves
are prepaid at entry, never borrowed from that later reservation.

These fixed ceilings are deliberately separated from input-dependent counts. They
must be audited against the current helper bodies; replay agreement alone does not
prove them. A source edit adding a loop or dynamic ownership requires a new site
schedule, not an assumption that this fixed budget still covers it.

Before reserve_exact, the implementation computes capacity=max(4,2*old_capacity)
with checked arithmetic, charges the initialized copy work, and charges the entire
new capacity × actual target Frame width. Requested cumulative backing is therefore
covered even if realloc moves; allocator physical overhead is outside the SDK's
requested-byte model. Every initialized frame has prepaid cleanup. Wide shallow
ASTs use constant frame depth; deep ASTs grow geometrically. An allocation overflow
uses the same sticky allocation failure path; work arithmetic overflow first checks
for a prior failure/cancellation before poisoning the work ledger.

All successful and failed final schedule sums are checked before publication. The
owned Parsed AST accompanies its completed certificate. Failure consumes and drops
that AST under the AST stage's separate prepaid cleanup; an isolated inventory
allocator measurement must not incorrectly charge AST cleanup to frame storage.

The first implementation's per-child additive totals and observations remain in
initial-subtree-totals/. Its known frozen-1,024 metadata work was 6,945,792 and the
provisional AST+HIR+NFA+metadata sum exceeded the old 20M default before remaining
Reader/guard/Plan work. The global collector removes real repeated operations and
storage; new measured aggregate defaults remain a separate gate.
