import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";

// Capture the module's memory so the test can observe what crosses into it.
let memory;
const instantiate = WebAssembly.instantiate;
WebAssembly.instantiate = async function (...args) {
  const result = await Reflect.apply(instantiate, WebAssembly, args);
  memory = (result instanceof WebAssembly.Instance ? result : result.instance)
    .exports.memory;
  return result;
};
try {
  await sdk.initialize(
    await readFile(
      new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
    ),
  );
} finally {
  WebAssembly.instantiate = instantiate;
}

const MAX = 8 * 1024 * 1024;
const limit = (error) =>
  error instanceof sdk.SdkError &&
  error.code === "resource" &&
  /max_identity_bytes/.test(error.message);

test("a resource URI past the identity allowance crosses only as its refusing prefix", () => {
  const value = sdk.parseJson("true").value;
  const set = new sdk.ExternalSchemas([["urn:first", value]]);
  try {
    // The largest transfer the bound allows: one unit past the allowance.
    assert.throws(() => set.with("urn:" + "x".repeat(MAX - 3), value), limit);
    const before = memory.buffer.byteLength;
    const huge = "urn:" + "x".repeat(8 * MAX);
    assert.throws(() => new sdk.ExternalSchemas([[huge, value]]), limit);
    assert.throws(() => set.with(huge, value), limit);
    const refused = set.prepare(huge);
    assert.equal(refused.status, set.prepare("urn:" + "x".repeat(MAX)).status);
    assert.notEqual(refused.status, "ready");
    assert.equal(memory.buffer.byteLength, before);
    // The cut never splits a scalar character, and multibyte text is refused alike.
    assert.throws(
      () =>
        set.with("urn:" + "x".repeat(MAX - 4) + "\u{1F600}".repeat(4), value),
      limit,
    );
    assert.throws(() => set.with("urn:" + "\u00e9".repeat(MAX), value), limit);
    assert.equal(value.text, "true");
    new sdk.ExternalSchemas([["urn:healthy", value]]).dispose();
  } finally {
    set.dispose();
    value.dispose();
  }
});

test("an earlier registration error still precedes an oversized URI, and iteration stops", () => {
  const value = sdk.parseJson("true").value;
  let continued = false;
  function* entries() {
    yield ["relative", value];
    yield ["urn:" + "x".repeat(8 * MAX), value];
    continued = true;
  }
  try {
    assert.throws(
      () => new sdk.ExternalSchemas(entries()),
      (error) =>
        error instanceof sdk.SdkError &&
        error.code === "resource" &&
        !/max_identity_bytes/.test(error.message),
    );
    assert.equal(continued, false);
  } finally {
    value.dispose();
  }
});
