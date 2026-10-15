import * as testing from "../dist/testing.js";
import { conformantContracts } from "./conformant-contracts.mjs";
/** Public-package detail/privacy/retention controls; portable across Node and browsers. */
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
    let proof, context;
    try {
      const checked = parsed.value.validate();
      check(checked.status === "conformant", "proof");
      proof = checked.document;
      context = conformantContracts(proof, options);
      const ready = context.prepare("op", "input");
      check(ready.status === "ready", "ready");
      return ready.valueContract;
    } finally {
      context?.dispose();
      proof?.dispose();
      parsed.value.dispose();
    }
  }
  const doc = (schema) =>
    `{"openbindings":"0.2.0","operations":{"op":{"input":${schema}}}}`;
  function run(schema, input, options = {}) {
    const contract = prepare(doc(schema), options);
    const value = sdk.parseJson(input);
    check(value.status === "parsed", "exact input");
    try {
      return contract.validate(value.value);
    } finally {
      value.value.dispose();
      contract.dispose();
    }
  }
  const cases = [
    ['{"type":"integer"}', '"SECRET-instance"', "type"],
    ['{"required":["SECRET/member~key"]}', "{}", "required"],
    ['{"minimum":9007199254740993}', "9007199254740992", "numeric-bound"],
    ['{"maxLength":3}', '"long"', "size-bound"],
    [
      '{"enum":["SECRET-choice",9007199254740993,{"x":1e-999}]}',
      "null",
      "enum",
    ],
  ];
  // Exact operands remain source facts after the engine's numeric substitution.
  for (const token of [
    "1e400",
    "1e" + "9".repeat(1000),
    "9007199254740993",
    "-0.00",
  ]) {
    for (const diagnostics of ["coarse", "upstream"]) {
      const result = run(`{"exclusiveMinimum":${token}}`, token, {
        diagnostics,
      });
      check(result.outcome === "fails", "exact exclusive bound");
      check(!result.problemsComplete, "truthful first-error completeness");
      if (diagnostics === "coarse")
        check(result.problems.length === 0, "coarse omits payloads");
      else {
        const problem = result.problems[0];
        check(
          problem.details.bound === token,
          "original numeric token, never handle",
        );
        check(
          problem.schemaLocation.pointer ===
            "/operations/op/input/exclusiveMinimum",
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
  for (const [schema, input, kind] of cases) {
    const ordinary = run(schema, input),
      detailed = run(schema, input, { diagnostics: "upstream" });
    check(
      ordinary.outcome === "fails" && detailed.outcome === "fails",
      "same failure",
    );
    check(
      ordinary.problems.every(
        (p) => p.details === undefined && !p.message.includes("SECRET"),
      ),
      "private default",
    );
    const detail = detailed.problems[0].details;
    check(!detailed.problemsComplete && detail.kind === kind, "atomic fact");
    if (kind === "required")
      check(
        detail.member === "SECRET/member~key" &&
          detailed.problems[0].instancePointer === "",
        "required parent location",
      );
    if (kind === "numeric-bound")
      check(detail.bound === "9007199254740993", "exact numeric fact");
    if (kind === "enum")
      check(
        JSON.stringify(detail.choices) ===
          JSON.stringify([
            '"SECRET-choice"',
            "9007199254740993",
            '{"x":1e-999}',
          ]),
        "exact enum choices",
      );
    examples.push(detailed);
  }
  const largeName = "é".repeat(10000);
  const large = run(JSON.stringify({ required: [largeName] }), "{}", {
    diagnostics: "upstream",
  });
  check(
    large.problems[0].details.member === largeName,
    "upstream retains library-provided name",
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
  function fromResource(text, diagnostics) {
    const parsed = sdk.parseJson(text);
    check(parsed.status === "parsed", "resource parse");
    let resources;
    try {
      resources = new sdk.SchemaResources([
        ["https://example.test/schema", parsed.value],
      ]);
      return prepare(documentText, { resources, diagnostics });
    } finally {
      resources?.dispose();
      parsed.value.dispose();
    }
  }
  // Warm all process-global schema arenas before comparing application retention.
  let warm = fromResource(resourceText, "coarse");
  warm.dispose();
  const baseline = testing.liveStorageOwners();
  const defaultContract = fromResource(resourceText, "coarse");
  const defaultArenas = testing.liveStorageOwners();
  defaultContract.dispose();
  const old = fromResource(resourceText, "upstream");
  const optInArenas = testing.liveStorageOwners();
  const newer = fromResource(replacementText, "upstream");
  try {
    const oldResult = old.validate({ "a/~": 6 }),
      newResult = newer.validate({ "a/~": 6 });
    check(
      oldResult.problems[0].details.bound === "9007199254740993" &&
        newResult.problems[0].details.bound === "7",
      "replacement preserves original resource",
    );
    check(
      oldResult.problems[0].instancePointer === "/a~1~0" &&
        oldResult.problems[0].schemaLocation.pointer ===
          "/allOf/0/properties/a~1~0/minimum",
      "escaped nested coordinates",
    );
  } finally {
    old.dispose();
    newer.dispose();
  }
  const released = testing.liveStorageOwners();
  check(released === baseline, "all application owners released");
  check(optInArenas > defaultArenas, "opt-in source retention observable");
  const probe = sdk.Document.parse(doc("true"));
  check(probe.status === "parsed", "option probe");
  try {
    let error;
    try {
      conformantContracts(probe.value, { diagnostics: "yes" });
    } catch (e) {
      error = e;
    }
    check(
      error?.code === "invalid-diagnostics",
      "bad disclosure option rejected",
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
        optInArenas,
        released,
        optInOriginalSourceBytes: utf8(documentText) + utf8(resourceText),
        meaning:
          "logical exact arenas and known original byte extent; not heap, RSS or Wasm capacity",
      },
    },
  };
}
