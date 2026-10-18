// The Document API admits the corpus and runs both evaluator modes.
import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import * as sdk from "../dist/index.js";
await sdk.initialize();
const root = fileURLToPath(new URL("../../../", import.meta.url));
const python = (source, input) =>
  JSON.parse(
    execFileSync("python3", ["-c", source], {
      cwd: root,
      input: input === undefined ? undefined : JSON.stringify(input),
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    }),
  );
const imports = `import sys,json,importlib.util
from pathlib import Path
sys.path.insert(0,"tools/qualification/replay")
import corpus
spec=importlib.util.spec_from_file_location("judge","tools/qualification/replay/judge-browser.py")
judge=importlib.util.module_from_spec(spec);spec.loader.exec_module(judge)
`;
const { core, fixture } = python(
  imports +
    `
fixture=judge.current_fixture(json.loads(Path("crates/openbindings-schema-evaluator-test-support/fixtures/cases.json").read_text()))
print(json.dumps({"core":[corpus.request(c) for c in corpus.load()],"fixture":fixture}))
`,
);
const exact = (text) => {
  const result = sdk.parseJson(text);
  assert.equal(result.status, "parsed");
  return result.value;
};
test("all 440 corpus documents use the same admission and proof evidence", () => {
  assert.equal(core.length, 440);
  for (const request of core) {
    const bytes = Buffer.from(request.given.documentBase64, "base64");
    const opened = sdk.Document.parse(bytes);
    const expected = sdk.validateDocument(bytes);
    if (opened.status !== "parsed") {
      assert.equal(expected.status, "report", request.id);
      assert.notEqual(expected.report.conclusion, "conformant", request.id);
      continue;
    }
    const document = opened.value;
    let proof;
    try {
      assert.deepEqual(document.toBytes(), new Uint8Array(bytes), request.id);
      const checked = document.validate();
      if (checked.status === "conformant") {
        proof = checked.document;
        assert.ok(proof instanceof sdk.ConformantDocument, request.id);
        assert.equal(expected.status, "report", request.id);
        assert.equal(expected.report.conclusion, "conformant", request.id);
        assert.deepEqual(checked.report, expected.report, request.id);
        assert.deepEqual(proof.toBytes(), document.toBytes(), request.id);
      } else assert.deepEqual(checked, expected, request.id);
    } finally {
      proof?.dispose();
      document.dispose();
    }
  }
});
for (const call of ["validate", "explain"]) {
  test(`all 495 evaluator groups through ConformantDocument, ${call}`, () => {
    assert.equal(fixture.groups.length, 495);
    const observations = [];
    for (const group of fixture.groups) {
      const opened = sdk.Document.parse(group.document);
      assert.equal(opened.status, "parsed", group.id);
      const document = opened.value;
      let proof, resources, contract, prepared;
      const inputs = [];
      try {
        const checked = document.validate();
        assert.equal(checked.status, "conformant", group.id);
        proof = checked.document;
        resources = new sdk.SchemaResources(
          group.resources.map((key) => {
            const resource = fixture.resources[key],
              value = exact(resource.documentJson);
            inputs.push(value);
            return [resource.uri, value];
          }),
        );
        const operation = proof.resolveOperation("op");
        try {
          contract = operation?.input();
        } finally {
          operation?.dispose();
        }
        const setup = contract?.prepare({ resources });
        if (setup?.status === "ready") prepared = setup.valueContract;
        const values = group.cases.map((entry) => {
          if (!prepared)
            return setup?.status === "no-verdict"
              ? { outcome: "no-verdict", detail: setup.detail }
              : { outcome: operation ? "no-contract" : "operation-missing" };
          const value = exact(entry.value);
          try {
            return prepared[call](value);
          } finally {
            value.dispose();
          }
        });
        observations.push({ id: group.id, executed: true, values });
      } finally {
        prepared?.dispose();
        contract?.dispose();
        resources?.dispose();
        for (const input of inputs) input.dispose();
        proof?.dispose();
        document.dispose();
      }
    }
    const result = python(
      imports +
        `
fixture,observations=json.load(sys.stdin)
rows,accounting=judge.judge_evaluator(fixture,observations)
failures=[r for r in rows if r["status"]!="pass"]
print(json.dumps({"cases":len(rows),"failures":failures,"accounting":accounting}))
`,
      [fixture, observations],
    );
    assert.equal(result.cases, 1564);
    assert.deepEqual(result.accounting, []);
    assert.deepEqual(result.failures, []);
  });
}
