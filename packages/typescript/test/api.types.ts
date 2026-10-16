import {
  parseJson,
  Document,
  SchemaResources,
  ExactJson,
  ConformantDocument,
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
  using created = Document.create();
  created.setOperations({
    run: {
      input: { type: "integer" },
      examples: { a: { input: 1, output: null } },
    },
  });
  created.setSource("local", { kind: "test", content: null });
  const checked = created.validate();
  if (checked.status === "conformant") {
    using proof = checked.document;
    using publication = new DiscoveryPublication(proof, { allowOrigin: "any" });
    return publication.respond(request);
  }
  if (checked.status === "version-refused")
    checked.refusal.declared satisfies string;
  if (checked.status === "report") checked.report.conclusion satisfies string;
  const parsed = Document.parse(bytes),
    resource = parseJson('{"type":"integer"}');
  if (parsed.status === "parsed" && resource.status === "parsed") {
    using document = parsed.value;
    using schema = resource.value;
    using empty = new SchemaResources();
    using resources = empty.with("https://example.test/schema", schema);
    const proof = document.validate();
    if (proof.status !== "conformant") return undefined;
    using validated = proof.document;
    using contracts = validated.valueContracts({
      resources,
      diagnostics: "upstream",
      limits: { evaluationSteps: 2000000 },
    });
    const prepared = contracts.prepare("run", "input", {
      signal: new AbortController().signal,
    });
    if (prepared.status !== "ready") return undefined;
    using input = prepared.valueContract;
    using value = ExactJson.from(7);
    const result: ValueOutcome = input.validate(value);
    if (result.outcome === "fails") {
      result.problems[0]!.instancePointer satisfies string;
      const detail: ValueProblemDetails | undefined =
        result.problems[0]!.details;
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
new ConformantDocument();
// @ts-expect-error raw ABI constructors are not exported
new ExactJson();
// @ts-expect-error undefined generic content is never silently omitted
ExactJson.from({ missing: undefined });
// @ts-expect-error output-only JSON cannot contain an exact handle
const output: JsonOutput = ExactJson.from(1);
void output;

declare const parsedOnly: Document;
// @ts-expect-error publication requires established conformance
new DiscoveryPublication(parsedOnly);
// @ts-expect-error parsed is not nominally validated
const invalidAssignment: ConformantDocument = parsedOnly;
void invalidAssignment;
// Proof is nominal and has no editing or unchecked construction surface.
declare const proofOnly: ConformantDocument;
// @ts-expect-error a proof cannot be edited
proofOnly.setDescription("changed");
// @ts-expect-error a plain object is not a conformance proof
const plainProof: ConformantDocument = {};
// @ts-expect-error proof and editable document are separate classes
const proofAsEditable: Document = proofOnly;
using editableAgain = proofOnly.intoDocument();
using independentDocument = proofOnly.asDocument();
void [plainProof, proofAsEditable, editableAgain, independentDocument];

// @ts-expect-error metadata fields match the immutable runtime snapshot
parsedOnly.operations[0]!.key = "changed";
// @ts-expect-error proof construction has one public method
ConformantDocument.fromParsed(parsedOnly);
// @ts-expect-error root no longer exports the HTTP companion
import { discover as rootDiscovery } from "../dist/index.js";

declare const preparedContract: import("../dist/index.js").ValueContract;
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

// Check actual emitted declarations.
import {
  ExactMember,
  BindingView,
  SourceView,
  DependencyView,
  ExampleView,
  type JsonInput,
  type ExactIterator,
} from "../dist/index.js";
function roundOneTypes(document: Document) {
  using root = document.value();
  using members = root.members();
  if (members) {
    members satisfies ExactIterator<ExactMember>;
    members.return() satisfies IteratorResult<ExactMember, undefined>;
    for (using member of members) {
      using name = member.name();
      using value = member.value();
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
    // @ts-expect-error Immutable metadata is not editable input data.
    metadata.tags?.push("x");
  }
  using source = document.source("a");
  using binding = document.binding("b");
  using dependency = document.dependency("d");
  if (source) {
    source.metadata.kind satisfies string;
    using content = source.content();
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
      using value = example.input();
      example.metadata.hasInput satisfies boolean;
    }
  }
  document.setOperation("added", {
    aliases: ["alias"],
    tags: ["tag"],
    examples: {},
  });
  document.setExample("added", "one", { input: null });
  document.setDependency("d", { operation: "added", kinds: ["a"] });
  document.setSource("s", { kind: "a" });
  document.setBinding("b", { operation: "added", source: "s" });
  document.set("/schemas", { a: true });
  document.remove("/operations/added");
  const opaque: JsonInput = { nested: [1, 2] as const };
  document.set("/x-opaque", opaque);
  // @ts-expect-error opaque values do not promise an exact owner
  opaque.retain();
  // @ts-expect-error typed part fields do not accept null in place of description
  document.setOperation("bad", { description: null });
}
function inaccessibleConstructors() {
  // @ts-expect-error Acquire through exact members().
  new ExactMember();
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
parsedOnly.setOperations(readonlyRoundOneInput.operations);
parsedOnly.setDependencies(readonlyRoundOneInput.dependencies);
void roundOneTypes;
void inaccessibleConstructors;

function preparedResources(
  valueContract: import("../dist/index.js").ValueContract,
) {
  // @ts-expect-error Evaluator completeness metadata is no longer a product API.
  void valueContract.resourceCompleteness;
  // @ts-expect-error The planner's metadata type is not exported under a new name.
  type Removed = import("../dist/index.js").ResourceCompleteness;
}
void preparedResources;

// only observable failure kinds belong to this union.
type DiscoveryFailureKind = Extract<
  import("../dist/http-discovery.js").DiscoveryResult,
  { status: "transport-error" | "body-error" }
>["error"]["kind"];
const observableDiscoveryFailure: DiscoveryFailureKind = "redirect-policy";
// @ts-expect-error Timeouts have their own result status, not a failure kind.
const obsoleteDiscoveryTimeout: DiscoveryFailureKind = "timeout";
// @ts-expect-error Fetch exposes no positive content-coding failure evidence.
const inventedDiscoveryDecode: DiscoveryFailureKind = "decode";
void observableDiscoveryFailure;
void obsoleteDiscoveryTimeout;
void inventedDiscoveryDecode;

const exactAllowOrigin: import("../dist/http-discovery.js").AllowOrigin = {
  origin: "https://client.example",
};
const omittedAllowOrigin: import("../dist/http-discovery.js").AllowOrigin =
  "omit";
// @ts-expect-error CORS wildcard spelling is represented by the typed "any" choice
const oldAllowOrigin: import("../dist/http-discovery.js").AllowOrigin = "*";
void [exactAllowOrigin, omittedAllowOrigin, oldAllowOrigin];

// @ts-expect-error unproven documents cannot construct normative value contracts
parsedOnly.valueContracts();

function validatedReads(proof: ConformantDocument, parsed: Document) {
  proof.openbindings satisfies string;
  proof.name satisfies string | undefined;
  proof.version satisfies string | undefined;
  proof.description satisfies string | undefined;
  const schemas: ReadonlyMap<string, ExactJson> | undefined = proof.schemas();
  for (const value of schemas?.values() ?? []) value.dispose();
  using editor = proof.asDocument();
  const selection = parsed.resolveOperation("run");
  if (selection.status === "found") {
    using operation = selection.operation;
    using input = operation.input();
    using output = operation.output();
    input satisfies ExactJson | undefined;
    output satisfies ExactJson | undefined;
  }
  return editor.name;
}
void validatedReads;

import { liveStorageOwners } from "../dist/testing.js";
// @ts-expect-error Test bookkeeping is available only through the testing entry.
import { liveStorageOwners as productionCounter } from "../dist/index.js";
liveStorageOwners() satisfies number;

// Undefined is its own exhaustive value outcome, with constructible plain detail.
const undefinedDetail: import("../dist/index.js").UndefinedResult = {
  code: "custom",
  message: "established undefined result",
  location: null,
};
const undefinedOutcome: ValueOutcome = {
  outcome: "undefined",
  detail: undefinedDetail,
};
undefinedOutcome.detail satisfies import("../dist/index.js").UndefinedResult;
// @ts-expect-error semantic undefinedness is not an inability reason
const obsoleteUndefinedReason: import("../dist/index.js").NoVerdictReason =
  "undefined";
function exhaustiveValueOutcome(value: ValueOutcome): string {
  switch (value.outcome) {
    case "satisfies":
      return value.outcome;
    case "fails":
      return value.message;
    case "undefined":
      return value.detail.code;
    case "no-verdict":
      return value.detail.reason;
    default: {
      const unreachable: never = value;
      return unreachable;
    }
  }
}

// Part names match the specification vocabulary, with no retired aliases.
import type {
  Operation,
  Binding,
  Source,
  Dependency,
  Example,
} from "../dist/index.js";
function namedParts(document: Document) {
  const example: Example = { input: null };
  const operation: Operation = { input: true, examples: { sample: example } };
  const source: Source = { kind: "custom", content: null };
  const binding: Binding = { operation: "run", source: "local" };
  const dependency: Dependency = { operation: "run" };
  document.setOperation("run", operation);
  document.setBinding("binding", binding);
  document.setSource("local", source);
  document.setDependency("dependency", dependency);
  document.setExample("run", "sample", example);
}
void namedParts;
// @ts-expect-error retired part name has no alias
import type { OperationDraft } from "../dist/index.js";
// @ts-expect-error retired part name has no alias
import type { BindingDraft } from "../dist/index.js";
// @ts-expect-error retired part name has no alias
import type { SourceDraft } from "../dist/index.js";
// @ts-expect-error retired part name has no alias
import type { DependencyDraft } from "../dist/index.js";
// @ts-expect-error retired part name has no alias
import type { ExampleDraft } from "../dist/index.js";
declare const partError: import("../dist/index.js").EditError;
partError.partPointer satisfies string | null;
// @ts-expect-error retired pointer name has no alias
partError.draftPointer;

// Typed maps and exact pointer edits share the existing batch API.
function typedBatch(
  document: Document,
  operation: Operation,
  source: Source,
  binding: Binding,
  dependency: Dependency,
) {
  document.edit([
    { op: "set-operations", value: { run: operation } },
    { op: "set-sources", value: { local: source } },
    { op: "set-bindings", value: { binding } },
    { op: "set-dependencies", value: { dependency } },
    { op: "set", pointer: "/name", value: "Batch" },
  ]);
  // @ts-expect-error typed source kind is required in a batch, too
  document.edit([{ op: "set-sources", value: { local: {} } }]);
}
void typedBatch;
