# H01–H08 decision review

Reviewer: Claude Opus 5.5, 2026-10-12. Input: `claude-decisions-input.txt` only. No other file, draft or agent was consulted.

## Verdict

The initial hypotheses form one coherent consumer contract once five gaps close. H01 cannot carry FR08's second resource, so a reloaded OBI would depend on host state. H04 puts an HTTP status in error data, which the binding-invoker role discourages. H06 lacks a value identity and an unsatisfiable-choice rule. H07 leaves callers unable to discover parameter names. H08 conflicts with OAS patch-version guidance. Choose the initial hypothesis everywhere except H07, where the named alternative wins.

## Decisions

**H01. A by-value resource set of JSON values.** Content: an absolute entry `retrievalUri`, the entry document, and further resources keyed by absolute URI. FR08 needs `parts.json` after reload; a host-supplied resource lets one OBI mean different operations under different hosts. Carry parsed JSON, not text: text puts a YAML parser in every Wasm invocation and hides the description from document tooling. YAML becomes a constructor convenience that refuses non-JSON-compatible input. The retrieval URI is effect-bearing: OAS resolves relative server URLs, including the default `/`, against the containing document's location. A `file:` or `.invalid` base must route to H06 or refusal, never silently become a destination.

**H02. The path/method pair.** `path` exactly equals an entry-document `paths` key (no template equivalence, Unicode normalization or percent-decoding); `method` exactly equals a lowercase Path Item field name, without case folding. This addresses the mount rather than a component-local item, needs no `~0`/`~1` escaping for FR06, and ignores `operationId`, so FR07's duplicate IDs cannot redirect a call. Use the RFC 6901 pointer only as coverage `sourceRef`.

**H03. Location-separated maps.** `{path?, query?, body?}`, keyed by OAS `in` and `name`, mirrors upstream parameter identity, so no renaming exists to document. Refuse other members (header and cookie are outside the profile), undeclared names, missing required parameters and body/requestBody mismatches with ERR_REFUSED before dispatch. Freeze null parameters as refused; omission and literal `null` text are the other plausible readings and belong in the observation ledger. Serialize integers from exact decimal text, never through `f64`. Spelling is provisional: `path` would mean different things in binding content and request values.

**H04. The ordinary value model, tightened.** Success is status class 2xx, whether or not OAS documents that status. Complete JSON content yields one output then `complete`; zero-length content yields zero outputs then `complete`, whatever the Content-Type (record that reading); JSON `null` yields one `null` output. Non-JSON, malformed or truncated content after a 2xx is an error with no output. Non-2xx, including unfollowed 3xx, is one kind-defined error code. The role says statuses do not belong in `data` "unless they are themselves application data under an explicit governing rule"; defining `data: {status}` in that code's rule is arguable, not clear. The fallback carries the status in the code string. The envelope alternative loses: it reports a 500 as `complete` and makes every author output schema describe HTTP.

**H05. Wait for close.** Only this variant refuses a second value before any request. Before freezing, verify that the generic `operation::Invoker` single-value path sends `close` before awaiting output; otherwise every ordinary call deadlocks. Zero inputs then close means every request member is absent. Resolve context at `open`, before reading input. The portable meaning is "one HTTP request per invocation from at most one value"; dispatch timing is policy.

**H06. Declared identity at point `server`, path `""`.** The value is an absolute URL that exactly equals one of the operation's effective servers after relative resolution (operation over path item over root; a mounted path item's servers resolve against its own resource). The challenge carries a schema `enum` of those URLs and the entry retrieval URI as `target`. Templated servers are implementation-unsupported. An unusable supplied value re-challenges. If no candidate is an http(s) origin, return ERR_REFUSED, because a challenge with an empty enum is unsatisfiable. For the harness, map declared origins to localhost in a test transport rather than rewriting fixture servers; substitution changes source content and blurs the reload comparison.

**H07. The alternative: a structural input schema.** Derive `path`, `query` and `body` member names and `required` from Parameter and Request Body Objects, with `additionalProperties: false` and value schemas `true`. It copies no Schema Object, matches H03 enforcement exactly and satisfies N4; without it a synthesized OBI reveals no parameter names. Emit no output schema. A declaration the caller contract drops (response headers, security requirements, non-JSON media, header/cookie parameters, callbacks) makes its unit `lossy` or `implementation-unsupported`, so `fullyRepresented` is false. Withheld `parts.json` yields `exhaustive: false` with a limitation naming the unresolved URI.

**H08. A revisioned private label and 3.1.x admission.** Use `local.openapi-json-probe.r1` and change the suffix whenever frozen meaning changes after fixtures are hashed; otherwise saved OBIs silently change meaning under one string. OAS 3.1.x says tooling should not consider the patch version, so refusing 3.1.0/3.1.1 would contradict upstream. Admit `3.1.*` under the 3.1.2 reading, qualify only 3.1.2, and refuse other lines before effects. Verify that wording against the pinned text.

## Cross-cutting hazards

1. Possible effects begin when the prepared request reaches the transport. No later failure, including connection refusal, may report ERR_REFUSED or CONTEXT_REQUIRED unless the transport contract proves nothing was sent.
2. Derived Rust `Option<Value>` maps `"body": null` to `None`. Use an explicit three-state type. In TypeScript, `??` and `undefined` checks conflate the same cases, and exact integers need the planned text boundary, not `JSON.parse`.
3. Reload identity rests on the closed resource set, exact locator matching and the label revision. Test reload with the original resources unreachable.
4. Resource-key equivalence (RFC 3986 resolution then exact comparison, or further normalization) is client policy now but decides which operation a document means.

## Specification candidates versus policy

Candidates, each needing two plausible readings before promotion: source content and base semantics; the locator pair and exact matching; request shape with absence/null for bodies and parameters; the 2xx, zero-length, null and non-2xx mapping with its failure data; "at most one value, one request"; point `server` and its value identity; resource-key equivalence; patch admission.

Policy: YAML convenience, dispatch timing, refusals outside the profile, synthesis key naming, structural schema emission, coverage reason codes, harness origin mapping, caching and cancellation arbitration.

## For Matthew

Only H04's status placement touches published-role meaning. The rest is under the kind and can be frozen at checkpoint A on the evidence above.
