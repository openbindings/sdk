import {
  authorDocument,
  parseJson,
  parseDocument,
  SchemaResources,
  ExactJson,
  ValidatedDocument,
  type ParsedDocument,
  initialize,
  type JsonOutput,
  type ValueOutcome,
} from "../dist/index.js";
import { DiscoveryPublication, discover } from "../dist/http-discovery.js";

async function caller(
  bytes: Uint8Array,
  request: Request,
): Promise<Response | undefined> {
  await initialize(bytes);
  const authored = authorDocument({
    operations: {
      run: {
        input: { type: "integer" },
        examples: { a: { input: 1, output: null } },
      },
    },
    sources: { local: { kind: "test", content: null } },
  });
  if (authored.status === "authored") {
    using document = authored.document;
    const checked = document.validate();
    if (checked.status === "validated") {
      using validated = checked.document;
      using publication = new DiscoveryPublication(validated, {
        allowOrigin: "*",
      });
      return publication.respond(request);
    }
    if (checked.status === "version-refused")
      checked.refusal.declared satisfies string;
    if (checked.status === "assessed")
      checked.report.conclusion satisfies string;
  }
  const parsed = parseDocument(bytes),
    resource = parseJson('{"type":"integer"}');
  if (parsed.status === "parsed" && resource.status === "parsed") {
    using document = parsed.value;
    using schema = resource.value;
    using empty = new SchemaResources();
    using resources = empty.with("https://example.test/schema", schema);
    using contracts = document.contracts({
      resources,
      limits: { maxProblems: 4 },
    });
    const prepared = contracts.prepare("run", "input", {
      signal: new AbortController().signal,
    });
    if (prepared.status !== "ready") return undefined;
    using input = prepared.contract;
    using value = ExactJson.from(7);
    const result: ValueOutcome = input.validate(value);
    if (result.outcome === "mismatch") {
      result.problems[0].instancePointer satisfies string;
    }
    if (result.outcome === "no-verdict") result.detail.reason satisfies string;
    const ordinary = value.toValue();
    if (ordinary.status === "converted") ordinary.value satisfies JsonOutput;
    // @ts-expect-error implementation handles are not a supported API
    document.raw;
  }
  const discovery = await discover("https://example.test", {
    fetch: async (url, init) => fetch(url, init),
  });
  if (discovery.status === "found") discovery.document.dispose();
  if (discovery.status === "body-limit") discovery.limit satisfies number;
  if (discovery.status === "non-conformant") discovery.document?.dispose();
  return undefined;
}
void caller;
// @ts-expect-error conformant handles can only come from assessment
new ValidatedDocument();
// @ts-expect-error raw ABI constructors are not exported
new ExactJson();
// @ts-expect-error undefined generic content is never silently omitted
ExactJson.from({ missing: undefined });
// @ts-expect-error output-only JSON cannot contain an exact handle
const output: JsonOutput = ExactJson.from(1);
void output;

declare const parsedOnly: ParsedDocument;
// @ts-expect-error publication requires established conformance
new DiscoveryPublication(parsedOnly);
// @ts-expect-error parsed is not nominally validated
const invalidAssignment: ValidatedDocument = parsedOnly;
void invalidAssignment;
const invalidDraft = authorDocument({
  operations: {},
  additionalFields: { operations: {} },
});
if (invalidDraft.status === "authoring-error") {
  invalidDraft.error.code satisfies string;
  invalidDraft.error.draftPointer satisfies string | null;
}

// @ts-expect-error metadata fields match the immutable runtime snapshot
parsedOnly.operations[0].key = "changed";
// @ts-expect-error proof construction has one public method
ValidatedDocument.fromParsed(parsedOnly);
// @ts-expect-error root no longer exports the HTTP companion
import { discover as rootDiscovery } from "../dist/index.js";
