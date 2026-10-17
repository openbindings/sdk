import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as bridge from "../dist/wasm/openbindings_wasm.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const text = String.raw`{"openbindings":"0.2.0","name":"\ud800\udc00","version":"\uffff","description":"\ud800\udc00","schemas":{"t":true,"f":false,"o":{}},"operations":{"run":{"aliases":["go"],"description":"\ud800\udc00","tags":["\ud800\udc00"],"input":true,"output":false,"examples":{"e":{"description":"\ud800\udc00","input":null}}}},"sources":{"s":{"kind":"\ud800\udc00","description":"\ud800\udc00","content":null}},"dependencies":{"d":{"operation":"run","kinds":["\ud800\udc00"],"description":"\ud800\udc00"}},"bindings":{"b":{"source":"s","operation":"run","description":"\ud800\udc00"}}}`;

test("Rust proof transfer is single-use and unavailable to parsed or forged handles", () => {
  const parsed = bridge.WasmDocument.parseBytes(new TextEncoder().encode(text));
  // A parsed document's operation carries exact schema values, never contracts.
  const selection = parsed.resolveOperation("go");
  const plain = selection.takeOperation();
  selection.free();
  assert.ok(!(plain instanceof bridge.WasmConformantOperation));
  const plainInput = plain.input();
  assert.ok(plainInput instanceof bridge.WasmJson);
  assert.equal(typeof plainInput.prepare, "undefined");
  plainInput.free();
  plain.free();
  const transfer = parsed.validate();
  assert.equal(JSON.parse(transfer.report()).report.conclusion, "conformant");
  const proof = transfer.takeConformant();
  assert.ok(proof instanceof bridge.WasmConformantDocument);
  assert.equal(transfer.takeConformant(), undefined);
  transfer.free();
  parsed.free();
  const retained = proof.retain();
  proof.free();
  const resources = new bridge.WasmResources();
  const operation = retained.resolveOperation("go");
  assert.ok(operation instanceof bridge.WasmConformantOperation);
  const contract = operation.input();
  assert.ok(contract instanceof bridge.WasmValueContract);
  const preparation = contract.prepareWith(resources, "{}", "coarse", false);
  assert.equal(JSON.parse(preparation.result()).status, "ready");
  preparation.free();
  contract.free();
  operation.free();
  resources.free();
  retained.free();
  for (const source of [
    '{"openbindings":"0.2.0"}',
    '{"openbindings":"0.3.0","operations":{}}',
  ]) {
    const unproven = bridge.WasmDocument.parseBytes(
      new TextEncoder().encode(source),
    );
    const result = unproven.validate();
    assert.equal(result.takeConformant(), undefined);
    assert.equal(result.takeConformant(), undefined);
    result.free();
    unproven.free();
  }
  const unproven = sdk.Document.parse(text).value;
  const selected = unproven.resolveOperation("go");
  assert.equal(selected.status, "found");
  assert.ok(!(selected.operation instanceof sdk.ConformantOperationView));
  const exactInput = selected.operation.input();
  assert.ok(exactInput instanceof sdk.ExactJson);
  assert.equal(typeof exactInput.prepare, "undefined");
  exactInput.dispose();
  selected.operation.dispose();
  assert.throws(() =>
    sdk.ConformantDocument.prototype.resolveOperation.call(unproven, "go"),
  );
  assert.throws(() =>
    sdk.ConformantDocument.prototype.resolveOperation.call(
      Object.create(sdk.ConformantDocument.prototype),
      "go",
    ),
  );
  // Overriding JS evidence cannot mint Rust proof.
  const invalid = sdk.Document.parse('{"openbindings":"0.2.0"}').value;
  invalid.validate = () => ({
    status: "report",
    report: { conclusion: "conformant" },
  });
  assert.equal(sdk.Document.prototype.validate.call(invalid).status, "report");
  invalid.dispose();
  unproven.dispose();
});

