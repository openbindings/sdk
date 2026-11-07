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
export function firstUse(
  input = { id: 7 },
  documentBytes = exampleDocument,
  operation = "find",
) {
  const parsed = Document.parse(documentBytes);
  if (parsed.status !== "parsed") return parsed;
  let proof, selected, schema, prepared;
  try {
    const checked = parsed.value.validate();
    if (checked.status !== "conformant") return checked;
    proof = checked.document;
    const entries = proof.operations();
    let operations;
    try {
      operations = entries.map((entry) => ({
        key: entry.key,
        aliases: entry.aliases,
        description: entry.description,
      }));
    } finally {
      for (const entry of entries) entry.dispose();
    }
    selected = proof.resolveOperation(operation); // primary name or alias
    if (!selected) return { status: "operation-missing" };
    schema = selected.input();
    if (!schema) return { status: "no-schema" };
    const setup = schema.prepare();
    if (setup.status !== "ready") return setup;
    prepared = setup.schema;
    return { status: "checked", operations, result: prepared.evaluate(input) };
  } finally {
    prepared?.dispose();
    schema?.dispose();
    selected?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}
