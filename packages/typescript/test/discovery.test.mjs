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
test("normalized literal loopback admission and Fetch automatic following", async () => {
  for (const host of ["2130706433", "0x7f.1", "%31%32%37.0.0.1", "127.1"]) {
    const result = await discovery.discover(`http://${host}`, {
      fetch: async (url, init) => {
        assert.equal(url, "http://127.0.0.1/.well-known/openbindings");
        assert.equal(init.redirect, "follow");
        return responseUrl(new Response(text), "https://example.test/final");
      },
    });
    assert.equal(result.status, "found");
    assert.equal(
      result.metadata.requestedUrl,
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
    const result = await discovery.discover("https://example.test", {
      fetch: async () => response,
    });
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

// Fetch follows the five redirect statuses, so a visible Location was left unfollowed.
test("unfollowed visible redirect is a redirect-policy refusal", async () => {
  for (const status of [301, 302, 303, 307, 308]) {
    let cancelled = false;
    const result = await discovery.discover("https://example.test", {
      fetch: async () =>
        new Response(
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
  for (const timeoutMs of [0, -1, NaN, Infinity])
    await assert.rejects(
      discovery.discover("https://example.test", {
        timeoutMs,
        fetch: async () => {
          throw Error("dispatched");
        },
      }),
      { code: "invalid-timeout" },
    );
  const result = await discovery.discover("https://example.test", {
    timeoutMs: "unbounded",
    fetch: async () => new Response(text),
  });
  assert.equal(result.status, "found");
  result.document.dispose();
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

// canonical CORS values, permissive default, exact reserved route spelling.
test("publication uses serialized origins and matches the route exactly", async () => {
  const parsed = sdk.Document.parse(text).value;
  const proof = parsed.validate().document;
  const publication = new discovery.DiscoveryPublication(proof);
  const response = publication.respond(
    new Request("https://example.test/.well-known/openbindings"),
  );
  assert.equal(response.headers.get("access-control-allow-origin"), "*");
  for (const path of [
    "/.well-known%2Fopenbindings",
    "/.well-known%2fopenbindings",
    "/.well-known/openbindings/",
    "/%2ewell-known/openbindings",
  ])
    assert.equal(
      publication.respond(new Request("https://example.test" + path)).status,
      404,
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

// the default installs one thirty-second timer and releases it on success.
test("default thirty-second timer and late response cleanup", async () => {
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
