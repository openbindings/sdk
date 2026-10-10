/** Public-package detail/privacy/retention controls; portable across Node and browsers. */
export function diagnosticDetailsCases(sdk) {
  let assertions = 0;
  const check = (ok, text) => {
    assertions++;
    if (!ok) throw new Error(text);
  };
  const utf8 = (text) => new TextEncoder().encode(text).length;
  const strings = (value) =>
    typeof value === "string"
      ? utf8(value)
      : value && typeof value === "object"
        ? Object.values(value).reduce((n, v) => n + strings(v), 0)
        : 0;
  function prepare(text, options = {}) {
    const parsed = sdk.parseDocument(text);
    check(parsed.status === "parsed", "parse");
    let proof, context;
    try {
      const checked = parsed.value.validate();
      check(checked.status === "validated", "proof");
      proof = checked.document;
      context = proof.contracts(options);
      const ready = context.prepare("op", "input");
      check(ready.status === "ready", "ready");
      return ready.contract;
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
  const examples = [];
  for (const [schema, input, kind] of cases) {
    const ordinary = run(schema, input),
      detailed = run(schema, input, { includeSchemaDetails: true });
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
    check(detailed.problemsComplete && detail.kind === kind, "atomic fact");
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
  const schema = '{"enum":["SECRET-choice-that-takes-space",9007199254740993]}';
  const base = strings(run(schema, "null").problems[0]);
  const full = strings(
    run(schema, "null", { includeSchemaDetails: true }).problems[0],
  );
  const boundaries = [];
  for (const diagnosticBytes of [
    0,
    1,
    base + 8,
    base + 9,
    full - 1,
    full,
    full + 1,
  ]) {
    const result = run(schema, "null", {
      includeSchemaDetails: true,
      limits: { diagnosticBytes },
    });
    const retained = result.problems.reduce((n, p) => n + strings(p), 0);
    check(
      result.outcome === "fails" && retained <= diagnosticBytes,
      "bounded failure",
    );
    check(
      utf8(JSON.stringify(result)) <=
        59 + 160 * result.problems.length + 9 * retained,
      "detail wire bound",
    );
    if (diagnosticBytes < base + 9)
      check(
        result.problems.length === 0 && !result.problemsComplete,
        "marker reserved before base",
      );
    else if (diagnosticBytes < full)
      check(
        result.problems[0].details.kind === "truncated" &&
          !result.problemsComplete,
        "useful base remains",
      );
    else
      check(
        result.problems[0].details.kind === "enum" && result.problemsComplete,
        "entire enum fits",
      );
    boundaries.push({
      diagnosticBytes,
      retained,
      problems: result.problems.length,
      complete: result.problemsComplete,
    });
  }
  const largeName = "é".repeat(10000);
  const omitted = run(JSON.stringify({ required: [largeName] }), "{}", {
    includeSchemaDetails: true,
    limits: { diagnosticBytes: 1024 },
  });
  check(
    omitted.problems.length === 1 &&
      omitted.problems[0].details.kind === "truncated",
    "large required name never removes useful base",
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
  function fromResource(text, includeSchemaDetails) {
    const parsed = sdk.parseJson(text);
    check(parsed.status === "parsed", "resource parse");
    let resources;
    try {
      resources = new sdk.SchemaResources([
        ["https://example.test/schema", parsed.value],
      ]);
      return prepare(documentText, { resources, includeSchemaDetails });
    } finally {
      resources?.dispose();
      parsed.value.dispose();
    }
  }
  // Warm all process-global schema arenas before comparing application retention.
  let warm = fromResource(resourceText, false);
  warm.dispose();
  const baseline = sdk.liveStorageOwners();
  const defaultContract = fromResource(resourceText, false);
  const defaultArenas = sdk.liveStorageOwners();
  defaultContract.dispose();
  const old = fromResource(resourceText, true);
  const optInArenas = sdk.liveStorageOwners();
  const newer = fromResource(replacementText, true);
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
  const released = sdk.liveStorageOwners();
  check(released === baseline, "all application owners released");
  check(optInArenas > defaultArenas, "opt-in source retention observable");
  const probe = sdk.parseDocument(doc("true"));
  check(probe.status === "parsed", "option probe");
  try {
    let error;
    try {
      probe.value.contracts({ includeSchemaDetails: "yes" });
    } catch (e) {
      error = e;
    }
    check(
      error?.code === "invalid-schema-details",
      "bad disclosure option rejected",
    );
  } finally {
    probe.value.dispose();
  }
  check(
    sdk.liveStorageOwners() === baseline,
    "invalid options release all owners",
  );
  return {
    diagnosticDetails: {
      assertions,
      examples,
      boundaries,
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
