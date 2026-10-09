import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  repository,
  packageRoot,
  rustSourceIdentity,
  sha256,
} from "./source-identity.mjs";
const cli = process.env.WASM_BINDGEN ?? "wasm-bindgen";
const version = execFileSync(cli, ["--version"], { encoding: "utf8" }).trim();
if (version !== "wasm-bindgen 0.2.129")
  throw new Error("Build requires wasm-bindgen 0.2.129.");
const before = await rustSourceIdentity();
execFileSync(
  "cargo",
  [
    "build",
    "--locked",
    "--release",
    "--target",
    "wasm32-unknown-unknown",
    "-p",
    "openbindings-wasm",
  ],
  { cwd: repository, stdio: "inherit" },
);
const target = path.resolve(
  repository,
  process.env.CARGO_TARGET_DIR ?? "target",
);
const output = path.join(packageRoot, "src/wasm");
await mkdir(output, { recursive: true });
execFileSync(
  cli,
  [
    "--target",
    "web",
    "--out-dir",
    output,
    path.join(target, "wasm32-unknown-unknown/release/openbindings_wasm.wasm"),
  ],
  { stdio: "inherit" },
);
const after = await rustSourceIdentity();
if (before.sha256 !== after.sha256)
  throw new Error(
    "Rust source changed during the Wasm build; rebuild the stable source.",
  );
// A source archive may sit inside an unrelated Git checkout. Only attribute
// this package to Git when its own root carries a repository/worktree marker.
const checkout = existsSync(path.join(repository, ".git"));
const info = {
  format: "openbindings.wasm-build@1",
  rustSource: after,
  commit: checkout
    ? execFileSync("git", ["rev-parse", "HEAD"], {
        cwd: repository,
        encoding: "utf8",
      }).trim()
    : null,
  treeClean: checkout
    ? !execFileSync("git", ["status", "--porcelain"], {
        cwd: repository,
        encoding: "utf8",
      }).trim()
    : null,
  cargo: execFileSync("cargo", ["--version"], { encoding: "utf8" }).trim(),
  wasmBindgen: version,
  artifacts: Object.fromEntries(
    await Promise.all(
      ["openbindings_wasm.js", "openbindings_wasm_bg.wasm"].map(
        async (name) => [name, sha256(await readFile(path.join(output, name)))],
      ),
    ),
  ),
};
await writeFile(
  path.join(output, "build-info.json"),
  JSON.stringify(info, null, 2) + "\n",
);
console.log(
  JSON.stringify({
    rustSource: info.rustSource.sha256,
    artifacts: info.artifacts,
  }),
);
