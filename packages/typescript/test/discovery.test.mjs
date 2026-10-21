import * as testing from "../dist/testing.js";
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as discovery from "../dist/http-discovery.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const text = '{"openbindings":"0.2.0","operations":{}}';
function responseUrl(response, url, type = "default") {
  Object.defineProperties(response, {
    url: { value: url },
    type: { value: type },
  });
  return response;
}

// admission and requested metadata use the normalized client URL.
test("normalized literal loopback admission and manual redirect dispatch", async () => {
  for (const host of ["2130706433", "0x7f.1", "%31%32%37.0.0.1", "127.1"]) {
    const result = await discovery.discover(`http://${host}`, {
      fetch: async (url, init) => {
        assert.equal(url, "http://127.0.0.1/.well-known/openbindings");
        assert.equal(init.redirect, "manual");
        return responseUrl(new Response(text), url);
      },
    });
    assert.equal(result.status, "found");
    assert.equal(
      result.metadata.requestedUrl,
      "http://127.0.0.1/.well-known/openbindings",
    );
    assert.equal(
      result.metadata.finalUrl,
      "http://127.0.0.1/.well-known/openbindings",
    );
    result.document.dispose();
  }
  for (const host of [
    "localhost",
    "a.localhost",
    "127.attacker.test",
    "128.0.0.1",
    "[::ffff:127.0.0.1]",
  ]) {
    await assert.rejects(
      discovery.discover(`http://${host}`, {
        fetch: async () => {
          throw Error("dispatched");
        },
      }),
      { code: "origin-policy" },
    );
  }
});

// A transport answering by URL; each call is recorded with its redirect mode and headers.
function transport(routes) {
  const calls = [];
  const fetch = async (url, init) => {
    calls.push({ url, redirect: init.redirect, headers: init.headers });
    const route = routes[url];
    if (!route) throw Error("unexpected dispatch " + url);
    return route(init);
  };
  return { calls, fetch };
}
const redirectTo = (location, status = 302) =>
  new Response(null, { status, headers: { location } });
// What a browser returns for a redirect it hides under redirect: "manual".
const hidden = () =>
  responseUrl(new Response(null, { status: 200 }), "", "opaqueredirect");

// a host that hides redirects keeps the final-URL check it is limited to.
test("a host hiding redirects is followed by the host and its final URL checked", async () => {
  const { calls, fetch } = transport({
    "https://example.test/.well-known/openbindings": (init) =>
      init.redirect === "manual"
        ? hidden()
        : responseUrl(new Response(text), "https://moved.test/final"),
  });
  const result = await discovery.discover("https://example.test", { fetch });
  assert.equal(result.status, "found");
  assert.equal(result.metadata.finalUrl, "https://moved.test/final");
  assert.deepEqual(
    calls.map((call) => call.redirect),
    ["manual", "follow"],
  );
  result.document.dispose();
});

// one observed opaqueredirect makes later discoveries through that transport follow directly.
test("a transport seen hiding redirects is followed directly afterwards, others keep hop checks", async () => {
  const modes = [];
  let final = "https://moved.test/final";
  const hiding = async (url, init) => {
    modes.push(init.redirect);
    return init.redirect === "manual"
      ? hidden()
      : responseUrl(new Response(text), final);
  };
  for (const expected of [["manual", "follow"], ["follow"]]) {
    modes.length = 0;
    const result = await discovery.discover("https://example.test", {
      fetch: hiding,
    });
    assert.equal(result.status, "found");
    assert.deepEqual(modes, expected);
    result.document.dispose();
  }
  // Following directly keeps the final-URL checks.
  modes.length = 0;
  final = "http://127.0.0.1/final";
  const downgraded = await discovery.discover("https://example.test", {
    fetch: hiding,
  });
  assert.deepEqual(modes, ["follow"]);
  assert.equal(downgraded.status, "transport-error");
  assert.equal(downgraded.error.kind, "redirect-policy");
  // Another transport that exposes hops is not affected by that observation.
  const { calls, fetch } = transport({
    "https://example.test/.well-known/openbindings": () =>
      redirectTo("http://127.0.0.1/downgrade"),
  });
  const refused = await discovery.discover("https://example.test", { fetch });
  assert.equal(refused.status, "transport-error");
  assert.equal(refused.error.kind, "redirect-policy");
  assert.deepEqual(
    calls.map((call) => call.redirect),
    ["manual"],
  );
});

