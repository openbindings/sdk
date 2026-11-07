# Compiled literal scratch qualification

Run the counting global allocator in its own test binary:

```sh
python3 tools/literal-scratch/run.py
python3 tools/literal-scratch/run.py --release --test hostile_costs
python3 tools/literal-scratch/run.py --release --baseline --test hostile_costs
```

The runner copies the evaluator crate beneath `target`, links the other existing
workspace inputs, and builds a separate test target. Only that copied target
permits the unsafe forwarding required by `GlobalAlloc`. Product dependencies,
safe-code policy and public API stay unchanged. Cargo runs serially.

Allocation counts include one direct `Lane::outcome` call. Parsing, preparation,
printing and assertions are outside the window. Thread-local counters exclude
test-harness background work. Each call must record exactly one compiled value
entry and no construction entries. Failed outcomes own a fixed 33-byte message;
successful integer checks request zero bytes. Cold and repeated counts must
match at 1 KiB, 1 MiB and 16 MiB.

`--baseline` reads only the two guard files from `7fcc101` using `git show`.
`--bypass-estimate` disables the scratch flag only in the measurement copy, to
test the old pass before changing admission. It leaves all other guards intact.
The regular allocation run exercises the actual selection and new scan.

Hostile measurements retain prepared owners and parsed values, alternate nine
pairs against the forced Original path, compare complete outcome results, and
print nanoseconds and compiled entry counts. Run the baseline to establish
which path those same inputs previously took. Native timings do not establish
workerd or browser timings.

The Page route fixture is the nine-definition reached closure of `s464` from
`receipts/2026-11-05-production-worker/corpus/sources/intravox-schema-carrier.json`
in the parent migration workspace. Its internal references and schema values
are unchanged. The 73-byte generated witness is retained in the route test.
