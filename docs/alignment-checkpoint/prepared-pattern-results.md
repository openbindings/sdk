# Original evaluator: retained pattern readings

The original-node Plan retains each activated pattern's immutable reading under
its original operand occurrence. Identical normalized value-pattern forms share
the same reading; patternProperties names retain their individually counted forms.
Simple matchers are shared directly. Calls no longer repeat source parsing or
Unicode normalization. VM-backed readings still compile the retained normalized
form into call-local engines and caches; full once-prepared VM programs remain open.

Invalid readings remain Undefined effects; unsupported readings remain typed local
inabilities. Original effect identities, profiles, dynamic context and exact values
are unchanged. The artifact retains neither preparation control nor instance.
Arc/index construction is reserved before allocation, including the full Reading
enum layout for small unsupported payloads. No default limit was raised.

Both Rust toolchains pass 263 evaluator tests/doctests (three existing release-only
controls remain ignored) and 1,546 required-value judgments. Current strict Clippy,
Wasm/Node 180, types, installed npm archive and Chromium/WebKit checks pass; each
browser independently judges 505 core and 1,546 evaluator observations. The final
explicit Arc-layout correction was followed by fresh complete executions of these
lanes; earlier logs are retained separately.

Independent review passes the bounded increment: retained Simple under 128 work
units, invalid/unsupported/decisive cases, original effect identity, concurrency,
owner disposal, fresh retry, limits, ignored vocabulary positions and 1,024 unused
non-Simple patterns. It exercises all 47 observed warm-preparation cancellation
boundaries for each of Simple/invalid/backreference readings and 29 VM evaluation
boundaries, then reruns four prepared-artifact and six pattern-correctness groups.

The design review demonstrates why eager VM construction is a separate gate:
400 minimal first-build reservations at the current schedule consume 630,784,000
bytes before forms, exceeding the unchanged 512 MiB preparation limit. The existing
original engine correctly accepts those unused patterns. Keeping lazy VM compilation
here avoids creating that new blanket refusal while a better aggregate compiler
schedule is designed and qualified.

Full VM-once preparation, demand-sensitive capacity masking, general pattern compiler
certification, compact execution, local syntax admission, diagnostics and whole-SDK
performance/qualification remain open. This is not a replacement-engine switch.
