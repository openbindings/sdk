import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import * as sdk from "../dist/index.js";
import {
  recordsSchema,
  recordsDoc,
  recordsValue,
} from "../../../tools/workers-memory/fixtures.mjs";
import {
  jsonLimits,
  preparationLimits,
  prepareOptions,
} from "../../../tools/workers-memory/guide.mjs";

await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

test("documented recipe options match the measured runner", async () => {
  const guide = await readFile(
    new URL("../../../docs/workers.md", import.meta.url),
    "utf8",
  );
  const code = guide.match(/```ts\n([\s\S]*?)```/)[1];
  const options = runInNewContext(
    code + "\n({ jsonLimits, preparationLimits, prepareOptions })",
  );
  assert.deepEqual(JSON.parse(JSON.stringify(options)), {
    jsonLimits,
    preparationLimits,
    prepareOptions,
  });
});

test("supplied records register and evaluate within the guide allowances after a refusal", () => {
  for (const n of [1000, 3000]) {
    const owners = [];
    const own = (value) => (owners.push(value), value);
    const parsed = (result) => (
      assert.equal(result.status, "parsed"),
      own(result.value)
    );
    try {
      const resource = parsed(
        sdk.parseJson(JSON.stringify(recordsSchema(n, false)), {
          limits: jsonLimits,
        }),
      );
      const limits = {
        ...preparationLimits,
        maxSdkReservationBytes: (n === 1000 ? 4 : 16) * 1048576,
      };
      const refused = { limits: { ...limits, maxSdkReservationBytes: 1024 } };
      const uri = "https://records.test/schema";
      assert.throws(() => new sdk.ExternalSchemas([[uri, resource]], refused), {
        code: "resource",
      });
      const empty = own(new sdk.ExternalSchemas([], { limits }));
      assert.throws(() => empty.with(uri, resource, refused), {
        code: "resource",
      });
      const sets = [
        own(new sdk.ExternalSchemas([[uri, resource]], { limits })),
        own(empty.with(uri, resource, { limits })),
      ];
      const document = parsed(
        sdk.Document.parse(recordsDoc(n, true, false), { limits: jsonLimits }),
      );
      const checked = document.validate({ limits: { maxFindings: 100 } });
      assert.equal(checked.status, "conformant");
      const proof = own(checked.document);
      const schema = own(proof.inputSchema("op"));
      const value = parsed(
        sdk.parseJson(JSON.stringify(recordsValue(n, false)), {
          limits: jsonLimits,
        }),
      );
      for (const resources of sets) {
        const result = schema.prepare({ ...prepareOptions, resources });
        assert.equal(result.status, "ready");
        assert.equal(own(result.schema).evaluate(value).status, "satisfies");
      }
    } finally {
      for (const owner of owners.reverse()) owner.dispose();
    }
  }
});
