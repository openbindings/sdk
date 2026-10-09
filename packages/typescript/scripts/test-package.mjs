import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
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
  writeFileSync(
    join(temporary, "consumer.mjs"),
    `
import assert from "node:assert/strict";
import { readFile, realpath } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { initialize, parseDocument } from "@openbindings/sdk";
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
console.log("Installed SDK package: public import, packaged Wasm, parse and validation passed.");
`,
  );
  execFileSync(process.execPath, [join(temporary, "consumer.mjs")], {
    cwd: temporary,
    stdio: "inherit",
  });
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
