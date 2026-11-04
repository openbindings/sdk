# TypeScript domain organization checkpoint

The 3,103-line internal bucket is removed. Runtime, diagnostics, ownership, exact
JSON, schemas, documents and optional HTTP discovery now own their implementations.
There is no internal compatibility barrel. The public package entry points expose
the current designed API; language-specific ownership helpers stay private to the
module graph. `source-hashes.json` pins the inspected files.

Validation passes: TypeScript compilation and type consumers; all 180 Node tests;
the emitted API reference graph (69 unique exports / 428 entries); installed npm
archive consumers; current Chromium 156 / WebKit 27.2 with matching Playwright
1.64; and real local workerd correctness checks. Both browsers independently pass
505 document cases and 1,546 evaluator values, plus existing host/lifetime/editor
controls. These overlapping inventories are not additive coverage.

The initial six Node failures were reference-test mutations targeting the removed
internal.d.ts; controls now locate declarations by owner and keep the same failure
expectations. Browser fixtures serve actual generated modules. The workerd lifetime
caller still named removed SchemaResources and now uses ExternalSchemas through
the current resources option. No compatibility export was restored. Initial
configuration failures and subsequent successful checks remain in the logs.

Workerd checks include all three unchanged workload tiers, 1,000 retained calls,
200 snapshot replacements, cancellation/recovery and release to the warmed owner
count (17 here). This is correctness/ownership evidence, not a timing campaign.
Independent organizational review and whole-SDK final qualification are separate.
