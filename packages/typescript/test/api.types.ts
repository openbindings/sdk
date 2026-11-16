import {
  parseJson,
  Document,
  ExternalSchemas,
  ExactJson,
  Schema,
  ConformantDocument,
  initialize,
  type JsonOutput,
  type SchemaOrigin,
  type SourceLocation,
  type ValueOutcome,
  type ValueAdmissionFailure,
  type ValueProblemDetails,
  type DocumentReport,
  type DocumentValidation,
  validateDocument,
} from "../dist/index.js";
import { DiscoveryPublication, discover } from "../dist/http-discovery.js";

async function caller(
  bytes: Uint8Array,
  request: Request,
): Promise<Response | undefined> {
  await initialize(bytes);
  using created = Document.create();
  created.set("/operations", {
    run: {
      input: { type: "integer" },
      examples: { a: { input: 1, output: null } },
    },
  });
  created.set("/sources/" + "local", { kind: "test", data: null });
  // DocumentValidation names what Rust's does: the result that can carry proof.
  const checked: DocumentValidation = created.validate();
  // @ts-expect-error a DocumentReport has no conformant branch to carry a proof
  const proofless: DocumentReport = checked;
  void proofless;
  validateDocument(bytes) satisfies DocumentReport;
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
    using empty = new ExternalSchemas();
    using resources = empty.with("https://example.test/schema", schema);
    const proof = document.validate();
    if (proof.status !== "conformant") return undefined;
    using validated = proof.document;
    const operation = validated.resolveOperation("run");
    using contract = operation
      ? validated.inputSchema(operation.key)
      : undefined;
    if (!contract) return undefined;
    const prepared = contract.prepare({
      resources,
      limits: { evaluationSdkWorkUnits: 2000000 },
      signal: new AbortController().signal,
    });
    if (prepared.status !== "ready") return undefined;
    using input = prepared.schema;
    using value = ExactJson.from(7);
    input.evaluate(value) satisfies ValueOutcome;
    contract.prepare({
      // @ts-expect-error unknown preparation options are rejected
      unknownPreparationOption: true,
    });
    contract.prepare({
      limits: {
        evaluationSdkWorkUnits: null,
        evaluationSdkReservationBytes: 0,
      },
    });
    // @ts-expect-error Retired work name has no compatibility alias.
    contract.prepare({ limits: { evaluationSteps: 1 } });
    // @ts-expect-error Retired storage name has no compatibility alias.
    contract.prepare({ limits: { programBytes: 1 } });
    const result: ValueOutcome = input.explain(value);
    if (result.status === "fails") {
      result.problems[0]!.instance satisfies string;
      value.locate(result.problems[0]!.instance) satisfies
        | SourceLocation
        | undefined;
      result.problems[0]!.schema satisfies SchemaOrigin;
      validated.locate(result.problems[0]!.schema) satisfies
        | SourceLocation
        | undefined;
      resources.locate(result.problems[0]!.schema) satisfies
        | SourceLocation
        | undefined;
      const detail: ValueProblemDetails | undefined =
        result.problems[0]!.details;
      if (detail?.kind === "required") detail.member satisfies string;
      if (detail?.kind === "enum") detail.choices satisfies readonly string[];
      if (detail?.kind === "type") detail.expected satisfies readonly string[];
      if (detail?.kind === "numeric-bound" || detail?.kind === "size-bound")
        detail.bound satisfies string;
    }
    if (result.status === "no-verdict") result.detail.reason satisfies string;
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
proofOnly.set("/description", "changed");
// @ts-expect-error a plain object is not a conformance proof
const plainProof: ConformantDocument = {};
// @ts-expect-error proof and editable document are separate classes
const proofAsEditable: Document = proofOnly;
using editableAgain = proofOnly.intoDocument();
using independentDocument = proofOnly.asDocument();
void [plainProof, proofAsEditable, editableAgain, independentDocument];

// @ts-expect-error canonical keys are immutable fields of the proved snapshot
proofOnly.operation("run")!.key = "changed";
// @ts-expect-error proof construction has one public method
ConformantDocument.fromParsed(parsedOnly);
// @ts-expect-error root no longer exports the HTTP companion
import { discover as rootDiscovery } from "../dist/index.js";

declare const preparedSchema: import("../dist/index.js").PreparedSchema;
declare const exactInput: ExactJson;
const exactOutcome: ValueOutcome = preparedSchema.evaluate(exactInput);
const ordinaryOutcome: import("../dist/index.js").ValueCheck =
  preparedSchema.evaluate({ nested: exactInput });
// @ts-expect-error ordinary admission may return input-error
const incompleteOutcome: ValueOutcome = preparedSchema.evaluate(7);
if (ordinaryOutcome.status === "input-error") {
  ordinaryOutcome.error.instancePointer satisfies string | null;
}
const textOutcome: import("../dist/index.js").JsonCheck =
  preparedSchema.evaluateJson(new Uint8Array());
// @ts-expect-error exact text admission may return input-error
const unadmittedText: ValueOutcome = preparedSchema.evaluateJson("7");
// @ts-expect-error evaluateJson takes exact text or bytes, not ordinary values
preparedSchema.evaluateJson({ nested: exactInput });
if (textOutcome.status === "input-error") {
  textOutcome.error.byteOffset satisfies number | undefined;
}
void unadmittedText;
// An untyped or JsonInput-typed value can fail ordinary admission, so its
// result keeps the input-error branch; only an ExactJson argument drops it.
declare const untyped: any;
declare const ordinaryInput: import("../dist/index.js").JsonInput;
const untypedCheck = preparedSchema.evaluate(untyped);
if (untypedCheck.status === "input-error")
  untypedCheck.error.instancePointer satisfies string | null;
const untypedExplain = preparedSchema.explain(untyped);
if (untypedExplain.status === "input-error")
  untypedExplain.error.instancePointer satisfies string | null;
const ordinaryCheck = preparedSchema.evaluate(ordinaryInput);
if (ordinaryCheck.status === "input-error")
  ordinaryCheck.error.instancePointer satisfies string | null;
const ordinaryExplain = preparedSchema.explain(ordinaryInput);
if (ordinaryExplain.status === "input-error")
  ordinaryExplain.error.instancePointer satisfies string | null;
const untypedText = preparedSchema.evaluateJson(untyped);
if (untypedText.status === "input-error")
  untypedText.error.code satisfies string;
// @ts-expect-error an untyped value can return input-error
const untypedOutcome: ValueOutcome = preparedSchema.evaluate(untyped);
// @ts-expect-error an untyped value can return input-error
const untypedExplanation: ValueOutcome = preparedSchema.explain(untyped);
const exactExplanation: ValueOutcome = preparedSchema.explain(exactInput);
void [untypedOutcome, untypedExplanation, exactExplanation];
// @ts-expect-error Raw drafts expose exact JSON, not partially interpreted operations.
parsedOnly.resolveOperation("run");
using batchResources = new ExternalSchemas([
  ["https://schema.test", exactInput] as const,
]);
void exactOutcome;
void incompleteOutcome;

import { SdkError } from "../dist/index.js";
const detailed = new SdkError("configuration", "invalid setting");
const specificCode: string = detailed.code;
void specificCode;
const obsoleteOutcome: ValueOutcome = {
  // @ts-expect-error old value verdict spelling is intentionally not a synonym
  status: "mismatch",
  problems: [],
  problemsComplete: true,
};
void obsoleteOutcome;
const outcomeDiscriminant: ValueOutcome = {
  // @ts-expect-error value results discriminate on status, as every result does
  outcome: "satisfies",
};
void outcomeDiscriminant;

// Check actual emitted declarations.
import {
  ExactMember,
  type Operation,
  type Binding,
  type Source,
  type Dependency,
  type Example,
  acceptsKind,
  type JsonInput,
  type ExactIterator,
} from "../dist/index.js";
function roundOneTypes(proof: ConformantDocument) {
  using document = proof.asDocument();
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
  for (const metadata of proof.operations()) {
    metadata.tags satisfies readonly string[] | undefined;
    metadata.deprecated satisfies boolean | undefined;
    // @ts-expect-error Immutable metadata is not editable input data.
    metadata.tags?.push("x");
  }
  const source = proof.source("a");
  const binding = proof.binding("b");
  const dependency = proof.dependency("d");
  if (source) {
    source.kind satisfies string;
    using content = proof.sourceData(source.key);
  }
  if (binding) {
    binding.preference satisfies number | undefined;
    const copied: Binding = { ...binding };
    copied.sourceKey satisfies string;
  }
  if (dependency) dependency.kinds satisfies readonly string[] | undefined;
  const operation = proof.resolveOperation("run");
  const example = operation
    ? proof.example(operation.key, "example")
    : undefined;
  using exampleInput =
    operation && example
      ? proof.exampleInput(operation.key, example.key)
      : undefined;
  example?.description satisfies string | undefined;
  document.set("/operations/" + "added", {
    aliases: ["alias"],
    tags: ["tag"],
    examples: {},
  });
  document.set("/operations/" + "added" + "/examples/" + "one", {
    input: null,
  });
  document.set("/dependencies/" + "d", { operation: "added", kinds: ["a"] });
  document.set("/sources/" + "s", { kind: "a" });
  document.set("/bindings/" + "b", { operation: "added", source: "s" });
  document.set("/schemas", { a: true });
  document.remove("/operations/added");
  const opaque: JsonInput = { nested: [1, 2] as const };
  document.set("/x-opaque", opaque);
  // @ts-expect-error opaque values do not promise an exact owner
  opaque.retain();
  // Exact editing can produce invalid drafts; conformance is separate.
  document.set("/operations/" + "bad", { description: null });
}
function inaccessibleConstructors() {
  // @ts-expect-error Acquire through exact members().
  new ExactMember();
  // @ts-expect-error Acquire through keyed lookup.
  new Binding();
  // @ts-expect-error Acquire through keyed lookup.
  new Source();
  // @ts-expect-error Acquire through keyed lookup.
  new Dependency();
  // @ts-expect-error Acquire through example lookup.
  new Example();
}
const readonlyRoundOneInput = {
  operations: {
    run: { aliases: ["a"], tags: ["t"], examples: { e: { input: 1 } } },
  },
  dependencies: { d: { operation: "run", kinds: ["k"] } },
} as const;
parsedOnly.set("/operations", readonlyRoundOneInput.operations);
parsedOnly.set("/dependencies", readonlyRoundOneInput.dependencies);
void roundOneTypes;
void inaccessibleConstructors;

function preparedResources(
  prepared: import("../dist/index.js").PreparedSchema,
) {
  // @ts-expect-error Evaluator completeness metadata is no longer a product API.
  void prepared.resourceCompleteness;
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

// an attempt deadline is a number of milliseconds, or omitted for none.
type DiscoveryOptions = import("../dist/http-discovery.js").DiscoveryOptions;
const boundedDiscovery: DiscoveryOptions = { timeoutMs: 30_000 };
const openDiscovery: DiscoveryOptions = {};
const plainHttpDiscovery: DiscoveryOptions = { allowHttp: true };
// @ts-expect-error HTTP admission is a boolean, not a string.
const invalidHttpDiscovery: DiscoveryOptions = { allowHttp: "true" };
void plainHttpDiscovery;
void invalidHttpDiscovery;
// @ts-expect-error Omit timeoutMs for no deadline; there is no "unbounded" spelling.
const unboundedDiscovery: DiscoveryOptions = { timeoutMs: "unbounded" };
void [boundedDiscovery, openDiscovery, unboundedDiscovery];

const exactAllowOrigin: import("../dist/http-discovery.js").AllowOrigin = {
  origin: "https://client.example",
};
const omittedAllowOrigin: import("../dist/http-discovery.js").AllowOrigin =
  "omit";
// @ts-expect-error CORS wildcard spelling is represented by the typed "any" choice
const oldAllowOrigin: import("../dist/http-discovery.js").AllowOrigin = "*";
void [exactAllowOrigin, omittedAllowOrigin, oldAllowOrigin];

// @ts-expect-error No second raw operation model is exported.
import type { OperationView } from "../dist/index.js";
// @ts-expect-error No extra conformant view naming layer is exported.
import type { ConformantOperationView } from "../dist/index.js";
// @ts-expect-error No ambiguous raw-domain lookup carrier is exported.
import type { OperationSelection } from "../dist/index.js";
function validatedReads(proof: ConformantDocument, parsed: Document) {
  proof.openbindings satisfies string;
  proof.name satisfies string | undefined;
  proof.version satisfies string | undefined;
  proof.description satisfies string | undefined;
  const schemas: ReadonlyMap<string, Schema> | undefined = proof.schemas();
  for (const value of schemas?.values() ?? []) value.dispose();
  using editor = proof.asDocument();
  // Conformance excludes ambiguity: selection is the view or undefined.
  const conformantOperation = proof.resolveOperation("run");
  using retainedProof = proof.retain();
  const record =
    conformantOperation && retainedProof.operation(conformantOperation.key);
  using contract = record ? retainedProof.inputSchema(record.key) : undefined;
  using schema = contract?.json();
  schema satisfies ExactJson | undefined;
  // @ts-expect-error the contract already names its field; prepare takes options only
  contract?.prepare("input");
  // @ts-expect-error Drafts have no domain metadata getters.
  parsed.name;
  using raw = editor.value();
  using name = raw.get("name");
  return name?.text;
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
  status: "undefined",
  detail: undefinedDetail,
};
undefinedOutcome.detail satisfies import("../dist/index.js").UndefinedResult;
// @ts-expect-error semantic undefinedness is not an inability reason
const obsoleteUndefinedReason: import("../dist/index.js").NoVerdictReason =
  "undefined";
function exhaustiveValueOutcome(value: ValueOutcome): string {
  switch (value.status) {
    case "satisfies":
      return value.status;
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

// Canonical names denote detached readonly records. Payload queries need a live proof.
function namedParts(document: ConformantDocument) {
  for (const operation of document.operations()) {
    operation satisfies Operation;
    using input = document.inputSchema(operation.key);
    using output = document.outputSchema(operation.key);
    using exact = document.operationValue(operation.key);
    operation.tags satisfies readonly string[] | undefined;
    operation.aliases satisfies readonly string[] | undefined;
    operation.deprecated satisfies boolean | undefined;
    for (const binding of document.operationBindings(operation.key) ?? []) {
      binding satisfies Binding;
      const relatedOperation = document.operation(binding.operationKey);
      const source = document.source(binding.sourceKey);
      source satisfies Source | undefined;
      using data = document.sourceData(binding.sourceKey);
      using bindingData = document.bindingData(binding.key);
      using bindingValue = document.bindingValue(binding.key);
      using sourceValue = document.sourceValue(binding.sourceKey);
      binding.operationKey satisfies string;
      binding.sourceKey satisfies string;
      binding.preference satisfies number | undefined;
      binding.idempotent satisfies boolean | undefined;
    }
    for (const example of document.examples(operation.key) ?? []) {
      example satisfies Example;
      using input = document.exampleInput(operation.key, example.key);
      using output = document.exampleOutput(operation.key, example.key);
      using value = document.exampleValue(operation.key, example.key);
    }
    // @ts-expect-error Detached records have no disposable handle.
    operation.dispose();
    // @ts-expect-error Retain a proof when later navigation is needed.
    operation.retain();
    // @ts-expect-error Detached metadata cannot acquire a document.
    operation.document();
    // @ts-expect-error Query the live proof by primary key for schemas.
    operation.input();
    // @ts-expect-error Detached records cannot be bound with using.
    using owned = operation;
  }
  for (const dependency of document.dependencies() ?? []) {
    dependency satisfies Dependency;
    acceptsKind(dependency, "kind") satisfies boolean;
    const operation = document.operation(dependency.operationKey);
    using exact = document.dependencyValue(dependency.key);
  }
  const record: Operation = { key: "local", tags: ["tag"] };
  // @ts-expect-error Readonly fields cannot be assigned.
  record.key = "changed";
  // @ts-expect-error Schema payloads are queried from the proof, not record fields.
  const payload: Operation = { key: "local", input: true };
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
partError.inputPointer satisfies string | null;
// @ts-expect-error Removed authoring-part diagnostic has no compatibility alias.
partError.partPointer;
// @ts-expect-error retired pointer name has no alias
partError.draftPointer;

// All authoring is exact JSON editing; a batch does not interpret domain fields.
function exactBatch(document: Document, value: JsonInput) {
  using edited = document.edit([
    { op: "set", pointer: "/operations", value: { run: { input: true } } },
    {
      op: "set",
      pointer: "/sources",
      value: { local: { kind: "example", data: value } },
    },
    { op: "set", pointer: "/name", value: "Batch" },
  ]);
  // @ts-expect-error Retired typed namespace replacement is not a batch op.
  document.edit([{ op: "set-sources", value: { local: {} } }]);
  // @ts-expect-error Retired typed setter has no compatibility alias.
  document.setOperation("run", {});
}
void exactBatch;

// Edits return independently owned immutable snapshots.
using changedSnapshot: Document = parsedOnly.set("/description", "changed");
using removedSnapshot: Document = changedSnapshot.remove("/description");
using emptySnapshot: Document = parsedOnly.edit([]);

function suppliedRoot(resources: ExternalSchemas) {
  const result = resources.prepare("https://example.test/root", {
    limits: { patternBytes: 1024 },
    signal: new AbortController().signal,
  });
  if (result.status === "ready") {
    result.schema.evaluate({ value: 1 });
    result.schema.dispose();
  } else {
    const failure: string = result.failure.kind;
    void failure;
  }
  // @ts-expect-error This owner supplies the resource set for preparation.
  resources.prepare("https://example.test/root", { resources });
}
void suppliedRoot;

// Ordinary and exact admission share these precise limit categories.
for (const code of ["byte-limit", "node-limit", "depth-limit"] as const) {
  code satisfies ValueAdmissionFailure["code"];
}

const jsonExplanation: import("../dist/index.js").JsonCheck =
  preparedSchema.explainJson("7");
preparedSchema.explainJson(new Uint8Array(), { signal: AbortSignal.abort() });
if (jsonExplanation.status === "fails")
  jsonExplanation.problems[0]!.instance satisfies string;
if (jsonExplanation.status === "input-error")
  jsonExplanation.error.byteOffset satisfies number | undefined;
// @ts-expect-error text admission can fail
const unadmittedExplanation: ValueOutcome = preparedSchema.explainJson("7");
// @ts-expect-error explainJson takes JSON text or bytes
preparedSchema.explainJson(exactInput);
// @ts-expect-error explainJson does not transfer an owner
jsonExplanation.dispose();
void unadmittedExplanation;
declare const registrationError: SdkError;
registrationError.details?.resource satisfies string | undefined;
if (registrationError.details) {
  // @ts-expect-error error details are immutable
  registrationError.details.resource = "changed";
}
