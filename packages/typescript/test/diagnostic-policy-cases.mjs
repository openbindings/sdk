/** Public diagnostic policy checks shared with a freshly installed package. */
export function diagnosticPolicyCases(sdk) {
  let assertions = 0;
  const check = (condition, label) => {
    assertions++;
    if (!condition) throw new Error(label);
  };
  const owners = [];
  const own = (value) => {
    owners.push(value);
    return value;
  };
  const document = (value) => {
    const parsed = sdk.parseDocument(
      typeof value === "string" ? value : JSON.stringify(value),
    );
    check(parsed.status === "parsed", "document admitted");
    return own(parsed.value);
  };
  const withSchema = (schema) =>
    document({ openbindings: "0.2.0", operations: { op: { input: schema } } });
  const thrown = (call) => {
    try {
      call();
    } catch (error) {
      return error;
    }
    throw new Error("expected supported error");
  };
  const safe = (message) => {
    check(
      message.length <= 192 &&
        !message.includes("SECRET") &&
        !/[\x00-\x1f\x7f]/.test(message),
      "bounded message without source data",
    );
  };
  try {
    for (const [text, code, pointer] of [
      ['{"openbindings":"0.2.0"}', "missing-operations", ""],
      [
        '{"openbindings":"0.2.0","operations":[]}',
        "invalid-operations-object",
        "/operations",
      ],
      [
        '{"openbindings":"0.2.0","operations":{"op":{"input":true},"other":null}}',
        "invalid-operation-object",
        "/operations/other",
      ],
      [
        '{"openbindings":"0.2.0","operations":{"op":{"input":true},"other":{"aliases":false}}}',
        "invalid-operation-aliases",
        "/operations/other/aliases",
      ],
      [
        '{"openbindings":"0.2.0","operations":{"op":{"input":true},"other":{"aliases":[7]}}}',
        "invalid-operation-alias",
        "/operations/other/aliases/0",
      ],
    ]) {
      const doc = document(text);
      const raw = own(doc.value);
      const source = own(raw.at(pointer));
      for (const call of [
        () => doc.contracts(),
        () => doc.resolveOperation("op"),
      ]) {
        const error = thrown(call);
        check(
          error instanceof sdk.SdkError &&
            error.code === "interpretation" &&
            error.interpretationCode === code,
          "specific interpretation code survives facade",
        );
        check(
          error.location?.pointer === pointer &&
            error.location.byteOffset === source.metadata.location.byteOffset,
          "original source location survives facade",
        );
        check(
          Object.isFrozen(error.location),
          "error source location remains immutable",
        );
      }
    }
    const duplicate = document(
      '{"openbindings":"0.2.0","operations":{"op":{"input":true,"input":false}}}',
    );
    const duplicateError = thrown(() => duplicate.contracts());
    check(
      duplicateError instanceof sdk.SdkError &&
        duplicateError.code === "interpretation" &&
        duplicateError.interpretationCode === "duplicate-members",
      "duplicate interpretation code survives without fabricating a location",
    );
    check(
      duplicateError.location === undefined,
      "unlocated broad refusal remains unlocated",
    );
    const draft = document({
      openbindings: "0.2.0",
      description: 42,
      operations: {
        op: { input: false },
        empty: {},
        a: { aliases: ["shared"] },
        b: { aliases: ["shared"] },
      },
    });
    check(
      draft.assess().report.conclusion === "non-conformant",
      "draft metadata remains nonconformant",
    );
    const ctx = own(draft.contracts());
    check(
      ctx.prepare("missing", "input").status === "operation-missing",
      "missing remains setup state",
    );
    check(
      ctx.prepare("shared", "input").status === "operation-ambiguous",
      "ambiguous remains setup state",
    );
    check(
      ctx.prepare("empty", "input").status === "no-contract",
      "absent remains setup state",
    );
    const prepared = ctx.prepare("op", "input");
    check(
      prepared.status === "ready",
      "nonconformant metadata allows draft preparation",
    );
    const contract = own(prepared.contract);
    check(
      contract.validate(null).outcome === "fails",
      "false schema establishes fails",
    );
    check(
      contract.validate(NaN).outcome === "input-error",
      "ordinary input errors remain admission outcomes",
    );
    const error = thrown(() => ctx.prepare("op", "invalid-side"));
    check(
      error instanceof sdk.SdkError &&
        error.code === "invalid-side" &&
        error.interpretationCode === undefined,
      "non-interpretation errors retain broad code only",
    );
    for (const schema of [
      { $ref: "#/operations/op/input" },
      { anyOf: [true, { $ref: "#/operations/op/input" }] },
    ]) {
      const state = own(withSchema(schema).contracts()).prepare("op", "input");
      check(
        state.status === "no-verdict" &&
          state.detail.reason === "conservative-preparation" &&
          state.detail.code === "in-place-cycle",
        "potential cycles remain conservative",
      );
    }
    const secret =
      "https://SECRET-user:SECRET-password@example.invalid/SECRET-path?token=SECRET-query#SECRET-fragment";
    const patternUri =
      "https://SECRET-user:SECRET-password@example.invalid/SECRET-resource";
    for (const pattern of ["[", "[" + "SECRET-pattern".repeat(2000)]) {
      for (const schema of [
        { type: "string", pattern },
        { patternProperties: { [pattern]: true } },
      ]) {
        const parsed = sdk.parseJson(JSON.stringify(schema));
        check(parsed.status === "parsed", "pattern resource admitted");
        const resources = own(
          new sdk.SchemaResources([[patternUri, own(parsed.value)]]),
        );
        for (const [entry, options] of [
          [schema, undefined],
          [
            {
              $defs: { target: schema },
              $ref: "#/operations/op/input/$defs/target",
            },
            undefined,
          ],
          [{ $ref: patternUri }, { resources }],
        ]) {
          const state = own(withSchema(entry).contracts(options)).prepare(
            "op",
            "input",
          );
          check(
            state.status === "no-verdict" &&
              state.detail.reason === "conservative-preparation" &&
              state.detail.code === "schema-pattern-compilation" &&
              state.detail.message ===
                "a schema regular expression could not be compiled; inspect pattern and patternProperties",
            "pattern refusal provides safe actionable classification",
          );
          check(
            state.detail.location == null,
            "pattern refusal does not guess a source location",
          );
          safe(state.detail.message);
          check(
            !JSON.stringify(state.detail).includes("SECRET"),
            "pattern refusal never exposes source pattern or resource data",
          );
        }
      }
    }
    for (const reference of [
      secret,
      secret + "\nSECRET-control",
      "https://example.invalid/" + "SECRET-long".repeat(2000),
    ]) {
      const doc = withSchema({ $ref: reference });
      const report = doc.references();
      check(
        report.references[0].spelling === reference,
        "explicit source spelling preserved",
      );
      safe(report.references[0].resolution.detail.message);
      const state = own(doc.contracts()).prepare("op", "input");
      check(
        state.status === "no-verdict",
        "unresolved reference refuses preparation",
      );
      safe(state.detail.message);
    }
    const ambiguous = withSchema({
      $ref: "https://example.invalid/SECRET-id",
      $defs: {
        a: { $id: "https://example.invalid/SECRET-id" },
        b: { $id: "https://example.invalid/SECRET-id" },
      },
    });
    const unresolved = ambiguous.references().references[0].resolution;
    check(
      unresolved.detail.code === "ambiguous-resource",
      "ambiguous identifier retains its code",
    );
    safe(unresolved.detail.message);
    const exact = sdk.parseJson("true");
    check(exact.status === "parsed", "resource admitted");
    own(exact.value);
    for (const uri of [
      secret,
      "SECRET-relative",
      "https://example.invalid/" + "SECRET-long".repeat(2000) + "#SECRET",
    ]) {
      const error = thrown(() => new sdk.SchemaResources([[uri, exact.value]]));
      check(
        error instanceof sdk.SdkError && error.code === "resource",
        "resource bridge preserves error category",
      );
      safe(error.message);
      safe(String(error));
    }
    const uri =
      "https://SECRET-user:SECRET-password@example.invalid/?SECRET-query";
    const duplicateResource = thrown(
      () =>
        new sdk.SchemaResources([
          [uri, exact.value],
          [uri, exact.value],
        ]),
    );
    safe(duplicateResource.message);
    for (const [schema, instance, expected] of [
      [{ type: "integer" }, "SECRET-instance", "expected JSON type: integer"],
      [
        { type: ["string", "null"] },
        17,
        "expected one of JSON types: null, string",
      ],
      [
        { enum: ["SECRET-schema"] },
        "SECRET-instance",
        "value does not satisfy the constraint at the schema location",
      ],
      [
        { required: ["SECRET-member"] },
        {},
        "object is missing a required member; inspect the required keyword at the schema location",
      ],
    ]) {
      const state = own(withSchema(schema).contracts()).prepare("op", "input");
      check(state.status === "ready", "type/message contract prepared");
      const outcome = own(state.contract).validate(instance);
      check(
        outcome.outcome === "fails" && outcome.problems[0].message === expected,
        "useful expected type or safe generic guidance",
      );
      for (const problem of outcome.problems) safe(problem.message);
    }
    return { diagnosticPolicyAssertions: assertions };
  } finally {
    for (const owner of owners.reverse()) owner.dispose();
  }
}
