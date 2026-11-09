import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  mkdtempSync,
  realpathSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, sep } from "node:path";
import { fileURLToPath } from "node:url";
import {
  discoverySnippet,
  firstUseJavaScriptSnippet,
  firstUseSnippet,
} from "./readme-snippet.mjs";

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
    "worker-parent-node.mjs",
    "worker-node.mjs",
    "first-use.html",
    "service-lifecycle.mjs",
    "inspect-edit.ts",
  ])
    assert(
      packed[0].files.some((file) => file.path === "examples/" + name),
      `Package must include the runnable ${name} example`,
    );
  // The JavaScript is tsc's unbundled output and the TypeScript sources are not
  // shipped, so the package carries no source maps that would name them.
  assert(
    !packed[0].files.some((file) => file.path.endsWith(".map")),
    "Package must not ship source maps",
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
  // Compile the exact TypeScript blocks delivered in the installed README.
  const installed = join(temporary, "node_modules/@openbindings/sdk");
  for (const file of packed[0].files.filter(({ path }) => path.endsWith(".js")))
    assert.doesNotMatch(
      readFileSync(join(installed, file.path), "utf8"),
      /sourceMappingURL/,
      `${file.path} must not reference a source map`,
    );
  const readme = readFileSync(join(installed, "README.md"), "utf8");
  writeFileSync(join(temporary, "readme.mts"), firstUseSnippet(readme));
  writeFileSync(
    join(temporary, "first-use.mjs"),
    firstUseJavaScriptSnippet(readme),
  );
  writeFileSync(join(temporary, "discovery.mts"), discoverySnippet(readme));
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
      join(temporary, "readme.mts"),
      join(temporary, "discovery.mts"),
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
for (const path of ["./readme-build/readme.mjs", "./first-use.mjs"]) {
  const results = [];
  const log = console.log;
  try {
    console.log = (result) => results.push(JSON.parse(result));
    await import(path);
  } finally {
    console.log = log;
  }
  assert.equal(results.length, 2);
  assert.equal(results[0].result.status, "satisfies");
  const failure = results[1].result;
  assert.equal(failure.status, "fails");
  assert.equal(failure.problemsComplete, true);
  assert.equal(failure.problems[0].instance, "");
  assert.equal(failure.problems[0].code, "type");
  assert.deepEqual(failure.problems[0].details, { kind: "type", expected: ["integer"] });
}
console.log("Installed README TypeScript and JavaScript first-use blocks ran verbatim.");
`,
  );
  execFileSync(process.execPath, [join(temporary, "readme-runner.mjs")], {
    cwd: temporary,
    stdio: "inherit",
  });
  writeFileSync(
    join(temporary, "discovery-runner.mjs"),
    `
import assert from "node:assert/strict";
import { initialize } from "@openbindings/sdk";
import { liveStorageOwners } from "@openbindings/sdk/testing";
await initialize();
const baseline = liveStorageOwners();
const fetch = globalThis.fetch;
const log = console.log;
const cases = [
  [200, '{"openbindings":"0.2.0","operations":{"lookup":{}}}', "found a conformant document"],
  [404, "", "absent (404)"],
  [401, "", "gated (401 or 403)"],
  [403, "", "gated (401 or 403)"],
  [0, "", "transport error: network"],
  [200, "{}", "discovery did not establish a conformant document"],
];
try {
  for (const [index, [status, body, expected]] of cases.entries()) {
    const messages = [];
    globalThis.fetch = async (url) => {
      assert.equal(url, "https://example.com/.well-known/openbindings");
      if (!status) throw new TypeError("simulated network failure");
      return new Response(body, { status });
    };
    console.log = (...args) => messages.push(args.join(" "));
    await import("./readme-build/discovery.mjs?case=" + index);
    assert.deepEqual(messages, [expected]);
    assert.equal(liveStorageOwners(), baseline, "Discovery example must release document owners");
  }
} finally {
  globalThis.fetch = fetch;
  console.log = log;
}
console.log("Installed README discovery block compiled and ran verbatim with simulated Fetch responses.");
`,
  );
  execFileSync(process.execPath, [join(temporary, "discovery-runner.mjs")], {
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
  copyFileSync(
    join(root, "test/conformant-contracts.mjs"),
    join(temporary, "conformant-contracts.mjs"),
  );
  for (const name of [
    "api-quality",
    "fixed-diagnostic",
    "diagnostic-details",
    "inspect-edit-example",
  ]) {
    const filename = join(temporary, name + "-cases.mjs");
    writeFileSync(
      filename,
      readFileSync(filename, "utf8").replaceAll(
        "../dist/testing.js",
        "@openbindings/sdk/testing",
      ),
    );
  }
  const declaration = readdirSync(join(installed, "dist"))
    .filter((name) => name.endsWith(".d.ts"))
    .sort()
    .map((name) => readFileSync(join(installed, "dist", name), "utf8"))
    .join("\n");
  assert(!readdirSync(join(installed, "dist")).includes("internal.js"));
  assert.doesNotMatch(declaration, /interpretationCode/);
  assert.match(declaration, /readonly inputPointer: string \| null/);
  assert.match(declaration, /export declare class Operation\b/);
  writeFileSync(
    join(temporary, "consumer.mjs"),
    `
import assert from "node:assert/strict";
import { realpath } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import * as sdk from "@openbindings/sdk";
import * as http from "@openbindings/sdk/http-discovery";
import * as testing from "@openbindings/sdk/testing";
import { apiQualityCases } from "./api-quality-cases.mjs";
import { fixedDiagnosticCases } from "./fixed-diagnostic-cases.mjs";
import { diagnosticPolicyCases } from "./diagnostic-policy-cases.mjs";
import { diagnosticBudgetCases } from "./diagnostic-budget-cases.mjs";
import { diagnosticDetailsCases } from "./diagnostic-details-cases.mjs";
import { inspectEditExampleCases } from "./inspect-edit-example-cases.mjs";
import * as inspectionEditing from "./readme-build/inspect-edit.js";
assert(!Object.hasOwn(sdk, "liveStorageOwners"));
assert(!Object.hasOwn(http, "liveStorageOwners"));
assert.deepEqual(Object.keys(testing), ["liveStorageOwners"]);
assert.throws(() => testing.liveStorageOwners(), { code: "not-initialized" });
const { initialize, Document } = sdk;
const resolved = fileURLToPath(import.meta.resolve("@openbindings/sdk"));
assert.equal(await realpath(resolved), resolved, "Package must not resolve through a workspace symlink");
assert(resolved.startsWith(${JSON.stringify(join(temporary, "node_modules") + sep)}));
globalThis.fetch = () => { throw new Error("Node default initialization must not fetch"); };
const first = initialize();
assert.equal(first, initialize());
await first;
assert.equal(typeof testing.liveStorageOwners(), "number");
const parsed = Document.parse('{"openbindings":"0.2.0","operations":{"run":{}}}');
assert.equal(parsed.status, "parsed");
try {
  const checked = parsed.value.validate();
  assert.equal(checked.status, "conformant");
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
  assert.equal(firstUse.accepted.result.status, "satisfies");
  assert.equal(firstUse.fails.result.status, "fails");
  assert.equal(firstUse.fails.result.problems[0].instance, "/id");
  assert.equal(firstUse.fails.result.problems[0].code, "minimum");
  assert.deepEqual(firstUse.fails.result.problems[0].details, {
    kind: "numeric-bound",
    bound: "1",
  });
  assert.equal(firstUse.invalidInput.result.status, "input-error");
  const worker = JSON.parse(
    execFileSync(process.execPath, [join(examples, "worker-parent-node.mjs")], {
      cwd: temporary,
      encoding: "utf8",
      timeout: 30_000,
    }),
  );
  assert.deepEqual(worker, firstUse.fails);
  const lifecycle = JSON.parse(
    execFileSync(process.execPath, [join(examples, "service-lifecycle.mjs")], {
      cwd: temporary,
      encoding: "utf8",
    }),
  );
  assert.equal(lifecycle.outcomes.inFlight.status, "satisfies");
  assert.equal(lifecycle.outcomes.current.status, "satisfies");
  assert.equal(lifecycle.outcomes.cancelled.detail.reason, "cancelled");
  assert.equal(lifecycle.outcomes.sameOwnerRecovery.status, "satisfies");
  assert.equal(lifecycle.arenas.released, lifecycle.arenas.warmed);
  console.log(
    "Installed first-use, Node worker and service lifecycle examples passed.",
  );
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
