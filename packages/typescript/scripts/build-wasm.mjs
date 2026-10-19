import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
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
const target = path.resolve(
  repository,
  process.env.CARGO_TARGET_DIR ?? "target",
);
// The module embeds the source path of every panic location it can report.
// Cargo passes workspace sources as relative paths, but registry sources live
// under the builder's Cargo home. Map each builder-specific directory to a fixed
// name so the module's bytes, and its recorded hash, do not depend on where it
// was built. rustc applies the last matching mapping, so the most specific
// directory goes last.
const hostDirectories = [
  [
    path.resolve(process.env.CARGO_HOME ?? path.join(os.homedir(), ".cargo")),
    "/cargo",
  ],
  [
    execFileSync(process.env.RUSTC ?? "rustc", ["--print", "sysroot"], {
      cwd: repository,
      encoding: "utf8",
    }).trim(),
    "/rustc/sysroot",
  ],
  [path.resolve(repository), "/openbindings-sdk"],
  [target, "/target"],
].sort(([a], [b]) => a.length - b.length);
// CARGO_ENCODED_RUSTFLAGS takes precedence over every other flag source, so
// carry any flags the builder set in the environment.
const inherited =
  process.env.CARGO_ENCODED_RUSTFLAGS !== undefined
    ? process.env.CARGO_ENCODED_RUSTFLAGS.split("\x1f")
    : (process.env.RUSTFLAGS ?? "").split(/\s+/);
const rustflags = [
  ...inherited.filter(Boolean),
  ...hostDirectories.map(
    ([from, to]) => "--remap-path-prefix=" + from + "=" + to,
  ),
];
const env = { ...process.env, CARGO_ENCODED_RUSTFLAGS: rustflags.join("\x1f") };
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
  { cwd: repository, stdio: "inherit", env },
);
const output = path.join(packageRoot, "src/wasm");
await mkdir(output, { recursive: true });
execFileSync(
  cli,
  [
    "--target",
    "web",
    "--remove-name-section",
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
// A builder path in the output would make its hash depend on the builder.
for (const name of ["openbindings_wasm.js", "openbindings_wasm_bg.wasm"]) {
  const bytes = await readFile(path.join(output, name));
  for (const directory of [
    ...hostDirectories.map(([from]) => from),
    os.homedir(),
  ]) {
    const prefix = path.join(directory, path.sep);
    if (prefix !== path.sep && bytes.includes(prefix))
      throw new Error(name + " embeds the build-host path " + prefix + ".");
  }
}
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
