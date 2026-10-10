// Shared Node/browser tests: the facade keeps the same context → prepare → validate workflow.
export function partialResourceCases(sdk) {
  let count = 0;
  const check = (condition, message) => {
    if (!condition) throw Error(message);
    count++;
  };
  const U = "https://review.invalid/U";
  const hole = { $ref: U };
  const run = (
    schema,
    value,
    expected,
    keyword,
    completeness = "incomplete",
  ) => {
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
      const owners = sdk.liveStorageOwners();
      const metadata = contract.resourceCompleteness;
      check(metadata.status === completeness, "declared resource completeness");
      check(
        metadata === contract.resourceCompleteness && Object.isFrozen(metadata),
        "cached frozen metadata",
      );
      if (metadata.status === "incomplete") {
        check(
          Object.isFrozen(metadata.evidence) &&
            Object.isFrozen(metadata.evidence.location),
          "deeply frozen evidence",
        );
        check(
          metadata.evidence.reason === "resource-unavailable",
          "located missing evidence",
        );
      }
      check(sdk.liveStorageOwners() === owners, "metadata adds no Wasm owner");
      const retained = contract.retain();
      try {
        check(
          retained.resourceCompleteness === metadata,
          "retain shares cached metadata",
        );
      } finally {
        retained.dispose();
      }
      context.dispose();
      document.dispose();
      const result = contract.validate(value);
      check(
        result.outcome === expected,
        `partial result: ${JSON.stringify(result)}`,
      );
      if (expected === "no-verdict") {
        check(
          result.detail.reason === "resource-unavailable",
          "dependent value has missing evidence",
        );
        check(
          JSON.stringify(result.detail) === JSON.stringify(metadata.evidence),
          "metadata and undecided-value evidence agree",
        );
      }
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
      if (contract) {
        let code;
        try {
          void contract.resourceCompleteness;
        } catch (error) {
          code = error.code;
        }
        check(
          code === "disposed-handle",
          "cached metadata still checks disposal",
        );
      }
      context?.dispose();
      document.dispose();
    }
  };
  // Warm instance-lifetime meta-schema owners before measuring release.
  run({ type: "integer" }, 7, "satisfies", undefined, "complete");
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
  run(
    { then: hole, else: hole, contentSchema: hole, $defs: { unused: hole } },
    null,
    "satisfies",
    undefined,
    "complete",
  );
  for (const schema of [
    { not: hole },
    { oneOf: [true, hole] },
    { anyOf: [true, hole], unevaluatedItems: false },
  ]) {
    const parsed = sdk.parseDocument(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: { op: { input: schema } },
      }),
    );
    const context = parsed.value.contracts();
    try {
      const result = context.prepare("op", "input");
      check(
        result.status === "no-verdict" &&
          result.detail.reason === "resource-unavailable",
        "declined fallback preserves recovery cause",
      );
      check(
        result.detail.code === "resource-unavailable" &&
          result.detail.message ===
            "static preparation requires a resource that was not supplied",
        "strict refusal preserved",
      );
      check(
        result.detail.location?.pointer.startsWith("/operations/op/input"),
        "strict original location preserved",
      );
    } finally {
      context.dispose();
      parsed.value.dispose();
    }
  }
  const V = "https://review.invalid/V";
  const doc = sdk.parseDocument(
    JSON.stringify({
      openbindings: "0.2.0",
      operations: {
        op: { input: { $ref: U } },
        other: { input: { $ref: "https://unselected.invalid/U" } },
      },
    }),
  ).value;
  const known = sdk.parseJson(
    JSON.stringify({ properties: { x: { $ref: V } } }),
  ).value;
  const supplied = new sdk.SchemaResources([[U, known]]);
  const oldContext = doc.contracts({ resources: supplied });
  const old = oldContext.prepare("op", "input").contract;
  let completeResources, yes, newContext, next, retained;
  try {
    const info = old.resourceCompleteness;
    check(
      info.status === "incomplete" &&
        info.evidence.location.resource === U &&
        info.evidence.location.pointer === "/properties/x/$ref",
      "transitive supplied-resource evidence",
    );
    retained = old.retain();
    old.dispose();
    oldContext.dispose();
    check(
      retained.resourceCompleteness === info,
      "evidence survives old owner/context disposal",
    );
    yes = sdk.parseJson("true").value;
    completeResources = supplied.with(V, yes);
    newContext = doc.contracts({ resources: completeResources });
    next = newContext.prepare("op", "input").contract;
    check(
      next.resourceCompleteness.status === "complete",
      "new complete context ignores unselected references",
    );
    check(
      retained.resourceCompleteness === info,
      "old partial snapshot unchanged",
    );
    check(
      retained.validate({ x: 1 }).outcome === "no-verdict" &&
        next.validate({ x: 1 }).outcome === "satisfies",
      "old/new value behavior retained",
    );
  } finally {
    retained?.dispose();
    old.dispose();
    oldContext.dispose();
    next?.dispose();
    newContext?.dispose();
    completeResources?.dispose();
    yes?.dispose();
    supplied.dispose();
    known.dispose();
    doc.dispose();
  }
  // Phase migration must not create leaked bridge owners.
  check(sdk.liveStorageOwners() === initial, "partial owners released");
  return { checks: count };
}
