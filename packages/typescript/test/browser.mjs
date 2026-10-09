import fs from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import crypto from "node:crypto";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { chromium, webkit } from "playwright-core";
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
  files.set("/" + name, bytes);
  files.set("/dist/" + name, bytes); // installed example's unchanged relative URLs
}
for (const name of ["first-use.html", "first-use.mjs"])
  files.set(
    "/examples/" + name,
    await fs.readFile(path.join(packageRoot, "examples", name)),
  );
files.set(
  "/fixed-diagnostic-cases.mjs",
  await fs.readFile(path.join(packageRoot, "test/fixed-diagnostic-cases.mjs")),
);
files.set(
  "/observer.mjs",
  await fs.readFile(
    path.join(root, "tools/qualification/replay/sdk-observe.mjs"),
  ),
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
    const sdk = await import("/index.js"),
      { observe } = await import("/observer.mjs"),
      { fixedDiagnosticCases } = await import("/fixed-diagnostic-cases.mjs");
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
    };
  }, requests);
  const examplePage = await browser.newPage();
  const exampleErrors = [];
  examplePage.on("pageerror", (error) => exampleErrors.push(error.message));
  examplePage.on("requestfailed", (request) =>
    exampleErrors.push(request.url()),
  );
  examplePage.on("response", (response) => {
    if (response.status() >= 400)
      exampleErrors.push(`${response.status()} ${response.url()}`);
  });
  await examplePage.goto(
    `http://127.0.0.1:${server.address().port}/examples/first-use.html`,
  );
  await examplePage.waitForFunction(() => {
    const output = document.querySelector("#result");
    return output && output.textContent !== "Loading…";
  });
  assert.deepEqual(
    exampleErrors,
    [],
    "First-use example must load without errors",
  );
  const firstUse = JSON.parse(
    await examplePage.locator("#result").textContent(),
  );
  assert.equal(firstUse.accepted.result.outcome, "satisfies");
  assert.equal(firstUse.accepted.operations[0].key, "lookup");
  assert.equal(firstUse.mismatch.result.outcome, "mismatch");
  assert.equal(firstUse.invalidInput.result.outcome, "input-error");
  await fs.writeFile(
    path.join(output, "first-use.json"),
    JSON.stringify(
      {
        firstUse,
        fixedDiagnostics: result.fixedDiagnostics,
        errors: exampleErrors,
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