// final downgrade and opaque visibility are acquisition policy failures.
test("final HTTPS downgrade and opaque responses cannot establish policy", async () => {
  for (const type of ["default", "opaque", "opaqueredirect"]) {
    let cancelled = false;
    const response = responseUrl(
      new Response(
        new ReadableStream({
          cancel() {
            cancelled = true;
          },
        }),
      ),
      "http://127.0.0.1/final",
      type,
    );
    const modes = [];
    const result = await discovery.discover("https://example.test", {
      // The host hides the first redirect, then follows to the final response.
      fetch: async (url, init) => {
        modes.push(init.redirect);
        return init.redirect === "manual" ? hidden() : response;
      },
    });
    assert.deepEqual(modes, ["manual", "follow"]);
    assert.equal(result.status, "transport-error");
    assert.equal(result.error.kind, "redirect-policy");
    assert.equal(result.body, undefined);
    assert.equal(
      result.metadata.finalUrl,
      type === "default" ? response.url : null,
    );
    assert.ok(cancelled);
  }
});

// A following host follows the five redirect statuses, so a visible Location was left unfollowed.
test("unfollowed visible redirect is a redirect-policy refusal", async () => {
  for (const status of [301, 302, 303, 307, 308]) {
    let cancelled = false;
    const result = await discovery.discover("https://example.test", {
      fetch: async (url, init) =>
        init.redirect === "manual"
          ? hidden()
          : new Response(
              new ReadableStream({
                cancel() {
                  cancelled = true;
                },
              }),
              { status, headers: { location: "https://example.test/moved" } },
            ),
    });
    assert.equal(result.status, "transport-error");
    assert.equal(result.error.kind, "redirect-policy");
    assert.equal(result.metadata.status, status);
    assert.equal(result.body, undefined);
    assert.ok(cancelled);
  }
  // Nothing to follow: no Location, or a status outside the five.
  for (const [status, headers] of [
    [302, {}],
    [304, {}],
    [300, { location: "https://example.test/moved" }],
  ]) {
    const result = await discovery.discover("https://example.test", {
      fetch: async () => new Response(null, { status, headers }),
    });
    assert.equal(result.status, "http-status");
    assert.equal(result.metadata.status, status);
  }
});

// visible redirects are checked hops: fresh requests to resolved, admitted URLs.
test("checked hops follow the five redirect statuses with relative Locations", async () => {
  for (const status of [301, 302, 303, 307, 308]) {
    let released = false;
    const { calls, fetch } = transport({
      "https://example.test/.well-known/openbindings": () =>
        new Response(
          new ReadableStream({
            cancel() {
              released = true;
            },
          }),
          { status, headers: { location: "../a/b?x#fragment" } },
        ),
      "https://example.test/a/b?x": () => redirectTo("//other.test/c", status),
      "https://other.test/c": () => new Response(text),
    });
    const result = await discovery.discover("https://example.test", { fetch });
    assert.equal(result.status, "found");
    assert.equal(result.metadata.finalUrl, "https://other.test/c");
    assert.equal(result.metadata.status, 200);
    assert.ok(released, "redirect body is released before the next hop");
    for (const call of calls) {
      assert.equal(call.redirect, "manual");
      assert.deepEqual(Object.keys(call.headers), ["Accept"]);
    }
    assert.equal(calls.length, 3);
    result.document.dispose();
  }
  // HTTP literal loopback may move to other loopback spellings and upgrade to HTTPS.
  const { calls, fetch } = transport({
    "http://127.0.0.1/.well-known/openbindings": () =>
      redirectTo("http://[0:0:0:0:0:0:0:1]:8080/x"),
    "http://[::1]:8080/x": () => redirectTo("https://example.test/y"),
    "https://example.test/y": () => new Response(text),
  });
  const upgraded = await discovery.discover("http://127.0.0.1", { fetch });
  assert.equal(upgraded.status, "found");
  assert.equal(calls.length, 3);
  upgraded.document.dispose();
});

