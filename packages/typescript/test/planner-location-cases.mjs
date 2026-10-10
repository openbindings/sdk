// Shared Node/browser controls at real production caps; no test knobs.
export function plannerLocationCases(sdk) {
  // Warm instance-lifetime meta-schema/evaluator storage before owner deltas.
  const warm = sdk.parseDocument(
    '{"openbindings":"0.2.0","operations":{"op":{"input":true}}}',
  ).value;
  warm.assess();
  const warmContext = warm.contracts();
  const warmPrepared = warmContext.prepare("op", "input");
  if (warmPrepared.status !== "ready") throw Error("warm preparation");
  warmPrepared.contract.validate(null);
  warmPrepared.contract.dispose();
  warmContext.dispose();
  warm.dispose();
  const initial = sdk.liveStorageOwners();
  const U = "https://absent.invalid/U";
  const R = "https://known.invalid/R";
  const input = "/operations/op/input";
  let checks = 0;
  const observations = [];
  const check = (condition, message) => {
    if (!condition) throw Error(message);
    checks++;
  };
  const run = (schema, entries, code, pointer, resource = null) => {
    const parsed = sdk.parseDocument(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: { op: { input: schema }, healthy: { input: true } },
      }),
    );
    check(parsed.status === "parsed", "planner document admitted");
    const document = parsed.value;
    const values = [];
    let resources, context;
    try {
      check(
        document.assess().report.conclusion === "conformant",
        "conformant witness",
      );
      resources = new sdk.SchemaResources(
        entries.map(([uri, value]) => {
          const parsed = sdk.parseJson(JSON.stringify(value));
          check(parsed.status === "parsed", "resource admitted");
          values.push(parsed.value);
          return [uri, parsed.value];
        }),
      );
      context = document.contracts({ resources });
      const cancelled = new AbortController();
      cancelled.abort();
      const stopped = context.prepare("op", "input", {
        signal: cancelled.signal,
      });
      check(
        stopped.status === "no-verdict" &&
          stopped.detail.reason === "cancelled",
        "cancellation retained",
      );
      const result = context.prepare("op", "input");
      check(result.status === "no-verdict", "planner refuses");
      const detail = result.detail;
      const reason = code.endsWith("-limit")
        ? "limit-exceeded"
        : code === "resource-unavailable"
          ? "resource-unavailable"
          : "conservative-preparation";
      check(
        detail.code === code && detail.reason === reason,
        `actual cause: ${JSON.stringify(detail)}`,
      );
      check(
        detail.location?.resource === resource &&
          detail.location?.pointer === pointer,
        `original location: ${JSON.stringify(detail)}`,
      );
      check(
        detail.message.length < 192 &&
          !detail.message.includes("sdk-bounds.openbindings.invalid"),
        "fixed original diagnostic",
      );
      check(
        JSON.stringify(context.prepare("op", "input")) ===
          JSON.stringify(result),
        "cached refusal unchanged",
      );
      const healthy = context.prepare("healthy", "input");
      check(healthy.status === "ready", "healthy subsequent preparation");
      try {
        check(
          healthy.contract.validate(null).outcome === "satisfies",
          "healthy validation",
        );
      } finally {
        healthy.contract.dispose();
      }
      observations.push({ code, location: detail.location });
    } finally {
      context?.dispose();
      resources?.dispose();
      for (const value of values) value.dispose();
      document.dispose();
    }
  };
  for (const [schema, suffix] of [
    [{ anyOf: [true, { $ref: `${U}#/~2` }] }, "/anyOf/1/$ref"],
    [
      { properties: { 'quote"/tilde~雪': { $ref: `${U}#/~2` } } },
      '/properties/quote"~1tilde~0雪/$ref',
    ],
  ]) {
    run(schema, [], "invalid-reference-fragment", input + suffix);
    run({ $ref: R }, [[R, schema]], "invalid-reference-fragment", suffix, R);
  }
  // Both applicators are still excluded; oneOf qualification is preserved.
  const excluded = {
    properties: {
      a: { not: { $ref: U } },
      b: { contains: { $ref: "https://absent.invalid/V" } },
    },
  };
  for (let i = 0; i < 8; i++) {
    run(excluded, [], "resource-unavailable", input + "/properties/a/not");
    for (const missing of [false, true]) {
      const cycle = {
        ...(missing ? { $ref: U } : {}),
        $defs: {
          a: { $ref: "#" + input + "/$defs/b" },
          b: { $ref: "#" + input + "/$defs/a" },
        },
        allOf: [{ $ref: "#" + input + "/$defs/a" }],
      };
      run(cycle, [], "in-place-cycle", input + "/$defs/a");
    }
  }
  // Affordable actual production boundaries (~0.5/1 MiB), including replacement.
  for (const count of [100000, 200000]) {
    const schema = { $ref: U, allOf: Array(count).fill(true) };
    run(
      schema,
      [],
      count === 100000 ? "schema-node-limit" : "schema-edge-limit",
      input + `/allOf/${count - 1}`,
    );
    run(schema, [[U, true]], "schema-node-limit", input + "/allOf/99998");
  }
  check(sdk.liveStorageOwners() === initial, "planner owners released");
  return { checks, observations };
}
