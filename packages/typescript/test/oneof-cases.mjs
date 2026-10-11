// Shared Node/browser controls. Fixtures retain frozen independent expected truths.
export function oneOfCases(sdk, fixtures) {
  let checks = 0;
  const check = (condition, label) => {
    checks++;
    if (!condition) throw Error(label);
  };
  const run = (
    text,
    operation,
    side,
    valueText,
    entries = {},
    options = {},
    completeness = "incomplete",
  ) => {
    const parsed = sdk.parseDocument(text);
    check(parsed.status === "parsed", "oneOf document admitted");
    const values = Object.entries(entries).map(([uri, value]) => [
      uri,
      sdk.parseJson(JSON.stringify(value)).value,
    ]);
    const resources = new sdk.SchemaResources(values);
    let context, contract, value;
    try {
      context = parsed.value.contracts({ ...options, resources });
      const setup = context.prepare(operation, side);
      check(
        setup.status === "ready",
        `oneOf prepared: ${JSON.stringify(setup)}`,
      );
      contract = setup.contract;
      check(
        contract.resourceCompleteness.status === completeness,
        "oneOf resource evidence",
      );
      value = sdk.parseJson(valueText).value;
      context.dispose();
      parsed.value.dispose();
      resources.dispose();
      const result = contract.validate(value);
      if (result.outcome === "no-verdict")
        check(
          ["resource-unavailable", "limit-exceeded"].includes(
            result.detail.reason,
          ),
          "truthful bounded cause",
        );
      check(
        !JSON.stringify(result).includes("sdk-bounds"),
        "no generated identities",
      );
      return result;
    } finally {
      value?.dispose();
      contract?.dispose();
      context?.dispose();
      resources.dispose();
      values.forEach(([, value]) => value.dispose());
      parsed.value.dispose();
    }
  };
  const doc = (schema) =>
    JSON.stringify({
      openbindings: "0.2.0",
      operations: { probe: { input: schema } },
    });
  // Warm the shared metadata owners, then require the baseline after all cases.
  run(fixtures.c22, "2x.thumbnail", "output", "false");
  const initial = sdk.liveStorageOwners();
  // Uniform FF/TT completions satisfy, mixed completions fail: no fixed verdict.
  const nested = doc({
    oneOf: [
      {
        oneOf: [
          { $ref: "https://sol-independent.invalid/U" },
          { $ref: "https://sol-independent.invalid/V" },
        ],
      },
      true,
    ],
  });
  for (const pair of [
    null,
    [false, false],
    [false, true],
    [true, false],
    [true, true],
  ]) {
    const entries =
      pair === null
        ? {}
        : {
            "https://sol-independent.invalid/U": pair[0],
            "https://sol-independent.invalid/V": pair[1],
          };
    const result = run(
      nested,
      "probe",
      "input",
      "null",
      entries,
      {},
      pair === null ? "incomplete" : "complete",
    );
    check(
      result.outcome ===
        (pair === null
          ? "no-verdict"
          : pair[0] === pair[1]
            ? "satisfies"
            : "fails"),
      "nested endpoint success is not independence",
    );
    if (pair === null)
      check(
        result.detail?.reason === "resource-unavailable",
        "nested uncertainty is resource dependence, not work exhaustion",
      );
  }
  for (const op of ["2x.thumbnail", "thumbnail.make"]) {
    let text = fixtures.c22.replaceAll('"2x.thumbnail"', JSON.stringify(op));
    if (op !== "2x.thumbnail")
      text = text.replace(
        '"aliases":["thumb.render"',
        '"aliases":["2x.thumbnail","thumb.render"',
      );
    for (const test of fixtures.c22Cases) {
      if (test.valueText === null) continue;
      const result = run(text, op, "output", test.valueText);
      check(
        result.outcome === test.expected,
        `${test.caseId}: ${JSON.stringify(result)}`,
      );
      if (result.outcome === "fails") {
        check(
          result.problemsComplete && result.problems.length > 0,
          "C22 useful complete summary",
        );
        check(
          result.problems.every(
            (p) =>
              p.code === "oneOf" &&
              p.instancePointer === "" &&
              p.schemaLocation?.resource === null &&
              p.schemaLocation?.pointer === `/operations/${op}/output/oneOf`,
          ),
          "C22 original oneOf boundary",
        );
      }
    }
  }
  for (const test of fixtures.controls.cases) {
    const result = run(
      doc(test.schema),
      "probe",
      "input",
      test.valueText,
      test.resources,
    );
    check(
      result.outcome === test.expected,
      `${test.id}: ${JSON.stringify(result)}`,
    );
    if (result.outcome !== "fails") continue;
    check(
      result.problemsComplete && result.problems.length > 0,
      "useful owner-control explanation",
    );
    if (test.summary)
      check(
        result.problems.some(
          (p) =>
            p.code === "oneOf" &&
            p.schemaLocation?.pointer === test.summary &&
            p.schemaLocation?.resource === (test.summaryResource ?? null) &&
            p.instancePointer === (test.instancePointer ?? ""),
        ),
        "original summary and instance location",
      );
    if (test.requiredCode)
      check(
        result.problems.some((p) => p.code === test.requiredCode),
        "ordinary sibling cause",
      );
    if (test.forbiddenCode)
      check(
        result.problems.every((p) => p.code !== test.forbiddenCode),
        "no fabricated oneOf failure",
      );
    check(
      result.problems.every((p) => !p.message.includes("matched")),
      "no authored branch-count claim",
    );
  }
  const schema = { oneOf: [true, true, { $ref: "https://oneof.invalid/U" }] };
  const full = run(
    doc(schema),
    "probe",
    "input",
    '{"secret":"no-instance-echo"}',
  );
  const utf8 = (s) => new TextEncoder().encode(s).length;
  const size = full.problems.reduce(
    (n, p) =>
      n +
      utf8(p.code) +
      utf8(p.message) +
      utf8(p.instancePointer) +
      utf8(p.schemaLocation?.pointer ?? "") +
      utf8(p.schemaLocation?.resource ?? ""),
    0,
  );
  check(!JSON.stringify(full).includes("no-instance-echo"), "summary privacy");
  for (const bytes of [0, size - 1, size]) {
    const result = run(
      doc(schema),
      "probe",
      "input",
      "null",
      {},
      { limits: { diagnosticBytes: bytes } },
    );
    check(
      result.outcome === "fails",
      "diagnostic truncation preserves failure",
    );
    check(
      result.problems.length === (bytes === size ? 1 : 0) &&
        result.problemsComplete === (bytes === size),
      "exact summary byte allowance",
    );
  }
  const exhausted = run(
    doc(schema),
    "probe",
    "input",
    "null",
    {},
    { limits: { evaluationSteps: 0 } },
  );
  check(
    exhausted.outcome === "no-verdict" &&
      exhausted.detail.reason === "limit-exceeded",
    "incomplete evaluation proves no verdict",
  );
  let recursive = {
    $id: "https://oneof.invalid/tree",
    oneOf: [
      {
        type: "object",
        properties: {
          next: { $ref: "#" },
          p: { $ref: "https://oneof.invalid/U" },
        },
      },
      { type: "null" },
    ],
  };
  for (const [text, expected] of [
    ['{"next":{"next":null}}', "satisfies"],
    ['{"next":false}', "fails"],
    ['{"next":{"p":1}}', "no-verdict"],
  ]) {
    check(
      run(doc(recursive), "probe", "input", text).outcome === expected,
      "advancing recursive polarity",
    );
  }
  check(sdk.liveStorageOwners() === initial, "oneOf final-owner release");
  return { checks };
}
