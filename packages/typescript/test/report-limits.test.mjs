import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
await sdk.initialize();

function open(body) {
  const parsed = sdk.Document.parse(JSON.stringify(body));
  assert.equal(parsed.status, "parsed");
  return parsed.value;
}

function plainValidation(document, limits) {
  const result =
    limits === undefined ? document.validate() : document.validate({ limits });
  if (result.status === "conformant") result.document.dispose();
  return result;
}

test("forty rename sites require completion and a caller-selected allowance", () => {
  const document = open({
    openbindings: "0.2.0",
    operations: {},
    schemas: {
      ["a".repeat(256 * 1024)]: {
        allOf: Array.from({ length: 40 }, () => ({ $ref: "#/schemas/t" })),
      },
      t: true,
    },
  });
  const result = document.validate();
  assert.equal(result.status, "conformant");
  try {
    assert.deepEqual(sdk.referenceLimits(), {
      maxReportBytes: 8 * 1024 * 1024,
    });
    assert.ok(Object.isFrozen(sdk.referenceLimits()));
    assert.notEqual(sdk.referenceLimits(), sdk.referenceLimits());
    for (const owner of [document, result.document]) {
      const partial = owner.references();
      assert.equal(partial.complete, false);
      assert.equal(Object.hasOwn(partial, "references"), false);
      assert.equal(partial.retained.length, 31);
      assert.equal(partial.limitation.code, "reference-report-bytes");
      const full = owner.references({
        limits: { maxReportBytes: 16 * 1024 * 1024 },
      });
      assert.equal(full.complete, true);
      assert.equal(full.references.length, 40);
      assert.equal(Object.hasOwn(full, "retained"), false);
      assert.equal(Object.hasOwn(full, "limitation"), false);
      const zero = owner.references({ limits: { maxReportBytes: 0 } });
      assert.equal(zero.complete, false);
      assert.deepEqual(zero.retained, []);
      assert.equal(owner.references().retained.length, 31);
    }
  } finally {
    result.document.dispose();
    document.dispose();
  }
});

test("validation retention can exceed the default or be zero without changing evidence", () => {
  const document = open({
    openbindings: "0.2.0",
    operations: {},
    schemas: Object.fromEntries(
      Array.from({ length: 4200 }, (_, i) => [`s${i}`, { type: 42 }]),
    ),
  });
  try {
    assert.deepEqual(sdk.validationLimits(), {
      maxFindings: 4096,
      maxPointerBytes: 8 * 1024 * 1024,
    });
    assert.ok(Object.isFrozen(sdk.validationLimits()));
    assert.notEqual(sdk.validationLimits(), sdk.validationLimits());
    const baseline = document.validate();
    assert.equal(baseline.status, "report");
    assert.equal(baseline.report.findings.length, 4096);
    assert.equal(baseline.report.findingsTruncated, true);
    for (const maxFindings of [0, 1, 4095, 4096, 4097, 4200]) {
      const actual = document.validate({ limits: { maxFindings } });
      assert.equal(actual.status, "report");
      assert.equal(actual.report.conclusion, "non-conformant");
      assert.deepEqual(actual.report.evidence, baseline.report.evidence);
      assert.equal(actual.report.findings.length, maxFindings);
      assert.equal(actual.report.findingsTruncated, maxFindings < 4200);
    }
    assert.deepEqual(document.validate(), baseline);
  } finally {
    document.dispose();
  }
});

test("validation pointer retention charges exact UTF-8 bytes", () => {
  const document = open({
    openbindings: "0.2.0",
    operations: {},
    schemas: { x: { type: 42 } },
  });
  try {
    const baseline = document.validate();
    const bytes = Buffer.byteLength(
      baseline.report.findings[0].location.pointer,
      "utf8",
    );
    for (const maxPointerBytes of [0, bytes - 1, bytes, 0xffffffff]) {
      const actual = document.validate({ limits: { maxPointerBytes } });
      assert.deepEqual(actual.report.evidence, baseline.report.evidence);
      assert.equal(actual.report.conclusion, baseline.report.conclusion);
      assert.equal(actual.report.findingsTruncated, maxPointerBytes < bytes);
      assert.equal(
        actual.report.findings.length,
        Number(maxPointerBytes >= bytes),
      );
      if (actual.report.findings.length)
        assert.deepEqual(actual.report.findings, baseline.report.findings);
    }
  } finally {
    document.dispose();
  }
});

test("zero finding retention preserves JSON failures and real conformance proofs", () => {
  for (const text of [
    '{"x":1,"x":2}',
    '{"x":"\\ud800"}',
    '{"openbindings":"0.2.0","operations":{}}',
    '{"openbindings":"0.3.0","operations":{}}',
  ]) {
    const document = sdk.Document.parse(text).value;
    try {
      const expected = plainValidation(document);
      const actual = plainValidation(document, {
        maxFindings: 0,
        maxPointerBytes: 0,
      });
      assert.equal(actual.status, expected.status);
      if (actual.status !== "version-refused") {
        assert.deepEqual(actual.report.evidence, expected.report.evidence);
        assert.equal(actual.report.conclusion, expected.report.conclusion);
        assert.deepEqual(actual.report.findings, []);
        assert.equal(
          actual.report.findingsTruncated,
          expected.report.findings.length > 0,
        );
      } else assert.deepEqual(actual, expected);
    } finally {
      document.dispose();
    }
  }
});

