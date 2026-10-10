// Correctness-only installed-package Worker/editor qualification (no timings).
import fs from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import assert from "node:assert/strict";
import { editorPage } from "./editor-browser.mjs";
const root = path.resolve(
  process.env.SDK_PACKAGE_ROOT ??
    fileURLToPath(new URL("../", import.meta.url)),
);
const engine = process.argv[2] ?? "chromium";
const playwright = await import(
  process.env.PLAYWRIGHT_MODULE
    ? pathToFileURL(process.env.PLAYWRIGHT_MODULE).href
    : "playwright-core"
);
const detailCases = await fs.readFile(
  new URL("./diagnostic-details-cases.mjs", import.meta.url),
);
const server = http.createServer(async (req, res) => {
  try {
    const name = new URL(req.url, "http://localhost").pathname;
    if (name === "/") {
      res.setHeader("Content-Type", "text/html");
      res.end("<!doctype html><title>SDK editor controls</title>");
      return;
    }
    if (name === "/diagnostic-details-cases.mjs") {
      res.setHeader("Content-Type", "text/javascript");
      res.end(detailCases);
      return;
    }
    if (name === "/details-worker.mjs") {
      res.setHeader("Content-Type", "text/javascript");
      res.end(
        `import * as sdk from '/dist/index.js'; import { diagnosticDetailsCases } from '/diagnostic-details-cases.mjs'; try { await sdk.initialize(); postMessage(diagnosticDetailsCases(sdk)); } catch(e) { postMessage({error:String(e),stack:e.stack}); }`,
      );
      return;
    }
    const target = path.resolve(root, "." + name);
    if (
      !target.startsWith(root + path.sep) ||
      !/^\/(dist|examples)\//.test(name)
    ) {
      res.writeHead(404);
      res.end();
      return;
    }
    const bytes = await fs.readFile(target);
    res.setHeader(
      "Content-Type",
      name.endsWith(".wasm")
        ? "application/wasm"
        : name.endsWith(".html")
          ? "text/html"
          : "text/javascript",
    );
    res.end(bytes);
  } catch {
    res.writeHead(404);
    res.end();
  }
});
let browser;
try {
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  browser = await playwright[engine].launch({ headless: true });
  const editor = await editorPage(browser, base + "/examples/editor.html");
  const page = await browser.newPage();
  await page.goto(base);
  const details = await page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const worker = new Worker("/details-worker.mjs", { type: "module" });
        worker.onmessage = ({ data }) => {
          worker.terminate();
          resolve(data);
        };
        worker.onerror = (e) => {
          worker.terminate();
          reject(new Error(e.message));
        };
      }),
  );
  assert(!details.error, details.stack ?? details.error);
  await page.close();
  console.log(
    JSON.stringify(
      {
        engine,
        version: browser.version(),
        packageRoot: root,
        ...editor,
        ...details,
      },
      null,
      2,
    ),
  );
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
