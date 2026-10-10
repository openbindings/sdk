// Shared public-API controls for Node, installed-package and browser checks.
// Explanatory wording can evolve; test useful information and safety boundaries.
export function fixedDiagnosticCases(sdk) {
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const byteLength = (text) => new TextEncoder().encode(text).length;
  const unexpectedMessage =
    "this member is not permitted here; extension member names begin with x-";
  const cases = [
    {
      name: "unexpected member identifies escaped original key after multibyte prefix",
      text: '{"openbindings":"0.2.0","name":"é 😀","operations":{"x":{"inputs/~\\n":0}}}',
      pointer: "/operations/x/inputs~1~0\n",
      token: '"inputs/',
      unexpected: true,
    },
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
        if (fixture.unexpected)
          check(
            finding.message === unexpectedMessage,
            `${fixture.name}: disallowed member explained`,
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
  for (const count of [4096, 4097]) {
    const fields = Array.from({ length: count }, (_, i) => `,"bad${i}":0`).join(
      "",
    );
    const text = `{"openbindings":"0.2.0","operations":{}${fields}}`;
    for (const invalidSchema of [false, true]) {
      const source = invalidSchema
        ? text.replace(
            '"operations":{}',
            '"operations":{},"schemas":{"s":{"type":7}}',
          )
        : text;
      const parsed = sdk.parseDocument(source);
      check(parsed.status === "parsed", "saturated document parses");
      try {
        const assessment = parsed.value.assess();
        check(
          assessment.status === "assessed",
          "saturated assessment completes",
        );
        const report = assessment.report;
        check(
          report.conclusion === "non-conformant",
          "saturated conformance remains refused",
        );
        check(report.findings.length === 4096, "finding count remains bounded");
        check(
          report.findingsTruncated === (invalidSchema || count > 4096),
          "truthful truncation",
        );
        check(
          report.evidence["OBI-02"] === "violated",
          "document evidence preserved",
        );
        check(
          Object.keys(report.evidence).length === 13,
          "all rule evidence retained",
        );
        if (invalidSchema)
          check(
            report.evidence["OBI-10"] === "violated",
            "later evidence survives finding saturation",
          );
        else
          check(
            report.findings.at(-1).location.pointer === "/bad4095",
            "original member order retained",
          );
      } finally {
        parsed.value.dispose();
      }
    }
    results.push({
      name: `bounded ${count} unexpected members`,
      conclusion: "non-conformant",
    });
  }
  for (const [name, count] of [
    ["x-duplicates", 4095],
    ["x-duplicates", 4096],
    ["x-duplicates", 4097],
    ["x-" + "a".repeat(4096), 4096],
  ]) {
    const text = `{"openbindings":"0.2.0","operations":{},"x-pad":"${"p".repeat(128 * 1024)}",\n"${name}":{"k":0${',"k":0'.repeat(count)}}}`;
    const parsed = sdk.parseDocument(text);
    check(parsed.status === "parsed", "duplicate input parses exactly");
    try {
      const assessed = parsed.value.assess();
      check(assessed.status === "assessed", "duplicate assessment completes");
      const report = assessed.report;
      const expected = Math.min(
        count,
        4096,
        Math.floor((8 * 1024 * 1024) / (name.length + 1)),
      );
      check(
        report.findings.length === expected,
        "duplicate retention honors both caps",
      );
      check(
        report.findingsTruncated === expected < count,
        "duplicate truncation is truthful",
      );
      check(
        report.evidence["OBI-01"] === "violated" &&
          report.evidence["OBI-02"] === "not-applicable",
        "duplicate evidence is independent of retention",
      );
      for (const finding of report.findings) {
        check(finding.code === "duplicate-member", "duplicate code preserved");
        check(
          finding.location.pointer === "/" + name,
          "containing-object pointer preserved",
        );
        check(
          finding.location.byteOffset === text.indexOf('{"k":'),
          "original containing-object offset preserved",
        );
        check(finding.location.line === 2, "original line preserved");
      }
      results.push({
        name: `duplicate bounds ${name.length} ${count}`,
        findings: report.findings.length,
      });
    } finally {
      parsed.value.dispose();
    }
  }
  const invalidName = sdk.parseDocument(
    '{"openbindings":"0.2.0","operations":{"/private":{"aliases":[".private"],"examples":{"-private":{}}}}}',
  );
  check(invalidName.status === "parsed", "invalid names parse");
  try {
    const assessed = invalidName.value.assess();
    check(assessed.status === "assessed", "invalid names assess");
    const names = assessed.report.findings.filter(
      (f) => f.code === "name-grammar",
    );
    check(
      names.length === 3,
      "name guidance covers keys, aliases and example names",
    );
    for (const f of names)
      check(
        f.message.includes("start with a letter, digit or underscore") &&
          f.message.includes("dots or hyphens") &&
          !f.message.includes("private"),
        "grammar is useful and does not echo rejected names",
      );
  } finally {
    invalidName.value.dispose();
  }
  check(
    sdk.liveStorageOwners() === arenas,
    "all expanded diagnostic storage released",
  );
  return results;
}