test("report limits reject invalid overrides before entering Wasm", () => {
  const document = open({ openbindings: "0.2.0", operations: {} });
  try {
    for (const n of [-1, 0.5, NaN, Infinity, 0x100000000, "1", null]) {
      assert.throws(
        () => document.references({ limits: { maxReportBytes: n } }),
        (e) => e.code === "invalid-reference-limits",
      );
      for (const key of ["maxFindings", "maxPointerBytes"])
        assert.throws(
          () => document.validate({ limits: { [key]: n } }),
          (e) => e.code === "invalid-validation-limits",
        );
    }
    for (const limits of [null, [], 1, { surprise: 1 }]) {
      assert.throws(
        () => document.references({ limits }),
        (e) => e.code === "invalid-reference-limits",
      );
      assert.throws(
        () => document.validate({ limits }),
        (e) => e.code === "invalid-validation-limits",
      );
    }
    assert.deepEqual(plainValidation(document, {}), plainValidation(document));
  } finally {
    document.dispose();
  }
});

test("validation pointer allowance can exceed the default", () => {
  const document = open({
    openbindings: "0.2.0",
    operations: {},
    schemas: {
      ["a".repeat(256 * 1024)]: {
        allOf: Array.from({ length: 40 }, () => ({ type: 42 })),
      },
    },
  });
  try {
    const baseline = document.validate();
    assert.equal(baseline.report.findings.length, 31);
    assert.equal(baseline.report.findingsTruncated, true);
    const raised = document.validate({
      limits: { maxPointerBytes: 16 * 1024 * 1024 },
    });
    assert.deepEqual(raised.report.evidence, baseline.report.evidence);
    assert.equal(raised.report.conclusion, baseline.report.conclusion);
    assert.equal(raised.report.findings.length, 40);
    assert.equal(raised.report.findingsTruncated, false);
  } finally {
    document.dispose();
  }
});

test("custom report transfer cannot retain proof after a raw receiver edit", async () => {
  const bridge = await import("../dist/wasm/openbindings_wasm.js");
  const raw = bridge.WasmDocument.parseBytes(
    new TextEncoder().encode('{"openbindings":"0.2.0","operations":{}}'),
  );
  const transfer = raw.validateWithLimits(0, 0);
  raw.free();
  try {
    assert.equal(JSON.parse(transfer.report()).report.conclusion, "conformant");
    bridge.WasmDocument.prototype.remove.call(transfer, "/openbindings");
    assert.equal(
      JSON.parse(transfer.report()).report.conclusion,
      "non-conformant",
    );
    assert.equal(transfer.takeConformant(), undefined);
  } finally {
    transfer.free();
  }
});

test("validation options reject unknown names and malformed option objects", () => {
  const document = open({ operations: {} });
  try {
    assert.equal(
      Object.hasOwn(sdk.Document.prototype, "validateWithLimits"),
      false,
    );
    for (const options of [
      null,
      [],
      1,
      "options",
      { maxFindings: 0 },
      { limits: {}, unexpected: true },
      { "SECRET-option": true },
    ]) {
      assert.throws(
        () => document.validate(options),
        (error) => {
          assert.equal(error.code, "invalid-validation-options");
          assert.equal(error.message.includes("SECRET-option"), false);
          return true;
        },
      );
    }
  } finally {
    document.dispose();
  }
});

test("omitted validation limits keep the cached default bridge route", async () => {
  const bridge = await import("../dist/wasm/openbindings_wasm.js");
  const document = open({ operations: {} });
  const prototype = bridge.WasmDocument.prototype;
  const originalDefault = prototype.validate;
  const originalLimited = prototype.validateWithLimits;
  let defaults = 0;
  let limited = 0;
  prototype.validate = function (...args) {
    defaults++;
    return originalDefault.apply(this, args);
  };
  prototype.validateWithLimits = function (...args) {
    limited++;
    return originalLimited.apply(this, args);
  };
  try {
    const baseline = document.validate();
    for (const options of [undefined, {}, { limits: undefined }])
      assert.deepEqual(document.validate(options), baseline);
    assert.equal(defaults, 4);
    assert.equal(limited, 0);
    assert.deepEqual(document.validate({ limits: {} }), baseline);
    const omitted = document.validate({
      limits: { maxFindings: 0, maxPointerBytes: 0 },
    });
    assert.deepEqual(omitted.report.evidence, baseline.report.evidence);
    assert.deepEqual(omitted.report.findings, []);
    assert.equal(limited, 2);
    assert.deepEqual(document.validate({}), baseline);
    assert.equal(defaults, 5);
  } finally {
    prototype.validate = originalDefault;
    prototype.validateWithLimits = originalLimited;
    document.dispose();
  }
});
