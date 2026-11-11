import { constructDocument } from "./construct-document.mjs";
import * as testing from "../dist/testing.js";
import { prepareInput } from "./conformant-contracts.mjs";
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as discovery from "../dist/http-discovery.js";
import * as bridge from "../dist/wasm/openbindings_wasm.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const ready = (preparation) => {
  assert.equal(preparation.status, "ready");
  return preparation.schema;
};
const text =
  ' {"openbindings":"0.2.0","operations":{"run":{"input":{"type":"integer"},"aliases":["execute"]}}} ';
const parsed = (input) => {
  const result = sdk.Document.parse(input);
  assert.equal(result.status, "parsed");
  return result.value;
};
const exact = (input) => {
  const result = sdk.parseJson(input);
  assert.equal(result.status, "parsed");
  return result.value;
};
test("author every normative object; absence/null and exact opaque values survive", () => {
  const large = exact("9007199254740993");
  const document = constructDocument(sdk, {
    name: "example",
    operations: {
      run: {
        aliases: ["execute"],
        input: { type: "integer" },
        output: false,
        examples: { demo: { input: large, output: null } },
      },
    },
    sources: { local: { kind: "https://kind.example/raw", data: null } },
    bindings: {
      default: {
        operation: "run",
        source: "local",
        data: { large },
        preference: 1,
        idempotent: true,
      },
    },
    dependencies: {
      dep: { operation: "run", kinds: ["https://kind.example/raw"] },
    },
    schemas: { extra: true },
    "x-extra": { large },
  });
  large.dispose();
  const validation = document.validate();
  assert.equal(validation.status, "conformant");
  const value = document.value();
  assert.equal(value.get("description"), undefined);
  for (const path of [
    "/sources/local/data",
    "/operations/run/examples/demo/output",
  ]) {
    const child = value.at(path);
    assert.equal(child.text, "null");
    child.dispose();
  }
  const preserved = value.at("/bindings/default/data/large");
  assert.equal(preserved.text, "9007199254740993");
  assert.equal(preserved.toValue().status, "inexact");
  preserved.dispose();
  value.dispose();
  validation.document.dispose();
  document.dispose();
  const edited = sdk.Document.create();
  assert.throws(
    () => edited.set("/operations/run", { "x-missing": undefined }),
    (e) =>
      e.code === "unsupported-value" && e.inputPointer === "/0/value/x-missing",
  );
  const malformed = edited.set("/operations/run", { description: null });
  assert.equal(malformed.validate().status, "report");
  malformed.dispose();
  edited.dispose();
});
test("ordinary conversion rejects lossy/side-effecting values and exact handles retain ownership", () => {
  let getter = 0;
  for (const value of [
    undefined,
    1n,
    NaN,
    Infinity,
    new Date(),
    [undefined],
    { a: undefined },
    {
      get a() {
        getter++;
        return 1;
      },
    },
    [, ...[]],
    Object.assign([1], { extra: 2 }),
  ])
    assert.throws(() => sdk.ExactJson.from(value));
  assert.equal(getter, 0);
  const value = exact('{"n":9007199254740993,"empty":null}');
  const child = value.get("n");
  value.dispose();
  assert.equal(child.text, "9007199254740993");
  assert.equal(child.toValue().status, "inexact");
  child.dispose();
  child.dispose();
  assert.throws(() => child.text, /disposed/);
  assert.throws(
    () => child.toValue(),
    (e) => e instanceof sdk.SdkError && e.code === "disposed-handle",
  );
  const duplicate = exact('{"a":1,"\\u0061":2}');
  assert.equal(duplicate.toValue().status, "inexact");
  duplicate.dispose();
  const ordinary = sdk.ExactJson.from({ small: 1, null: null, text: "é" });
  assert.deepEqual(ordinary.toValue(), {
    status: "converted",
    value: { small: 1, null: null, text: "é" },
  });
  ordinary.dispose();
});
test("exact conversion propagates disposal during host conversion", () => {
  const value = exact("1");
  const parse = JSON.parse;
  JSON.parse = (...args) => {
    const result = parse(...args);
    if (args[0] === "1") value.dispose();
    return result;
  };
  try {
    assert.throws(
      () => value.toValue(),
      (e) => e instanceof sdk.SdkError && e.code === "disposed-handle",
    );
  } finally {
    JSON.parse = parse;
    value.dispose();
  }
});
test("ordinary number serialization stays distinct from exact JSON conversion", () => {
  for (const [text, status] of [
    ["9007199254740993", "inexact"],
    ["9007199254740992", "converted"],
    ["1000000000000000128", "inexact"],
    ["1e999999", "inexact"],
  ]) {
    const value = exact(text);
    try {
      const result = value.toValue();
      assert.equal(result.status, status, text);
      if (result.status === "converted")
        assert.equal(result.value, 9007199254740992);
      assert.equal(value.text, text);
    } finally {
      value.dispose();
    }
  }
  const ordinary = sdk.ExactJson.from(1000000000000000128);
  try {
    assert.equal(ordinary.text, "1000000000000000100");
  } finally {
    ordinary.dispose();
  }
  const document = parsed(
    '{"openbindings":"0.2.0","operations":{"run":{"input":{"const":9007199254740993}}}}',
  );
  const schema = ready(prepareInput(document, "run"));
  const exactInput = exact("9007199254740993");
  try {
    assert.equal(schema.evaluate(9007199254740992).status, "fails");
    assert.equal(schema.evaluate(exactInput).status, "satisfies");
  } finally {
    exactInput.dispose();
    schema.dispose();
    document.dispose();
  }
});
test("retained contracts and metadata support repeated native-feeling caller use", () => {
  const warmDocument = parsed(text);
  ready(prepareInput(warmDocument, "run")).dispose();
  warmDocument.dispose();
  const before = testing.liveStorageOwners();
  for (let i = 0; i < 1000; i++) {
    const document = parsed(text);
    const checked = document.validate();
    assert.equal(checked.status, "conformant");
    const entries = checked.document.operations();
    assert.ok(Object.isFrozen(entries));
    for (const entry of entries) entry.dispose();
    const op = checked.document.resolveOperation("execute");
    assert.equal(op.key, "run");
    assert.deepEqual(op.bindings(), []);
    op.dispose();
    const run = checked.document.resolveOperation("run");
    const contract = run.input();
    const input = ready(contract.prepare());
    const output = run.output();
    contract.dispose();
    run.dispose();
    checked.document.dispose();
    document.dispose();
    const good = sdk.ExactJson.from(7),
      bad = sdk.ExactJson.from("private-value");
    assert.equal(input.evaluate(good).status, "satisfies");
    const mismatch = input.evaluate(bad);
    assert.equal(mismatch.status, "fails");
    assert.ok(!JSON.stringify(mismatch).includes("private-value"));
    assert.equal(output, undefined);
    good.dispose();
    bad.dispose();
    input.dispose();
  }
  assert.equal(testing.liveStorageOwners(), before);
});
test("publication copies the snapshot and uses ordinary Fetch request/response values", async () => {
  const document = parsed(text),
    valid = document.validate();
  assert.equal(valid.status, "conformant");
  const publication = new discovery.DiscoveryPublication(valid.document, {
    allowOrigin: "any",
  });
  valid.document.dispose();
  document.dispose();
  for (const accept of [undefined, "text/html", "application/json"]) {
    const response = publication.respond(
      new Request("https://example.test/.well-known/openbindings?ignored=1", {
        headers: accept ? { accept } : {},
      }),
    );
    assert.equal(response.status, 200);
    assert.equal(await response.text(), text);
    assert.equal(
      response.headers.get("content-type"),
      discovery.discoveryPolicy().mediaType,
    );
    assert.equal(response.headers.get("access-control-allow-origin"), "*");
  }
  const head = publication.respond(
    new Request("https://example.test/.well-known/openbindings", {
      method: "HEAD",
    }),
  );
  assert.equal(await head.text(), "");
  assert.equal(
    head.headers.get("content-length"),
    String(new TextEncoder().encode(text).length),
  );
  const denied = publication.respond(
    new Request("https://example.test/.well-known/openbindings", {
      method: "POST",
    }),
  );
  assert.equal(denied.status, 405);
  assert.equal(denied.headers.get("allow"), "GET, HEAD");
  assert.equal(
    publication.respond(new Request("https://example.test/other")).status,
    404,
  );
  publication.dispose();
});
test("Fetch discovery keeps assessment and transport distinctions with complete bounded bytes", async () => {
  for (const media of [
    "application/vnd.openbindings+json",
    "application/json",
    "application/json; charset=utf-8",
    "text/plain",
    null,
  ]) {
    const r = await discovery.discover("HTTPS://EXAMPLE.test/", {
      fetch: async (url, init) => {
        assert.equal(url, "https://example.test/.well-known/openbindings");
        assert.equal(init.headers.Accept, discovery.discoveryPolicy().accept);
        return new Response(text, {
          headers: media ? { "content-type": media } : {},
        });
      },
    });
    assert.equal(r.status, "found");
    assert.equal(new TextDecoder().decode(r.body), text);
    // The SDK follows hops itself, so the final URL is the URL it dispatched.
    assert.equal(
      r.metadata.finalUrl,
      "https://example.test/.well-known/openbindings",
    );
    r.document.dispose();
  }
  for (const [status, expected] of [
    [301, "http-status"],
    [302, "http-status"],
    [303, "http-status"],
    [307, "http-status"],
    [308, "http-status"],
    [401, "gated"],
    [403, "gated"],
    [404, "absent"],
    [500, "http-status"],
  ]) {
    let cancelled = false;
    const body = new ReadableStream({
      cancel() {
        cancelled = true;
      },
    });
    const r = await discovery.discover("https://example.test", {
      fetch: async () =>
        new Response(body, {
          status,
          headers: { "www-authenticate": "Bearer dummy" },
        }),
    });
    assert.equal(r.status, expected);
    assert.equal(r.metadata.headers["www-authenticate"], "Bearer dummy");
    assert.ok(cancelled);
    assert.equal(r.body, undefined);
  }
  for (const [body, expected] of [
    ["{", "non-conformant"],
    ['{"openbindings":"8.0.0"}', "version-refused"],
    // OBI-01 excludes the surrogate before later document rules apply.
    [
      '{"openbindings":"0.2.0","operations":{},"x":"\\ud800"}',
      "non-conformant",
    ],
  ]) {
    const r = await discovery.discover("https://example.test", {
      fetch: async () => new Response(body),
    });
    assert.equal(r.status, expected);
    assert.equal(new TextDecoder().decode(r.body), body);
    r.document?.dispose();
  }
  for (const limit of [undefined, text.length, text.length - 1]) {
    const r = await discovery.discover("https://example.test", {
      maxDocumentBytes: limit,
      fetch: async () => new Response(text),
    });
    assert.equal(r.status, limit === text.length - 1 ? "body-limit" : "found");
    r.document?.dispose();
  }
  for (const maxDocumentBytes of [-1, 0, Number.MAX_SAFE_INTEGER + 1]) {
    await assert.rejects(
      () => discovery.discover("https://example.test", { maxDocumentBytes }),
      (e) => e instanceof sdk.SdkError && e.code === "invalid-byte-limit",
    );
  }
  const broken = await discovery.discover("https://example.test", {
    fetch: async () =>
      new Response(
        new ReadableStream({
          start(c) {
            c.error(new Error("test body failure"));
          },
        }),
      ),
  });
  assert.equal(broken.status, "body-error");
  assert.equal(broken.metadata.status, 200);
});
test("AbortSignal cancels before dispatch and during a pending reader", async () => {
  const before = new AbortController();
  before.abort();
  const r = await discovery.discover("https://example.test", {
    signal: before.signal,
    fetch: async () => {
      throw new Error("must not dispatch");
    },
  });
  assert.equal(r.status, "cancelled");
  assert.equal(r.metadata, undefined);
  const during = new AbortController();
  let released = false;
  const pending = discovery.discover("https://example.test", {
    signal: during.signal,
    fetch: async () =>
      new Response(
        new ReadableStream({
          cancel() {
            released = true;
          },
        }),
      ),
  });
  setTimeout(() => during.abort(), 5);
  const result = await pending;
  assert.equal(result.status, "cancelled");
  assert.equal(result.metadata.status, 200);
  assert.ok(released);
  const good = await discovery.discover("https://example.test", {
    fetch: async () => new Response(text),
  });
  assert.equal(good.status, "found");
  good.document.dispose();
});
test("explicit resources, evaluator limits and pre-cancellation keep their distinctions", () => {
  const document = parsed(
    '{"openbindings":"0.2.0","operations":{"run":{"input":{"$ref":"https://example.test/schema"}}}}',
  );
  const schema = exact('{"type":"integer"}'),
    empty = new sdk.ExternalSchemas(),
    resources = empty.with("https://example.test/schema", schema);
  empty.dispose();
  schema.dispose();
  const references = document.references({ resources });
  assert.equal(references.complete, true);
  assert.equal(references.references[0].resolution.status, "located");
  const control = new AbortController();
  control.abort();
  const checked = document.validate();
  assert.equal(checked.status, "conformant");
  const operation = checked.document.resolveOperation("run");
  checked.document.dispose();
  const contract = operation.input();
  operation.dispose();
  const cancelled = contract.prepare({ resources, signal: control.signal });
  const value = sdk.ExactJson.from(7);
  assert.equal(cancelled.status, "refused");
  assert.equal(cancelled.failure.kind, "cancelled");
  const healthy = ready(contract.prepare({ resources }));
  assert.equal(healthy.evaluate(value).status, "satisfies");
  assert.equal(
    healthy.evaluate(value, { signal: control.signal }).detail.reason,
    "cancelled",
  );
  assert.equal(healthy.evaluate(value).status, "satisfies");
  const input = ready(
    contract.prepare({ resources, limits: { evaluationSdkWorkUnits: 0 } }),
  );
  assert.equal(input.evaluate(value).status, "no-verdict");
  assert.equal(input.evaluate(value).detail.reason, "limit-exceeded");
  input.dispose();
  // By default no work cap applies; null selects that, and a number caps
  // every pass.
  assert.equal(sdk.evaluatorLimits().evaluationSdkWorkUnits, null);
  const grown = ready(
    contract.prepare({ resources, limits: { evaluationSdkWorkUnits: null } }),
  );
  assert.equal(grown.evaluate(value).status, "satisfies");
  grown.dispose();
  // Only evaluationSdkWorkUnits and schemaResources have null defaults, and the
  // refusal says so.
  assert.throws(
    () => contract.prepare({ limits: { patternDepth: null } }),
    (e) =>
      e.code === "invalid-evaluator-limits" &&
      e.message.includes("4294967295") &&
      e.message.includes(
        "evaluationSdkWorkUnits and schemaResources may also be null",
      ),
  );
  const partial = document.references({ resources, signal: control.signal });
  assert.equal(partial.complete, false);
  assert.equal(partial.limitation.reason, "cancelled");
  assert.throws(
    () => contract.prepare({ limits: { evaluationSdkWorkUnits: -1 } }),
    (e) => e.code === "invalid-evaluator-limits",
  );
  assert.throws(
    () => contract.prepare({ limits: { unknown: 1 } }),
    (e) => e.code === "invalid-evaluator-limits",
  );
  assert.equal("raw" in document, false);
  healthy.dispose();
  value.dispose();
  contract.dispose();
  resources.dispose();
  document.dispose();
});

