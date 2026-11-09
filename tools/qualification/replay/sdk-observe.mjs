/** Host-neutral observer using only the shipped TypeScript SDK surface. */
export function observe(sdk, request) {
  const owned = [];
  const own = (value) => {
    if (value) owned.push(value);
    return value;
  };
  const parsed = (result) => {
    if (result.status !== "parsed") throw new Error(JSON.stringify(result));
    return own(result.value);
  };
  // Normalize the staged public API to the corpus observation vocabulary, while
  // recording the setup state separately. Non-ready setup never validates values.
  const preparationOutcome = (result) =>
    result.status === "refused"
      ? { status: "preparation-refused", failure: result.failure }
      : { status: result.status === "no-contract" ? "no-schema" : result.status };
  const output = { id: request.id, executed: true };
  const given = request.given;
  const bytes = Uint8Array.from(atob(given.documentBase64), (c) =>
    c.charCodeAt(0),
  );
  try {
    if (request.action === "validate-document") {
      const assessment = sdk.validateDocument(bytes);
      if (assessment.status === "version-refused")
        return {
          ...output,
          outcome: "version-refused",
          refusal: assessment.refusal,
        };
      return {
        ...output,
        outcome: assessment.report.conclusion,
        rules: Object.fromEntries(Object.entries(assessment.report.evidence).map(([rule, evidence]) => [rule, evidence === "met" ? "satisfied" : evidence])),
        findings: assessment.report.findings.map(finding => ({ ...finding, status: finding.status === "met" ? "satisfied" : finding.status, code: ((["OBI-06", "OBI-08"].includes(finding.rule) && finding.code === "unknown-operation") || (finding.rule === "OBI-07" && finding.code === "unknown-source")) ? "missing-target" : finding.code })),
      };
    }
    const doc = parsed(sdk.Document.parse(bytes));
    const validation = doc.validate();
    if (validation.status !== "conformant") throw new Error("Fixture document lacks conformance proof");
    const conformant = own(validation.document);
    if (request.action === "resolve-operation") {
      const op = own(conformant.resolveOperation(given.name));
      if (!op) return { ...output, outcome: "not-found" };
      return {
        ...output,
        outcome: "resolved",
        operationKey: op.key,
        bindingKeys: op.bindings().map(binding => own(binding).key),
      };
    }
    if (request.action === "check-dependency-kind") {
      const binding = own(conformant.binding(given.binding));
      const source = own(binding.source());
      const dependency = own(conformant.dependency(given.dependency));
      return { ...output, outcome: dependency.acceptsKind(source.kind) ? "meets" : "does-not-meet" };
    }
    let resources = own(new sdk.ExternalSchemas());
    for (const r of given.resources ?? []) {
      const value = parsed(sdk.parseJson(r.documentJson));
      const next = own(resources.with(r.uri, value));
      resources.dispose();
      resources = next;
    }
    // A missing operation and an absent contract are setup states found before
    // preparation; conformance excludes ambiguous selection.
    const prepare = (name, side) => {
      if (side !== "input" && side !== "output")
        throw new Error("Unknown operation side: " + side);
      const operation = own(conformant.resolveOperation(name));
      if (!operation) return { status: "operation-missing" };
      const contract = own(side === "input" ? operation.input() : operation.output());
      if (!contract) return { status: "no-contract" };
      return contract.prepare({ resources });
    };
    if (request.action === "validate-operation-values") {
      const preparation = prepare(given.operation, given.side);
      if (preparation.status !== "ready")
        return {
          ...output,
          preparation,
          values: given.valuesJson.map(() => preparationOutcome(preparation)),
        };
      const prepared = own(preparation.schema);
      return {
        ...output,
        preparation: { status: "ready" },
        values: given.valuesJson.map((text) => {
          const v = parsed(sdk.parseJson(text));
          try {
            return prepared.evaluate(v);
          } finally {
            v.dispose();
          }
        }),
      };
    }
    if (request.action === "check-examples") {
      const op = own(conformant.resolveOperation(given.operation));
      if (!op) throw new Error("Example operation missing");
      const examples = op.examples()?.map(own);
      const results = [];
      if (examples) {
        for (const example of examples) {
          for (const direction of ["input", "output"]) {
            const value = own(direction === "input" ? example.input() : example.output());
            if (value) {
              const preparation = prepare(given.operation, direction);
              const outcome =
                preparation.status === "ready"
                  ? own(preparation.schema).evaluate(value)
                  : preparationOutcome(preparation);
              results.push({ ...outcome, example: example.key, side: direction });
            }
          }
        }
      }
      return { ...output, examples: results };
    }
    return { ...output, executed: false, outcome: "unknown-action" };
  } finally {
    for (const value of owned.reverse()) value.dispose();
  }
}
