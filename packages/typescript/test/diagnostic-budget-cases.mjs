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
      for (const call of ["validate", "explain"]) {
        const result = setup.valueContract[call](42);
        check(result.status === "fails", "same verdict");
        check(
          result.message === "value does not satisfy the value contract",
          "fixed explanation",
        );
        check(
          !result.problemsComplete,
          "first error never claims completeness",
        );
        check(
          result.problems.length === (call === "explain" ? 1 : 0),
          "call selects detail",
        );
        if (call === "explain")
          check(result.problems[0].code === "type", "library first error");
      }
    } finally {
      setup.valueContract.dispose();
    }
    for (const field of ["diagnosticBytes", "maxProblems"]) {
      check(!(field in sdk.evaluatorLimits()), "retired default");
      let rejected = false;
      try {
        prepareInput(parsed.value, "op", { limits: { [field]: 1 } });
      } catch {
        rejected = true;
      }
      check(rejected, "retired limit rejected");
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
      scope: "cheap check or library-bounded first error",
    },
  };
}
