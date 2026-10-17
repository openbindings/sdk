// Real Node Fetch exposes redirect responses, so discovery checks every hop itself.
import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import http from "node:http";
import https from "node:https";
import tls from "node:tls";
import os from "node:os";
import path from "node:path";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as discovery from "../dist/http-discovery.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const text = '{"openbindings":"0.2.0","operations":{}}';
const secret = "redirect-test-secret";

// One loopback server that records every request and answers through `route`.
async function serve(route, options) {
  const requests = [];
  const handler = (request, response) => {
    requests.push({ url: request.url, headers: request.headers });
    const [status, headers = {}, body = ""] = route(request.url);
    response.writeHead(status, headers);
    response.end(body);
  };
  const server = options
    ? https.createServer(options, handler)
    : http.createServer(handler);
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const scheme = options ? "https" : "http";
  return {
    origin: `${scheme}://127.0.0.1:${server.address().port}`,
    requests,
    async close() {
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}
const found = () => [200, { "content-type": "application/json" }, text];
const moved = (location, status = 302) => [status, { location }];

// A private CA trusted only by this test process; no OS trust changes.
async function privateTls() {
  const dir = await mkdtemp(path.join(os.tmpdir(), "obi-redirects-"));
  const openssl = (...args) =>
    execFileSync("openssl", args, { cwd: dir, stdio: "pipe" });
  const curve = ["-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1"];
  openssl(
    "req",
    "-x509",
    ...curve,
    "-nodes",
    "-days",
    "1",
    "-subj",
    "/CN=OpenBindings private test CA",
    "-keyout",
    "ca.key",
    "-out",
    "ca.pem",
    "-addext",
    "basicConstraints=critical,CA:TRUE",
  );
  openssl(
    "req",
    ...curve,
    "-nodes",
    "-subj",
    "/CN=127.0.0.1",
    "-keyout",
    "server.key",
    "-out",
    "server.csr",
  );
  await writeFile(
    path.join(dir, "extensions"),
    "subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n",
  );
  openssl(
    "x509",
    "-req",
    "-in",
    "server.csr",
    "-CA",
    "ca.pem",
    "-CAkey",
    "ca.key",
    "-CAcreateserial",
    "-days",
    "1",
    "-extfile",
    "extensions",
    "-out",
    "server.pem",
  );
  const [ca, key, cert] = await Promise.all(
    ["ca.pem", "server.key", "server.pem"].map((name) =>
      readFile(path.join(dir, name), "utf8"),
    ),
  );
  await rm(dir, { recursive: true, force: true });
  const previous = tls.getCACertificates("default");
  tls.setDefaultCACertificates([...previous, ca]);
  return {
    server: { key, cert },
    restore: () => tls.setDefaultCACertificates(previous),
  };
}

// The caller's transport adds credentials to every request it is handed.
const withCredentials = (url, init) =>
  fetch(url, {
    ...init,
    headers: {
      ...init.headers,
      authorization: `Bearer ${secret}`,
      "x-api-key": secret,
    },
  });

test("Node refuses an HTTPS to plaintext to HTTPS chain before the plaintext hop", async () => {
  const trust = await privateTls();
  let secure, plain;
  try {
    plain = await serve(() => moved(`${secure.origin}/done`));
    secure = await serve(
      (url) => (url === "/done" ? found() : moved(`${plain.origin}/hop`)),
      trust.server,
    );
    const result = await discovery.discover(secure.origin, {
      timeoutMs: 5000,
      fetch: withCredentials,
    });
    assert.equal(result.status, "transport-error");
    assert.equal(result.error.kind, "redirect-policy");
    assert.equal(result.metadata.status, 302);
    assert.equal(
      result.metadata.finalUrl,
      `${secure.origin}/.well-known/openbindings`,
    );
    // The plaintext origin never received a request, so no caller header reached it.
    assert.deepEqual(plain.requests, []);
    assert.deepEqual(
      secure.requests.map((request) => request.url),
      ["/.well-known/openbindings"],
    );
    // The trusted chain itself is reachable: an HTTP loopback hop may upgrade to HTTPS.
    const upgraded = await serve(() => moved(`${secure.origin}/done`, 307));
    try {
      const result = await discovery.discover(upgraded.origin, {
        timeoutMs: 5000,
      });
      assert.equal(result.status, "found");
      assert.equal(result.metadata.finalUrl, `${secure.origin}/done`);
      result.document.dispose();
    } finally {
      await upgraded.close();
    }
  } finally {
    trust.restore();
    await secure?.close();
    await plain?.close();
  }
});

test("Node follows at most ten redirects and never dispatches the next hop", async () => {
  let length = 0;
  const chain = await serve((url) => {
    const hop = Number(url.split("/hop/")[1] ?? 0);
    return hop < length ? moved(`/hop/${hop + 1}`) : found();
  });
  try {
    for (const hops of [10, 11]) {
      length = hops;
      chain.requests.length = 0;
      const result = await discovery.discover(chain.origin, {
        timeoutMs: 5000,
      });
      // The initial request plus ten followed redirects, in either case.
      assert.equal(chain.requests.length, 11);
      if (hops === 10) {
        assert.equal(result.status, "found");
        assert.equal(result.metadata.finalUrl, `${chain.origin}/hop/10`);
        result.document.dispose();
      } else {
        assert.equal(result.status, "transport-error");
        assert.equal(result.error.kind, "redirect-policy");
        assert.equal(result.metadata.finalUrl, `${chain.origin}/hop/10`);
      }
    }
  } finally {
    await chain.close();
  }
});

test("each admitted hop is a fresh request, so origin-scoped credentials stay home", async () => {
  let home, foreign;
  try {
    foreign = await serve(found);
    home = await serve(() => moved(`${foreign.origin}/moved`, 308));
    const calls = [];
    const result = await discovery.discover(home.origin, {
      timeoutMs: 5000,
      fetch: (url, init) => {
        calls.push({ url, headers: init.headers, redirect: init.redirect });
        return new URL(url).origin === home.origin
          ? withCredentials(url, init)
          : fetch(url, init);
      },
    });
    assert.equal(result.status, "found");
    assert.equal(result.metadata.finalUrl, `${foreign.origin}/moved`);
    result.document.dispose();
    // The SDK carries nothing from one hop to the next: only its own Accept header.
    assert.deepEqual(
      calls.map((call) => [call.url, call.redirect, Object.keys(call.headers)]),
      [
        [`${home.origin}/.well-known/openbindings`, "manual", ["Accept"]],
        [`${foreign.origin}/moved`, "manual", ["Accept"]],
      ],
    );
    assert.equal(home.requests[0].headers["x-api-key"], secret);
    assert.equal(foreign.requests[0].headers["x-api-key"], undefined);
    assert.equal(foreign.requests[0].headers.authorization, undefined);
  } finally {
    await home?.close();
    await foreign?.close();
  }
});
