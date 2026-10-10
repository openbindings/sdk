// Host-neutral public-package workloads. No private Wasm exports or SDK internals.
export function check(condition, message) {
  if (!condition) throw Error(message);
}
const utf8 = new TextEncoder();
const bytes = (value) => utf8.encode(value).length;
export function parsed(sdk, source, document = false) {
  const result = document ? sdk.parseDocument(source) : sdk.parseJson(source);
  check(result.status === "parsed", JSON.stringify(result));
  return result.value;
}
export function prepare(sdk, source) {
  const document = parsed(sdk, source, true);
  let context;
  try {
    context = document.contracts();
    const result = context.prepare("check", "input");
    check(result.status === "ready", JSON.stringify(result));
    return result.contract;
  } finally {
    context?.dispose();
    document.dispose();
  }
}
export function describe(result) {
  const wire = JSON.stringify(result);
  return {
    outcome: result.outcome,
    problems: result.problems?.length ?? 0,
    complete: result.problemsComplete ?? null,
    wireBytes: bytes(wire),
    firstProblem: result.problems?.[0] ?? null,
  };
}
export function callsPerSecond(perCallMs) {
  return Number.isFinite(perCallMs) && perCallMs > 0 ? 1000 / perCallMs : null;
}
// Preparation/admission stay outside this one outer timer. The in-loop verdict
// guard is deliberate caller overhead, present identically on both candidates.
export function retainedBatch(
  contract,
  value,
  expectedOutcome,
  repetitions,
  clock,
) {
  check(
    Number.isSafeInteger(repetitions) && repetitions > 0,
    "positive batch count",
  );
  const start = clock();
  for (let n = 0; n < repetitions; n++) {
    const result = contract.validate(value);
    if (result.outcome !== expectedOutcome)
      throw Error("retained batch changed verdict: " + result.outcome);
  }
  const elapsed = clock() - start;
  check(Number.isFinite(elapsed) && elapsed >= 0, "invalid timer duration");
  return { elapsedMs: elapsed, perCallMs: elapsed / repetitions };
}
// The budgeted diagnostic job includes validate, an outcome guard, and exactly
// one JSON.stringify per call. No per-call JSON inspection or parsing is added.
export function invalidSerializedBatch(contract, value, repetitions, clock) {
  check(
    Number.isSafeInteger(repetitions) && repetitions > 0,
    "positive batch count",
  );
  let lastResult, lastWire;
  const start = clock();
  for (let n = 0; n < repetitions; n++) {
    lastResult = contract.validate(value);
    if (lastResult.outcome !== "fails")
      throw Error("diagnostic batch changed verdict: " + lastResult.outcome);
    lastWire = JSON.stringify(lastResult);
  }
  const elapsed = clock() - start;
  check(Number.isFinite(elapsed) && elapsed >= 0, "invalid timer duration");
  return {
    elapsedMs: elapsed,
    perCallMs: elapsed / repetitions,
    lastResult,
    serializedCodeUnits: lastWire.length,
  };
}
export function runTier(sdk, fixture, timed = true) {
  const clock = timed ? () => performance.now() : () => 0;
  const stages = [
    "parseDocument",
    "assess",
    "contextAndPrepare",
    "parseValue",
    "firstValidation",
    "hotSingleDescriptive",
    "parseInvalid",
    "invalid",
    "serialize",
    "cleanup",
    "complete",
    "invalidAndSerializeDescriptive",
  ];
  let last;
  function once() {
    let document, context, contract, value, invalid;
    const row = {};
    const take = (name, run) => {
      const t = clock();
      const result = run();
      row[name] = clock() - t;
      return result;
    };
    try {
      document = take("parseDocument", () =>
        parsed(sdk, fixture.document, true),
      );
      const assessment = take("assess", () => document.assess());
      check(
        assessment.status === "assessed" &&
          assessment.report.conclusion === "conformant",
        JSON.stringify(assessment),
      );
      contract = take("contextAndPrepare", () => {
        context = document.contracts();
        const result = context.prepare("check", "input");
        check(result.status === "ready", JSON.stringify(result));
        return result.contract;
      });
      value = take("parseValue", () => parsed(sdk, fixture.valid));
      const first = take("firstValidation", () => contract.validate(value));
      check(first.outcome === fixture.validOutcome, JSON.stringify(first));
      if (fixture.validOutcome === "no-verdict")
        check(
          first.detail.code === "evaluation-work-limit",
          "expected near-admission evaluation refusal",
        );
      const hot = take("hotSingleDescriptive", () => contract.validate(value));
      check(hot.outcome === fixture.validOutcome, JSON.stringify(hot));
      invalid = take("parseInvalid", () => parsed(sdk, fixture.invalid));
      const failed = take("invalid", () => contract.validate(invalid));
      check(failed.outcome === "fails", JSON.stringify(failed));
      const wire = take("serialize", () => JSON.stringify(failed));
      last = {
        ...describe(failed),
        wireBytes: bytes(wire),
        assessment: assessment.report.conclusion,
        validOutcome: first.outcome,
        validRefusal: first.detail ?? null,
      };
    } finally {
      take("cleanup", () => {
        invalid?.dispose();
        value?.dispose();
        contract?.dispose();
        context?.dispose();
        document?.dispose();
      });
    }
    row.complete =
      row.parseDocument +
      row.assess +
      row.contextAndPrepare +
      row.parseValue +
      row.firstValidation;
    row.invalidAndSerializeDescriptive = row.invalid + row.serialize;
    return row;
  }
  // Full job warms lazy process-global fixed schema owners before comparison.
  once();
  once();
  const warmedOwners = sdk.liveStorageOwners();
  const samples = Object.fromEntries(stages.map((stage) => [stage, []]));
  const repetitions = timed ? fixture.repetitions : 1;
  for (let sample = 0; sample < (timed ? 7 : 1); sample++) {
    const sums = Object.fromEntries(stages.map((stage) => [stage, 0]));
    for (let n = 0; n < repetitions; n++) {
      const row = once();
      for (const stage of stages) sums[stage] += row[stage];
    }
    for (const stage of stages) samples[stage].push(sums[stage] / repetitions);
    check(
      sdk.liveStorageOwners() === warmedOwners,
      "tier cleanup must match fully warmed owners",
    );
  }
  const hotContract = prepare(sdk, fixture.document);
  const hotValue = parsed(sdk, fixture.valid);
  const hotBatchTotalsMs = [];
  samples.hot = [];
  try {
    const batch = () =>
      retainedBatch(
        hotContract,
        hotValue,
        fixture.validOutcome,
        fixture.hotBatchRepetitions,
        clock,
      );
    batch();
    batch();
    for (let n = 0; n < (timed ? 7 : 1); n++) {
      const row = batch();
      samples.hot.push(row.perCallMs);
      hotBatchTotalsMs.push(row.elapsedMs);
    }
  } finally {
    hotValue.dispose();
    hotContract.dispose();
  }
  check(sdk.liveStorageOwners() === warmedOwners, "retained batch cleanup");
  const invalidContract = prepare(sdk, fixture.document);
  let invalidValue;
  const invalidBatchTotalsMs = [];
  samples.invalidAndSerialize = [];
  try {
    invalidValue = parsed(sdk, fixture.invalid);
    const batch = () =>
      invalidSerializedBatch(
        invalidContract,
        invalidValue,
        fixture.invalidBatchRepetitions,
        clock,
      );
    batch();
    batch();
    for (let n = 0; n < (timed ? 7 : 1); n++) {
      const row = batch();
      samples.invalidAndSerialize.push(row.perCallMs);
      invalidBatchTotalsMs.push(row.elapsedMs);
    }
  } finally {
    invalidValue?.dispose();
    invalidContract.dispose();
  }
  check(sdk.liveStorageOwners() === warmedOwners, "diagnostic batch cleanup");
  return {
    samplesMs: timed ? samples : null,
    repetitions,
    observation: last,
    owners: { warmed: warmedOwners, released: sdk.liveStorageOwners() },
    concurrency: 1,
    hotCallsPerSecond: timed ? samples.hot.map(callsPerSecond) : null,
    hotBatchRepetitions: fixture.hotBatchRepetitions,
    hotBatchTotalsMs: timed ? hotBatchTotalsMs : null,
    hotMeasurement:
      "one outer timer around retained calls; exact admission and preparation excluded; per-call verdict guard included",
    invalidBatchRepetitions: fixture.invalidBatchRepetitions,
    invalidBatchTotalsMs: timed ? invalidBatchTotalsMs : null,
    invalidMeasurement:
      "one outer timer around retained invalid validations; verdict guard and exactly one JSON.stringify per call inside; prepare/admit and result inspection outside",
    invalidClockResolutionLimited:
      timed && samples.invalidAndSerialize.some((ms) => ms <= 0),
    clockResolutionLimited: timed && samples.hot.some((ms) => ms <= 0),
    throughputMeaning:
      fixture.validOutcome === "satisfies"
        ? "successful retained validations"
        : "resource refusals, not successful validations",
  };
}
export function lifetime(sdk, fixture, timed = true) {
  const clock = timed ? () => performance.now() : () => 0;
  // Warm resource path, including preparation errors and exact value admissions.
  const uri = "https://qualification.invalid/schema";
  const make = (type, generation) => {
    const resource = parsed(sdk, JSON.stringify({ type }));
    const resources = new sdk.SchemaResources([[uri, resource]]);
    const document = parsed(
      sdk,
      JSON.stringify({
        openbindings: "0.2.0",
        "x-generation": generation,
        operations: {
          run: { input: { $ref: uri } },
          ...Object.fromEntries(
            Array.from({ length: 6 }, (_, n) => [
              `op${n}`,
              { input: { type: "integer" } },
            ]),
          ),
        },
      }),
      true,
    );
    let context;
    try {
      context = document.contracts({ resources });
      const result = context.prepare("run", "input");
      check(result.status === "ready", JSON.stringify(result));
      for (let n = 0; n < 6; n++) {
        const other = context.prepare(`op${n}`, "input");
        check(other.status === "ready", "cache entry");
        other.contract.dispose();
      }
      return result.contract; // survives original cache eviction and all parent disposal
    } finally {
      context?.dispose();
      document.dispose();
      resources.dispose();
      resource.dispose();
    }
  };
  const warm = make("integer", -1);
  warm.validate(1);
  warm.dispose();
  const contract = prepare(sdk, fixture.document);
  contract.validate([1]);
  contract.dispose();
  const warmed = sdk.liveStorageOwners();
  let active = make("integer", 0);
  const checkpoints = [],
    replacementSamples = [];
  try {
    for (let n = 0; n < 1000; n++)
      check(
        active.validate(1).outcome === "satisfies",
        "sustained retained call",
      );
    for (let n = 1; n <= 200; n++) {
      const old = active;
      const t = clock();
      active = make(n % 2 ? "string" : "integer", n);
      const oldValue = n % 2 ? 1 : "old";
      check(
        old.validate(oldValue).outcome === "satisfies",
        "old snapshot survives replacement",
      );
      check(
        active.validate(n % 2 ? "new" : 1).outcome === "satisfies",
        "new snapshot",
      );
      check(
        active.validate(oldValue).outcome === "fails",
        "snapshots must not mix",
      );
      old.dispose();
      replacementSamples.push(clock() - t);
      if ([20, 50, 100, 200].includes(n)) {
        active.dispose();
        checkpoints.push({
          replacements: n,
          releasedOwners: sdk.liveStorageOwners(),
        });
        check(
          sdk.liveStorageOwners() === warmed,
          "replacement owner accumulation",
        );
        active = make(n % 2 ? "string" : "integer", n);
      }
    }
    // Exceptions in consumers and pre-aborted validation do not need finalization.
    try {
      const temporary = active.retain();
      try {
        throw Error("caller");
      } finally {
        temporary.dispose();
      }
    } catch (error) {
      check(error.message === "caller", "unexpected exception");
    }
    const cancellation = new AbortController();
    cancellation.abort();
    check(
      active.validate(1, { signal: cancellation.signal }).outcome ===
        "no-verdict",
      "pre-cancellation",
    );
    check(
      active.validate(1).outcome === "satisfies",
      "recovery after cancellation",
    );
  } finally {
    active.dispose();
  }
  check(sdk.liveStorageOwners() === warmed, "final owner cleanup");
  return {
    hotCalls: 1000,
    replacements: 200,
    checkpoints,
    warmedOwners: warmed,
    releasedOwners: sdk.liveStorageOwners(),
    replacementSamplesMs: timed ? replacementSamples : null,
  };
}
export async function discovery(sdk, http, fixtures, timed = true, origin) {
  const rows = {};
  for (const [tier, fixture] of Object.entries(fixtures)) {
    const samples = [];
    const warmed = sdk.liveStorageOwners();
    for (let n = 0; n < (timed ? 9 : 1); n++) {
      const start = timed ? performance.now() : 0;
      const result = await http.discover(
        origin ?? "https://qualification.invalid",
        {
          maxDocumentBytes: 64 * 1024 * 1024,
          fetch: origin
            ? () => fetch(`${origin}/document/${tier}`)
            : async () =>
                new Response(fixture.document, {
                  headers: { "content-type": "application/json" },
                }),
        },
      );
      check(result.status === "found", JSON.stringify(result));
      try {
        check(result.document.operations.length > 0, "discovered inspection");
      } finally {
        result.document.dispose();
      }
      if (n >= 2) samples.push(performance.now() - start);
      check(sdk.liveStorageOwners() === warmed, "discovery cleanup");
    }
    rows[tier] = {
      samplesMs: timed ? samples : null,
      transport: origin
        ? "observed loopback HTTP"
        : "injected in-memory Fetch Response; excludes transport",
      maxDocumentBytes: 64 * 1024 * 1024,
    };
  }
  return rows;
}

