// Shared public-API controls for Node, installed-package and browser checks.
// Explanatory wording can evolve; test useful information and safety boundaries.
export function fixedDiagnosticCases(sdk) {
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const byteLength = (text) => new TextEncoder().encode(text).length;
  const cases = [
    {
      name: "description type and original UTF-8 coordinates",
      text: '{\n "openbindings":"0.2.0","name":"é", "operations":{"x":{"description":{"s":"REJECTED_SENTINEL"}}}}',
      pointer: "/operations/x/description",
      token: '{"s":',
      expectedType: "string",
    },
    {
      name: "aliases array type",
      text: '{"openbindings":"0.2.0","operations":{"x":{"aliases":"REJECTED_SENTINEL"}}}',
      pointer: "/operations/x/aliases",
      token: '"REJECTED_SENTINEL"',
      expectedType: "array",
    },
    {
      name: "operations object type",
      text: '{"openbindings":"0.2.0","operations":["REJECTED_SENTINEL"]}',
      pointer: "/operations",
      token: '["REJECTED',
      expectedType: "object",
    },
    {
      name: "required root member uses containing object",
      text: ' \n {"openbindings":"0.2.0"}',
      pointer: "",
      token: "{",
      required: "operations",
    },
    {
      name: "required source member uses containing object",
      text: '{"openbindings":"0.2.0","operations":{},"sources":{"s":{"content":"REJECTED_SENTINEL"}}}',
      pointer: "/sources/s",
      token: '{"content":',
      required: "kind",
    },
    {
      name: "complex failure keeps a generic schema explanation",
      text: '{"openbindings":"0.2.0","operations":{"x":{"input":null}}}',
      pointer: "/operations/x/input",
      token: "null",
      fallback: true,
    },
    {
      name: "corrected healthy document",
      text: '{"openbindings":"0.2.0","operations":{"x":{"description":"A string","aliases":["a"],"input":true}}}',
      healthy: true,
    },
  ];
  const warm = sdk.parseDocument(cases.at(-1).text);
  check(warm.status === "parsed", "warm document parses");
  try {
    warm.value.assess();
  } finally {
    warm.value.dispose();
  }
  const arenas = sdk.liveStorageOwners();
  const results = [];
  for (const fixture of cases) {
    const parsed = sdk.parseDocument(fixture.text);
    check(parsed.status === "parsed", `${fixture.name}: exact parse`);
    try {
      const assessment = parsed.value.assess();
      check(assessment.status === "assessed", `${fixture.name}: assessment`);
      const { report } = assessment;
      check(
        report.conclusion ===
          (fixture.healthy ? "conformant" : "non-conformant"),
        `${fixture.name}: conformance conclusion`,
      );
      check(
        !report.findingsTruncated,
        `${fixture.name}: complete bounded findings`,
      );
      for (const finding of report.findings) {
        check(
          byteLength(finding.message) <= 512,
          `${fixture.name}: bounded message`,
        );
        check(
          !/REJECTED_SENTINEL|internal-meta|__openbindings|projection/i.test(
            finding.message,
          ),
          `${fixture.name}: no rejected value or internal identifier`,
        );
      }
      if (!fixture.healthy) {
        check(
          report.evidence["OBI-02"] === "violated",
          `${fixture.name}: rule evidence`,
        );
        const finding = report.findings.find(
          (item) =>
            item.rule === "OBI-02" &&
            item.code === "schema-mismatch" &&
            item.location?.pointer === fixture.pointer,
        );
        check(
          finding?.status === "violated",
          `${fixture.name}: original finding classification`,
        );
        const preceding = fixture.text.slice(
          0,
          fixture.text.indexOf(fixture.token),
        );
        check(
          finding.location.byteOffset === byteLength(preceding),
          `${fixture.name}: original byte offset`,
        );
        check(
          finding.location.line === preceding.split("\n").length,
          `${fixture.name}: original line`,
        );
        check(
          finding.location.byteColumn ===
            byteLength(preceding.slice(preceding.lastIndexOf("\n") + 1)) + 1,
          `${fixture.name}: original byte column`,
        );
        if (fixture.expectedType)
          check(
            finding.message.includes(fixture.expectedType),
            `${fixture.name}: expected type explained`,
          );
        if (fixture.required)
          check(
            finding.message.includes(JSON.stringify(fixture.required)),
            `${fixture.name}: required member explained`,
          );
        if (fixture.fallback) {
          check(
            /schema/i.test(finding.message),
            `${fixture.name}: generic schema explanation`,
          );
          check(
            !/\b(?:boolean|object|array|string)\b/i.test(finding.message),
            `${fixture.name}: no invented branch repair`,
          );
        }
      }
      results.push({ name: fixture.name, conclusion: report.conclusion });
    } finally {
      parsed.value.dispose();
    }
    check(
      sdk.liveStorageOwners() === arenas,
      `${fixture.name}: released document storage`,
    );
  }
  return results;
}