// every refused Location is a policy refusal before anything is dispatched to it.
test("refused redirect destinations are never dispatched", async () => {
  for (const [origin, location] of [
    ["https://example.test", "http://127.0.0.1/downgrade"],
    ["https://example.test", "http://[::1]/downgrade"],
    ["http://127.0.0.1", "http://localhost/x"],
    ["http://127.0.0.1", "http://10.0.0.1/x"],
    ["https://example.test", "https://user:pass@example.test/x"],
    ["https://example.test", "https://user@example.test/x"],
    ["https://example.test", "ftp://example.test/x"],
    ["https://example.test", "data:,x"],
    ["https://example.test", "file:///etc/passwd"],
    ["https://example.test", "http://[::1"],
    ["https://example.test", "https://example.test/a b"],
    ["https://example.test", "https://example.test\\x"],
    ["https://example.test", "https://bücher.example/"],
    ["https://example.test", "https://example.test/\u0001"],
  ]) {
    let released = false;
    const { calls, fetch } = transport({
      [`${origin}/.well-known/openbindings`]: () =>
        new Response(
          new ReadableStream({
            cancel() {
              released = true;
            },
          }),
          { status: 302, headers: { location } },
        ),
    });
    const result = await discovery.discover(origin, { fetch });
    assert.equal(result.status, "transport-error", location);
    assert.equal(result.error.kind, "redirect-policy");
    assert.equal(result.metadata.status, 302);
    assert.equal(
      result.metadata.finalUrl,
      `${origin}/.well-known/openbindings`,
    );
    assert.equal(calls.length, 1, location);
    assert.ok(released);
  }
});

// the eleventh redirect is refused and its destination is never dispatched.
test("checked hops stop at ten redirects", async () => {
  for (const hops of [10, 11]) {
    const routes = {
      "https://example.test/.well-known/openbindings": () => redirectTo("/1"),
    };
    for (let hop = 1; hop <= hops; hop++)
      routes[`https://example.test/${hop}`] = () =>
        hop < hops ? redirectTo(`/${hop + 1}`) : new Response(text);
    const { calls, fetch } = transport(routes);
    const result = await discovery.discover("https://example.test", { fetch });
    assert.equal(calls.length, 11);
    assert.equal(result.metadata.finalUrl, "https://example.test/10");
    if (hops === 10) {
      assert.equal(result.status, "found");
      result.document.dispose();
    } else {
      assert.equal(result.status, "transport-error");
      assert.equal(result.error.kind, "redirect-policy");
    }
  }
});

// a transport asked not to follow must not report a different, followed response.
test("hidden following by a manual transport is a redirect-policy refusal", async () => {
  for (const response of [
    responseUrl(new Response(text), "https://elsewhere.test/final"),
    Object.defineProperty(new Response(text), "redirected", { value: true }),
  ]) {
    const result = await discovery.discover("https://example.test", {
      fetch: async () => response,
    });
    assert.equal(result.status, "transport-error");
    assert.equal(result.error.kind, "redirect-policy");
    assert.equal(result.body, undefined);
    assert.equal(
      result.metadata.finalUrl,
      response.url || "https://example.test/.well-known/openbindings",
    );
  }
  // A host that exposed one redirect cannot hide the next one.
  const { calls, fetch } = transport({
    "https://example.test/.well-known/openbindings": () => redirectTo("/next"),
    "https://example.test/next": () => hidden(),
  });
  const result = await discovery.discover("https://example.test", { fetch });
  assert.equal(result.status, "transport-error");
  assert.equal(result.error.kind, "redirect-policy");
  assert.equal(result.metadata.finalUrl, null);
  assert.deepEqual(
    calls.map((call) => call.redirect),
    ["manual", "manual"],
  );
});

