# Performance repair verification

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. Input: `claude-performance-input.txt`.

## Scope

**Read, in the current working tree:**
- `crates/openbindings-openapi/src/security.rs`;
- the grant cases in `examples/openapi-json-path-consumer/src/auth.rs`;
- `examples/openapi-json-path-consumer/examples/auth_measure.rs`;
- the header and claims of `auth_diagnostics.rs`.

**Confirmed read-only with `git status`/`git diff`:** core and the role crates are unmodified, and the client is still pinned to `fe09522` in both `Cargo.toml` and `Cargo.lock`.

**Not done:** I ran no builds, tests or benchmarks, because CPU timing is in progress. Every statement below comes from reading the code. I make no claim about the revised CPU run. Run 1 remains a recorded failure.

## Verdict

**Accept.** I found no semantic or resource regression in the four changes.

## Findings by change

### 1. Grant normalization

`matches_target |= uri::origin(raw).map_err(..)? == origin` (security.rs 167–187):
- normalizes each grant exactly once;
- applies `?` to every element, so a malformed grant after a matching one still refuses. `|=` on `bool` does not short-circuit;
- still refuses an empty array or a list with no match.

The full supplied list still reaches `Credential.origins`, so the client's check of the actual request origin is unchanged.

`auth.rs` 389 adds the matching-then-malformed case. That case sits in the context loop, which asserts `ERR_REFUSED` with zero host entries.

This behaves the same as the previous code, minus one `origin()` call per grant.

### 2. Lazy choice inventory and selection

The inventory is now a local `OnceCell` (64–77) that is built only through the challenge closures. Its sorted, deduplicated content and the challenge identity are identical to before.

**Explicit selector:** `position` (88–96) returns the first alternative whose sorted key set equals the selector's sorted `BTreeSet`. That is the lowest index, matching the old map-with-`rev()` lookup.

**Missing selector:** inference happens only when every alternative has the same key set as `security[0]` (97–105). That is equivalent to the old "one distinct choice" rule.

**Indexing `security[0]` is safe.** The guard at line 46 has already returned for profiles with no non-empty alternative, so the vector is non-empty here.

**No secret is cached.** The cell is per call and holds only source names and indexes. Nothing is held across an await, because `apply` is synchronous.

### 3. Public consumer regression

The matching-then-malformed origin case is present. The reordered, duplicate-alternative and anonymous selection cases are unchanged.

### 4. Measurement harness

`Oracle` checks, on the first warm-up call of each condition:
- the exact target and body;
- for `authorization` and `x-key`, the exact header counts and values;
- exactly one dispatch.

These checks run on both the direct path and the adapter path, so both are shown to do equivalent work. The inactive-flag cost (`Cell` read and replace) is identical on both paths.

`auth_diagnostics.rs` labels its rows `diagnostic` and claims no direct denominator for coverage.

## Notes (non-blocking)

- **Keep the two CPU runs separate in the record.** The revised run measures a different candidate. Record Run 1's failure (3.0.4 AND 64 KiB, first process pair, 8109 ns against a 6582 ns ceiling) separately from the new registered run, rather than replacing it.
- **The oracle only checks the first warm-up call.** A regression that appeared only in later iterations would go unseen. This is acceptable for a timing harness, because correctness is covered by the functional suites.
- **The acceptance audit's subcases are still open.** The missing literal subcases it identified (cross-kind auth isolation, conjunction omissions, wrong-server grants, auth resource failures) are outside this narrow review and remain open until their focused tests exist.
