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
    result.status === "no-verdict"
      ? { outcome: "no-verdict", detail: result.detail }
      : { outcome: result.status };
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
    if (request.action === "resolve-operation") {
      const selection = doc.resolveOperation(given.name);
      if (selection.status !== "found")
        return { ...output, outcome: "not-found", selection };
      const op = own(selection.operation);
      return {
        ...output,
        outcome: "resolved",
        operationKey: op.key,
        bindingKeys: op.bindings,
      };
    }
    if (request.action === "check-dependency-kind") {
      const root = own(doc.value());
      const bindings = own(root.get("bindings"));
      const binding = own(bindings.get(given.binding));
      const source = own(binding.get("source"));
      const sourceName = JSON.parse(source.text);
      const sources = own(root.get("sources"));
      const sourceEntry = own(sources.get(sourceName));
      const kind = own(sourceEntry.get("kind"));
      return {
        ...output,
        outcome: doc.dependencyAcceptsKind(
          given.dependency,
          JSON.parse(kind.text),
        )
          ? "meets"
          : "does-not-meet",
      };
    }
    let resources = own(new sdk.SchemaResources());
    for (const r of given.resources ?? []) {
      const value = parsed(sdk.parseJson(r.documentJson));
      const next = own(resources.with(r.uri, value));
      resources.dispose();
      resources = next;
    }
    const validation = doc.validate();
    if (validation.status !== "conformant") throw new Error("Fixture document lacks conformance proof");
    const conformant = own(validation.document);
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
      const prepared = own(preparation.contract);
      return {
        ...output,
        preparation: { status: "ready" },
        values: given.valuesJson.map((text) => {
          const v = parsed(sdk.parseJson(text));
          try {
            return prepared.validate(v);
          } finally {
            v.dispose();
          }
        }),
      };
    }
    if (request.action === "check-examples") {
      const selection = doc.resolveOperation(given.operation);
      if (selection.status !== "found")
        throw new Error("Example operation selection: " + selection.status);
      const op = own(selection.operation);
      const opValue = own(op.value());
      const examples = own(opValue.get("examples"));
      const results = [];
      if (examples) {
        for (const name of Object.keys(JSON.parse(examples.text))) {
          const example = own(examples.get(name));
          for (const direction of ["input", "output"]) {
            const value = own(example.get(direction));
            if (value) {
              const preparation = prepare(given.operation, direction);
              const outcome =
                preparation.status === "ready"
                  ? own(preparation.contract).validate(value)
                  : preparationOutcome(preparation);
              results.push({ ...outcome, example: name, side: direction });
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