// a failed later hop is acquisition failure, keeping the redirect's metadata.
test("a later hop's network failure is a transport error", async () => {
  const result = await discovery.discover("https://example.test", {
    fetch: async (url) => {
      if (url.endsWith("/next")) throw new TypeError("fetch failed");
      return redirectTo("/next");
    },
  });
  assert.equal(result.status, "transport-error");
  assert.equal(result.error.kind, "network");
  assert.equal(result.metadata.status, 302);
});

// one deadline covers never-ready headers and body, independent of caller signals.
test("SDK deadline interrupts stalled acquisition and retains body-phase metadata", async () => {
  // A controlled clock passes the deadline only once the phase under test is
  // reached, so host speed or load cannot move expiry to an earlier phase.
  const original = performance.now;
  let now = 0;
  Object.defineProperty(performance, "now", {
    configurable: true,
    value: () => now,
  });
  try {
    for (const phase of ["headers", "body"]) {
      now = 0;
      let cancelled = false;
      const result = await discovery.discover("https://example.test", {
        timeoutMs: 10,
        fetch: async () => {
          if (phase === "headers") {
            now = 11;
            return new Promise(() => {});
          }
          // A zero high-water mark pulls only for the SDK's own read, so the
          // deadline passes while that read is waiting on the body.
          return new Response(
            new ReadableStream(
              {
                pull() {
                  now = 11;
                  return new Promise(() => {});
                },
                cancel() {
                  cancelled = true;
                },
              },
              { highWaterMark: 0 },
            ),
          );
        },
      });
      assert.equal(result.status, "timeout");
      assert.equal(!!result.metadata, phase === "body");
      assert.equal(result.body, undefined);
      assert.equal(cancelled, phase === "body");
    }
  } finally {
    Object.defineProperty(performance, "now", {
      configurable: true,
      value: original,
    });
  }
  for (const timeoutMs of [0, -1, NaN, Infinity, "unbounded"])
    await assert.rejects(
      discovery.discover("https://example.test", {
        timeoutMs,
        fetch: async () => {
          throw Error("dispatched");
        },
      }),
      { code: "invalid-timeout" },
    );
});

// without timeoutMs no deadline applies: only the caller's signal ends a stalled attempt.
test("an attempt without timeoutMs waits until the caller aborts", async () => {
  // A controlled clock leaps far past any deadline while acquisition stalls.
  const original = performance.now;
  let now = 0;
  Object.defineProperty(performance, "now", {
    configurable: true,
    value: () => now,
  });
  try {
    for (const phase of ["headers", "body"]) {
      now = 0;
      let cancelled = false;
      const control = new AbortController();
      const pending = discovery.discover("https://example.test", {
        signal: control.signal,
        fetch: async () => {
          now = 365 * 24 * 60 * 60 * 1000;
          if (phase === "headers") return new Promise(() => {});
          return new Response(
            new ReadableStream(
              {
                pull: () => new Promise(() => {}),
                cancel() {
                  cancelled = true;
                },
              },
              { highWaterMark: 0 },
            ),
          );
        },
      });
      const settled = await Promise.race([
        pending.then(() => true),
        new Promise((resolve) => setTimeout(() => resolve(false), 20)),
      ]);
      assert.equal(settled, false);
      control.abort();
      const result = await pending;
      assert.equal(result.status, "cancelled");
      assert.equal(result.reason, "aborted");
      assert.equal(!!result.metadata, phase === "body");
      assert.equal(result.body, undefined);
      assert.equal(cancelled, phase === "body");
    }
  } finally {
    Object.defineProperty(performance, "now", {
      configurable: true,
      value: original,
    });
  }
});

// already-ready cancellation wins a tie; first observed expiry stays timeout.
test("caller cancellation and timeout remain distinct with deterministic ties", async () => {
  // A controlled clock makes the tie exact: the deadline passes and the caller
  // aborts at the same observation, independent of how fast the host runs.
  const original = performance.now.bind(performance);
  let now = 0;
  Object.defineProperty(performance, "now", {
    configurable: true,
    value: () => now,
  });
  try {
    const control = new AbortController();
    const result = await discovery.discover("https://example.test", {
      timeoutMs: 1,
      signal: control.signal,
      fetch: async () => {
        now = 10;
        control.abort();
        return new Response(text);
      },
    });
    assert.equal(result.status, "cancelled");
    assert.equal(result.reason, "aborted");
  } finally {
    Object.defineProperty(performance, "now", {
      configurable: true,
      value: original,
    });
  }
  const timeout = await discovery.discover("https://example.test", {
    signal: AbortSignal.timeout(5),
    timeoutMs: 100,
    fetch: async () => new Promise(() => {}),
  });
  assert.equal(timeout.status, "timeout");
});

