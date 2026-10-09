import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const temporary = realpathSync(
  mkdtempSync(join(tmpdir(), "openbindings-package-")),
);
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
try {
  const packed = JSON.parse(
    execFileSync(npm, ["pack", "--json", "--pack-destination", temporary], {
      cwd: root,
      encoding: "utf8",
    }),
  );
  assert.equal(packed.length, 1);
  for (const name of [
    "first-use.mjs",
    "first-use-node.mjs",
    "first-use.html",
    "service-lifecycle.mjs",
  ])
    assert(
      packed[0].files.some((file) => file.path === "examples/" + name),
      `Package must include the runnable ${name} example`,
    );
  writeFileSync(
    join(temporary, "package.json"),
    JSON.stringify({ private: true, type: "module" }),
  );
  execFileSync(
    npm,
    [
      "install",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--no-package-lock",
      join(temporary, packed[0].filename),
    ],
    { cwd: temporary, stdio: "inherit" },
  );
  copyFileSync(
    join(root, "test/api-quality-cases.mjs"),
    join(temporary, "api-quality-cases.mjs"),
  );
  writeFileSync(
    join(temporary, "consumer.mjs"),
    `
import assert from "node:assert/strict";
import { readFile, realpath } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import * as sdk from "@openbindings/sdk";
import * as http from "@openbindings/sdk/http-discovery";
import { apiQualityCases } from "./api-quality-cases.mjs";
const { initialize, parseDocument } = sdk;
const resolved = fileURLToPath(import.meta.resolve("@openbindings/sdk"));
assert.equal(await realpath(resolved), resolved, "Package must not resolve through a workspace symlink");
assert(resolved.startsWith(${JSON.stringify(join(temporary, "node_modules") + sep)}));
await initialize(await readFile(new URL(import.meta.resolve("@openbindings/sdk/openbindings.wasm"))));
const parsed = parseDocument('{"openbindings":"0.2.0","operations":{"run":{}}}');
assert.equal(parsed.status, "parsed");
try {
  const checked = parsed.value.validate();
  assert.equal(checked.status, "validated");
  checked.document.dispose();
} finally {
  parsed.value.dispose();
}
console.log(JSON.stringify({ installedPackage: true, ...await apiQualityCases(sdk, http) }));
`,
  );
  execFileSync(process.execPath, [join(temporary, "consumer.mjs")], {
    cwd: temporary,
    stdio: "inherit",
  });
  const examples = join(temporary, "node_modules/@openbindings/sdk/examples");
  const firstUse = JSON.parse(
    execFileSync(process.execPath, [join(examples, "first-use-node.mjs")], {
      cwd: temporary,
      encoding: "utf8",
    }),
  );
  assert.equal(firstUse.accepted.result.outcome, "satisfies");
  assert.equal(firstUse.mismatch.result.outcome, "mismatch");
  assert.equal(firstUse.invalidInput.result.outcome, "input-error");
  const lifecycle = JSON.parse(
    execFileSync(process.execPath, [join(examples, "service-lifecycle.mjs")], {
      cwd: temporary,
      encoding: "utf8",
    }),
  );
  assert.equal(lifecycle.outcomes.inFlight.outcome, "satisfies");
  assert.equal(lifecycle.outcomes.current.outcome, "satisfies");
  assert.equal(lifecycle.arenas.released, lifecycle.arenas.warmed);
  console.log("Installed first-use and service lifecycle examples passed.");
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
