import assert from "node:assert/strict";
import http from "node:http";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { DiscoveryPublication } from "../dist/http-discovery.js";

// HD-4/HD-9: real cross-origin Chromium/WebKit CORS and automatically followed redirects.
export async function discoveryPage(browser, pageOrigin) {
  await sdk.initialize(
    await readFile(
      new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
    ),
  );
  const parsed = sdk.parseDocument(
    '{"openbindings":"0.2.0","operations":{}}',
  ).value;
  const proof = parsed.validate().document;
  const publication = new DiscoveryPublication(proof);
  let targetOrigin;
  const target = http.createServer(async (request, response) => {
    const published = publication.respond(
      new Request(targetOrigin + request.url),
    );
    response.writeHead(published.status, Object.fromEntries(published.headers));
    response.end(Buffer.from(await published.arrayBuffer()));
  });
  const redirect = http.createServer((request, response) => {
    response.writeHead(302, {
      Location: targetOrigin + "/.well-known/openbindings",
      "Access-Control-Allow-Origin": "*",
    });
    response.end();
  });
  const slowHeaders = http.createServer(() => {});
  const slowBody = http.createServer((request, response) => {
    response.writeHead(200, {
      "Access-Control-Allow-Origin": "*",
      "Content-Type": "application/json",
    });
    response.flushHeaders();
    response.write("{");
  });
  const servers = [target, redirect, slowHeaders, slowBody];
  let page;
  const listen = async (server) => {
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
    return `http://127.0.0.1:${server.address().port}`;
  };
  try {
    targetOrigin = await listen(target);
    const origins = [
      targetOrigin,
      await listen(redirect),
      await listen(slowHeaders),
      await listen(slowBody),
    ];
    page = await browser.newPage();
    await page.goto(pageOrigin);
    const results = await page.evaluate(async (origins) => {
      const sdk = await import("/dist/index.js");
      const discovery = await import("/dist/http-discovery.js");
      await sdk.initialize();
      const results = [];
      for (const origin of origins) {
        const result = await discovery.discover(origin, { timeoutMs: 500 });
        results.push({ status: result.status, metadata: result.metadata });
        result.document?.dispose();
      }
      return results;
    }, origins);
    assert.deepEqual(
      results.map((r) => r.status),
      ["found", "found", "timeout", "timeout"],
    );
    assert.equal(results[0].metadata.responseType, "cors");
    assert.equal(
      results[1].metadata.requestedUrl,
      origins[1] + "/.well-known/openbindings",
    );
    assert.equal(
      results[1].metadata.finalUrl,
      targetOrigin + "/.well-known/openbindings",
    );
    assert.equal(results[2].metadata, undefined);
    assert.equal(results[3].metadata.status, 200);
    return results;
  } finally {
    await page?.close();
    for (const server of servers) {
      server.closeAllConnections();
      if (server.listening)
        await new Promise((resolve) => server.close(resolve));
    }
    publication.dispose();
    proof.dispose();
    parsed.dispose();
  }
}
