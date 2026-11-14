import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
await sdk.initialize();

const cycle = {};
cycle.self = cycle;
const extra = [];
extra.extra = 1;
const accessor = Object.defineProperty({}, "SECRET_PROBE", {
  get() {
    throw new Error("must not run");
  },
});
const arrayAccessor = Object.defineProperty([1], "0", {
  get() {
    throw new Error("must not run");
  },
});
const hidden = Object.defineProperty({}, "SECRET_PROBE", { value: 1 });
const shapes = [
  [NaN, "non-finite-number", "JSON numbers must be finite."],
  [Infinity, "non-finite-number", "JSON numbers must be finite."],
  ...[undefined, 1n, () => {}, Symbol("SECRET_PROBE")].map((value) => [
    value,
    "unsupported-value",
    "JSON cannot contain undefined, bigint, functions, or symbols.",
  ]),
  [
    Array(1),
    "sparse-array",
    "JSON arrays must be dense and have no extra properties.",
  ],
  [
    extra,
    "array-property",
    "JSON arrays must be dense and have no extra properties.",
  ],
  [cycle, "cyclic-value", "JSON cannot contain cycles."],
  [new Date(0), "non-plain-object", "JSON objects must be plain objects."],
  [
    accessor,
    "accessor-property",
    "JSON objects must contain enumerable data properties.",
  ],
  [
    arrayAccessor,
    "accessor-property",
    "JSON arrays cannot contain accessors or holes.",
  ],
  [
    hidden,
    "non-enumerable-property",
    "JSON objects must contain enumerable data properties.",
  ],
  [
    { [Symbol("SECRET_PROBE")]: 1 },
    "symbol-key",
    "JSON objects cannot contain symbol keys.",
  ],
];

function prepared() {
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":true}}}',
  ).value;
  const proof = document.validate().document;
  const operation = proof.operation("run");
  const original = operation.input();
  const schema = original.prepare().schema;
  for (const owner of [original, operation, proof, document]) owner.dispose();
  return schema;
}

test("ordinary shape errors keep TypeError and fixed messages with stable codes", () => {
  const schema = prepared();
  const document = sdk.Document.create();
  try {
    for (const [value, code, message] of shapes) {
      assert.throws(
        () => sdk.ExactJson.from(value),
        (error) => {
          assert.equal(Object.getPrototypeOf(error), TypeError.prototype);
          assert.equal(error.code, code);
          assert.equal(error.message, message);
          assert.equal(error.message.includes("SECRET_PROBE"), false);
          return true;
        },
      );
      for (const check of [
        () => schema.evaluate(value),
        () => schema.explain(value),
      ]) {
        const result = check();
        assert.equal(result.status, "input-error");
        assert.equal(result.error.code, code);
      }
      assert.throws(
        () => document.set("/value", value),
        (error) => error instanceof sdk.EditError && error.code === code,
      );
    }
  } finally {
    document.dispose();
    schema.dispose();
  }
});

function limitsAgreeOnCause(value, text, exactCode, message) {
  const schema = prepared();
  try {
    assert.throws(
      () => sdk.ExactJson.from(value),
      (error) => {
        assert.equal(Object.getPrototypeOf(error), RangeError.prototype);
        assert.equal(error.code, "input-limit");
        assert.equal(error.message, message);
        return true;
      },
    );
    assert.equal(schema.evaluate(value).error.code, "input-limit");
    assert.equal(schema.explain(value).error.code, "input-limit");
    assert.equal(sdk.parseJson(text).error.code, exactCode);
    assert.equal(schema.evaluateJson(text).error.code, exactCode);
  } finally {
    schema.dispose();
  }
}

test("ordinary and exact depth refusals retain their different codes", () => {
  let value = null;
  for (let i = 0; i < 10_001; i++) value = [value];
  limitsAgreeOnCause(
    value,
    "[".repeat(10_001) + "null" + "]".repeat(10_001),
    "depth-limit",
    "Ordinary value exceeds the 10000-level nesting limit; parse it with parseJson and a larger limits.maxDepth.",
  );
});

test("ordinary and exact node refusals retain their different codes", () => {
  limitsAgreeOnCause(
    Array(1_000_000).fill(null),
    "[" + "null,".repeat(999_999) + "null]",
    "node-limit",
    "Ordinary value exceeds the 1000000-node limit; parse it with parseJson and a larger limits.maxNodes.",
  );
});

test("ordinary character and Rust UTF-8 byte refusals carry input-limit", () => {
  const ascii = "x".repeat(33_554_432);
  limitsAgreeOnCause(
    ascii,
    JSON.stringify(ascii),
    "byte-limit",
    "Ordinary value exceeds the 33554432-character limit; parse it with parseJson and a larger limits.maxBytes.",
  );
  const unicode = "☃".repeat(Math.ceil(33_554_432 / 3));
  limitsAgreeOnCause(
    unicode,
    JSON.stringify(unicode),
    "byte-limit",
    "The ordinary value exceeds the admitted JSON limits; parse it with parseJson and larger limits.",
  );
});

test("nested exact owners use the same limit code and caller exceptions are untouched", () => {
  const text = "[".repeat(10_000) + "null" + "]".repeat(10_000);
  const exact = sdk.parseJson(text).value;
  try {
    assert.throws(
      () => sdk.ExactJson.from([exact]),
      (error) => error instanceof RangeError && error.code === "input-limit",
    );
  } finally {
    exact.dispose();
  }
  const sentinel = new TypeError("caller exception");
  const proxy = new Proxy(
    {},
    {
      ownKeys() {
        throw sentinel;
      },
    },
  );
  assert.throws(
    () => sdk.ExactJson.from(proxy),
    (error) => error === sentinel && !Object.hasOwn(error, "code"),
  );
});

test("metadata exposes exactly the six JSON kinds", () => {
  for (const [text, kind] of [
    ["null", "null"],
    ["true", "boolean"],
    ["1", "number"],
    ['"x"', "string"],
    ["[]", "array"],
    ["{}", "object"],
  ]) {
    const value = sdk.parseJson(text).value;
    try {
      assert.equal(value.metadata.kind, kind);
    } finally {
      value.dispose();
    }
  }
});
