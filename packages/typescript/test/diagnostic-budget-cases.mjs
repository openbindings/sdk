/** Maintained value-diagnostic resource controls; runs unchanged on package and browser hosts. */
export function diagnosticBudgetCases(sdk) {
  let assertions = 0;
  const check = (condition, label) => {
    assertions++;
    if (!condition) throw new Error(label);
  };
  const utf8 = (text) => new TextEncoder().encode(text).length;
  const size = (problem) =>
    utf8(problem.instancePointer) +
    utf8(problem.code) +
    utf8(problem.message) +
    utf8(problem.schemaLocation?.resource ?? "") +
    utf8(problem.schemaLocation?.pointer ?? "");
  const run = (
    schema,
    input,
    diagnosticBytes,
    maxProblems = 256,
    resources,
  ) => {
    const parsed = sdk.parseDocument(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: { op: { input: schema } },
      }),
    );
    check(parsed.status === "parsed", "source admitted");
    try {
      const context = parsed.value.contracts({
        limits: { diagnosticBytes, maxProblems },
        resources,
      });
      try {
        const setup = context.prepare("op", "input");
        check(setup.status === "ready", "contract ready");
        try {
          return setup.contract.validate(input);
        } finally {
          setup.contract.dispose();
        }
      } finally {
        context.dispose();
      }
    } finally {
      parsed.value.dispose();
    }
  };
  const bounded = (result, bytes, count = 256) => {
    check(result.outcome === "fails", "established failure survives allowance");
    check(result.problems.length <= Math.max(1, count), "count bound");
    const retained = result.problems.reduce((n, p) => n + size(p), 0);
    check(retained <= bytes, "aggregate retained UTF-8 budget");
    check(
      utf8(JSON.stringify(result)) <=
        58 + 94 * result.problems.length + 6 * retained,
      "escaped serialized-result bound",
    );
    return result.problems;
  };
  check(
    sdk.evaluatorLimits().diagnosticBytes === 1_048_576,
    "documented default",
  );
  const key = "k".repeat(32768);
  const schema = {
    type: "object",
    properties: { [key]: { type: "object", additionalProperties: false } },
  };
  const input = {
    [key]: Object.fromEntries(
      Array.from({ length: 257 }, (_, i) => [`f${i}`, 0]),
    ),
  };
  const full = run(schema, input, 32 * 1024 * 1024);
  check(
    full.problems.length === 256 && utf8(JSON.stringify(full)) > 16_000_000,
    "maintained amplification control",
  );
  const witness = [];
  for (const count of [1, 256]) {
    const result = run(schema, input, 1_048_576, count);
    const problems = bounded(result, 1_048_576, count);
    check(
      problems.length > 0 && !result.problemsComplete,
      "retained witness prefix is incomplete",
    );
    for (let i = 0; i < problems.length; i++) {
      check(problems[i].code === "falseSchema", "stable witness code");
      check(
        JSON.stringify(problems[i]) === JSON.stringify(full.problems[i]),
        "original coordinates and order survive",
      );
    }
    witness.push({
      count,
      problems: problems.length,
      retainedBytes: problems.reduce((n, p) => n + size(p), 0),
      wireBytes: utf8(JSON.stringify(result)),
    });
  }
  for (const name of ["plain", '~/\0\n\t"\\', "é東京🦀"]) {
    const schema = { properties: { [name]: { type: "string" } } },
      input = { [name]: 7 };
    const full = run(schema, input, 1_048_576),
      bytes = size(full.problems[0]);
    for (const budget of [0, 1, bytes - 1, bytes, bytes + 1, 1_048_576]) {
      const result = run(schema, input, budget, 0),
        problems = bounded(result, budget, 0);
      check(
        problems.length === (budget >= bytes ? 1 : 0),
        "whole-problem boundary",
      );
      check(
        result.problemsComplete === budget >= bytes,
        "boundary completeness",
      );
      if (problems.length)
        check(
          JSON.stringify(problems) === JSON.stringify(full.problems),
          "exact escaped/multibyte coordinates",
        );
    }
  }
  for (const budget of [0, 1, 1_048_576])
    check(
      run(true, 0, budget).outcome === "satisfies",
      "success independent of diagnostics",
    );
  const countSchema = { allOf: [false, false, false] };
  const countFull = run(countSchema, 0, 1_048_576);
  const oneSize = size(countFull.problems[0]);
  for (const bytes of [
    oneSize - 1,
    oneSize,
    oneSize + 1,
    2 * oneSize - 1,
    2 * oneSize,
    3 * oneSize,
  ]) {
    for (const count of [0, 1, 2, 3, 4]) {
      const result = run(countSchema, 0, bytes, count);
      const problems = bounded(result, bytes, count);
      const expected = Math.min(
        Math.floor(bytes / oneSize),
        Math.max(1, count),
        3,
      );
      check(
        problems.length === expected &&
          result.problemsComplete === (expected === 3),
        "count and byte precedence",
      );
    }
  }
  const uri = "https://example.test/" + "u".repeat(32768);
  const resource = sdk.ExactJson.from({ type: "string" });
  const resources = new sdk.SchemaResources([[uri, resource]]);
  try {
    const result = run({ $ref: uri }, 7, 512, 256, resources);
    check(
      bounded(result, 512).length === 0 && !result.problemsComplete,
      "oversized original resource omitted whole",
    );
  } finally {
    resources.dispose();
    resource.dispose();
  }
  let seed = 0x5eedcafe;
  for (let i = 0; i < 32; i++) {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    const name =
      ["~", "/", "\n", "é", "🦀"][seed % 5].repeat((seed % 31) + 1) + seed;
    const schema = {
        properties: { [name]: { allOf: [{ type: "string" }, false] } },
      },
      input = { [name]: 7 };
    const full = run(schema, input, 1_048_576),
      boundary = size(full.problems[0]);
    for (const budget of [
      0,
      boundary - 1,
      boundary,
      full.problems.reduce((n, p) => n + size(p), 0),
    ]) {
      const result = run(schema, input, budget),
        problems = bounded(result, budget);
      check(
        JSON.stringify(result) === JSON.stringify(run(schema, input, budget)),
        "repeat determinism",
      );
      check(
        JSON.stringify(problems) ===
          JSON.stringify(full.problems.slice(0, problems.length)),
        "prefix mutation invariant",
      );
    }
  }
  return {
    diagnosticBudget: {
      assertions,
      seed: "0x5eedcafe",
      witness,
      scope:
        "logical retained and compact serialized bytes; not allocator or timing measurements",
    },
  };
}
