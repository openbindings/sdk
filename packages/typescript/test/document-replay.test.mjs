// The Document API admits the corpus and runs both evaluator modes.
import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import * as sdk from "../dist/index.js";
await sdk.initialize();
const root = fileURLToPath(new URL("../../../", import.meta.url));
// Run a Python program that reads its request from `request` and writes its
// answer to `answer`. Both travel through files: each exchange here carries
// megabytes, and a synchronous exchange through the child's pipes once hung with
// the child waiting on stdin, so no bulk data crosses a pipe. The child's stdout
// is discarded and its stderr shown; a stalled child fails the test.
const python = (source, input = null) => {
  const directory = mkdtempSync(join(tmpdir(), "document-replay-"));
  try {
    const request = join(directory, "request.json");
    const answer = join(directory, "answer.json");
    writeFileSync(request, JSON.stringify(input));
    execFileSync("python3", ["-c", imports + source, request, answer], {
      cwd: root,
      stdio: ["ignore", "ignore", "inherit"],
      timeout: 600_000,
    });
    return JSON.parse(readFileSync(answer, "utf8"));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
};
const imports = `import sys,json,importlib.util
from pathlib import Path
request=json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
def answer(value):
    Path(sys.argv[2]).write_text(json.dumps(value),encoding="utf-8")
sys.path.insert(0,"tools/qualification/replay")
import corpus
spec=importlib.util.spec_from_file_location("judge","tools/qualification/replay/judge-browser.py")
judge=importlib.util.module_from_spec(spec);spec.loader.exec_module(judge)
`;
test("the Python exchange carries requests and answers larger than any pipe buffer", () => {
  const request = { text: "r".repeat(16 * 1024 * 1024) };
  const result = python(
    `answer({"echo":request["text"],"extra":"a"*(40*1024*1024)})`,
    request,
  );
  assert.equal(result.echo, request.text);
  assert.equal(result.extra.length, 40 * 1024 * 1024);
});
const { core, fixture } = python(`
fixture=judge.current_fixture(json.loads(Path("crates/openbindings-schema-evaluator-test-support/fixtures/cases.json").read_text(encoding="utf-8")))
answer({"core":[corpus.request(c) for c in corpus.load()],"fixture":fixture})
`);
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
              ? { status: "no-verdict", detail: setup.detail }
              : { status: operation ? "no-contract" : "operation-missing" };
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
      `
fixture,observations=request
rows,accounting=judge.judge_evaluator(fixture,observations)
failures=[r for r in rows if r["status"]!="pass"]
answer({"cases":len(rows),"failures":failures,"accounting":accounting})
`,
      [fixture, observations],
    );
    assert.equal(result.cases, 1564);
    assert.deepEqual(result.accounting, []);
    assert.deepEqual(result.failures, []);
  });
}