// checking monotonic time catches elapsed validation even with a blocked event loop.
test("over-deadline synchronous validation releases handles and retains complete bytes", async () => {
  const original = performance.now.bind(performance);
  const before = testing.liveStorageOwners();
  let now = 0;
  Object.defineProperty(performance, "now", {
    configurable: true,
    value: () => now,
  });
  try {
    const result = await discovery.discover("https://example.test", {
      timeoutMs: 10,
      fetch: async () => new Response(text),
    });
    // Separate boundary witness below uses a clock that expires only after body acquisition.
    assert.equal(result.status, "found");
    result.document.dispose();
    let calls = 0;
    Object.defineProperty(performance, "now", {
      configurable: true,
      value: () => (++calls >= 10 ? 11 : 0),
    });
    const timed = await discovery.discover("https://example.test", {
      timeoutMs: 10,
      fetch: async () => new Response(text),
    });
    assert.equal(timed.status, "timeout");
    assert.equal(new TextDecoder().decode(timed.body), text);
    assert.equal(testing.liveStorageOwners(), before);
  } finally {
    Object.defineProperty(performance, "now", {
      configurable: true,
      value: original,
    });
  }
});

// SDK-detected chunk contract errors are not fabricated network failures.
test("invalid body chunks have a typed non-network failure", async () => {
  const result = await discovery.discover("https://example.test", {
    fetch: async () =>
      new Response(
        new ReadableStream({
          start(c) {
            c.enqueue("bad chunk");
            c.close();
          },
        }),
      ),
  });
  assert.equal(result.status, "body-error");
  assert.equal(result.error.kind, "other");
});

// canonical CORS values, permissive default; a percent-encoded unreserved character
// names the route (RFC 3986 section 2.3), while a reserved escape stays distinct.
test("publication uses serialized origins and matches the route by URI equivalence", async () => {
  const parsed = sdk.Document.parse(text).value;
  const proof = parsed.validate().document;
  const publication = new discovery.DiscoveryPublication(proof);
  const response = publication.respond(
    new Request("https://example.test/.well-known/openbindings"),
  );
  assert.equal(response.headers.get("access-control-allow-origin"), "*");
  for (const path of [
    "/.well-known/%6Fpenbindings",
    "/.well-known/%6fpenbindings",
    "/%2ewell-known/openbindings",
  ]) {
    const encoded = publication.respond(
      new Request("https://example.test" + path),
    );
    assert.equal(encoded.status, 200, path);
    assert.equal(await encoded.text(), text, path);
  }
  for (const path of [
    "/.well-known%2Fopenbindings",
    "/.well-known%2fopenbindings",
    "/.well-known/openbindings/",
    "/.well-known/%4Fpenbindings",
  ])
    assert.equal(
      publication.respond(new Request("https://example.test" + path)).status,
      404,
      path,
    );
  for (const allowOrigin of [
    "HTTPS://A.EXAMPLE",
    "https://a.example:443",
    "file://x",
  ])
    assert.throws(
      () =>
        new discovery.DiscoveryPublication(proof, {
          allowOrigin: { origin: allowOrigin },
        }),
      { code: "invalid-allow-origin" },
    );
  for (const [allowOrigin, expected] of [
    ["any", "*"],
    ["null", "null"],
    [{ origin: "https://client.example" }, "https://client.example"],
  ]) {
    const configured = new discovery.DiscoveryPublication(proof, {
      allowOrigin,
    });
    assert.equal(
      configured
        .respond(new Request("https://example.test/.well-known/openbindings"))
        .headers.get("access-control-allow-origin"),
      expected,
    );
    configured.dispose();
  }
  for (const origin of ["", "*", "null"])
    assert.throws(
      () =>
        new discovery.DiscoveryPublication(proof, { allowOrigin: { origin } }),
      { code: "invalid-allow-origin" },
    );
  const omitted = new discovery.DiscoveryPublication(proof, {
    allowOrigin: "omit",
  });
  assert.equal(
    omitted
      .respond(new Request("https://example.test/.well-known/openbindings"))
      .headers.get("access-control-allow-origin"),
    null,
  );
  omitted.dispose();
  publication.dispose();
  proof.dispose();
  parsed.dispose();
});

