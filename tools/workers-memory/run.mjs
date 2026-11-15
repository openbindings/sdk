import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const root = fileURLToPath(new URL("../../", import.meta.url));
const packagePath = resolve(process.argv[2] ?? `${root}/packages/typescript`);
const output = `${root}/target/workers-memory`;
const probe = fileURLToPath(new URL("./probe.mjs", import.meta.url));
const MiB = 1048576;
const mib = bytes => (bytes / MiB).toFixed(3);
const raw = [], sections = [];
const table = (title, headers, rows) => sections.push(`## ${title}\n\n` +
  [headers, headers.map(() => "---"), ...rows].map(r => `| ${r.join(" | ")} |`).join("\n"));
function run(params) {
  const result = JSON.parse(execFileSync(process.execPath, [probe, packagePath, JSON.stringify(params)], {
    encoding: "utf8", maxBuffer: 1048576,
  }));
  raw.push({ params, ...result });
  return result;
}
const prepare = (kind, n, extra = {}) => run({ kind, n, ...extra });
const work = evaluationSdkWorkUnits => ({ evaluationSdkWorkUnits });
const allowance = maxSdkReservationBytes => ({ maxSdkReservationBytes });

console.error("Document and default preparation tables");
const ordinary = [1, 2, 8, 16, 24, 32].map(size => [size, ...["many", "nested"].map(kind => {
  const result = run({ kind, bytes: size === 32 ? 33554000 : size * MiB });
  assert.equal(result.outcome, "conformant");
  return mib(result.afterBytes);
})]);
table("Document memory", ["Requested MiB", "Many MiB", "Nested MiB"], ordinary);
const dense = [{ kind: "array", n: 999000 }, { kind: "object", n: 499000 },
  { kind: "ids", n: 333320, bytes: 32 * MiB }].map(p => {
  const r = run(p);
  assert.equal(r.outcome, "conformant");
  return [p.kind, r.inputBytes, mib(r.afterBytes)];
});
table("Dense nodes and copied identifiers", ["Shape", "Input bytes", "Linear MiB"], dense);
const records = [];
for (const supplied of [false, true])
  for (const [n, distinct] of [[100, false], [1000, false], [3000, false], [3000, true]]) {
    const r = prepare("records", n, { supplied, distinct });
    assert.equal(r.status, "ready");
    assert.equal(r.outcome, "satisfies");
    records.push([n, supplied ? "supplied" : "contained", distinct,
      mib(r.beforeBytes), mib(r.growthBytes), mib(r.afterBytes)]);
  }
table("Default records", ["Records", "Placement", "Distinct", "Before MiB", "Growth MiB", "After MiB"], records);
table("Default chains", ["Links", "Before MiB", "Growth MiB", "After MiB"],
  [1000, 4000, 16000, 32000, 49000].map(n => {
    const r = prepare("chain", n);
    assert.equal(r.status, "ready");
    assert.equal(r.outcome, "satisfies");
    return [n, mib(r.beforeBytes), mib(r.growthBytes), mib(r.afterBytes)];
  }));

console.error("Preparation allowance sweep and completing thresholds");
const sweep = [];
for (const capped of [false, true])
  for (const cap of [8, 16, 32, 64, 512]) {
    const cells = [];
    for (const [kind, n, supplied] of [["records", 3000, false], ["records", 3000, true], ["chain", 49000, false]]) {
      const r = prepare(kind, n, { supplied, preparationLimits: allowance(cap * MiB),
        ...(capped ? { limits: work(1e8) } : {}) });
      cells.push(`${r.status}, ${mib(r.growthBytes)}`);
    }
    sweep.push([capped ? "100,000,000" : "none", cap, ...cells]);
  }
table("Preparation sweep", ["Evaluation work cap", "Allowance MiB", "Contained records: result, growth MiB",
  "Supplied records: result, growth MiB", "Chain: result, growth MiB"], sweep);

// All candidates are nondefault, keeping Original selected without adding a
// work cap. Lower endpoint refuses; upper completes; relative bracket <= 1%.
const thresholds = [], capacities = new Map([8, 16, 32, 64].map(cap => [cap, { records: 0, chain: 0 }]));
for (const [kind, sizes] of [["records", [10, 100, 300, 1000, 3000]], ["chain", [1000, 4000, 16000, 49000]]]) {
  for (const n of sizes) {
    const trial = bytes => prepare(kind, n, { preparationLimits: allowance(bytes) });
    let low = 0, high = 512 * MiB - 1;
    assert.equal(trial(low).status, "refused");
    let success = trial(high);
    assert.equal(success.status, "ready");
    while (high - low > Math.max(1, low / 100)) {
      const mid = Math.floor((low + high) / 2);
      const r = trial(mid);
      if (r.status === "ready") { high = mid; success = r; }
      else { assert.equal(r.code, "preparation-sdk-reservation-bytes"); low = mid; }
    }
    assert.equal(success.outcome, "satisfies");
    thresholds.push({ kind, n, refusedBytes: low, completingBytes: high, ...success,
      ratio: success.growthBytes ? high / success.growthBytes : null });
    console.error(`${kind} ${n}: (${low}, ${high}] bytes; growth ${success.growthBytes}`);
    for (const [cap, largest] of capacities) {
      if (trial(cap * MiB).status === "ready") largest[kind] = n;
    }
  }
}
table("Smallest completing preparation allowance", ["Shape", "Size", "Refused MiB", "Completing MiB",
  "Growth MiB at completing allowance", "Allowance / growth"], thresholds.map(r => [r.kind, r.n,
  mib(r.refusedBytes), mib(r.completingBytes), mib(r.growthBytes), r.ratio === null ? "undefined (zero growth)" : r.ratio.toFixed(2)]));
