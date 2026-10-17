import { prepareInput } from "./conformant-contracts.mjs";
/** Typed diagnostic mode parity, including removal of the old bounded-detail API. */
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
    for (const diagnostics of [undefined, "coarse", "upstream"]) {
      const setup = prepareInput(parsed.value, "op", { diagnostics });
      check(setup.status === "ready", "ready");
      try {
        const result = setup.contract.validate(42);
        check(result.outcome === "fails", "same verdict");
        check(
          result.message === "value does not satisfy the contract",
          "fixed explanation",
        );
        check(
          !result.problemsComplete,
          "first error never claims completeness",
        );
        check(
          result.problems.length === (diagnostics === "upstream" ? 1 : 0),
          "mode selects detail",
        );
        if (diagnostics === "upstream")
          check(result.problems[0].code === "type", "library first error");
      } finally {
        setup.contract.dispose();
      }
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
    for (const diagnostics of [true, false, "details", null]) {
      let rejected = false;
      try {
        prepareInput(parsed.value, "op", { diagnostics });
      } catch (e) {
        rejected = e.code === "invalid-diagnostics";
      }
      check(rejected, "invalid mode rejected");
    }
  } finally {
    parsed.value.dispose();
  }
  return {
    diagnosticBudget: {
      assertions,
      scope: "coarse or library-bounded first error",
    },
  };
}
