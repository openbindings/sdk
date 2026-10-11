// Application-owned service lifetime, shared by Node and workerd adapters.
import { SchemaResources, parseDocument, parseJson } from "@openbindings/sdk";

export function prepareService(documentBytes, entries = []) {
  const parsed = parseDocument(documentBytes);
  if (parsed.status !== "parsed") return parsed;
  let proof, resources, context;
  try {
    const checked = parsed.value.validate();
    if (checked.status !== "validated") return checked;
    proof = checked.document;
    resources = new SchemaResources(entries); // borrowed schema owners
    context = proof.contracts({ resources });
    // A ready result owns its contract independently of setup locals.
    return context.prepare("lookup", "input");
  } finally {
    context?.dispose();
    resources?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}

export class ValidationService {
  #active;
  constructor(contract) {
    requireComplete(contract);
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
    requireComplete(contract);
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
function requireComplete(contract) {
  if (contract.resourceCompleteness.status !== "complete")
    throw new TypeError(
      "This service requires complete schema resources before replacement.",
    );
}
