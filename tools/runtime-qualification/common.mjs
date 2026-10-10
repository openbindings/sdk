import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import zlib from "node:zlib";
import os from "node:os";
export const hash = (data) =>
  crypto.createHash("sha256").update(data).digest("hex");
export async function fixtureSet(root) {
  const protocol = JSON.parse(
    await fs.readFile(new URL("./protocol.json", import.meta.url)),
  );
  const manifest = JSON.parse(
    await fs.readFile(path.join(root, "manifest.json")),
  );
  const fixtures = {};
  for (const [tier, row] of Object.entries(manifest.tiers)) {
    fixtures[tier] = {
      repetitions: row.repetitions,
      validOutcome: row.validOutcome,
      hotBatchRepetitions: protocol.hotBatchRepetitions[tier],
    };
    for (const [kind, identity] of Object.entries(row.files)) {
      const raw = await fs.readFile(path.join(root, identity.name));
      if (raw.length !== identity.bytes || hash(raw) !== identity.sha256)
        throw Error("fixture identity mismatch: " + identity.name);
      fixtures[tier][kind] = raw.toString();
    }
  }
  const adversarial = {};
  for (const [kind, row] of Object.entries(manifest.adversarial)) {
    const raw = await fs.readFile(path.join(root, row.name));
    if (raw.length !== row.bytes || hash(raw) !== row.sha256)
      throw Error("adversarial identity mismatch");
    adversarial[kind] = raw.toString();
  }
  return { fixtures, manifest, adversarial };
}
export async function identity(packageRoot) {
  const assets = {};
  for (const name of [
    "dist/index.js",
    "dist/internal.js",
    "dist/http-discovery.js",
    "dist/wasm/openbindings_wasm.js",
    "dist/wasm/openbindings_wasm_bg.wasm",
  ]) {
    const raw = await fs.readFile(path.join(packageRoot, name));
    const gzip9Bytes = zlib.gzipSync(raw, { level: 9 }).length;
    assets[name] = {
      sha256: hash(raw),
      rawBytes: raw.length,
      gzip9Bytes,
      brotli11Bytes: zlib.brotliCompressSync(raw).length,
      simulated10Mbps100msTransferMs: 100 + (gzip9Bytes * 8) / 10000,
    };
  }
  return {
    packageRoot: await fs.realpath(packageRoot),
    package: JSON.parse(
      await fs.readFile(path.join(packageRoot, "package.json")),
    ),
    assets,
  };
}
export const host = () => ({
  platform: os.platform(),
  release: os.release(),
  arch: os.arch(),
  cpus: os.cpus()[0]?.model,
  memoryBytes: os.totalmem(),
  node: process.version,
});
export async function save(file, data) {
  await fs.mkdir(path.dirname(file), { recursive: true });
  await fs.writeFile(file, JSON.stringify(data, null, 2) + "\n");
}
export function summaries(samples) {
  return Object.fromEntries(
    Object.entries(samples).map(([stage, values]) => {
      const sorted = [...values].sort((a, b) => a - b),
        medianMs = sorted[Math.floor(sorted.length / 2)];
      return [
        stage,
        {
          medianMs,
          minimumMs: sorted[0],
          maximumMs: sorted.at(-1),
          descriptiveP95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
          status:
            sorted[0] <= 0
              ? "inconclusive-resolution"
              : medianMs >= 1 && sorted.at(-1) / sorted[0] > 3
                ? "inconclusive-noise"
                : "measured",
        },
      ];
    }),
  );
}
