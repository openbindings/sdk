import { Document } from "@openbindings/sdk";

export const exampleDocument = `{
  "openbindings": "0.2.0",
  "operations": {
    "lookup": {
      "description": "Look up an item by its integer ID",
      "aliases": ["find"],
      "input": {
        "type": "object",
        "properties": { "id": { "type": "integer", "minimum": 1 } },
        "required": ["id"],
        "additionalProperties": false
      }
    }
  }
}`;

// Initialize in the host entry first. Every result is plain data; setup owners
// close here. A service that validates repeatedly should retain its contract
// instead, as shown in service.mjs and service-lifecycle.mjs.
export function firstUse(
  input = { id: 7 },
  documentBytes = exampleDocument,
  operation = "find",
) {
  const parsed = Document.parse(documentBytes);
  if (parsed.status !== "parsed") return parsed;
  let proof, selected, contract, prepared;
  try {
    const checked = parsed.value.validate();
    if (checked.status !== "conformant") return checked;
    proof = checked.document;
    const operations = proof.operations;
    selected = proof.resolveOperation(operation); // primary name or alias
    if (!selected) return { status: "operation-missing" };
    contract = selected.input();
    if (!contract) return { status: "no-value-contract" };
    const setup = contract.prepare();
    if (setup.status !== "ready") return setup;
    prepared = setup.contract;
    return { status: "checked", operations, result: prepared.validate(input) };
  } finally {
    prepared?.dispose();
    contract?.dispose();
    selected?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}
