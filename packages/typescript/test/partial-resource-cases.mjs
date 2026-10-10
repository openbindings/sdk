// Shared Node/browser tests: the facade keeps the same context → prepare → validate workflow.
export function partialResourceCases(sdk) {
  let count = 0;
  const check = (condition, message) => {
    if (!condition) throw Error(message);
    count++;
  };
  const U = "https://review.invalid/U";
  const hole = { $ref: U };
  const run = (schema, value, expected, keyword) => {
    const parsed = sdk.parseDocument(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: { op: { input: schema } },
      }),
    );
    check(parsed.status === "parsed", "partial fixture parsed");
    const document = parsed.value;
    let context, contract;
    try {
      context = document.contracts();
      const setup = context.prepare("op", "input");
      if (expected === "refuse") {
        check(
          setup.status === "no-verdict",
          "unqualified partial preparation refuses",
        );
        return;
      }
      check(setup.status === "ready", "qualified partial preparation is ready");
      contract = setup.contract;
      context.dispose();
      document.dispose();
      const result = contract.validate(value);
      check(
        result.outcome === expected,
        `partial result: ${JSON.stringify(result)}`,
      );
      if (expected === "no-verdict")
        check(
          result.detail.reason === "resource-unavailable",
          "dependent value has missing evidence",
        );
      if (keyword)
        check(
          result.problems.some(
            (p) =>
              p.code === keyword &&
              p.schemaLocation?.pointer.endsWith(`/${keyword}`),
          ),
          "original known diagnostic",
        );
      check(
        !JSON.stringify(result).includes("sdk-bounds"),
        "private targets never escape",
      );
    } finally {
      contract?.dispose();
      context?.dispose();
      document.dispose();
    }
  };
  // Warm instance-lifetime meta-schema owners before measuring release.
  run({ type: "integer" }, 7, "satisfies");
  const initial = sdk.liveStorageOwners();
  run(hole, null, "no-verdict");
  run({ allOf: [hole] }, 1, "no-verdict");
  run({ anyOf: [true, hole] }, { x: 1 }, "satisfies");
  run(
    { properties: { external: hole }, required: ["name"] },
    { external: 7 },
    "fails",
    "required",
  );
  run(
    { properties: { external: hole }, additionalProperties: false },
    { external: 7 },
    "no-verdict",
  );
  run(
    {
      properties: {
        external: hole,
        name: { oneOf: [{ type: "string" }, { type: "number" }] },
      },
    },
    { name: "ok" },
    "satisfies",
  );
  run({ prefixItems: [hole], items: false }, [1], "no-verdict");
  run({ prefixItems: [hole], items: false }, [1, 2], "fails");
  run({ prefixItems: [true], items: hole }, [1], "satisfies");
  run(
    { patternProperties: { "^x": hole }, additionalProperties: false },
    { xyz: 1 },
    "no-verdict",
  );
  run({ propertyNames: hole }, {}, "satisfies");
  run({ dependentSchemas: { x: hole } }, {}, "satisfies");
  run({ $ref: U, type: "string" }, 7, "fails", "type");
  run(
    {
      anyOf: [
        { properties: { a: hole } },
        { not: { properties: { b: hole } } },
      ],
    },
    { a: 1, b: 2 },
    "refuse",
  );
  run(
    { oneOf: [{ properties: { a: hole } }, { properties: { b: hole } }] },
    { a: 1, b: 2 },
    "refuse",
  );
  run(
    { anyOf: [true, hole], unevaluatedProperties: false },
    { x: 1 },
    "refuse",
  );
  run({ contains: hole, minContains: 1, maxContains: 1 }, [1, 2], "refuse");
  run({ if: hole, then: false, else: true }, null, "refuse");
  run({ $dynamicAnchor: "node", properties: { x: hole } }, {}, "refuse");
  run({ anyOf: [true, { $ref: `${U}#/~2` }] }, 1, "refuse");
  // Phase migration must not create leaked bridge owners.
  check(sdk.liveStorageOwners() === initial, "partial owners released");
  return { checks: count };
}
