import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { discoverRuntimeArtifacts } from "../scripts/wasm-artifacts.mjs";

const repository = fileURLToPath(new URL("../../../", import.meta.url));
const packageRoot = path.join(repository, "packages/typescript");

test("build record includes nested modules and rejects missing or altered snippets", async (t) => {
  const wasm = path.join(packageRoot, "src/wasm");
  const info = JSON.parse(await readFile(path.join(wasm, "build-info.json")));
  assert.deepEqual(
    Object.keys(info.artifacts),
    await discoverRuntimeArtifacts(wasm),
  );
  const snippets = Object.keys(info.artifacts).filter((name) =>
    name.startsWith("snippets/"),
  );
  assert.ok(snippets.length > 0, "operation transport emits a runtime snippet");

  // Exercise the actual build-record check on an isolated copy of its inputs.
  const temporary = path.join(repository, "target/wasm-artifacts-tests");
  await mkdir(temporary, { recursive: true });
  const fixture = await mkdtemp(path.join(temporary, "sdk-"));
  t.after(() => rm(fixture, { recursive: true, force: true }));
  for (const name of Object.keys(info.rustSource.files)) {
    const destination = path.join(fixture, name);
    await mkdir(path.dirname(destination), { recursive: true });
    await cp(path.join(repository, name), destination);
  }
  const fixturePackage = path.join(fixture, "packages/typescript");
  for (const name of ["scripts", "src/wasm", "src/wasm-module.d.ts"])
    await cp(path.join(packageRoot, name), path.join(fixturePackage, name), {
      recursive: true,
    });
  for (const name of ["LICENSE", "NOTICE"])
    await cp(path.join(repository, name), path.join(fixture, name));
  const check = () =>
    spawnSync(
      process.execPath,
      [path.join(fixturePackage, "scripts/copy-wasm.mjs")],
      { encoding: "utf8" },
    );
  assert.equal(check().status, 0, "the complete build is accepted");
  for (const name of snippets) {
    const snippet = path.join(fixturePackage, "src/wasm", name);
    const bytes = await readFile(snippet);
    assert.deepEqual(
      await readFile(path.join(fixturePackage, "dist/wasm", name)),
      bytes,
    );
    await rm(snippet);
    const missing = check();
    assert.notEqual(missing.status, 0);
    assert.match(missing.stderr, /ENOENT/);
    await writeFile(
      snippet,
      Buffer.concat([bytes, Buffer.from("\n// changed\n")]),
    );
    const altered = check();
    assert.notEqual(altered.status, 0);
    assert.match(altered.stderr, /Generated Wasm artifact changed/);
    await writeFile(snippet, bytes);
  }
});
