import fs from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { chromium, webkit } from "playwright-core";
import { firstUsePage, firstUseLoadingFailures } from "./first-use-browser.mjs";
import { editorPage } from "./editor-browser.mjs";
const packageRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const root = path.resolve(packageRoot, "../..");
const engine = process.argv[2] ?? "chromium";
const id = crypto.randomUUID(),
  output = path.join(root, "tools/qualification/hosts", engine);
await fs.mkdir(output, { recursive: true });
const requests = JSON.parse(
  await fs.readFile(
    path.join(root, "tools/qualification/corpus/browser-requests.json"),
    "utf8",
  ),
);
const files = new Map();
for (const name of [
  "index.js",
  "internal.js",
  "http-discovery.js",
  "wasm/openbindings_wasm.js",
  "wasm/openbindings_wasm_bg.wasm",
]) {
  const bytes = await fs.readFile(path.join(packageRoot, "dist", name));
  files.set("/dist/" + name, bytes); // match the installed package layout
}
for (const name of [
  "first-use.html",
  "first-use.mjs",
  "editor.html",
  "editor.mjs",
  "editor-worker.mjs",
  "worker-owner.mjs",
  "worker-view.mjs",
])
  files.set(
    "/examples/" + name,
    await fs.readFile(path.join(packageRoot, "examples", name)),
  );
files.set(
  "/fixed-diagnostic-cases.mjs",
  await fs.readFile(path.join(packageRoot, "test/fixed-diagnostic-cases.mjs")),
);
files.set(
  "/diagnostic-budget-cases.mjs",
  await fs.readFile(path.join(packageRoot, "test/diagnostic-budget-cases.mjs")),
);
files.set(
  "/observer.mjs",
  await fs.readFile(
    path.join(root, "tools/qualification/replay/sdk-observe.mjs"),
  ),
);
files.set(
  "/diagnostic-budget-worker.mjs",
  Buffer.from(`
import * as sdk from "/dist/index.js";
import { diagnosticBudgetCases } from "/diagnostic-budget-cases.mjs";
try { await sdk.initialize(); self.postMessage(diagnosticBudgetCases(sdk)); }
catch (error) { self.postMessage({ error: String(error) }); }
`),
);
const server = http.createServer((req, res) => {
  if (req.url === "/") {
    res.setHeader("Content-Type", "text/html");
    res.end(`<title>${id}</title>`);
  } else if (files.has(req.url)) {
    res.setHeader(
      "Content-Type",
      req.url.endsWith(".wasm")
        ? "application/wasm"
        : req.url.endsWith(".html")
          ? "text/html"
          : "text/javascript",
    );
    res.end(files.get(req.url));
  } else {
    res.statusCode = 404;
    res.end();
  }
});
let browser;
try {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  browser = await { chromium, webkit }[engine].launch({
    headless: true,
    ...(engine === "chromium" && process.env.CHROMIUM_EXECUTABLE
      ? { executablePath: process.env.CHROMIUM_EXECUTABLE }
      : {}),
  });
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  if ((await page.title()) !== id) throw Error("fresh host identity");
  const result = await page.evaluate(async (requests) => {
    const sdk = await import("/dist/index.js"),
      { observe } = await import("/observer.mjs"),
      { fixedDiagnosticCases } = await import("/fixed-diagnostic-cases.mjs"),
      { diagnosticBudgetCases } = await import("/diagnostic-budget-cases.mjs");
    await sdk.initialize();
    const run = (list) =>
      list.map((request) => {
        try {
          return observe(sdk, request);
        } catch (error) {
          return { id: request.id, executed: false, error: String(error) };
        }
      });
    return {
      core: run(requests.core),
      suite: run(requests.suite),
      fixedDiagnostics: fixedDiagnosticCases(sdk),
      ...diagnosticBudgetCases(sdk),
    };
  }, requests);
  result.diagnosticBudgetWorker = await page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const worker = new Worker("/diagnostic-budget-worker.mjs", {
          type: "module",
        });
        worker.onmessage = ({ data }) => {
          worker.terminate();
          if (data.error) reject(new Error(data.error));
          else resolve(data);
        };
        worker.onerror = (error) => {
          worker.terminate();
          reject(new Error(error.message));
        };
      }),
  );
  const exampleUrl = `http://127.0.0.1:${server.address().port}/examples/first-use.html`;
  const firstUse = await firstUsePage(browser, exampleUrl);
  const loadingFailures = await firstUseLoadingFailures(browser, exampleUrl);
  const editor = await editorPage(
    browser,
    new URL("./editor.html", exampleUrl).href,
  );
  await fs.writeFile(
    path.join(output, "editor.json"),
    JSON.stringify(editor, null, 2),
  );
  await fs.writeFile(
    path.join(output, "first-use.json"),
    JSON.stringify(
      {
        ...firstUse,
        fixedDiagnostics: result.fixedDiagnostics,
        diagnosticBudget: result.diagnosticBudget,
        diagnosticBudgetWorker: result.diagnosticBudgetWorker,
        loadingFailures,
      },
      null,
      2,
    ),
  );
  await fs.writeFile(
    path.join(output, "core.json"),
    JSON.stringify(result.core, null, 2),
  );
  await fs.writeFile(
    path.join(output, "evaluator.json"),
    JSON.stringify(result.suite, null, 2),
  );
  await fs.writeFile(
    path.join(output, "host.json"),
    JSON.stringify(
      {
        engine,
        browserVersion: browser.version(),
        assets: Object.fromEntries(
          [...files].map(([name, bytes]) => [
            name,
            crypto.createHash("sha256").update(bytes).digest("hex"),
          ]),
        ),
      },
      null,
      2,
    ),
  );
  console.log(
    `${engine}: observed ${result.core.length} core cases and ${result.suite.length} evaluator groups; run the independent judge next.`,
  );
  if ([...result.core, ...result.suite].some((row) => !row.executed))
    process.exitCode = 1;
} finally {
  if (browser) await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
