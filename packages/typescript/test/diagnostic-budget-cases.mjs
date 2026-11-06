import { prepareInput } from "./conformant-contracts.mjs";
/** Per-call detail parity, including removal of the old bounded-detail and mode options. */
export function diagnosticBudgetCases(sdk) {
  let assertions = 0;
  const check = (condition, label) => {
    assertions++;
    if (!condition) throw Error(label);
  };
  const parsed = sdk.Document.parse(
    JSON.stringify({
      openbindings: "0.2.0",
      operations: { op: { input: { allOf: [{ type: "string" }, false] } } },
    }),
  );
  try {
    const setup = prepareInput(parsed.value, "op");
    check(setup.status === "ready", "ready");
    try {
      for (const call of ["evaluate", "explain"]) {
        const result = setup.schema[call](42);
        check(result.status === "fails", "same verdict");
        check(
          result.message === "value does not satisfy the schema",
          "fixed explanation",
        );
        check(
          result.problemsComplete === (call === "explain"),
          "diagnostic completeness",
        );
        check(
          result.problems.length === (call === "explain" ? 2 : 0),
          "call selects detail",
        );
        if (call === "explain")
          check(
            result.problems[0].code === "type" &&
              result.problems[1].code === "falseSchema" &&
              result.problems[1].schema.pointer ===
                "/operations/op/input/allOf/1",
            "all original assertion failures",
          );
      }
    } finally {
      setup.schema.dispose();
    }
    check(
      sdk.evaluatorLimits().evaluationSdkWorkUnits === null,
      "work default",
    );
    check(
      sdk.evaluatorLimits().evaluationSdkReservationBytes === 48 * 1024 * 1024,
      "reservation default",
    );
    for (const field of [
      "diagnosticBytes",
      "maxProblems",
      "evaluationSteps",
      "programBytes",
    ]) {
      check(!(field in sdk.evaluatorLimits()), "retired default");
      let rejected = false;
      try {
        prepareInput(parsed.value, "op", { limits: { [field]: 1 } });
      } catch {
        rejected = true;
      }
      check(rejected, "retired limit rejected");
    }
    for (const limits of [
      { evaluationSteps: null, evaluationSdkWorkUnits: null },
      { programBytes: 0, evaluationSdkReservationBytes: 0 },
      ...[null, -1, 1.5, 0x100000000].map((evaluationSdkReservationBytes) => ({
        evaluationSdkReservationBytes,
      })),
    ]) {
      let rejected = false;
      try {
        prepareInput(parsed.value, "op", { limits });
      } catch (e) {
        rejected = e.code === "invalid-evaluator-limits";
      }
      check(rejected, "retired mixed or invalid reservation rejected");
    }
    // Detail is chosen per call; preparation has no mode option.
    for (const diagnostics of [
      true,
      false,
      "details",
      null,
      "coarse",
      "upstream",
    ]) {
      let rejected = false;
      try {
        prepareInput(parsed.value, "op", { diagnostics });
      } catch (e) {
        rejected = e.code === "invalid-work-options";
      }
      check(rejected, "removed mode option rejected");
    }
  } finally {
    parsed.value.dispose();
  }
  return {
    diagnosticBudget: {
      assertions,
      scope: "cheap check or bounded original assertion witness",
    },
  };
}
