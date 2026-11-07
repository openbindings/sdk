// Application-owned service lifetime, shared by Node and workerd adapters.
import { ExternalSchemas, Document, parseJson } from "@openbindings/sdk";

export function prepareService(documentBytes, entries = []) {
  const parsed = Document.parse(documentBytes);
  if (parsed.status !== "parsed") return parsed;
  let proof, operation, schema, resources;
  try {
    const checked = parsed.value.validate();
    if (checked.status !== "conformant") return checked;
    proof = checked.document;
    operation = proof.resolveOperation("lookup");
    if (!operation) return { status: "operation-missing" };
    schema = operation.input();
    if (!schema) return { status: "no-schema" };
    resources = new ExternalSchemas(entries); // borrowed schema owners
    // A ready result owns its prepared schema independently of setup locals.
    return schema.prepare({ resources });
  } finally {
    resources?.dispose();
    schema?.dispose();
    operation?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}

export class ValidationService {
  #active;
  constructor(schema) {
    this.#active = schema;
  } // ownership transfers in
  async checkBytes(body) {
    const held = this.#current().retain(); // acquire BEFORE the first await
    let value;
    try {
      const parsed = parseJson(await body);
      if (parsed.status !== "parsed") return parsed;
      value = parsed.value;
      return held.evaluate(value);
    } finally {
      value?.dispose();
      held.dispose();
    }
  }
  check(value, options) {
    return this.#current().evaluate(value, options);
  }
  replace(schema) {
    this.#current();
    if (schema === this.#active)
      throw new TypeError("Replacement must be an independently owned handle.");
    if (schema.disposed) throw new TypeError("Replacement is disposed.");
    const previous = this.#active;
    this.#active = schema;
    previous.dispose();
  }
  #current() {
    if (!this.#active) throw new Error("Service is closed.");
    return this.#active;
  }
  dispose() {
    this.#active?.dispose();
    this.#active = undefined;
  }
}
