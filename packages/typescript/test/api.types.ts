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
  type ValueProblemDetails,
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
      includeSchemaDetails: true,
      limits: { maxProblems: 4 },
    });
    const prepared = contracts.prepare("run", "input", {
      signal: new AbortController().signal,
    });
    if (prepared.status !== "ready") return undefined;
    using input = prepared.contract;
    using value = ExactJson.from(7);
    const result: ValueOutcome = input.validate(value);
    if (result.outcome === "fails") {
      result.problems[0].instancePointer satisfies string;
      const detail: ValueProblemDetails | undefined =
        result.problems[0].details;
      if (detail?.kind === "required") detail.member satisfies string;
      if (detail?.kind === "enum") detail.choices satisfies readonly string[];
      if (detail?.kind === "type") detail.expected satisfies readonly string[];
      if (detail?.kind === "numeric-bound" || detail?.kind === "size-bound")
        detail.bound satisfies string;
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

declare const preparedContract: import("../dist/index.js").PreparedContract;
declare const exactInput: ExactJson;
const exactOutcome: ValueOutcome = preparedContract.validate(exactInput);
const ordinaryOutcome: import("../dist/index.js").ValueCheck =
  preparedContract.validate({ nested: exactInput });
// @ts-expect-error ordinary admission may return input-error
const incompleteOutcome: ValueOutcome = preparedContract.validate(7);
if (ordinaryOutcome.outcome === "input-error") {
  ordinaryOutcome.error.instancePointer satisfies string | null;
}
const selection = parsedOnly.resolveOperation("run");
if (selection.status === "ambiguous") {
  // @ts-expect-error ambiguity candidates are immutable
  selection.candidates.push("another");
}
using batchResources = new SchemaResources([
  ["https://schema.test", exactInput] as const,
]);
void exactOutcome;
void incompleteOutcome;

// Specific interpretation causes are optional; broad error categories remain stable.
import { SdkError } from "../dist/index.js";
const detailed = new SdkError(
  "interpretation",
  "invalid namespace",
  undefined,
  "invalid-operation-alias",
);
const specificCode: string | undefined = detailed.interpretationCode;
void specificCode;
const obsoleteOutcome: ValueOutcome = {
  // @ts-expect-error old value verdict spelling is intentionally not a synonym
  outcome: "mismatch",
  problems: [],
  problemsComplete: true,
};
void obsoleteOutcome;

// Round 1: actual emitted declarations, not declaration-only proposal stubs.
import {
  ExactMember,
  OwnedDocumentDraft,
  BindingView,
  SourceView,
  DependencyView,
  ExampleView,
  type EditableDocumentDraft,
  type DocumentDraft,
  type JsonInput,
  type ExactIterator,
} from "../dist/index.js";
function roundOneTypes(document: ParsedDocument) {
  using root = document.value;
  using members = root.members();
  if (members) {
    members satisfies ExactIterator<ExactMember>;
    members.return() satisfies IteratorResult<ExactMember, undefined>;
    for (using member of members) {
      using name = member.name;
      using value = member.value;
      member.index satisfies number;
      name.text satisfies string;
      value.metadata.kind satisfies string;
      using retained = member.retain();
    }
  }
  using elements = root.elements();
  if (elements) for (using element of elements) element.text satisfies string;
  for (const metadata of document.operations) {
    metadata.tags satisfies readonly string[] | null;
    metadata.deprecated satisfies boolean | null;
    // @ts-expect-error Immutable metadata is not editable draft data.
    metadata.tags?.push("x");
  }
  using source = document.source("a");
  using binding = document.binding("b");
  using dependency = document.dependency("d");
  if (source) {
    source.metadata.kind satisfies string;
    using content = source.content;
  }
  if (binding) {
    binding.metadata.preference satisfies number | null;
    using retained = binding.retain();
  }
  if (dependency) dependency.metadata.kinds satisfies readonly string[] | null;
  const selected = document.resolveOperation("run");
  if (selected.status === "found") {
    using operation = selected.operation;
    using example = operation.example("example");
    if (example) {
      using value = example.input;
      example.metadata.hasInput satisfies boolean;
    }
  }
  const conversion = document.toDraft();
  if (conversion.status !== "drafted") {
    conversion.error.sourceLocation?.byteOffset satisfies number | undefined;
    return;
  }
  using editing = conversion.draft;
  const draft: EditableDocumentDraft = editing.value;
  editWritableDraft(draft);
  const input: DocumentDraft = draft;
  authorDocument(input);
  const leaf = draft.schemas?.a;
  if (leaf instanceof ExactJson) {
    using retained = leaf.retain();
    draft.additionalFields = {
      ...draft.additionalFields,
      "x-retained": retained,
    };
  }
  // @ts-expect-error Opaque JsonInput does not promise an exact owner.
  leaf?.retain();
  // @ts-expect-error No second convenience authoring surface.
  editing.clone();
  // @ts-expect-error No implicit aggregate capture convenience.
  editing.keep(root);
}
function editWritableDraft(draft: EditableDocumentDraft) {
  draft.operations.added = { aliases: ["alias"], tags: ["tag"], examples: {} };
  draft.operations.added.aliases?.push("alias2");
  draft.operations.added.tags?.splice(0, 1);
  draft.operations.added.examples!.newExample = { input: null };
  delete draft.operations.added.examples!.newExample;
  draft.dependencies ??= {};
  draft.dependencies.newDependency = { operation: "added", kinds: [] };
  draft.dependencies.newDependency.kinds?.push("a");
  delete draft.dependencies.newDependency;
  draft.sources ??= {};
  draft.sources.newSource = { kind: "a" };
  delete draft.sources.newSource;
  draft.bindings ??= {};
  draft.bindings.newBinding = { operation: "added", source: "x" };
  delete draft.bindings.newBinding;
  draft.schemas ??= {};
  draft.schemas.a = true;
  delete draft.schemas.a;
  delete draft.operations.added;
  const opaque: JsonInput = { nested: [1, 2] as const };
  draft.schemas.a = opaque;
}
function inaccessibleConstructors() {
  // @ts-expect-error Acquire through exact members().
  new ExactMember();
  // @ts-expect-error Acquire through toDraft().
  new OwnedDocumentDraft();
  // @ts-expect-error Acquire through keyed lookup.
  new BindingView();
  // @ts-expect-error Acquire through keyed lookup.
  new SourceView();
  // @ts-expect-error Acquire through keyed lookup.
  new DependencyView();
  // @ts-expect-error Acquire through example lookup.
  new ExampleView();
}
const readonlyRoundOneInput = {
  operations: {
    run: { aliases: ["a"], tags: ["t"], examples: { e: { input: 1 } } },
  },
  dependencies: { d: { operation: "run", kinds: ["k"] } },
} as const;
authorDocument(readonlyRoundOneInput);
void roundOneTypes;
void inaccessibleConstructors;
