# Independent review of the guarded AST stage

Status: **approved to implement as a private, inactive stage**, with the corrected visitor work term. This is not full VM-construction approval or permission to activate eager compilation.

The reviewed authority is the exact guarded subset and regex-syntax 0.8.11 source. All 34 entries in source-pins.json match the locked local dependency. The work schedule is a conservative logical ledger of byte inspections/copies, scalar operations, and target-word transfers; it is neither an instruction count nor a deadline. Its use requires a fresh parser with the stated options, the guard applied to the same input, no diagnostic formatting, and the target layout checks.

## Source and error paths

I inspected ParserI::parse_with_comments, bump/bump_if/peek, group/alternative stack operations, primitive/escape/hex/count parsing, class construction, HeapVisitor, NestLimiter, and Ast/ClassSet destruction. The guard excludes named-capture storage, lookaround/property parsing, comments/whitespace mode, nested class algebra, specialty-boundary rewinds, and ASCII-class speculation. Every remaining source loop advances its byte position or terminates. Numeric scratch is shared, cleared without shrinking, and receives only a disjoint subset of the input digits.

The one-error-source-copy claim holds under this guard. In parse_counted_repetition, the generic replacement errors after parse_decimal require EOF or a missing closing delimiter. The guard rules these out. With a comma and a second bound, an overflowing first bound is propagated before reading the second bound; at-least/exact forms likewise propagate it. Other parser errors return immediately. NestLimiter is reached only after successful parsing. The stage must discard the backend error directly; formatting it would introduce an unreserved operation outside this certificate.

## Work coefficients

The source-byte allowance covers monotone cursor/escape/digit access separately from fixed group-prefix probes. The 128-per-byte/token-context scalar allowance applies only to the closed parser branches: cursor/position updates and checked arithmetic, dispatch/EOF/whitespace-disabled tests, fixed primitive/range decisions, and bounded flag/capture checks. Loops over digits or class members are charged by their consumed source; vector movement is charged separately. This is a source-site allowance, not a claim that an arbitrary future parser branch fits 128 operations. A dependency/guard/options change reopens it.

The six typed-transfer factor covers local construction, result/enum transfer, and boxed installation. Vector terms separately cover insertion/removal and less than four times total inserted initialized elements relocated during growth; no capacity slack is charged as written work. Empty replacements and class primitive/range conversions are additional named terms. Child AST values move; descendants are not cloned. GroupState/ClassState and visitor element widths must remain actual target layout values.

I requested that the visitor scalar term change from 12 to **32*(N+r+k)**. The corrected term explicitly covers up to 16 enter/descend operations and 16 ascend/continuation operations, including HeapVisitor::induct/pop/child and NestLimiter's depth checks. The original 12 coefficient omitted that grouping and could only rely on unrelated slack. The correction is now present in both document and executable schedule. Typed frame transfers remain separate.

The destructor terms are coherent: each non-temporary node is transferred to one work stack once; child extraction substitutes an empty leaf. Subsequent wrapper destruction sees only the emptied child collection. The initial match, stack dispatch, child extraction/drain, and final emptied-wrapper dispatch fit the eight-per-invocation/node grouping. Flat ClassSet cleanup similarly has one union drain, without recursive binary/bracketed subtrees. Empty payloads and their transfers are charged explicitly. Early parse/visitor failure yields disjoint partial forests, not a second traversal of the same owned descendants.

## Evidence

Parent event and allocation records cover 4,287 guard-admitted inputs: 3,355 successes and 932 representation/nesting refusals, with no exceeded event bound, allocation undercharge, or retained allocation. The minimum-Rust and actual Wasm32 records independently retain target results. These observations challenge the source argument; they do not replace it.

I added twelve independent controls in independent-inputs.jsonl and replayed the actual guard/instrumented allocation probe. They cover overflowing first/second/exact/open bounds, a 10,000-digit all-zero count, early nesting refusal, temporary empty branch/group roots, flat ranges, repeated quantifiers, empty input, and the fixed boundary group. All twelve are guard-admitted; all requests stay below reservation and all owners are released. The overflowing-first-bound case requests only one source-copy-sized error allocation. Results and the executed probe hash are in independent-results.json.

The corrected 1,024 tiny-program AST work sum is 5,270,528 native and 5,331,968 Wasm-width units. Combined with the current separate HIR/NFA proposals, this leaves finite aggregate headroom; Reader, metadata, Plan construction and actual target retry schedules still need the full aggregate gate. No cap is raised or semantic result reclassified here.

## Required implementation boundary

The stage design is accepted with these requirements:

- Inspect the exact form internally; do not accept an externally paired inventory. Compute checked costs before parser creation and charge both ledgers before dependency entry.
- Check existing cancellation/sticky failure before arithmetic and again after parsing/cleanup of the parser. A completion refusal must drop the AST and error under the prepaid cleanup allowance.
- Keep backend representation/nesting refusal distinct from enclosing Attempt exhaustion, and never turn either into an Undefined atom.
- Return only the owned AST and exact inventory; retain no control, parser, source text, formatted diagnostic, or partial shared cache.
- Use size_of for public layouts and explicit pinned target tables for private layouts. Unsupported layouts must refuse before parsing.
- Include zero/exact/one-below/overflow/prior-failure and post-parse cancellation tests with a no-parser-entry witness. Keep this stage inactive until HIR/NFA, physical stack and full aggregate/default-workload qualification close.

No production source was edited by this review.
