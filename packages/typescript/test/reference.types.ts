// Compile the reference's ownership and proof boundaries through public entrypoints.
import {
  initialize,
  Document,
  parseJson,
  type JsonInput,
  type SchemaOrigin,
  type ValueOutcome,
  type ValueCheck,
  type SchemaPreparation,
  type PreparedSchema,
  type DocumentValidation,
  type ValidationOptions,
  type DocumentReport,
  validateDocument,
} from "../dist/index.js";
import { DiscoveryPublication } from "../dist/http-discovery.js";

export async function referenceExample(wasmBytes: Uint8Array, source: string) {
  await initialize(wasmBytes);
  const parsed = Document.parse(source);
  if (parsed.status !== "parsed") return parsed.error;
  using document = parsed.value;
  const inventory = document.references();
  // @ts-expect-error Reading the list without handling completion caused stale renames.
  inventory.references;
  if (inventory.complete) {
    for (const reference of inventory.references) {
      reference.resourceRoot satisfies SchemaOrigin | null;
    }
    // @ts-expect-error Partial observations exist only in the incomplete case.
    inventory.retained;
  } else {
    inventory.retained satisfies readonly import("../dist/index.js").Reference[];
    inventory.limitation satisfies import("../dist/index.js").ReferenceRefusal;
    // @ts-expect-error Incomplete observations cannot be read as the full inventory.
    inventory.references;
  }
  document.references({ limits: { maxReportBytes: 16 * 1024 * 1024 } });
  const validationOptions: ValidationOptions = {
    limits: { maxFindings: 4200, maxPointerBytes: 16 * 1024 * 1024 },
  };
  document.validate(validationOptions);
  document.validate({});
  document.validate(undefined);
  // @ts-expect-error The removed separate method has no compatibility alias.
  document.validateWithLimits({ maxFindings: 4200 });
  // @ts-expect-error Limits belong under the limits option.
  document.validate({ maxFindings: 4200 });
  // @ts-expect-error Validation options reject unknown names.
  document.validate({ unexpected: true });
  // @ts-expect-error Validation retention accepts only known budgets.
  document.validate({ limits: { unexpected: 0 } });
  // Every access returns its own owner; this one is scoped independently.
  using original = document.value();
  const copy: Uint8Array = original.bytes;
  const proof: DocumentValidation = document.validate();
  // validateDocument returns plain data: a report or a refusal, never a proof.
  const plain: DocumentReport = validateDocument(source);
  if (plain.status === "report") void plain.report.conclusion;
  if (proof.status !== "conformant") return proof;
  using validated = proof.document;
  using publication = new DiscoveryPublication(validated);
  using operation = validated.resolveOperation("run"); // primary name or alias
  if (!operation) return { status: "operation-missing" as const };
  using input = operation.input();
  if (!input) return { status: "no-schema" as const };
  const setup: SchemaPreparation = input.prepare({
    limits: { evaluationSdkWorkUnits: 2000000 },
  });
  if (setup.status !== "ready") return setup;
  using contract: PreparedSchema = setup.schema;
  const exact = parseJson("9007199254740993");
  if (exact.status !== "parsed") return exact.error;
  using value = exact.value;
  const exactOutcome: ValueOutcome = contract.evaluate(value);
  const ordinary: JsonInput = { precise: value };
  const ordinaryOutcome: ValueCheck = contract.evaluate(ordinary);
  if (ordinaryOutcome.status === "input-error") {
    ordinaryOutcome.error.instancePointer satisfies string | null;
  }
  if (exactOutcome.status === "no-verdict")
    exactOutcome.detail.reason satisfies string;
  // @ts-expect-error Ordinary admission introduces the input-error branch.
  const missingAdmission: ValueOutcome = contract.evaluate(ordinary);
  // @ts-expect-error Exact inspection cannot manufacture normative proof.
  new DiscoveryPublication(document);
  void missingAdmission;
  return { exactOutcome, ordinaryOutcome, sourceBytes: copy };
}

// Public snapshot accounting and parser coordinates are first-class facade types.
function snapshotBytes(document: import("../dist/index.js").Document): number {
  return document.retainedBytes();
}
function inputCoordinates(
  failure: import("../dist/index.js").InputFailure,
): [number, number] {
  return [failure.error.line, failure.error.byteColumn];
}
void snapshotBytes;
void inputCoordinates;
