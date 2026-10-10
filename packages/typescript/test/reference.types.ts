// Compile the reference's ownership and proof boundaries through public entrypoints.
import {
  initialize,
  parseDocument,
  parseJson,
  type JsonInput,
  type ValueOutcome,
  type ValueCheck,
  type ContractPreparation,
  type ValidationResult,
} from "../dist/index.js";
import { DiscoveryPublication } from "../dist/http-discovery.js";

export async function referenceExample(wasmBytes: Uint8Array, source: string) {
  await initialize(wasmBytes);
  const parsed = parseDocument(source);
  if (parsed.status !== "parsed") return parsed.error;
  using document = parsed.value;
  // Every access returns its own owner; this one is scoped independently.
  using original = document.value;
  const copy: Uint8Array = original.bytes;
  const proof: ValidationResult = document.validate();
  if (proof.status !== "validated") return proof;
  using validated = proof.document;
  using publication = new DiscoveryPublication(validated);
  using context = validated.contracts({
    cacheCapacity: 0,
    limits: { maxProblems: 4 },
  });
  const setup: ContractPreparation = context.prepare("run", "input");
  if (setup.status !== "ready") return setup;
  using contract = setup.contract;
  const exact = parseJson("9007199254740993");
  if (exact.status !== "parsed") return exact.error;
  using value = exact.value;
  const exactOutcome: ValueOutcome = contract.validate(value);
  const ordinary: JsonInput = { precise: value };
  const ordinaryOutcome: ValueCheck = contract.validate(ordinary);
  if (ordinaryOutcome.outcome === "input-error") {
    ordinaryOutcome.error.instancePointer satisfies string | null;
  }
  if (exactOutcome.outcome === "no-verdict")
    exactOutcome.detail.reason satisfies string;
  // @ts-expect-error Ordinary admission introduces the input-error branch.
  const missingAdmission: ValueOutcome = contract.validate(ordinary);
  // @ts-expect-error Exact inspection cannot manufacture normative proof.
  new DiscoveryPublication(document);
  void missingAdmission;
  return { exactOutcome, ordinaryOutcome, sourceBytes: copy };
}
