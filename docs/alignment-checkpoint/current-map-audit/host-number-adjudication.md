# Host exact-number witness adjudication

The initial `host-negative` sketch said “Unsafe JS number silently accepted as
exact input.” Read as a blanket `Number.isSafeInteger` admission rule, that is
an obsolete assumption, not a requirement of pinned specification §5.

The independent reviewer approved this correction on 2026-11-04: ordinary ingress
uses the supplied finite Number's JSON serialization. It cannot recover a numeric
spelling already lost before SDK ingress. This does not assert equality between
the Number's binary rational and every decimal spelling. Exact JSON conversion
back to an ordinary value must still pass exact round-trip comparison.

The clarified negative is: do not silently round exact JSON during conversion,
or equate an already rounded ordinary input with the original exact token. No
blanket rejection of every integer outside the safe-integer range is justified.
This is a correction to the initial witness, not preservation of an old API.

The new public TypeScript test `ordinary number serialization stays distinct from
exact JSON conversion` establishes all four controls requested by the reviewer:

- Exact `9007199254740993` returns `inexact` on conversion.
- Exact `9007199254740992` converts successfully.
- Ordinary `9007199254740992` fails `const:9007199254740993`, while that exact
  JSON token satisfies the schema.
- Exact `1000000000000000128` cannot silently convert to the host's rendered
  `1000000000000000100`. Ordinary ingress records that host serialization.

The Node test and package formatting check pass. Existing exact handles retain
their original text after conversion attempts. No production behavior changed.
Final cross-host execution and independent final map review remain separate gates.
