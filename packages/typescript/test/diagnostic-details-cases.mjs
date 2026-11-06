import * as testing from "../dist/testing.js";
import { prepareInput } from "./conformant-contracts.mjs";
/** Public-package location/privacy/retention controls; portable across Node and browsers. Explanations name each failing keyword at its original location and include exact schema facts without rejected instance data. */
export function diagnosticDetailsCases(sdk) {
  let assertions = 0;
  const check = (ok, text) => {
    assertions++;
    if (!ok) throw new Error(text);
  };
  const utf8 = (text) => new TextEncoder().encode(text).length;
  function prepare(text, options = {}) {
    const parsed = sdk.Document.parse(text);
    check(parsed.status === "parsed", "parse");
    let proof;
    try {
      const checked = parsed.value.validate();
      check(checked.status === "conformant", "proof");
      proof = checked.document;
      const ready = prepareInput(proof, "op", options);
      check(ready.status === "ready", "ready");
      return ready.schema;
    } finally {
      proof?.dispose();
      parsed.value.dispose();
    }
  }
  const doc = (schema) =>
    `{"openbindings":"0.2.0","operations":{"op":{"input":${schema}}}}`;
  // Check cheaply, or explain when `call` is "explain".
  function run(schema, input, call = "evaluate") {
    const contract = prepare(doc(schema));
    const value = sdk.parseJson(input);
    check(value.status === "parsed", "exact input");
    try {
      return contract[call](value.value);
    } finally {
      value.value.dispose();
      contract.dispose();
    }
  }
  const secret = "SECRET-instance";
  const numericSecret = "987654321.123456789";
  const cases = [
    [
      '{"type":"integer"}',
      '"SECRET-instance"',
      "type",
      { kind: "type", expected: ["integer"] },
      secret,
    ],
    [
      '{"type":["null","object"]}',
      '"SECRET-instance"',
      "type",
      { kind: "type", expected: ["null", "object"] },
      secret,
    ],
    [
      '{"required":["SECRET/member~key"]}',
      '{"present":"SECRET-instance"}',
      "required",
      { kind: "required", member: "SECRET/member~key" },
      secret,
    ],
    [
      String.raw`{"enum":["SECRET-choice",9007199254740993,{ "x" : 1e-999 },"schema\u002dchoice"]}`,
      '"SECRET-instance"',
      "enum",
      {
        kind: "enum",
        choices: [
          '"SECRET-choice"',
          "9007199254740993",
          '{ "x" : 1e-999 }',
          String.raw`"schema\u002dchoice"`,
        ],
      },
      secret,
    ],
  ];
  for (const [keyword, token] of [
    ["minimum", "1E+400"],
    ["maximum", "-0.00"],
    ["exclusiveMinimum", "9007199254740993"],
    ["exclusiveMaximum", "1e-999"],
  ])
    cases.push([
      `{"${keyword}":${token}}`,
      numericSecret,
      keyword,
      { kind: "numeric-bound", bound: token },
      numericSecret,
    ]);
  for (const [minimum, maximum, input] of [
    ["minLength", "maxLength", '"SECRET-instance"'],
    ["minItems", "maxItems", '["SECRET-instance"]'],
    ["minProperties", "maxProperties", '{"present":"SECRET-instance"}'],
  ]) {
    for (const [keyword, token] of [
      [minimum, "1.000e+3"],
      [maximum, "0e0"],
    ])
      cases.push([
        `{"${keyword}":${token}}`,
        input,
        keyword,
        { kind: "size-bound", bound: token },
        secret,
      ]);
  }
  // Exact operands fail at their original keywords after the engine's numeric
  // substitution.
  for (const token of [
    "1e400",
    "1e" + "9".repeat(1000),
    "9007199254740993",
    "-0.00",
  ]) {
    for (const call of ["evaluate", "explain"]) {
      const result = run(`{"exclusiveMinimum":${token}}`, token, call);
      check(result.status === "fails", "exact exclusive bound");
      check(
        result.problemsComplete === (call === "explain"),
        "diagnostic completeness",
      );
      if (call === "evaluate")
        check(result.problems.length === 0, "cheap check omits payloads");
      else {
        const problem = result.problems[0];
        check(
          problem.details?.kind === "numeric-bound" &&
            problem.details.bound === token,
          "exact original numeric token",
        );
        check(
          problem.schema.pointer === "/operations/op/input/exclusiveMinimum",
          "original numeric location",
        );
        check(
          !problem.message.includes(token),
          "no instance number in message",
        );
      }
    }
  }
  const examples = [];
  for (const [schema, input, keyword, facts, rejected] of cases) {
    const ordinary = run(schema, input),
      detailed = run(schema, input, "explain");
    check(
      ordinary.status === "fails" && detailed.status === "fails",
      "same failure",
    );
    check(
      ordinary.problems.every(
        (p) => p.details === undefined && !p.message.includes("SECRET"),
      ),
      "private default",
    );
    const problem = detailed.problems[0];
    check(
      detailed.problemsComplete &&
        JSON.stringify(problem.details) === JSON.stringify(facts) &&
        problem.code === keyword &&
        problem.schema.pointer === "/operations/op/input/" + keyword &&
        !problem.message.includes("SECRET"),
      "original keyword location with exact schema facts",
    );
    check(
      !JSON.stringify(detailed).includes(rejected),
      "rejected value never serialized",
    );
    check(
      problem.message === "value fails this schema assertion",
      "fixed message",
    );
    check(
      JSON.stringify(
        JSON.parse(JSON.stringify(detailed)).problems[0].details,
      ) === JSON.stringify(facts),
      "serialized facts",
    );
    if (keyword === "required")
      check(problem.instance === "", "required parent location");
    examples.push(detailed);
  }
  const missing = run('{"required":["z","a"]}', "{}", "explain");
  check(
    missing.problemsComplete &&
      JSON.stringify(missing.problems.map((p) => p.details)) ===
        JSON.stringify([
          { kind: "required", member: "z" },
          { kind: "required", member: "a" },
        ]),
    "missing members retain array order",
  );
  for (const [schema, input] of [
    ['{"const":"SECRET-schema"}', '"SECRET-instance"'],
    ['{"pattern":"^SECRET-pattern$"}', '"SECRET-instance"'],
    ['{"multipleOf":2}', "3"],
    ['{"additionalProperties":false}', '{"present":"SECRET-instance"}'],
  ]) {
    const result = run(schema, input, "explain");
    check(
      result.status === "fails" &&
        result.problemsComplete &&
        result.problems.length === 1,
      "unsupported fact keyword still explained",
    );
    check(
      result.problems[0].details === undefined &&
        !JSON.stringify(result).includes("SECRET"),
      "no variant means no facts, pattern text stays out",
    );
  }
  const hostile = prepare(
    doc(
      JSON.stringify({
        items: {
          enum: Array.from(
            { length: 64 },
            (_, i) => `${i}:${"s".repeat(8192)}`,
          ),
        },
      }),
    ),
    { limits: { evaluationSdkReservationBytes: 256 * 1024 } },
  );
  try {
    const value = Array(32).fill(false);
    check(
      hostile.evaluate(value).status === "fails",
      "hostile enum establishes failure",
    );
    const result = hostile.explain(value);
    check(
      result.status === "fails" &&
        !result.problemsComplete &&
        result.problems.length === 32,
      "unfunded enum facts preserve all located failures",
    );
    check(
      result.problems.every((p) => p.details === undefined),
      "unfunded enum has no partial choices",
    );
  } finally {
    hostile.dispose();
  }
  const largeName = "é".repeat(10000);
  const large = run(JSON.stringify({ required: [largeName] }), "{}", "explain");
  check(
    large.problems[0].details?.member === largeName &&
      !large.problems[0].message.includes(largeName),
    "large schema member appears only in details",
  );
  const resourceText =
    '{"allOf":[{"properties":{"a/~":{"minimum":9007199254740993}}}],"x-padding":"' +
    "x".repeat(65536) +
    '"}';
  const replacementText = '{"properties":{"a/~":{"minimum":7}}}';
  const documentText =
    '{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/schema"}}},"x-padding":"' +
    "y".repeat(32768) +
    '"}';
  function fromResource(text) {
    const parsed = sdk.parseJson(text);
    check(parsed.status === "parsed", "resource parse");
    let resources;
    try {
      resources = new sdk.ExternalSchemas([
        ["https://example.test/schema", parsed.value],
      ]);
      return prepare(documentText, { resources });
    } finally {
      resources?.dispose();
      parsed.value.dispose();
    }
  }
  const unpaired = fromResource(String.raw`{"required":["\ud800","scalar"]}`);
  try {
    const result = unpaired.explain({});
    check(
      result.status === "fails" &&
        !result.problemsComplete &&
        result.problems.length === 2,
      "unrepresentable fact keeps failure incomplete",
    );
    check(
      result.problems[0].details === undefined &&
        result.problems[1].details.member === "scalar",
      "no lossy required member",
    );
  } finally {
    unpaired.dispose();
  }
  const exactEnum = fromResource(String.raw`{"enum":["\ud800"]}`);
  try {
    const result = exactEnum.explain(null);
    check(
      result.problemsComplete &&
        result.problems[0].details.choices[0] === String.raw`"\ud800"`,
      "enum retains exact unpaired token text",
    );
  } finally {
    exactEnum.dispose();
  }
  // Warm all process-global schema arenas before comparing application retention.
  let warm = fromResource(resourceText);
  warm.dispose();
  const baseline = testing.liveStorageOwners();
  const defaultContract = fromResource(resourceText);
  const defaultArenas = testing.liveStorageOwners();
  defaultContract.dispose();
  const old = fromResource(resourceText);
  const newer = fromResource(replacementText);
  const preparedArenas = testing.liveStorageOwners();
  let explainedArenas;
  try {
    const oldResult = old.explain({ "a/~": 6 }),
      newResult = newer.explain({ "a/~": 6 });
    explainedArenas = testing.liveStorageOwners();
    check(
      oldResult.problems[0].details.bound === "9007199254740993" &&
        newResult.problems[0].details.bound === "7",
      "facts use each original resource",
    );
    check(
      oldResult.problems[0].schema.pointer ===
        "/allOf/0/properties/a~1~0/minimum" &&
        newResult.problems[0].schema.pointer === "/properties/a~1~0/minimum",
      "replacement preserves original resource",
    );
    check(
      oldResult.problems[0].instance === "/a~1~0" &&
        oldResult.problems[0].schema.pointer ===
          "/allOf/0/properties/a~1~0/minimum",
      "escaped nested coordinates",
    );
  } finally {
    old.dispose();
    newer.dispose();
  }
  const released = testing.liveStorageOwners();
  check(released === baseline, "all application owners released");
  // Each original plan retains its document and supplied resource. Explain
  // adds no source owners; releasing every plan returns to the warm baseline.
  check(
    defaultArenas === baseline + 2 &&
      preparedArenas === baseline + 4 &&
      explainedArenas === preparedArenas,
    "original plans retain only their owned source environments",
  );
  const probe = sdk.Document.parse(doc("true"));
  check(probe.status === "parsed", "option probe");
  try {
    let error;
    try {
      prepareInput(probe.value, "op", { unknownPreparationOption: true });
    } catch (e) {
      error = e;
    }
    check(
      error?.code === "invalid-work-options",
      "unknown preparation option rejected",
    );
  } finally {
    probe.value.dispose();
  }
  check(
    testing.liveStorageOwners() === baseline,
    "invalid options release all owners",
  );
  return {
    diagnosticDetails: {
      assertions,
      examples,
      retention: {
        baseline,
        defaultArenas,
        explainedArenas,
        released,
        retainedOriginalSourceBytes: utf8(documentText) + utf8(resourceText),
        meaning:
          "logical exact arenas and known original byte extent; not heap, RSS or Wasm capacity",
      },
    },
  };
}
