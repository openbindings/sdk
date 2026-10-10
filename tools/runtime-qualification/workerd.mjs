import fs from "node:fs/promises";
import path from "node:path";
import net from "node:net";
import { spawn, execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { fixtureSet, identity, host, save } from "./common.mjs";
const [packageRoot, fixtureRoot, output, mode = "check"] =
  process.argv.slice(2);
if (!output || !process.env.WORKERD_BIN || !process.env.WORKERD_CAPNP)
  throw Error(
    "Usage: workerd.mjs PACKAGE FIXTURES OUTPUT check|measure; set WORKERD_BIN and WORKERD_CAPNP",
  );
const timed = mode === "measure";
const { fixtures, manifest, adversarial } = await fixtureSet(fixtureRoot);
const runRoot = path.resolve(path.dirname(output), `workerd-${mode}`);
await fs.mkdir(runRoot, { recursive: true });
await fs.copyFile(
  new URL("./workloads.mjs", import.meta.url),
  path.join(runRoot, "workloads.mjs"),
);
await fs.writeFile(
  path.join(runRoot, "worker.mjs"),
  `import module from 'asset';
import * as sdk from 'sdk/index.js';
import {runTier,lifetime,prepare,parsed,describe,amplification} from './workloads.mjs';
let contract,valid,invalid;
export default {async fetch(request){
 if(new URL(request.url).pathname==='/health')return new Response('ready');
 try { await sdk.initialize(module); const job=await request.json();
 if(job.kind==='amplification')return Response.json(amplification(sdk,job.fixture,false));
 if(job.kind==='check')return Response.json({tier:runTier(sdk,job.fixture,false)});
 if(job.kind==='lifetime')return Response.json(lifetime(sdk,job.fixture,false));
 if(job.kind==='prepare'){contract?.dispose();valid?.dispose();invalid?.dispose();contract=prepare(sdk,job.fixture.document);valid=parsed(sdk,job.fixture.valid);invalid=parsed(sdk,job.fixture.invalid);return Response.json({ready:true,owners:sdk.liveStorageOwners()});}
 if(job.kind==='hot'||job.kind==='invalid'){let result;for(let n=0;n<job.repetitions;n++)result=contract.validate(job.kind==='hot'?valid:invalid);return Response.json(describe(result));}
 if(job.kind==='dispose'){contract?.dispose();valid?.dispose();invalid?.dispose();contract=valid=invalid=undefined;return Response.json({owners:sdk.liveStorageOwners()});}
 return Response.json({initialized:true,owners:sdk.liveStorageOwners()});
 }catch(error){return Response.json({error:String(error),stack:error.stack},{status:500});}
}};`,
);
const quote = (value) => JSON.stringify(path.resolve(value));
const modules = [
  `(name="worker",esModule=embed "worker.mjs")`,
  `(name="workloads.mjs",esModule=embed "workloads.mjs")`,
  `(name="asset",wasm=embed ${quote(path.join(packageRoot, "dist/wasm/openbindings_wasm_bg.wasm"))})`,
  ...["index.js", "internal.js", "wasm/openbindings_wasm.js"].map(
    (name) =>
      `(name=${JSON.stringify("sdk/" + name)},esModule=embed ${quote(path.join(packageRoot, "dist", name))})`,
  ),
];
await fs.writeFile(
  path.join(runRoot, "config.capnp"),
  `using Workerd = import ${quote(process.env.WORKERD_CAPNP)};
const config :Workerd.Config = (services=[(name="main",worker=(modules=[${modules.join(",")}],compatibilityDate="2026-10-08"))],sockets=[(name="http",address="127.0.0.1:8080",http=(),service="main")]);`,
);
let child,
  log = "";
async function stop() {
  if (child && child.exitCode !== null) {
    child = undefined;
    return;
  }
  if (child) {
    const current = child;
    child = undefined;
    const closed = new Promise((resolve) => current.once("exit", resolve));
    current.kill("SIGTERM");
    await closed;
  }
}
async function start() {
  const probe = net.createServer();
  await new Promise((resolve) => probe.listen(0, "127.0.0.1", resolve));
  const port = probe.address().port;
  await new Promise((resolve) => probe.close(resolve));
  child = spawn(
    process.env.WORKERD_BIN,
    ["serve", "config.capnp", "-I/", "--socket-addr", `http=127.0.0.1:${port}`],
    { cwd: runRoot },
  );
  child.stdout.on("data", (bytes) => (log += bytes));
  child.stderr.on("data", (bytes) => (log += bytes));
  const origin = `http://127.0.0.1:${port}`;
  for (let n = 0; n < 100; n++) {
    if (child.exitCode !== null) throw Error("workerd exited: " + log);
    try {
      if (
        (await fetch(origin + "/health", { signal: AbortSignal.timeout(1000) }))
          .ok
      )
        return origin;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw Error("workerd readiness timeout: " + log);
}
async function call(origin, job) {
  const response = await fetch(origin, {
    method: "POST",
    body: JSON.stringify(job),
    signal: AbortSignal.timeout(120000),
  });
  const data = await response.json();
  if (!response.ok || data.error) throw Error(JSON.stringify(data));
  return data;
}
try {
  const origin = await start();
  const tiers = {};
  for (const [tier, fixture] of Object.entries(fixtures)) {
    const checked = await call(origin, { kind: "check", fixture });
    await call(origin, { kind: "prepare", fixture });
    const row = {
      observation: checked.tier.observation,
      samplesMs: { hot: [], invalidAndSerialize: [] },
      concurrency: 1,
      measurement:
        "external loopback HTTP per-call batch average; includes request overhead; workerd internal CPU clock is not used",
    };
    for (const [kind, metric] of [
      ["hot", "hot"],
      ["invalid", "invalidAndSerialize"],
    ]) {
      const repetitions = kind === "hot" ? fixture.repetitions : 1;
      for (let n = 0; n < (timed ? 9 : 1); n++) {
        const before = timed ? performance.now() : 0;
        const result = await call(origin, { kind, repetitions });
        const ms = timed ? (performance.now() - before) / repetitions : 0;
        if (
          result.outcome !== (kind === "hot" ? fixture.validOutcome : "fails")
        )
          throw Error(JSON.stringify(result));
        if (n >= 2) row.samplesMs[metric].push(ms);
        row[kind + "Output"] = result;
      }
    }
    row.released = await call(origin, { kind: "dispose" });
    tiers[tier] = row;
  }
  const witness = {
    samplesMs: [],
    observation: null,
    scope:
      "external HTTP full prepare/validate/report serialization/disposal; includes loopback and source parsing",
  };
  for (let n = 0; n < (timed ? 9 : 1); n++) {
    const start = timed ? performance.now() : 0;
    const row = await call(origin, {
      kind: "amplification",
      fixture: adversarial,
    });
    if (n >= 2) witness.samplesMs.push(performance.now() - start);
    witness.observation = row.observation;
  }
  const retained = await call(origin, {
    kind: "lifetime",
    fixture: fixtures.representative,
  });
  await stop();
  const cold = [];
  if (timed)
    for (let n = 0; n < 7; n++) {
      const started = performance.now();
      const origin = await start();
      const ready = performance.now();
      const result = await call(origin, { kind: "initialize" });
      cold.push({
        processStartToFirstRequestMs: performance.now() - started,
        readinessMs: ready - started,
        firstRequestMs: performance.now() - ready,
        result,
      });
      await stop();
    }
  await save(output, {
    kind: "workerd",
    timed,
    host: host(),
    version: execFileSync(process.env.WORKERD_BIN, ["--version"], {
      encoding: "utf8",
    }).trim(),
    package: await identity(packageRoot),
    fixtures: manifest,
    tiers,
    lifetime: retained,
    amplification: witness,
    cold,
    limitation:
      "local workerd only; no deployed Cloudflare latency/quota evidence; no Node compatibility flags; native Wasm module import compiled by host before request; process startup includes50ms readiness polling",
  });
  console.log(
    JSON.stringify({
      kind: "workerd",
      mode,
      output,
      owners: retained.releasedOwners,
    }),
  );
} finally {
  await stop();
  await fs.writeFile(path.join(runRoot, "host.log"), log);
}
