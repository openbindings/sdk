// Every case runs in a fresh Node process because a trapped instance stays unusable.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { chainDoc } from "../../../tools/workers-memory/fixtures.mjs";

const records = [];
globalThis.FinalizationRegistry = class {
  constructor(callback) {
    this.callback = callback;
  }
  register(_target, held) {
    records.push({ callback: this.callback, held });
  }
  unregister() {
    return true;
  }
};
const sdk = await import("../dist/index.js");
const http = await import("../dist/http-discovery.js");
const testing = await import("../dist/testing.js");
const bridge = await import("../dist/wasm/openbindings_wasm.js");
const mode = process.argv[2];
const trap = () => {
  throw new WebAssembly.RuntimeError("SECRET_PROBE");
};
const isTrap = (error) => {
  assert.ok(error instanceof sdk.SdkError);
  assert.equal(error.code, "wasm-trap");
  assert.equal(
    error.message,
    "The OpenBindings WebAssembly instance trapped and can no longer be used.",
  );
  assert.equal(Object.hasOwn(error, "cause"), false);
  assert.equal(error.stack.includes("SECRET_PROBE"), false);
  return true;
};
if (mode === "initialize") {
  const module = await WebAssembly.compile(
    await readFile(
      new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
    ),
  );
  const instantiate = WebAssembly.instantiate;
  let attempts = 0;
  WebAssembly.instantiate = (source, ...args) => {
    if (source === module) {
      attempts++;
      trap();
    }
    return Reflect.apply(instantiate, WebAssembly, [source, ...args]);
  };
  await assert.rejects(sdk.initialize(module), isTrap);
  await assert.rejects(sdk.initialize(module), isTrap);
  assert.equal(attempts, 1);
  assert.throws(() => sdk.Document.create(), isTrap);
} else {
  await sdk.initialize();
  const text =
    '{"openbindings":"0.2.0","operations":{"op":{"input":{"type":"integer"}}}}';
  const document = sdk.Document.parse(text).value;
  const proof = document.validate().document;
  const operation = proof.operation("op");
  const schema = operation.input();
  const prepared = schema.prepare().schema;
  const exact = sdk.parseJson('{"a":1}').value;
  const cursor = exact.members();
  const resources = new sdk.ExternalSchemas();
  const publication = new http.DiscoveryPublication(proof);
  // Warm cached metadata too: it must not remain usable after a trap.
  assert.equal(operation.key, "op");
  const owners = [
    publication,
    resources,
    cursor,
    exact,
    prepared,
    schema,
    operation,
    proof,
    document,
  ];
  let invoke;
  switch (mode) {
    case "memory": {
      const largeDocument = sdk.Document.parse(chainDoc(49_000)).value;
      const largeProof = largeDocument.validate().document;
      const largeOperation = largeProof.operation("op");
      const largeSchema = largeOperation.input();
      owners.push(largeSchema, largeOperation, largeProof, largeDocument);
      const prepare = bridge.WasmSchema.prototype.prepare;
      bridge.WasmSchema.prototype.prepare = function (...args) {
        try {
          return Reflect.apply(prepare, this, args);
        } catch (error) {
          assert.ok(error instanceof WebAssembly.RuntimeError);
          assert.equal(error.message, "unreachable");
          assert.equal(Object.hasOwn(error, "cause"), false);
          throw error;
        }
      };
      invoke = () => largeSchema.prepare();
      break;
    }
    case "parse":
      bridge.WasmJson.parseText = trap;
      invoke = () => sdk.parseJson("0");
      break;
    case "ordinary-parser":
      bridge.WasmJson.parseText = trap;
      invoke = () => sdk.ExactJson.from(0);
      break;
    case "ordinary-evaluate":
      bridge.WasmJson.parseText = trap;
      invoke = () => prepared.evaluate(0);
      break;
    case "ordinary-conversion":
      bridge.WasmJson.parseText = trap;
      invoke = () => exact.toValue();
      break;
    case "evaluate-json":
      bridge.WasmPrepared.prototype.evaluateJsonText = trap;
      invoke = () => prepared.evaluateJson("0");
      break;
    case "document-parse":
      bridge.WasmDocument.parseText = trap;
      invoke = () => sdk.Document.parse(text);
      break;
    case "create":
      bridge.WasmDocument.create = trap;
      invoke = () => sdk.Document.create();
      break;
    case "read":
      bridge.WasmJson.prototype.text = trap;
      invoke = () => exact.text;
      break;
    case "iterator":
      bridge.WasmMembers.prototype.next = trap;
      invoke = () => cursor.next();
      break;
    case "preparation-transfer":
      bridge.WasmPreparation.prototype.result = trap;
      invoke = () => schema.prepare();
      break;
    case "validation-transfer":
      bridge.WasmDocumentValidation.prototype.report = trap;
      invoke = () => document.validate();
      break;
    case "discovery-assessment": {
      const controller = new AbortController();
      bridge.WasmDiscovery.prototype.result = () => {
        controller.abort();
        trap();
      };
      await assert.rejects(
        http.discover("https://trap.test", {
          signal: controller.signal,
          fetch: async () =>
            new Response(text, {
              headers: { "Content-Type": "application/json" },
            }),
        }),
        isTrap,
      );
      break;
    }
    case "resource-registration":
      bridge.WasmResourceEntries.prototype.push = trap;
      invoke = () =>
        new sdk.ExternalSchemas([["https://trap.test/schema", exact]]);
      break;
    case "edit":
      bridge.WasmDocument.prototype.editEncoded = trap;
      invoke = () => document.set("/value", 1);
      break;
    case "dispose":
      bridge.WasmJson.prototype.free = trap;
      invoke = () => exact.dispose();
      break;
    case "finalizer": {
      const own = records.find(
        (record) =>
          typeof record.held === "object" &&
          record.held instanceof bridge.WasmJson,
      );
      own.held.free = trap;
      assert.doesNotThrow(() => own.callback(own.held));
      break;
    }
    default:
      throw new Error("Unknown trap fixture mode.");
  }
  if (invoke) assert.throws(invoke, isTrap);
  // Prove fail-fast behavior without entering the runtime again, even where
  // its allocator or validation might otherwise still happen to work.
  const entered = () => assert.fail("reentered trapped Wasm");
  bridge.WasmSchema.prototype.prepare = entered;
  bridge.WasmDocument.prototype.validate = entered;
  bridge.WasmJson.parseText = entered;
  bridge.WasmJson.prototype.free = entered;
  bridge.WasmResources.prototype.free = entered;
  const options = {
    get limits() {
      assert.fail("read options after a trap");
    },
  };
  for (const call of [
    () => sdk.validateDocument(text),
    () => document.validate(),
    () => document.validate(options),
    () => document.references(options),
    () => document.set(null, exact),
    () => document.remove(null),
    () => proof.operation(null),
    () => exact.locate(null),
    () => exact.get(null),
    () => resources.with("https://trap.test/schema", exact, options),
    () => schema.prepare(),
    () => sdk.parseJson("0"),
    () => sdk.ExactJson.from(0),
    () => sdk.Document.create(),
    () => new sdk.ExternalSchemas(),
    () => new http.DiscoveryPublication(proof),
    () => exact.text,
    () => exact.locate("/a"),
    () => cursor.next(),
    () => operation.key,
    () => resources.retain(),
    () => resources.prepare("https://trap.test/schema"),
    () => prepared.evaluate(0),
    () => prepared.explain(exact),
    () =>
      publication.respond(
        new Request("https://trap.test/.well-known/openbindings"),
      ),
    () => http.discoveryPolicy(),
    () => testing.liveStorageOwners(),
  ])
    assert.throws(call, isTrap);
  await assert.rejects(sdk.initialize(), isTrap);
  await assert.rejects(
    http.discover("https://trap.test", { fetch: entered }),
    isTrap,
  );
  for (const owner of owners) {
    assert.doesNotThrow(() => owner.dispose());
    assert.doesNotThrow(() => owner[Symbol.dispose]());
    assert.equal(owner.disposed, true);
  }
  assert.doesNotThrow(() => cursor.return());
  // Deliver both facade and bindgen callbacks deterministically. The numeric
  // held pointers identify bindgen's own registries, which need guarding too.
  assert.ok(records.some((record) => typeof record.held === "number"));
  for (const record of records)
    assert.doesNotThrow(() => record.callback(record.held));
}
console.log(
  JSON.stringify({ mode, code: "wasm-trap", finalizers: records.length }),
);
