import assert from "node:assert/strict";
import test from "node:test";
import { callsPerSecond, retainedBatch } from "./workloads.mjs";
test("retained timer encloses complete batch with a verdict guard", () => {
  const events = [];
  const value = { exact: true };
  const contract = {
    validate(input) {
      assert.equal(input, value);
      events.push("validate");
      return { outcome: "satisfies" };
    },
  };
  let ticks = 0;
  const result = retainedBatch(contract, value, "satisfies", 10, () => {
    events.push("clock");
    return ticks++ * 5;
  });
  assert.deepEqual(events, ["clock", ...Array(10).fill("validate"), "clock"]);
  assert.deepEqual(result, { elapsedMs: 5, perCallMs: 0.5 });
  assert.throws(
    () =>
      retainedBatch(
        { validate: () => ({ outcome: "fails" }) },
        value,
        "satisfies",
        10,
        () => 0,
      ),
    /changed verdict/,
  );
});
test("unresolved and nonfinite durations cannot become a throughput claim", () => {
  for (const duration of [0, -1, NaN, Infinity, -Infinity])
    assert.equal(callsPerSecond(duration), null);
  assert.equal(callsPerSecond(2), 500);
  const contract = { validate: () => ({ outcome: "satisfies" }) };
  assert.equal(
    retainedBatch(contract, null, "satisfies", 10, () => 0).perCallMs,
    0,
  );
  assert.throws(
    () => retainedBatch(contract, null, "satisfies", 10, () => NaN),
    /invalid timer/,
  );
});