test("Wasm reports retain specification evidence without application policy", () => {
  const document = parsed('{"openbindings":"0.2.0"}');
  assert.deepEqual(
    Object.keys(sdk.validateDocument(document.toBytes()).report).sort(),
    [
      "conclusion",
      "evidence",
      "findings",
      "findingsTruncated",
      "release",
      "revision",
    ],
  );
  document.dispose();
});

// Bulk resources preserve entries, refusals and independent owner lifetimes.
test("schema resources retain every entry and release owners", () => {
  const documents = Array.from({ length: 64 }, (_, i) =>
    exact(`{"type":"integer","title":"s${i}"}`),
  );
  const before = testing.liveStorageOwners();
  try {
    const uri = (i) => `https://example.test/s${i}`;
    const set = new sdk.ExternalSchemas(documents.map((d, i) => [uri(i), d]));
    for (const i of [0, 31, 63])
      assert.equal(
        set.locate({ sourceUri: uri(i), pointer: "/title" }).pointer,
        "/title",
      );
    set.dispose();
    new sdk.ExternalSchemas([]).dispose();
    new sdk.ExternalSchemas().dispose();
    // Refusals release collected entries and leave supplied owners usable.
    assert.throws(
      () =>
        new sdk.ExternalSchemas([
          [uri(0), documents[0]],
          [uri(0), documents[1]],
        ]),
      (e) => e instanceof sdk.SdkError && e.code === "resource",
    );
    assert.throws(
      () =>
        new sdk.ExternalSchemas([
          [uri(0), documents[0]],
          ["https://example.test/\ud800", documents[1]],
        ]),
      TypeError,
    );
    function* failing() {
      yield [uri(0), documents[0]];
      throw new Error("caller iterable failed");
    }
    assert.throws(() => new sdk.ExternalSchemas(failing()), /caller iterable/);
    assert.equal(documents[0].text, '{"type":"integer","title":"s0"}');
    assert.equal(testing.liveStorageOwners(), before);
  } finally {
    for (const document of documents) document.dispose();
  }
});

