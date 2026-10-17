// Application-owned service lifetime, shared by Node and workerd adapters.
import { SchemaResources, Document, parseJson } from "@openbindings/sdk";

export function prepareService(documentBytes, entries = []) {
  const parsed = Document.parse(documentBytes);
  if (parsed.status !== "parsed") return parsed;
  let proof, operation, contract, resources;
  try {
    const checked = parsed.value.validate();
    if (checked.status !== "conformant") return checked;
    proof = checked.document;
    operation = proof.resolveOperation("lookup");
    if (!operation) return { status: "operation-missing" };
    contract = operation.input();
    if (!contract) return { status: "no-value-contract" };
    resources = new SchemaResources(entries); // borrowed schema owners
    // A ready result owns its prepared contract independently of setup locals.
    return contract.prepare({ resources });
  } finally {
    resources?.dispose();
    contract?.dispose();
    operation?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}

export class ValidationService {
  #active;
  constructor(contract) {
    this.#active = contract;
  } // ownership transfers in
  async checkBytes(body) {
    const held = this.#current().retain(); // acquire BEFORE the first await
    let value;
    try {
      const parsed = parseJson(await body);
      if (parsed.status !== "parsed") return parsed;
      value = parsed.value;
      return held.validate(value);
    } finally {
      value?.dispose();
      held.dispose();
    }
  }
  check(value, options) {
    return this.#current().validate(value, options);
  }
  replace(contract) {
    this.#current();
    if (contract === this.#active)
      throw new TypeError("Replacement must be an independently owned handle.");
    if (contract.disposed) throw new TypeError("Replacement is disposed.");
    const previous = this.#active;
    this.#active = contract;
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
