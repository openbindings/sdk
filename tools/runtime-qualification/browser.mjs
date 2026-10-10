import fs from "node:fs/promises";
import path from "node:path";
import http from "node:http";
import { pathToFileURL, fileURLToPath } from "node:url";
import { fixtureSet, identity, host, save, summaries } from "./common.mjs";
const [packageRoot, fixtureRoot, output, engine = "chromium", mode = "check"] =
  process.argv.slice(2);
if (!output || !["chromium", "webkit"].includes(engine))
  throw Error(
    "Usage: browser.mjs PACKAGE FIXTURES OUTPUT chromium|webkit check|measure; PLAYWRIGHT_MODULE must resolve playwright-core/index.mjs",
  );
const { [engine]: launcher } = await import(
  pathToFileURL(process.env.PLAYWRIGHT_MODULE)
);
const { fixtures, manifest, adversarial } = await fixtureSet(fixtureRoot);
const timed = mode === "measure";
const here = path.dirname(fileURLToPath(import.meta.url));
const launchOptions = {
  headless: true,
  ...(process.env.BROWSER_EXECUTABLE
    ? { executablePath: process.env.BROWSER_EXECUTABLE }
    : {}),
};
const files = new Map();
for (const name of [
  "index.js",
  "internal.js",
  "http-discovery.js",
  "wasm/openbindings_wasm.js",
  "wasm/openbindings_wasm_bg.wasm",
])
  files.set(
    "/dist/" + name,
    await fs.readFile(path.join(packageRoot, "dist", name)),
  );
