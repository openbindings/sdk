// Reproduce the accepted entry-record timing table and synchronous disposal probe.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const [baseline, candidate = "packages/typescript"] = process.argv.slice(2);
if (!baseline) throw new Error("Pass the saved baseline package, then the candidate package.");
const packages = { baseline: resolve(baseline), candidate: resolve(candidate) };
const output = resolve(root, "target/entry-timings");
mkdirSync(output, { recursive: true });
const raw = {};
function run(name, script, suffix, args = [], gc = false) {
  const result = JSON.parse(execFileSync(process.execPath, [
    ...(gc ? ["--expose-gc"] : []), resolve(root, "tools/workers-memory", script),
    packages[name], ...args,
  ], { encoding: "utf8" }));
  raw[`${name}-${suffix}`] = result;
  writeFileSync(`${output}/${name}-${suffix}.json`, JSON.stringify(result, null, 2) + "\n");
  return result;
}
// Separate fresh processes, alternating baseline/candidate order by size.
for (const n of [10, 1000, 10000]) {
  for (const name of n === 1000 ? ["candidate", "baseline"] : ["baseline", "candidate"]) {
    run(name, "entry-reads.mjs", n, [String(n)], true);
    console.error(`${name}: ${n} operations`);
  }
}
for (const name of ["baseline", "candidate"]) {
  run(name, "entry-reads.mjs", "scalars", ["10", "scalars"], true);
  run(name, "disposal.mjs", "disposal");
}
const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
function buildIdentity(path) {
  const info = JSON.parse(readFileSync(`${path}/dist/wasm/build-info.json`, "utf8"));
  return { format: info.format, commit: info.commit, treeClean: info.treeClean,
    cargo: info.cargo, wasmBindgen: info.wasmBindgen,
    rustSourceSha256: info.rustSource.sha256, artifacts: info.artifacts };
}
const metadata = { node: process.version, platform: process.platform, arch: process.arch,
  packages: Object.fromEntries(Object.entries(packages).map(([name, path]) => [name, {
    build: buildIdentity(path),
    documentJsSha256: sha256(readFileSync(`${path}/dist/document.js`)),
    wasmSha256: sha256(readFileSync(`${path}/dist/wasm/openbindings_wasm_bg.wasm`)),
  }])) };
const sections = [];
const table = (title, header, rows) => sections.push(`## ${title}\n\n` +
  [header, header.map(() => "---"), ...rows].map(row => `| ${row.join(" | ")} |`).join("\n"));
for (const [title, field] of [["Warm proof", "warmNs"], ["Fresh proof wrapper", "coldNs"]]) {
  const rows = [];
  for (const n of [10, 1000, 10000])
    for (const name of ["operations", "operation", "operationField", "inputSchema"]) {
      const before = raw[`baseline-${n}`].results[name][field];
      const after = raw[`candidate-${n}`].results[name][field];
      rows.push([n, name, (before / 1000).toFixed(3), (after / 1000).toFixed(3),
        `${(100 * (after / before - 1)).toFixed(1)}%`]);
    }
  table(title, ["Operations", "Read", "Baseline µs", "Candidate µs", "Change"], rows);
}
table("Synchronous entry disposal", ["Package", "Calls", "Before bytes", "After bytes", "Growth bytes"],
  ["baseline", "candidate"].map(name => {
    const r = raw[`${name}-disposal`];
    return [name, r.calls, r.beforeBytes, r.afterBytes, r.growthBytes];
  }));
writeFileSync(`${output}/results.json`, JSON.stringify({ metadata, raw }, null, 2) + "\n");
writeFileSync(`${output}/tables.md`, sections.join("\n\n") + "\n");
console.log(sections.join("\n\n"));