test("canonical operations retain their proof and contracts independently", () => {
  const before = testing.liveStorageOwners(),
    document = parsed(text);
  const checked = document.validate();
  assert.equal(checked.status, "conformant");
  const original = checked.document.resolveOperation("execute"),
    retained = original.retain();
  assert.ok(retained instanceof sdk.Operation);
  assert.notEqual(retained, original);
  const raw = document.value();
  assert.throws(
    () => sdk.Operation.prototype.retain.call(raw),
    (e) => e.code === "invalid-domain-owner",
  );
  raw.dispose();
  original.dispose();
  checked.document.dispose();
  document.dispose();
  assert.equal(retained.key, "run");
  const contract = retained.input();
  retained.dispose();
  const json = contract.json();
  assert.equal(json.text, '{"type":"integer"}');
  json.dispose();
  const prepared = ready(contract.prepare());
  contract.dispose();
  assert.equal(prepared.evaluate(7).status, "satisfies");
  prepared.dispose();
  assert.equal(testing.liveStorageOwners(), before);
});

// a string is JSON text here, admitted exactly as parseJson admits it.
test("evaluateJson admits JSON text as parseJson does, then validates", () => {
  const document = parsed(text);
  const contract = ready(prepareInput(document, "run"));
  document.dispose();
  const before = testing.liveStorageOwners();
  const utf8 = (value) => new TextEncoder().encode(value);
  assert.equal(contract.evaluate("7").status, "fails");
  assert.equal(contract.evaluateJson("7").status, "satisfies");
  assert.equal(contract.evaluateJson(utf8(" 7 ")).status, "satisfies");
  assert.equal(contract.evaluateJson("9007199254740993").status, "satisfies");
  assert.equal(contract.evaluateJson("1.5").status, "fails");
  const failed = contract.evaluateJson('"7"');
  assert.deepEqual(failed, contract.evaluate("7"));
  const duplicate = contract.evaluateJson('{"a":1,"a":2}');
  assert.equal(duplicate.status, "no-verdict");
  assert.equal(duplicate.detail.code, "duplicate-instance-members");
  // A refusal's message says what was expected and where; code and offset
  // stay the machine-readable fields.
  assert.deepEqual(sdk.parseJson('{"a" ').error, {
    byteOffset: 5,
    line: 1,
    byteColumn: 6,
    code: "colon",
    kind: "Syntax",
    message: "expected a colon after the member name at byte 5",
  });
  assert.equal(
    sdk.Document.parse("[1 2]").error.message,
    "expected a comma or the end of the container at byte 3",
  );
  const deep = "[".repeat(10_001) + "]".repeat(10_001);
  for (const input of [
    "7 x",
    "",
    deep,
    new Uint8Array([0xef, 0xbb, 0xbf, 0x37]),
    new Uint8Array([0xff]),
  ]) {
    const admitted = sdk.parseJson(input);
    assert.equal(admitted.status, "input-error");
    // A limit names parseJson with limits, which evaluateJson does not take;
    // every other refusal is parseJson's.
    const message =
      input === deep
        ? "the input nests deeper than the admitted depth limit at byte 10000; parse the value with JsonValue::parse_with_limits and a larger JsonLimits::max_depth, then validate it (parseJson with a larger limits.maxDepth in TypeScript)"
        : admitted.error.message;
    assert.deepEqual(contract.evaluateJson(input), {
      status: "input-error",
      error: { ...admitted.error, message },
    });
  }
  assert.throws(() => contract.evaluateJson("\ud800"), TypeError);
  assert.throws(() => contract.evaluateJson(7), TypeError);
  const control = new AbortController();
  control.abort();
  assert.equal(
    contract.evaluateJson("7", { signal: control.signal }).detail.reason,
    "cancelled",
  );
  assert.throws(
    () => contract.evaluateJson("7", { unknown: true }),
    (e) => e.code === "invalid-work-options",
  );
  assert.equal(testing.liveStorageOwners(), before);
  contract.dispose();
  assert.throws(
    () => contract.evaluateJson("7"),
    (e) => e.code === "disposed-handle",
  );
});