test("raw document receiver substitution rechecks proof at every authority entry", () => {
  const resources = new bridge.WasmResources();
  const refuses = (code) => (error) => JSON.parse(error).code === code;
  try {
    for (const source of [
      '{"openbindings":"0.2.0","name":7,"operations":{"op":{"input":true}}}',
      '{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"#/name"}}},"name":"opaque"}',
      '{"openbindings":"0.3.0","operations":{"op":{"input":true}}}',
    ]) {
      const parsed = bridge.WasmDocument.parseBytes(
        new TextEncoder().encode(source),
      );
      const transfer = parsed.validate();
      try {
        assert.equal(transfer.takeConformant(), undefined);
        // Claude's exact receiver-substitution probe: no pointer or prototype edits.
        // Exercise both live snapshot carriers, including consumed evidence.
        for (const receiver of [parsed, transfer]) {
          assert.throws(
            () =>
              bridge.WasmConformantDocument.prototype.resolveOperation.call(
                receiver,
                "op",
              ),
            refuses("document-conformance"),
          );
          assert.throws(
            () => bridge.WasmConformantDocument.prototype.retain.call(receiver),
            refuses("document-conformance"),
          );
          assert.equal(
            bridge.WasmDocumentValidation.prototype.takeConformant.call(
              receiver,
            ),
            undefined,
          );
          const again =
            bridge.WasmConformantDocument.prototype.validate.call(receiver);
          assert.equal(again.takeConformant(), undefined);
          again.free();
          const editor =
            bridge.WasmConformantDocument.prototype.asDocument.call(receiver);
          assert.throws(
            () => bridge.WasmConformantDocument.prototype.retain.call(editor),
            refuses("document-conformance"),
          );
          editor.free();
          // Publication's generated argument check uses instanceof. Pass that
          // check deliberately to verify Rust still derives the actual proof.
          const originalPrototype = Object.getPrototypeOf(receiver);
          try {
            Object.setPrototypeOf(
              receiver,
              bridge.WasmConformantDocument.prototype,
            );
            assert.throws(
              () => new bridge.WasmPublication(receiver, "*"),
              refuses("publication-conformance"),
            );
          } finally {
            Object.setPrototypeOf(receiver, originalPrototype);
          }
        }
      } finally {
        transfer.free();
        parsed.free();
      }
    }
    // A substituted conformant snapshot can derive real proof, but only for
    // itself. Keep owners after dropping the original receiver and transfer.
    const parsed = bridge.WasmDocument.parseBytes(
      new TextEncoder().encode(
        '{"openbindings":"0.2.0","name":"readable","operations":{"op":{"input":true}}}',
      ),
    );
    const retained =
      bridge.WasmConformantDocument.prototype.retain.call(parsed);
    const proof =
      bridge.WasmDocumentValidation.prototype.takeConformant.call(parsed);
    assert.ok(proof);
    assert.equal(
      bridge.WasmDocumentValidation.prototype.takeConformant.call(parsed),
      undefined,
    );
    const operation =
      bridge.WasmConformantDocument.prototype.resolveOperation.call(
        parsed,
        "op",
      );
    parsed.free();
    const publication = new bridge.WasmPublication(proof, "*");
    publication.free();
    proof.free();
    retained.free();
    const contract = operation.input();
    operation.free();
    const preparation = contract.prepareWith(resources, "{}", "coarse", false);
    assert.equal(JSON.parse(preparation.result()).status, "ready");
    const prepared = preparation.takeContract();
    const value = bridge.WasmJson.parseBytes(new TextEncoder().encode("42"));
    assert.equal(
      JSON.parse(prepared.validate(value, false)).outcome,
      "satisfies",
    );
    value.free();
    prepared.free();
    preparation.free();
    contract.free();
  } finally {
    resources.free();
  }
});

test("top-level editor reads preserve field-local errors, exact text and absence", () => {
  // Claude's metadata-parity probe: only the version field is malformed.
  const parsed = sdk.Document.parse(
    '{"openbindings":"0.2.0","name":"readable","version":7,"operations":{}}',
  ).value;
  assert.equal(parsed.openbindings, "0.2.0");
  assert.equal(parsed.name, "readable");
  assert.equal(parsed.description, undefined);
  assert.throws(
    () => parsed.version,
    (error) =>
      error.code === "interpretation" &&
      error.interpretationCode === "invalid-version" &&
      error.location.pointer === "/version",
  );
  parsed.dispose();
  for (const bad of ["name", "version", "description"]) {
    const values = {
      openbindings: "0.2.0",
      name: "",
      version: "\ud800",
      description: "\udc00",
    };
    const source = JSON.stringify({ ...values, [bad]: 7, operations: {} });
    const doc = sdk.Document.parse(source).value;
    const raw = bridge.WasmDocument.parseBytes(
      new TextEncoder().encode(source),
    );
    try {
      for (const [field, expected] of Object.entries(values)) {
        if (field === bad) {
          assert.throws(
            () => doc[field],
            (error) =>
              error.interpretationCode === `invalid-${bad}` &&
              error.location.pointer === `/${bad}`,
          );
          assert.throws(
            () => raw[field](),
            (error) => JSON.parse(error).location.pointer === `/${bad}`,
          );
        } else {
          assert.equal(doc[field], expected);
          assert.equal(JSON.parse(raw[field]()), expected);
        }
      }
    } finally {
      raw.free();
      doc.dispose();
    }
  }
});

