import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
import { chainDocument } from "./preparation-chain.mjs";
await sdk.initialize();

test("a 49000-link preparation refusal leaves the same instance usable", () => {
  const document = sdk.Document.parse(chainDocument(49_000)).value;
  const proof = document.validate().document;
  const operation = proof.operation("op");
  const schema = operation.input();
  try {
    for (const [preparationLimits, code] of [
      [
        { maxSdkReservationBytes: 8 * 1024 * 1024 },
        "preparation-sdk-reservation-bytes",
      ],
      [{ maxSdkWorkUnits: 1_000_000 }, "preparation-sdk-work-units"],
    ]) {
      const refused = schema.prepare({ preparationLimits });
      assert.equal(refused.status, "refused");
      assert.equal(refused.failure.details.resource, code);
      const small = sdk.parseJson('{"type":"integer"}').value;
      const resources = new sdk.ExternalSchemas([["urn:small", small]]);
      try {
        const ready = resources.prepare("urn:small");
        assert.equal(ready.status, "ready");
        try {
          assert.equal(ready.schema.evaluate(7).status, "satisfies");
          assert.equal(ready.schema.evaluate("wrong").status, "fails");
        } finally {
          ready.schema.dispose();
        }
      } finally {
        resources.dispose();
        small.dispose();
      }
    }
  } finally {
    for (const owner of [schema, operation, proof, document]) owner.dispose();
  }
});
