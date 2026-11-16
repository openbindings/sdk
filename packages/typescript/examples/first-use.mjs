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
// close here. A service that validates repeatedly should retain its schema
// instead, as shown in service.mjs and service-lifecycle.mjs.
// Untrusted input: bound admission and evaluation work; calls cannot be interrupted.
const jsonLimits = { maxBytes: 4 * 1024 * 1024, maxNodes: 250_000 };
const prepareOptions = { limits: { evaluationSdkWorkUnits: 100_000_000 } };

export function firstUse(
  input = { id: 7 },
  documentBytes = exampleDocument,
  operation = "find",
) {
  const parsed = Document.parse(documentBytes, { limits: jsonLimits });
  if (parsed.status !== "parsed") return parsed;
  let proof, selected, schema, prepared;
  try {
    const checked = parsed.value.validate();
    if (checked.status !== "conformant") return checked;
    proof = checked.document;
    const operations = proof.operations(); // detached frozen records
    selected = proof.resolveOperation(operation); // primary name or alias
    if (!selected) return { status: "operation-missing" };
    schema = proof.inputSchema(selected.key);
    if (!schema) return { status: "no-schema" };
    const setup = schema.prepare(prepareOptions);
    if (setup.status !== "ready") return setup;
    prepared = setup.schema;
    return { status: "checked", operations, result: prepared.explain(input) };
  } finally {
    prepared?.dispose();
    schema?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}
