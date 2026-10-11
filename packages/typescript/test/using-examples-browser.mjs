import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import ts from "typescript";
import {
  browserBootstrapSnippet,
  firstUseSnippet,
} from "../scripts/readme-snippet.mjs";

export async function usingExampleFiles(packageRoot, files) {
  const readme = await fs.readFile(path.join(packageRoot, "README.md"), "utf8");
  files.set(
    "/using/bootstrap.mjs",
    Buffer.from(browserBootstrapSnippet(readme)),
  );
  for (const [name, source] of [
    ["first-use", firstUseSnippet(readme)],
    [
      "inspect-edit",
      await fs.readFile(
        path.join(packageRoot, "examples/inspect-edit.ts"),
        "utf8",
      ),
    ],
  ]) {
    // test:package also typechecks these exact sources against installed types.
    // Here the browser executes their unchanged ES2022 compilation.
    const compiled = ts.transpileModule(source, {
      fileName: name + ".ts",
      compilerOptions: {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.ES2022,
      },
      reportDiagnostics: true,
    });
    assert.deepEqual(compiled.diagnostics, [], `${name} must compile`);
    files.set(`/using/${name}.js`, Buffer.from(compiled.outputText));
  }
  files.set(
    "/using/app.js",
    await fs.readFile(path.join(packageRoot, "test/using-examples-app.mjs")),
  );
  files.set(
    "/using/index.html",
    Buffer.from(`<!doctype html><title>Compiled using examples</title>
<script type="importmap">{"imports":{"@openbindings/sdk":"/dist/index.js"}}</script>
<script type="module">
globalThis.originalDispose = Symbol.dispose;
try { await import("./bootstrap.mjs"); }
catch (error) { globalThis.usingError = String(error.stack ?? error); }
</script>`),
  );
}

export async function usingExamplesPage(browser, url) {
  // A fresh page is essential: Symbol.dispose must exist before SDK evaluation.
  const page = await browser.newPage();
  try {
    await page.goto(url);
    await page.waitForFunction(
      () => globalThis.usingResult || globalThis.usingError,
    );
    const result = await page.evaluate(() => ({
      result: globalThis.usingResult,
      error: globalThis.usingError,
    }));
    assert.equal(result.error, undefined, result.error);
    assert.equal(result.result.status, "passed");
    return result.result;
  } finally {
    await page.close();
  }
}
