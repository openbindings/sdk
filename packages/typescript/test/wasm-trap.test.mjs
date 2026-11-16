import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const run = promisify(execFile);
const fixture = fileURLToPath(new URL("./wasm-trap.mjs", import.meta.url));
for (const mode of [
  "memory",
  "initialize",
  "parse",
  "ordinary-parser",
  "ordinary-evaluate",
  "ordinary-conversion",
  "evaluate-json",
  "explain-json",
  "document-parse",
  "create",
  "read",
  "iterator",
  "preparation-transfer",
  "validation-transfer",
  "discovery-assessment",
  "resource-registration",
  "edit",
  "dispose",
  "finalizer",
]) {
  test(`Wasm trap at ${mode} makes the instance unusable and cleanup safe`, async () => {
    const flags = mode === "memory" ? ["--wasm-max-mem-pages=1024"] : [];
    const { stdout } = await run(process.execPath, [...flags, fixture, mode], {
      timeout: 120_000,
    });
    const result = JSON.parse(stdout);
    assert.equal(result.mode, mode);
    assert.equal(result.code, "wasm-trap");
  });
}