for (const name of ["worker.mjs", "worker-owner.mjs", "worker-view.mjs"]) {
  const raw = await fs.readFile(
    path.join(packageRoot, "examples", name),
    "utf8",
  );
  // Resolve the delivered example's bare package import exactly as a bundler would.
  files.set(
    "/examples/" + name,
    raw.replaceAll('"@openbindings/sdk"', '"/dist/index.js"'),
  );
}
files.set(
  "/workloads.mjs",
  await fs.readFile(path.join(here, "workloads.mjs")),
);
files.set(
  "/qualification-worker.mjs",
  `import {runTier,lifetime,discovery,amplification} from '/workloads.mjs';
let sdk; self.onmessage=async({data})=>{try {
 if(data.kind==='initialize') {
  const t=data.timed?()=>performance.now():()=>0; const setup={}; let s=t();
  sdk=await import('/dist/index.js'); setup.moduleImport=t()-s;
  s=t(); const bytes=await(await fetch('/dist/wasm/openbindings_wasm_bg.wasm')).arrayBuffer();setup.loopbackAssetFetch=t()-s;
  s=t(); const module=await WebAssembly.compile(bytes);setup.compile=t()-s;
  s=t();await sdk.initialize(module);setup.instantiateAndInitialize=t()-s;
  setup.total=Object.values(setup).reduce((a,b)=>a+b,0);postMessage({setup});
 } else if(data.kind==='tier')postMessage(runTier(sdk,data.fixture,data.timed));
 else if(data.kind==='amplification')postMessage(amplification(sdk,data.fixture,data.timed));
 else if(data.kind==='lifetime')postMessage(lifetime(sdk,data.fixture,data.timed));
 else if(data.kind==='discovery')postMessage(await discovery(sdk,await import('/dist/http-discovery.js'),data.fixtures,data.timed,self.location.origin));
} catch(error){postMessage({error:String(error),stack:error.stack});}};`,
);
const server = http.createServer((req, res) => {
  if (req.url === "/") {
    res.setHeader("Content-Type", "text/html");
    res.end("<title>Runtime qualification</title>");
  } else if (req.url.startsWith("/document/")) {
    res.setHeader("Content-Type", "application/json");
    res.end(fixtures[req.url.split("/")[2]].document);
  } else if (files.has(req.url)) {
    res.setHeader(
      "Content-Type",
      req.url.endsWith(".wasm") ? "application/wasm" : "text/javascript",
    );
    res.end(files.get(req.url));
  } else {
    res.statusCode = 404;
    res.end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
let browser;
const watchdog = setTimeout(() => {
  console.error("browser campaign exceeded 120 seconds");
  browser?.close();
}, 120000);
watchdog.unref();
try {
  browser = await launcher.launch(launchOptions);
  const version = browser.version();
  const page = await browser.newPage();
  await page.goto(origin);
  if (!timed) {
    page.on("console", (message) => console.error(message.text()));
    page.on("pageerror", (error) => console.error(error));
  }
  const result = await page.evaluate(
    async ({ fixtures, adversarial, timed }) => {
      const worker = new Worker("/qualification-worker.mjs", {
        type: "module",
      });
      const ask = (data) =>
        new Promise((resolve, reject) => {
          worker.onerror = (e) => reject(Error(e.message));
          worker.onmessage = ({ data }) =>
            data.error
              ? reject(Error(data.stack ?? data.error))
              : resolve(data);
          worker.postMessage(data);
        });
      try {
        if (!timed) console.log("Worker initializing");
        const setup = await ask({ kind: "initialize", timed });
        if (!timed) console.log("Worker initialized");
        const gaps = [];
        let last = performance.now();
        const heartbeat = setInterval(() => {
          const now = performance.now();
          gaps.push(now - last);
          last = now;
        }, 10);
        const tiers = {};
        try {
          for (const [tier, fixture] of Object.entries(fixtures)) {
            if (!timed) console.log("Tier " + tier);
            tiers[tier] = await ask({ kind: "tier", fixture, timed });
          }
        } finally {
          clearInterval(heartbeat);
        }
        const lifetime = await ask({
          kind: "lifetime",
          fixture: fixtures.representative,
          timed,
        });
        if (!timed) console.log("Lifetime finished");
        const discovery = await ask({ kind: "discovery", fixtures, timed });
        const amplification = await ask({
          kind: "amplification",
          fixture: adversarial,
          timed,
        });
        if (!timed) console.log("Discovery finished");
        const { WorkerOwner } = await import("/examples/worker-owner.mjs");
        const { createWorkerView } = await import("/examples/worker-view.mjs");
        const encode = (s) => new TextEncoder().encode(s).buffer;
        const owner = new WorkerOwner();
        const editorRoundtrip = {};
        try {
          await owner.run({ kind: "initialize" });
          for (const [tier, fixture] of Object.entries(fixtures)) {
            const samples = [];
            for (let n = 0; n < (timed ? 9 : 1); n++) {
              const document = encode(fixture.document),
                value = encode(fixture.valid);
              const start = timed ? performance.now() : 0;
              const result = await owner.run({ document, value }, [
                document,
                value,
              ]);
              if (
                result.status !== "evaluated" ||
                result.result.outcome !== fixture.validOutcome
              )
                throw Error(JSON.stringify(result));
              if (n >= 2) samples.push(performance.now() - start);
            }
            editorRoundtrip[tier] = timed ? samples : null;
          }
        } finally {
          owner.dispose();
        }
        const rendered = [];
        const view = createWorkerView((row) => rendered.push(row));
        let stale;
        try {
          const older = view.check(
            encode(fixtures.near.document),
            encode(fixtures.near.valid),
          );
          const newer = view.check(
            encode(fixtures.small.document),
            encode(fixtures.small.valid),
          );
          const [old, current] = await Promise.all([older, newer]);
          if (old.accepted || !current.accepted || rendered.length !== 1)
            throw Error("stale Worker result rendered");
          const malformed = await view.check(encode("{"), encode("[1]"));
          if (malformed.result.status !== "input-error")
            throw Error("malformed edit must recover");
          stale = {
            olderAccepted: old.accepted,
            newerAccepted: current.accepted,
            malformed: malformed.result.status,
          };
        } finally {
          view.dispose();
        }
        return {
          setup,
          tiers,
          lifetime,
          discovery,
          amplification,
          editorRoundtrip,
          stale,
          heartbeat: timed
            ? {
                intervalMs: 10,
                gapsMs: gaps,
                scope:
                  "after initialization, during Worker tier workloads only",
              }
            : null,
          mainRealmHeap: performance.memory
            ? {
                usedJSHeapSize: performance.memory.usedJSHeapSize,
                totalJSHeapSize: performance.memory.totalJSHeapSize,
                scope: "main realm only, excludes Worker Wasm heap",
              }
            : null,
        };
      } finally {
        worker.terminate();
      }
    },
    { fixtures, adversarial, timed },
  );
  await browser.close();
  browser = undefined;
  const cold = [];
  if (timed)
    for (let n = 0; n < 7; n++) {
      browser = await launcher.launch(launchOptions);
      const coldPage = await browser.newPage();
      await coldPage.goto(origin);
      cold.push(
        await coldPage.evaluate(
          () =>
            new Promise((resolve, reject) => {
              const worker = new Worker("/qualification-worker.mjs", {
                type: "module",
              });
              worker.onerror = (e) => {
                worker.terminate();
                reject(Error(e.message));
              };
              worker.onmessage = ({ data }) => {
                worker.terminate();
                data.error ? reject(Error(data.error)) : resolve(data.setup);
              };
              worker.postMessage({ kind: "initialize", timed: true });
            }),
        ),
      );
      await browser.close();
      browser = undefined;
    }
  if (timed)
    for (const row of Object.values(result.tiers))
      row.summary = summaries(row.samplesMs);
  await save(output, {
    kind: "browser-worker",
    engine,
    version,
    host: host(),
    timed,
    package: await identity(packageRoot),
    fixtures: manifest,
    ...result,
    cold,
    coldScope:
      "fresh browser process and Worker per trial; loopback uncompressed transfer only",
    limitation:
      "Worker Wasm capacity and RSS unavailable through public facade; no device-constrained or mobile claim",
  });
  console.log(
    JSON.stringify({
      engine,
      mode,
      output,
      stale: result.stale,
      owners: result.lifetime.releasedOwners,
    }),
  );
} finally {
  clearTimeout(watchdog);
  if (browser) await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
