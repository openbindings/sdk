import { liveStorageOwners } from "@openbindings/sdk/testing";
// node node_modules/@openbindings/sdk/examples/service-lifecycle.mjs
// The application retains ready owners and handles no-verdict at validation.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { initialize, parseJson } from "@openbindings/sdk";
import { prepareService, ValidationService } from "./service.mjs";

await initialize(
  await readFile(
    new URL(import.meta.resolve("@openbindings/sdk/openbindings.wasm")),
  ),
);
const uri = "https://schema.example/input";
const document = JSON.stringify({
  openbindings: "0.2.0",
  operations: { lookup: { input: { $ref: uri } } },
});
function prepare(schemaText, documentBytes = document) {
  const schema = parseJson(schemaText);
  if (schema.status !== "parsed") return schema;
  try {
    return prepareService(documentBytes, [[uri, schema.value]]);
  } finally {
    schema.value.dispose(); // prepareService borrowed this resource owner
  }
}
function replace(service, schemaText, documentBytes = document) {
  const next = prepare(schemaText, documentBytes);
  if (next.status !== "ready") return next;
  try {
    service.replace(next.valueContract); // ownership transfers only on success
    return { status: "replaced" };
  } catch (error) {
    next.valueContract.dispose();
    throw error;
  }
}

// Fixed evaluator storage can initialize lazily. Observe startup separately,
// then warm the complete job before checking repeated-use cleanup.
const startupArenas = liveStorageOwners();
const warm = prepare('{"type":"integer"}');
assert.equal(warm.status, "ready");
try {
  assert.equal(warm.valueContract.validate(7).status, "satisfies");
} finally {
  warm.valueContract.dispose();
}
const warmedArenas = liveStorageOwners();

const setup = prepare('{"type":"integer","const":9007199254740993}');
assert.equal(setup.status, "ready");
const service = new ValidationService(setup.valueContract);
let releaseBody, oldRequest;
const outcomes = {};
try {
  // The service retains the old contract before awaiting the request body.
  oldRequest = service.checkBytes(
    new Promise((resolve) => {
      releaseBody = resolve;
    }),
  );
  outcomes.replacement = replace(service, '{"type":"string"}');
  outcomes.current = service.check("new prepared value contract");
  releaseBody("9007199254740993");
  outcomes.inFlight = await oldRequest; // still uses the old exact const
  assert.equal(outcomes.current.status, "satisfies");
  assert.equal(outcomes.inFlight.status, "satisfies");

  outcomes.invalidReplacement = replace(
    service,
    '{"type":"integer"}',
    '{"openbindings":"0.2.0","operations":{"lookup":{"description":42}}}',
  );
  assert.equal(outcomes.invalidReplacement.status, "report");
  assert.equal(outcomes.invalidReplacement.report.conclusion, "non-conformant");
  outcomes.stillCurrent = service.check("replacement was refused");
  assert.equal(outcomes.stillCurrent.status, "satisfies");
  outcomes.incompleteReplacement = replace(
    service,
    '{"$ref":"https://schema.example/missing"}',
  );
  assert.equal(outcomes.incompleteReplacement.status, "replaced");
  assert.equal(service.check("missing resource").status, "no-verdict");
  replace(service, '{"type":"string"}');

  const cancellation = new AbortController();
  cancellation.abort();
  outcomes.cancelled = service.check("cancelled", {
    signal: cancellation.signal,
  });
  assert.equal(outcomes.cancelled.status, "no-verdict");
  assert.equal(outcomes.cancelled.detail.reason, "cancelled");
  outcomes.sameOwnerRecovery = service.check("after cancellation");
  assert.equal(outcomes.sameOwnerRecovery.status, "satisfies");
  outcomes.recovery = replace(service, '{"type":"integer"}');
  outcomes.ordinary = service.check(7);
  outcomes.exact = await service.checkBytes(
    Promise.resolve("9007199254740993"),
  );
  assert.equal(outcomes.ordinary.status, "satisfies");
  assert.equal(outcomes.exact.status, "satisfies");
} finally {
  // Settle an acquired request owner even if a later demonstration step throws.
  releaseBody?.("9007199254740993");
  try {
    await oldRequest;
  } finally {
    service.dispose();
  }
}
// Missing static content can prepare ready; a bare hole remains undecidable.
const partial = prepareService(document, []);
assert.equal(partial.status, "ready");
try {
  outcomes.missingResource = partial.valueContract.validate(7);
  assert.equal(outcomes.missingResource.status, "no-verdict");
  assert.equal(outcomes.missingResource.detail.reason, "resource-unavailable");
} finally {
  partial.valueContract.dispose();
}
const releasedArenas = liveStorageOwners();
assert.equal(releasedArenas, warmedArenas);
console.log(
  JSON.stringify(
    {
      outcomes,
      arenas: {
        startup: startupArenas,
        warmed: warmedArenas,
        released: releasedArenas,
      },
    },
    null,
    2,
  ),
);
