// Application-owned service lifetime, shared by Node and workerd adapters.
import { ExternalSchemas, Document, parseJson } from "@openbindings/sdk";

// Starting policy from docs/workers.md; measure it for the application's shapes.
export const jsonLimits = Object.freeze({
  maxBytes: 4 * 1024 * 1024,
  maxNodes: 250_000,
  maxDepth: 10_000,
});
const preparationLimits = {
  maxSdkReservationBytes: 16 * 1024 * 1024,
  maxSdkWorkUnits: 100_000_000,
};
const limits = {
  evaluationSdkWorkUnits: 100_000_000,
  evaluationSdkReservationBytes: 4 * 1024 * 1024,
};

// Enforce the host-body bound before allocating a complete copy. Content-Length
// alone cannot bound streamed or misleadingly declared bodies.
export async function readBody(stream) {
  if (!stream) return new Uint8Array();
  const reader = stream.getReader();
  const chunks = [];
  let length = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value.byteLength > jsonLimits.maxBytes - length) {
        await reader.cancel();
        throw new RangeError("Request body exceeds the byte limit.");
      }
      chunks.push(value);
      length += value.byteLength;
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

export function responseStatus(result) {
  if (result.status === "satisfies") return 200;
  if (result.status === "fails") return 422;
  if (result.status === "input-error") return 400;
  return 503; // no-verdict and undefined are not evidence of an invalid value.
}

export function prepareService(documentBytes, entries = []) {
  const parsed = Document.parse(documentBytes, { limits: jsonLimits });
  if (parsed.status !== "parsed") return parsed;
  let proof, operation, schema, resources;
  try {
    const checked = parsed.value.validate({ limits: { maxFindings: 100 } });
    if (checked.status !== "conformant") return checked;
    proof = checked.document;
    operation = proof.resolveOperation("lookup");
    if (!operation) return { status: "operation-missing" };
    schema = proof.inputSchema(operation.key);
    if (!schema) return { status: "no-schema" };
    resources = new ExternalSchemas(entries, { limits: preparationLimits }); // borrowed schema owners
    // A ready result owns its prepared schema independently of setup locals.
    return schema.prepare({ resources, preparationLimits, limits });
  } finally {
    resources?.dispose();
    schema?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}

export class ValidationService {
  // This application-owned cache intentionally survives request awaits and is
  // bounded to one prepared schema. Requests acquire no owner until their body
  // is ready; replacement and disposal remain synchronous.
  #active;
  constructor(schema) {
    this.#active = schema;
  } // ownership transfers in
  async checkBytes(body) {
    const bytes = await body;
    const held = this.#current(); // select the active schema after body I/O
    const parsed = parseJson(bytes, { limits: jsonLimits });
    if (parsed.status !== "parsed") return parsed;
    try {
      return held.evaluate(parsed.value);
    } finally {
      parsed.value.dispose();
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