test("validated text and retained reads preserve decoded text and presence", () => {
  const unproven = sdk.Document.parse(text).value;
  const result = unproven.validate();
  assert.equal(result.status, "conformant");
  const proof = result.document;
  assert.equal(proof.openbindings, "0.2.0");
  assert.equal(proof.name.charCodeAt(0), 0xd800);
  assert.equal(proof.version.charCodeAt(0), 0xffff);
  assert.equal(proof.description.charCodeAt(0), 0xd800);
  const parsed = proof.asDocument();
  assert.equal(parsed.name.charCodeAt(0), 0xd800);
  parsed.dispose();
  assert.equal(proof.operations[0].description.charCodeAt(0), 0xd800);
  assert.equal(proof.operations[0].tags[0].charCodeAt(0), 0xd800);
  assert.equal(proof.sources[0].kind.charCodeAt(0), 0xd800);
  assert.equal(proof.dependencies[0].kinds[0].charCodeAt(0), 0xd800);
  assert.equal(proof.bindings[0].description.charCodeAt(0), 0xd800);
  assert.equal(proof.dependencyAcceptsKind("d", "\ud800\udc00"), true);
  const schemas = proof.schemas();
  assert.deepEqual([...schemas.keys()], ["f", "o", "t"]);
  const operation = proof.resolveOperation("go");
  const example = operation.example("e");
  const inputContract = operation.input(),
    outputContract = operation.output();
  const input = inputContract.schema(),
    output = outputContract.schema();
  inputContract.dispose();
  outputContract.dispose();
  const exampleInput = example.input();
  assert.equal(example.output(), undefined);
  assert.equal(example.metadata.description.charCodeAt(0), 0xd800);
  example.dispose();
  operation.dispose();
  proof.dispose();
  unproven.dispose();
  assert.equal(input.text, "true");
  assert.equal(output.text, "false");
  assert.equal(exampleInput.text, "null");
  assert.deepEqual(
    [...schemas.values()].map((v) => v.text),
    ["false", "{}", "true"],
  );
  for (const value of [input, output, exampleInput, ...schemas.values()])
    value.dispose();
  for (const direction of ["input", "output"]) {
    for (const value of [undefined, "null", "true", "false", "{}"]) {
      const field = value === undefined ? "" : `,"${direction}":${value}`;
      const doc = sdk.Document.parse(
        `{"openbindings":"0.2.0","operations":{"run":{"description":"x"${field}}}}`,
      ).value;
      const op = doc.resolveOperation("run").operation;
      const acquired = op[direction]();
      assert.equal(acquired?.text, value);
      assert.equal(
        op.metadata[direction === "input" ? "hasInput" : "hasOutput"],
        value !== undefined,
      );
      acquired?.dispose();
      op.dispose();
      doc.dispose();
    }
  }
  for (const [fields, expected] of [
    ["", undefined],
    [',"schemas":{}', 0],
  ]) {
    const doc = sdk.Document.parse(
      `{"openbindings":"0.2.0","operations":{}${fields}}`,
    ).value;
    const proof = doc.validate().document;
    assert.equal(proof.schemas()?.size, expected);
    assert.equal(proof.name, undefined);
    proof.dispose();
    doc.dispose();
  }
});

test("conformant kind filters compare decoded text and handle unpaired caller strings", () => {
  const parsed = sdk.Document.parse(
    String.raw`{"openbindings":"0.2.0","name":"caf\u00e9","description":"\ud834\udd1e","version":"\uffff","operations":{"run":{}},"sources":{"s":{"kind":"caf\u00e9\ud834\udd1e"}},"dependencies":{"escaped":{"operation":"run","kinds":["caf\u00e9\uD834\uDD1E"]},"literal":{"operation":"run","kinds":["café𝄞"]},"all":{"operation":"run"}}}`,
  ).value;
  const checked = parsed.validate();
  assert.equal(checked.status, "conformant");
  const proof = checked.document;
  try {
    assert.equal(proof.name, "café");
    assert.equal(proof.description, "𝄞");
    assert.equal(proof.version, "\uffff");
    assert.equal(proof.sources[0].kind, "café𝄞");
    for (const key of ["escaped", "literal", "all"]) {
      assert.equal(proof.dependencyAcceptsKind(key, "café𝄞"), true);
    }
    assert.equal(proof.dependencyAcceptsKind("escaped", "cafe𝄞"), false);
    assert.equal(proof.dependencyAcceptsKind("literal", "\ud800"), false);
    assert.equal(proof.dependencyAcceptsKind("all", "\ud800"), true);
    assert.equal(proof.dependencyAcceptsKind("missing", "café𝄞"), undefined);
  } finally {
    proof.dispose();
    parsed.dispose();
  }
});

test("undetermined admission evidence cannot produce a proof through Rust/Wasm or TypeScript", () => {
  const source =
    '{"openbindings":"0.2.0","operations":{},"x":[' +
    "0,".repeat(1_000_000) +
    "0]}";
  const bytes = new TextEncoder().encode(source);
  const raw = JSON.parse(bridge.validateBytes(bytes));
  assert.equal(raw.report.conclusion, "undetermined");
  assert.throws(() => bridge.WasmDocument.parseBytes(bytes));
  const result = sdk.validateDocument(bytes);
  assert.equal(result.status, "report");
  assert.equal(result.report.conclusion, "undetermined");
  assert.equal(sdk.Document.parse(bytes).status, "input-error");
  assert.throws(() =>
    sdk.ConformantDocument.prototype.resolveOperation.call(result, "run"),
  );
});
