import { parseDocument } from "@openbindings/sdk";

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
  const parsed = parseDocument(documentBytes);
  if (parsed.status !== "parsed") return parsed;
  let proof, context, contract;
  try {
    const checked = parsed.value.validate();
    if (checked.status !== "validated") return checked;
    proof = checked.document;
    const operations = proof.operations;
    context = proof.contracts();
    const setup = context.prepare(operation, "input"); // primary name or alias
    if (setup.status !== "ready") return setup;
    contract = setup.contract;
    return { status: "checked", operations, result: contract.validate(input) };
  } finally {
    contract?.dispose();
    context?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}
