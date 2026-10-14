// Shared public-API controls for Node, installed-package and browser checks.
// Explanatory wording can evolve; test useful information and safety boundaries.
export function fixedDiagnosticCases(sdk) {
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const byteLength = (text) => new TextEncoder().encode(text).length;
  const unexpectedMessage = "value violates the fixed normative schema";
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
      // A union failure keeps the checked position and fixed message.
      name: "complex failure keeps its position without inventing a branch repair",
      text: '{"openbindings":"0.2.0","operations":{"x":{"input":null}}}',
      pointer: "/operations/x/input",
      token: "null",
      keyword: "anyOf",
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
    warm.value.validate();
  } finally {
    warm.value.dispose();
  }
  const arenas = sdk.liveStorageOwners();
  const results = [];
  for (const fixture of cases) {
    const parsed = sdk.parseDocument(fixture.text);
    check(parsed.status === "parsed", `${fixture.name}: exact parse`);
    try {
      const assessment = parsed.value.validate();
      check(assessment.status === "report", `${fixture.name}: assessment`);
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
        check(
          finding.message === unexpectedMessage,
          `${fixture.name}: fixed message`,
        );
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
  {
    const parsed = sdk.parseDocument(
      '{"openbindings":"0.2.0","operations":{"run":{"aliases":["valid",false,42],"description":0,"input":{"type":42}}}}',
    );
    try {
      const report = parsed.value.validate().report;
      const fixed = report.findings.filter((f) => f.code === "schema-mismatch");
      check(
        JSON.stringify(fixed.map((f) => [f.rule, f.location.pointer])) ===
          JSON.stringify([
            ["OBI-02", "/operations/run/aliases/1"],
            ["OBI-02", "/operations/run/aliases/2"],
            ["OBI-02", "/operations/run/description"],
            ["OBI-10", "/operations/run/input"],
          ]),
        "multiple positions and localized meta node",
      );
      check(
        fixed.every((f) => f.message === unexpectedMessage),
        "fixed messages in both rules",
      );
    } finally {
      parsed.value.dispose();
    }
    results.push({
      name: "multiple positions and localized meta node",
      conclusion: "non-conformant",
    });
  }
  for (const [text, expected] of [
    ['{"openbindings":"0.2.0","name":42}', ["/name", ""]],
    [
      '{"openbindings":"0.2.0","operations":{},"sources":{"s":{"description":42}}}',
      ["/sources/s/description", "/sources/s"],
    ],
    [
      '{"openbindings":"0.2.0","operations":{"op":{"aliases":[false,false]}}}',
      [
        "/operations/op/aliases/0",
        "/operations/op/aliases/1",
        "/operations/op/aliases",
      ],
    ],
  ]) {
    const report = sdk.validateDocument(text).report;
    const fixed = report.findings.filter((f) => f.rule === "OBI-02");
    check(
      JSON.stringify(fixed.map((f) => f.location.pointer)) ===
        JSON.stringify(expected),
      "container and child positions",
    );
    check(
      !report.findingsTruncated &&
        fixed.every((f) => f.message === unexpectedMessage),
      "complete fixed container guidance",
    );
    results.push({
      name: `container and child ${expected.join(",")}`,
      conclusion: report.conclusion,
    });
  }
  for (const count of [4096, 4097]) {
    const schemas = Object.fromEntries(
      Array.from({ length: count }, (_, i) => [`s${i}`, { minLength: -1 }]),
    );
    const parsed = sdk.parseDocument(
      JSON.stringify({ openbindings: "0.2.0", operations: {}, schemas }),
    );
    try {
      const report = parsed.value.validate().report;
      check(report.findings.length === 4096, "localized finding cap");
      check(report.findingsTruncated === count > 4096, "truthful truncation");
      check(
        report.evidence["OBI-02"] === "met" &&
          report.evidence["OBI-10"] === "violated",
        "independent evidence",
      );
      check(
        Object.keys(report.evidence).length === 13,
        "all rule evidence retained",
      );
    } finally {
      parsed.value.dispose();
    }
    results.push({
      name: `bounded ${count} schema nodes`,
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
      const assessed = parsed.value.validate();
      check(assessed.status === "report", "duplicate assessment completes");
      const report = assessed.report;
      const expected = Math.min(
        count,
        4096,
        Math.floor((8 * 1024 * 1024) / (name.length + 3)),
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
      // The first occurrence is valid; every later occurrence names its own
      // offending key token, even though all occurrences share one pointer.
      let tokenOffset = text.indexOf('"k":0');
      const offsets = new Set();
      for (const finding of report.findings) {
        tokenOffset = text.indexOf('"k":0', tokenOffset + 1);
        check(finding.code === "duplicate-member", "duplicate code preserved");
        check(
          finding.location.pointer === "/" + name + "/k",
          "offending-name pointer preserved",
        );
        check(
          finding.location.byteOffset === tokenOffset,
          "original offending-name offset preserved in ASCII fixture",
        );
        check(finding.location.line === 2, "original line preserved");
        check(
          finding.location.byteColumn === tokenOffset - text.indexOf("\n"),
          "original offending-name byte column preserved",
        );
        offsets.add(finding.location.byteOffset);
      }
      check(
        offsets.size === expected,
        "duplicate occurrences have distinct offsets",
      );
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
    const assessed = invalidName.value.validate();
    check(assessed.status === "report", "invalid names validate");
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
