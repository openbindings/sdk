import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  mkdtempSync,
  realpathSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { firstUseSnippet } from "./readme-snippet.mjs";

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
    "inspect-edit.ts",
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
  // Compile the exact first-use block delivered in the installed README.
  // Host initialization is intentionally outside this host-independent block.
  const installed = join(temporary, "node_modules/@openbindings/sdk");
  const snippet = firstUseSnippet(
    readFileSync(join(installed, "README.md"), "utf8"),
  );
  writeFileSync(join(temporary, "readme.ts"), snippet);
  copyFileSync(
    join(installed, "examples/inspect-edit.ts"),
    join(temporary, "inspect-edit.ts"),
  );
  execFileSync(
    process.execPath,
    [
      join(root, "node_modules/typescript/bin/tsc"),
      "--strict",
      "--target",
      "ES2022",
      "--module",
      "NodeNext",
      "--moduleResolution",
      "NodeNext",
      "--lib",
      "ES2022,DOM,ESNext.Disposable",
      "--outDir",
      join(temporary, "readme-build"),
      join(temporary, "readme.ts"),
      join(temporary, "inspect-edit.ts"),
    ],
    { cwd: temporary, stdio: "inherit" },
  );
  writeFileSync(
    join(temporary, "readme-runner.mjs"),
    `
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { initialize } from "@openbindings/sdk";
await initialize(await readFile(new URL(import.meta.resolve("@openbindings/sdk/openbindings.wasm"))));
const results = [];
const log = console.log;
try {
  console.log = (result) => results.push(result);
  await import("./readme-build/readme.js");
} finally {
  console.log = log;
}
assert.equal(results.length, 2);
assert.equal(results[0].result.outcome, "satisfies");
assert.equal(results[1].result.outcome, "fails");
console.log("Installed README TypeScript first-use block compiled and ran verbatim.");
`,
  );
  execFileSync(process.execPath, [join(temporary, "readme-runner.mjs")], {
    cwd: temporary,
    stdio: "inherit",
  });
  copyFileSync(
    join(root, "test/api-quality-cases.mjs"),
    join(temporary, "api-quality-cases.mjs"),
  );
  copyFileSync(
    join(root, "test/fixed-diagnostic-cases.mjs"),
    join(temporary, "fixed-diagnostic-cases.mjs"),
  );
  copyFileSync(
    join(root, "test/diagnostic-policy-cases.mjs"),
    join(temporary, "diagnostic-policy-cases.mjs"),
  );
  copyFileSync(
    join(root, "test/diagnostic-budget-cases.mjs"),
    join(temporary, "diagnostic-budget-cases.mjs"),
  );
  copyFileSync(
    join(root, "test/diagnostic-details-cases.mjs"),
    join(temporary, "diagnostic-details-cases.mjs"),
  );
  copyFileSync(
    join(root, "test/inspect-edit-example-cases.mjs"),
    join(temporary, "inspect-edit-example-cases.mjs"),
  );
  const declaration = readFileSync(
    join(installed, "dist/internal.d.ts"),
    "utf8",
  );
  assert.match(declaration, /interpretationCode\?: string/);
  assert.match(declaration, /Specific interpretation refusal/);
  writeFileSync(
    join(temporary, "consumer.mjs"),
    `
import assert from "node:assert/strict";
import { readFile, realpath } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import * as sdk from "@openbindings/sdk";
import * as http from "@openbindings/sdk/http-discovery";
import { apiQualityCases } from "./api-quality-cases.mjs";
import { fixedDiagnosticCases } from "./fixed-diagnostic-cases.mjs";
import { diagnosticPolicyCases } from "./diagnostic-policy-cases.mjs";
import { diagnosticBudgetCases } from "./diagnostic-budget-cases.mjs";
import { diagnosticDetailsCases } from "./diagnostic-details-cases.mjs";
import { inspectEditExampleCases } from "./inspect-edit-example-cases.mjs";
import * as inspectionEditing from "./readme-build/inspect-edit.js";
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
console.log(JSON.stringify({ installedPackage: true, ...await apiQualityCases(sdk, http), fixedDiagnostics: fixedDiagnosticCases(sdk), ...diagnosticPolicyCases(sdk), ...diagnosticBudgetCases(sdk), ...diagnosticDetailsCases(sdk) }));
console.log(JSON.stringify(inspectEditExampleCases(sdk, inspectionEditing)));
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
  assert.equal(firstUse.fails.result.outcome, "fails");
  assert.equal(firstUse.invalidInput.result.outcome, "input-error");
  const lifecycle = JSON.parse(
    execFileSync(process.execPath, [join(examples, "service-lifecycle.mjs")], {
      cwd: temporary,
      encoding: "utf8",
    }),
  );
  assert.equal(lifecycle.outcomes.inFlight.outcome, "satisfies");
  assert.equal(lifecycle.outcomes.current.outcome, "satisfies");
  assert.equal(lifecycle.outcomes.cancelled.detail.reason, "cancelled");
  assert.equal(lifecycle.outcomes.sameOwnerRecovery.outcome, "satisfies");
  assert.equal(lifecycle.arenas.released, lifecycle.arenas.warmed);
  console.log("Installed first-use and service lifecycle examples passed.");
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