// the default installs no timer; a set timeoutMs installs one and releases it on success.
test("a timer only for a set timeoutMs and late response cleanup", async () => {
  // A controlled clock fixes the timer's delay and moves past the deadline only
  // after dispatch, so host speed or load cannot expire the attempt earlier.
  const original = performance.now;
  let now = 0;
  Object.defineProperty(performance, "now", {
    configurable: true,
    value: () => now,
  });
  const nativeSetTimeout = globalThis.setTimeout;
  try {
    const delays = [];
    globalThis.setTimeout = (fn, delay, ...args) => {
      delays.push(delay);
      return nativeSetTimeout(fn, delay, ...args);
    };
    try {
      const result = await discovery.discover("https://example.test", {
        fetch: async () => new Response(text),
      });
      assert.equal(result.status, "found");
      result.document.dispose();
      assert.deepEqual(delays, []);
      const bounded = await discovery.discover("https://example.test", {
        timeoutMs: 30_000,
        fetch: async () => new Response(text),
      });
      assert.equal(bounded.status, "found");
      bounded.document.dispose();
      assert.deepEqual(delays, [30_000]);
    } finally {
      globalThis.setTimeout = nativeSetTimeout;
    }
    let resolve;
    const late = new Promise((r) => {
      resolve = r;
    });
    const timed = await discovery.discover("https://example.test", {
      timeoutMs: 5,
      fetch: async () => {
        now = 6;
        return late;
      },
    });
    assert.equal(timed.status, "timeout");
    let released = false;
    resolve(
      new Response(
        new ReadableStream({
          cancel() {
            released = true;
          },
        }),
      ),
    );
    await new Promise((r) => nativeSetTimeout(r, 0));
    assert.ok(released);
  } finally {
    Object.defineProperty(performance, "now", {
      configurable: true,
      value: original,
    });
  }
});

// successful EOF retains all bytes before checking the terminal cause.
test("EOF expiry retains complete bytes while interrupted prefixes stay absent", async () => {
  const original = performance.now;
  try {
    for (const eof of [false, true]) {
      for (const cancel of [false, true]) {
        let now = 0;
        Object.defineProperty(performance, "now", {
          configurable: true,
          value: () => now,
        });
        let reads = 0;
        const control = new AbortController();
        const result = await discovery.discover("https://example.test", {
          timeoutMs: 10,
          signal: control.signal,
          fetch: async () => {
            const response = new Response();
            Object.defineProperty(response, "body", {
              value: {
                getReader() {
                  return {
                    read: () => {
                      if (reads++ === 0)
                        return Promise.resolve({
                          done: false,
                          value: new TextEncoder().encode(text),
                        });
                      const part = eof
                        ? { done: true }
                        : { done: false, value: new Uint8Array([32]) };
                      if (!cancel) {
                        now = 11;
                        return Promise.resolve(part);
                      }
                      return new Promise((resolve) =>
                        queueMicrotask(() => {
                          resolve(part);
                          // Let interruptible commit the read, then abort before its caller resumes.
                          queueMicrotask(() => control.abort());
                        }),
                      );
                    },
                    cancel: async () => {},
                    releaseLock() {},
                  };
                },
              },
            });
            return response;
          },
        });
        assert.equal(result.status, cancel ? "cancelled" : "timeout");
        assert.equal(result.metadata.status, 200);
        assert.equal(
          result.body ? new TextDecoder().decode(result.body) : undefined,
          eof ? text : undefined,
        );
      }
    }
  } finally {
    Object.defineProperty(performance, "now", {
      configurable: true,
      value: original,
    });
  }
});
