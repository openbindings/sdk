# Decimal scratch measurement

The runner copies the evaluator crate into `target/decimal-scratch-snapshot`,
adds a private test binary with a counting global allocator, and uses the
workspace's other crates unchanged. Unsafe allocator forwarding is confined to
that copy. The SDK's unsafe-code lint, public API and dependencies are unchanged.
Keep `CARGO_BUILD_JOBS=4` and run commands serially with the requested toolchain.

```sh
python3 tools/decimal-scratch/run.py --release
python3 tools/decimal-scratch/run.py --release --baseline --test hostile_costs
python3 tools/decimal-scratch/run.py --release --test hostile_costs
```

`--bypass-estimate` is a diagnostic only. Other selection guards still apply.
`--baseline` reads only the guard files from `53ab0d4` with `git show`.
No git command changes state.

The allocator counts requested allocation traffic, outstanding bytes and peak
outstanding bytes in `Lane::outcome`. For a realloc it conservatively counts
both old and replacement buffers at the transition. It counts deallocation and
asserts no measured allocation remains live when the outcome is dropped. Parsing,
preparation, printing and assertions are outside the counting window. Every
sample asserts one compiled entry and a successful verdict. Both cold and warm
breadth samples run at exactly 1 KiB, 1 MiB and 16 MiB; increasing array breadth
must not change peak live bytes.

Length measurements include long significant fractions, mostly zero fractions,
and huge positive and negative exponent spellings. Small cases also use an
equal wide schema bound, exercising both comparison operands. Larger cases use
small bounds so schema preparation does not dominate the value measurement.
The default `allocation_proof` runs both breadth and token-length tests and is
wired into `tools/verify.py` in release mode. Length cases beyond the peak bound
first assert production declines without backend entry, then measure with only
the estimate disabled in the isolated test copy. Every peak must fit the same
`4 * (value token bytes + longest bound bytes) + 512` formula used by selection.
Timings use retained owners and parsed values. They do not measure parsing,
preparation, Wasm or workerd.

Hostile measurements report default-limit selected and Original outcomes plus
an independently prepared Original outcome with a test-only 2 GiB cumulative
allowance. Original can refuse long decimals
because its allocation allowance is cumulative. That refusal must not be
silently treated as a decisive verdict or an equivalent amount of completed work.

## Why four times the operand lengths is an upper bound

This bound covers requested live heap bytes for the SDK's exact decimal work,
including conservative old-plus-new realloc accounting, on the supported 32-bit
and 64-bit targets. It does not include the already prepared schema, parsed
value, allocator metadata or unrelated backend work.

- `backend::Number::as_str` borrows the value token. `numeric::Bound::admits`
  borrows its retained schema `Decimal`, parses one local value `Decimal`, and
  drops it before returning. `ExactBound::is_valid` retains no value scratch.
  Integer classification parses and drops one decimal without comparison.
- For valid JSON numbers, `Decimal::read` and `read_wide` handle the spelling;
  the generic `Wide::parse` fallback is not reached. The wide coefficient
  buffer has exact capacity at most the value token length. An inline
  coefficient converted for a wide exponent has at most 19 digits.
- `Power::parse` copies only significant exponent digits. Normalization adds
  or subtracts token-sized byte counts, never expands the exponent, and offsets
  are sequential. A huge exponent can grow by at most one digit in normalization
  and one more for the comparison's coefficient-length offset.
- In `Power::offset`, a huge exponent of length E can have its old boxed digits,
  one E-byte working vector and one result buffer live together. The carry path
  replaces the vector through a machine-sized prefix; while growing that prefix,
  the old prefix adds only a machine-sized constant. The vector is exact-sized
  initially and the prefix extends once by an exact-size digit iterator. Its
  resulting capacity is bounded by E plus the machine-sized prefix capacity.
  Boxing the result can overlap the working buffer, but cannot also overlap the
  earlier carry vector. Thus this phase needs at most three exponent-length
  buffers plus fixed overhead, not an additional copy per offset or token.
- During parsing, with coefficient length C and exponent length E, this gives
  `C + 3E + constant`. Since their written digits occupy disjoint parts of the
  value spelling V, this is at most `3V + constant`. During `Wide::compare`, the
  first adjusted exponent remains live while the second is built. The bound's
  original exponent is already prepared, so the live scratch is at most
  `C + 2Ev + 2Eb + constant`, bounded by `2(V + B) + constant`.
  Inline/wide comparison uses only one adjusted exponent.
- The fourth copy leaves room for capacity transitions. The 512-byte tail
  covers the `Wide` box (48 bytes on the measured 64-bit host), the few added
  exponent digits, coefficient formatting and the simultaneously live
  machine-sized signed offset/prefix strings (each at most 40 bytes). No BigUint
  operation occurs in comparison or integer classification.

All arithmetic in selection saturates. `B` is the maximum of all four comparison
keywords across the reached schema closure, including bounds below advancing
applications. The maximum conservatively covers every actual comparison.
Multiple keywords and value tokens execute sequentially, so neither their count
nor schema path weight multiplies live decimal scratch. The source loops make a
fixed number of linear digit passes; timings qualify their observed cost, not
an asymptotic claim inferred from samples alone.