export function amplification(sdk, fixture, timed = true) {
  const contract = prepare(sdk, fixture.document),
    value = parsed(sdk, fixture.invalid);
  const samples = { invalid: [], serialize: [], invalidAndSerialize: [] };
  let observation;
  try {
    for (let n = 0; n < (timed ? 9 : 1); n++) {
      const before = timed ? performance.now() : 0;
      const result = contract.validate(value);
      const after = timed ? performance.now() : 0;
      const wire = JSON.stringify(result);
      const ended = timed ? performance.now() : 0;
      check(
        result.outcome === "fails" && result.problemsComplete === false,
        "amplification witness preserves failure and truthful incompleteness",
      );
      observation = {
        outcome: result.outcome,
        complete: result.problemsComplete,
        problems: result.problems.length,
        wireBytes: bytes(wire),
        instancePointerBytes: result.problems.reduce(
          (n, p) => n + bytes(p.instancePointer),
          0,
        ),
        schemaPointerBytes: result.problems.reduce(
          (n, p) => n + bytes(p.schemaLocation?.pointer ?? ""),
          0,
        ),
        codes: [...new Set(result.problems.map((p) => p.code))],
      };
      if (n >= 2) {
        samples.invalid.push(after - before);
        samples.serialize.push(ended - after);
        samples.invalidAndSerialize.push(ended - before);
      }
    }
    return {
      samplesMs: timed ? samples : null,
      observation,
      outputComparison:
        "A changed diagnostic cap changes returned volume; compare costs and payload sizes, never label as equivalent-output speedup.",
    };
  } finally {
    value.dispose();
    contract.dispose();
  }
}
