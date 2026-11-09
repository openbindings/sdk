# Pinned identifier property ranges

This offline developer tool reproduces the private ID_Start/ID_Continue arrays
from regex-syntax 0.8.11's public HIR API. They use that dependency's existing
Unicode 16.0.0 data. Runtime membership performs binary search over constants;
it does not initialize a parser, regex engine or heap cache.

The required portable verification path runs:

```sh
cargo run --locked --manifest-path tools/identifier-ranges/Cargo.toml -- --check
```

After deliberately reviewing a dependency/data change, regenerate with `--write`
in place of `--check`, then run the range/membership, grammar and resource gates.
Ordinary builds never regenerate the table. The generated file keeps exact bytes
on every checkout through `.gitattributes`; the checker does not normalize drift.
The copied source carries the upstream Unicode data license beside it.

This tool is private development support, outside the SDK product workspace. It
does not define new ECMA property aliases or certify unrelated pattern reader/
compiler work. Group grammar still handles its explicit dollar, underscore and
join-control additions and compares decoded names for duplicates.
