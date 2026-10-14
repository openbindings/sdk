# Bundled schemas

`openbindings.schema.json` is the derived schema OBI-02 applies. From the SDK
root, `tools/verify-spec.mjs` compares it byte-for-byte with
`openbindings.schema.json` in the checkout of the SDK's declared applied
specification revision, alongside the frozen qualification inputs.

## JSON Schema 2020-12 meta-schemas

The dialect meta-schema and the seven vocabulary meta-schemas OBI-10 names
have the [JSON Schema 2020-12 publication](https://json-schema.org/draft/2020-12)
as their authoritative source. The bundled files must match the published
meta-schemas byte-for-byte or by exact JSON equality.

These files were originally copied from the distribution carried by
`github.com/santhosh-tekuri/jsonschema/v6` v6.0.3. The acquisition history is
retained here with `LICENSE-jsonschema-go`. Core embeds the meta-schemas in a
value contract's bundle when a schema references one the document and its
resources do not declare.

| Bundled file | Published URL |
| --- | --- |
| `draft2020-12/schema.json` | <https://json-schema.org/draft/2020-12/schema> |
| `draft2020-12/meta/core.json` | <https://json-schema.org/draft/2020-12/meta/core> |
| `draft2020-12/meta/applicator.json` | <https://json-schema.org/draft/2020-12/meta/applicator> |
| `draft2020-12/meta/unevaluated.json` | <https://json-schema.org/draft/2020-12/meta/unevaluated> |
| `draft2020-12/meta/validation.json` | <https://json-schema.org/draft/2020-12/meta/validation> |
| `draft2020-12/meta/meta-data.json` | <https://json-schema.org/draft/2020-12/meta/meta-data> |
| `draft2020-12/meta/format-annotation.json` | <https://json-schema.org/draft/2020-12/meta/format-annotation> |
| `draft2020-12/meta/content.json` | <https://json-schema.org/draft/2020-12/meta/content> |

Publication confirmation (2026-10-13): all eight bundled files are exactly
JSON-equal (formatting differs) to the meta-schemas of the release the
specification cites, JSON Schema Core `draft-bhutton-json-schema-01` (June
2022), as tagged `draft-bhutton-json-schema-01` in
<https://github.com/json-schema-org/json-schema-spec>. Do not replace them with
the copies json-schema.org serves today: those come from the later `2020-12`
maintenance branch, where commit `5a164002` (2023-11-01) removed `$vocabulary`
from the seven vocabulary meta-schemas. That removal does not change any
validation result, because a vocabulary meta-schema's own `$vocabulary` takes
effect only when the schema is used as a dialect meta-schema, but it is not the
cited publication.

`DIGESTS.json` records the SHA-256 of each bundled meta-schema's exact bytes,
including whitespace and line endings, using paths relative to this directory.
`tools/verify-spec.mjs` requires all eight entries and checks every digest.
Update the recorded digests only after confirming publication equivalence.

Run the verifier and its CR-7 negative regression harness from the SDK root:

```sh
node tools/verify-spec.mjs SDK_ROOT SPEC_CHECKOUT APPLIED_SPEC_COMMIT --self-test
```

The harness checks that an unmodified temporary copy passes, that changing
one byte of the derived schema or any meta-schema fails with a diagnostic
naming the file, and that omitting a digest fails. CI runs this harness with
the pinned specification checkout.