table("Largest completing tested size", ["Allowance MiB", "Contained records", "Chain links"],
  [...capacities].map(([cap, largest]) => [cap, largest.records || "none", largest.chain || "none"]));

console.error("Limit calibration cited by the guide");
const cases = [
  { kind: "ids", n: 40000, bytes: 4190000, limits: { maxBytes: 4 * MiB, maxNodes: 250000 } },
  ...[4190000, 8380000].map(bytes => ({ kind: "ids", n: 80000, bytes, pieces: "?#f",
    limits: { maxBytes: bytes < 5 * MiB ? 4 * MiB : 8 * MiB, maxNodes: 250000 } })),
  { kind: "many", bytes: 33554000, limits: { maxBytes: 4 * MiB } },
  { kind: "array", n: 250000 }, { kind: "object", n: 125000 },
  { kind: "array", n: 999000, limits: { maxNodes: 250000 } },
  { kind: "deep", n: 9990 },
  { kind: "findings", n: 100000 }, { kind: "findings", n: 100000, limits: { maxFindings: 100 } },
  ...[1e6, 1e7, 1e8].flatMap(maxSdkWorkUnits => ["records", "chain"].map(kind => ({ kind,
    n: kind === "records" ? 3000 : 49000, preparationLimits: { maxSdkWorkUnits }, limits: work(1e8) }))),
  ...[1e6, 1e7, 1e8].flatMap(cap => ["records", "chain"].map(kind => ({ kind,
    n: kind === "records" ? 3000 : 49000, limits: work(cap) }))),
  ...[1, 4].map(cap => ({ kind: "records", n: 3000, limits: { evaluationSdkReservationBytes: cap * MiB } })),
  { kind: "hostile" }, { kind: "hostile", abortAfterMs: 10 },
  { kind: "hostile", preAbort: true }, { kind: "hostile", limits: work(1e8) },
];
table("Limit calibration", ["Parameters", "Result / code", "Linear MiB", "Growth MiB", "Call ms", "Evaluate ms"],
  cases.map(p => {
    const r = run(p);
    return [JSON.stringify(p), `${r.status ?? r.outcome} / ${r.code ?? r.outcome}`,
      r.afterBytes === undefined ? "" : mib(r.afterBytes), r.growthBytes === undefined ? "" : mib(r.growthBytes),
      r.ms.toFixed(3), r.evaluateMs?.toFixed(3) ?? ""];
  }));

// Entry ownership is separate from the preparation tables above. One synchronous
// turn deliberately leaves every returned record undisposed, with no GC/yield.
const disposal = JSON.parse(execFileSync(process.execPath,
  [resolve(root, "tools/workers-memory/disposal.mjs"), packagePath], { encoding: "utf8" }));
assert.equal(disposal.operations, 27);
assert.equal(disposal.calls, 10000);
assert.equal(disposal.growthBytes, 0);
raw.push({ params: { kind: "entry-disposal" }, ...disposal });
table("Synchronous entry disposal", ["Operations", "Calls", "Before MiB", "After MiB", "Growth MiB"],
  [[disposal.operations, disposal.calls, mib(disposal.beforeBytes), mib(disposal.afterBytes), mib(disposal.growthBytes)]]);

const bytes = readFileSync(`${packagePath}/dist/wasm/openbindings_wasm_bg.wasm`);
const metadata = { node: process.version, platform: process.platform, arch: process.arch,
  wasmSha256: createHash("sha256").update(bytes).digest("hex"),
  build: JSON.parse(readFileSync(`${packagePath}/dist/wasm/build-info.json`, "utf8")) };
mkdirSync(output, { recursive: true });
writeFileSync(`${output}/results.json`, JSON.stringify({ metadata, raw, thresholds }, null, 2) + "\n");
writeFileSync(`${output}/tables.md`, sections.join("\n\n") + "\n");
console.log(sections.join("\n\n"));
console.error(`Saved ${raw.length} fresh-process observations to ${output}`);
