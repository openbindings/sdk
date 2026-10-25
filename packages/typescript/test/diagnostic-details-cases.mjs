import * as testing from "../dist/testing.js";
import { prepareInput } from "./conformant-contracts.mjs";
/** Public-package location/privacy/retention controls; portable across Node and browsers. Explanations name each failing keyword at its original location and carry no schema facts. */
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
      return ready.valueContract;
    } finally {
      proof?.dispose();
      parsed.value.dispose();
    }
  }
  const doc = (schema) =>
    `{"openbindings":"0.2.0","operations":{"op":{"input":${schema}}}}`;
  // Check cheaply, or explain when `call` is "explain".
  function run(schema, input, call = "validate") {
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
  const cases = [
    ['{"type":"integer"}', '"SECRET-instance"', "type"],
    ['{"required":["SECRET/member~key"]}', "{}", "required"],
    ['{"minimum":9007199254740993}', "9007199254740992", "minimum"],
    ['{"maxLength":3}', '"long"', "maxLength"],
    [
      '{"enum":["SECRET-choice",9007199254740993,{"x":1e-999}]}',
      "null",
      "enum",
    ],
  ];
  // Exact operands fail at their original keywords after the engine's numeric
  // substitution.
  for (const token of [
    "1e400",
    "1e" + "9".repeat(1000),
    "9007199254740993",
    "-0.00",
  ]) {
    for (const call of ["validate", "explain"]) {
      const result = run(`{"exclusiveMinimum":${token}}`, token, call);
      check(result.status === "fails", "exact exclusive bound");
      check(!result.problemsComplete, "truthful first-error completeness");
      if (call === "validate")
        check(result.problems.length === 0, "cheap check omits payloads");
      else {
        const problem = result.problems[0];
        check(problem.details === undefined, "no schema facts");
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
  for (const [schema, input, keyword] of cases) {
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
      !detailed.problemsComplete &&
        problem.details === undefined &&
        problem.code === keyword &&
        problem.schema.pointer === "/operations/op/input/" + keyword &&
        !problem.message.includes("SECRET"),
      "original keyword location without facts",
    );
    if (keyword === "required")
      check(problem.instance === "", "required parent location");
    examples.push(detailed);
  }
  const largeName = "é".repeat(10000);
  const large = run(JSON.stringify({ required: [largeName] }), "{}", "explain");
  check(
    large.problems[0].details === undefined &&
      !large.problems[0].message.includes(largeName),
    "explanation copies no source-controlled name",
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
      resources = new sdk.SchemaResources([
        ["https://example.test/schema", parsed.value],
      ]);
      return prepare(documentText, { resources });
    } finally {
      resources?.dispose();
      parsed.value.dispose();
    }
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
  // An explaining contract keeps neither its document nor its resources.
  check(
    defaultArenas === baseline &&
      preparedArenas === baseline &&
      explainedArenas === baseline,
    "explanations retain no source arenas",
  );
  const probe = sdk.Document.parse(doc("true"));
  check(probe.status === "parsed", "option probe");
  try {
    let error;
    try {
      prepareInput(probe.value, "op", { diagnostics: "upstream" });
    } catch (e) {
      error = e;
    }
    check(
      error?.code === "invalid-work-options" &&
        /explain\(value\)/.test(error.message),
      "removed disclosure option rejected, naming explain",
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
        unretainedOriginalSourceBytes: utf8(documentText) + utf8(resourceText),
        meaning:
          "logical exact arenas and known original byte extent; not heap, RSS or Wasm capacity",
      },
    },
  };
}
