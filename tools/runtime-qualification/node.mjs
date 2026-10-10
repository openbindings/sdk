import fs from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";
import { fixtureSet, identity, host, save, summaries } from "./common.mjs";
import { runTier, lifetime, discovery, amplification } from "./workloads.mjs";
const [packageRoot, fixtureRoot, output, mode = "check"] =
  process.argv.slice(2);
if (!output)
  throw Error(
    "Usage: node node.mjs UNPACKED_PACKAGE FIXTURES OUTPUT [check|measure|cold]",
  );
const timed = mode !== "check";
const clock = timed ? () => performance.now() : () => 0;
const setup = {};
let start = clock();
const sdk = await import(
  pathToFileURL(path.resolve(packageRoot, "dist/index.js"))
);
setup.moduleImport = clock() - start;
start = clock();
const wasm = await fs.readFile(
  path.join(packageRoot, "dist/wasm/openbindings_wasm_bg.wasm"),
);
setup.assetRead = clock() - start;
start = clock();
const module = await WebAssembly.compile(wasm);
setup.compile = clock() - start;
start = clock();
await sdk.initialize(module);
setup.instantiateAndInitialize = clock() - start;
setup.total = Object.values(setup).reduce((a, b) => a + b, 0);
if (mode === "cold") {
  console.log(JSON.stringify(setup));
  process.exit(0);
}
const { fixtures, manifest, adversarial } = await fixtureSet(fixtureRoot);
const before = process.memoryUsage();
const tiers = Object.fromEntries(
  Object.entries(fixtures).map(([tier, fixture]) => {
    const result = runTier(sdk, fixture, timed);
    if (timed) result.summary = summaries(result.samplesMs);
    return [tier, result];
  }),
);
const retained = lifetime(sdk, fixtures.representative, timed);
const http = await import(
  pathToFileURL(path.resolve(packageRoot, "dist/http-discovery.js"))
);
const found = await discovery(sdk, http, fixtures, timed);
const witness = amplification(sdk, adversarial, timed);
const cold = [];
if (timed)
  for (let n = 0; n < 7; n++) {
    const child = spawnSync(
      process.execPath,
      [process.argv[1], packageRoot, fixtureRoot, output, "cold"],
      { encoding: "utf8", timeout: 120000 },
    );
    if (child.status !== 0) throw Error(child.stderr);
    cold.push(JSON.parse(child.stdout));
  }
// Capture workload memory before gzip/Brotli asset inspection allocates its own
// compression buffers. These remain whole-process, not SDK-only, observations.
const memory = {
  before,
  after: process.memoryUsage(),
  maxRSSKiB: process.resourceUsage().maxRSS,
  meaning:
    "Whole Node process after workloads/cold orchestration, before asset compression; RSS/heap/external buffers are not live SDK bytes. Wasm capacity unavailable through public facade.",
};
await save(output, {
  host: host(),
  kind: "node",
  timed,
  package: await identity(packageRoot),
  fixtures: manifest,
  tiers,
  lifetime: retained,
  discovery: found,
  amplification: witness,
  cold,
  memory,
});
console.log(
  JSON.stringify({
    kind: "node",
    mode,
    output,
    owners: retained.releasedOwners,
    tiers: Object.keys(tiers),
  }),
);
