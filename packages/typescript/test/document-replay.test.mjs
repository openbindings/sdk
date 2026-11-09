// The Document API preserves admission and proof evidence across the corpus.
import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import * as sdk from "../dist/index.js";
await sdk.initialize();
const root = fileURLToPath(new URL("../../../", import.meta.url));
// Load the corpus through an answer file so its size is independent of pipe
// buffers. Discard stdout, show stderr and bound the child execution time.
const python = (source) => {
  const directory = mkdtempSync(join(tmpdir(), "document-replay-"));
  try {
    const answer = join(directory, "answer.json");
    execFileSync("python3", ["-c", imports + source, answer], {
      cwd: root,
      stdio: ["ignore", "ignore", "inherit"],
      timeout: 600_000,
    });
    return JSON.parse(readFileSync(answer, "utf8"));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
};
const imports = `import sys,json
from pathlib import Path
def answer(value):
    Path(sys.argv[1]).write_text(json.dumps(value),encoding="utf-8")
sys.path.insert(0,"tools/qualification/replay")
import corpus
`;

const core = python(`answer([corpus.request(c) for c in corpus.load()])`);
test("all 505 corpus documents use the same admission and proof evidence", () => {
  assert.equal(core.length, 505);
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
