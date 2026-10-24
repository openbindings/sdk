// A 200 body cut short is a body failure, never a partial document, whatever
// framing the response used and whether the connection was to close.
import test from "node:test";
import assert from "node:assert/strict";
import net from "node:net";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as discovery from "../dist/http-discovery.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const text = '{"openbindings":"0.2.0","operations":{"lookup":{}}}';
// Padding before the document puts a cut inside its JSON; after it, a cut
// leaves the document complete.
const leading = Buffer.from(" ".repeat(50000) + text);
const trailing = Buffer.from(text + " ".repeat(50000));
const chunks = (body) => {
  const out = [];
  for (let at = 0; at < body.length; at += 4096) {
    const part = body.subarray(at, at + 4096);
    out.push(Buffer.from(part.length.toString(16) + "\r\n"), part);
    out.push(Buffer.from("\r\n"));
  }
  return Buffer.concat(out);
};
const head = (framing, close) =>
  Buffer.from(
    "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.openbindings+json\r\n" +
      framing +
      (close ? "Connection: close\r\n" : "") +
      "\r\n",
  );
const chunked = "Transfer-Encoding: chunked\r\n";
const shapes = {
  "no terminating chunk": (close) =>
    Buffer.concat([head(chunked, close), chunks(trailing)]),
  "a cut inside a chunk": (close) =>
    Buffer.concat([
      head(chunked, close),
      Buffer.from(leading.length.toString(16) + "\r\n"),
      leading.subarray(0, leading.length - 10),
    ]),
  "a cut across chunks": (close) => {
    const whole = Buffer.concat([head(chunked, close), chunks(trailing)]);
    return whole.subarray(0, whole.length - 200);
  },
  "a cut in the last chunk's size line": (close) =>
    Buffer.concat([head(chunked, close), chunks(trailing), Buffer.from("0")]),
  "no final empty line": (close) =>
    Buffer.concat([
      head(chunked, close),
      chunks(trailing),
      Buffer.from("0\r\n"),
    ]),
  "a short fixed-length body": (close) =>
    Buffer.concat([
      head(`Content-Length: ${leading.length}\r\n`, close),
      leading.subarray(0, leading.length - 10),
    ]),
};
// Serve one raw response per connection, then end it, or reset it.
async function serve(response, reset = false) {
  const sockets = new Set();
  const server = net.createServer((socket) => {
    sockets.add(socket);
    socket.on("error", () => {});
    socket.once("data", () =>
      reset
        ? socket.write(response, () => socket.resetAndDestroy())
        : socket.end(response),
    );
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return {
    origin: `http://127.0.0.1:${server.address().port}`,
    async close() {
      for (const socket of sockets) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}
for (const close of [false, true])
  for (const [name, shape] of Object.entries(shapes))
    test(`${name}${close ? " under Connection: close" : ""} is a body error`, async () => {
      const server = await serve(shape(close));
      try {
        const result = await discovery.discover(server.origin);
        assert.equal(
          result.status,
          "body-error",
          JSON.stringify(result.report),
        );
        assert.equal(result.error.kind, "network");
        assert.equal(result.body, undefined);
        assert.equal(result.metadata.status, 200);
      } finally {
        await server.close();
      }
    });
for (const close of [false, true])
  test(`a reset after the chunks${close ? " under Connection: close" : ""} is a body error`, async () => {
    const server = await serve(
      Buffer.concat([head(chunked, close), chunks(trailing)]),
      true,
    );
    try {
      const result = await discovery.discover(server.origin);
      assert.equal(result.status, "body-error");
      assert.equal(result.body, undefined);
    } finally {
      await server.close();
    }
  });
test("complete chunked and connection-delimited bodies are found", async () => {
  for (const response of [
    Buffer.concat([
      head(chunked, true),
      chunks(leading),
      Buffer.from("0\r\n\r\n"),
    ]),
    Buffer.concat([
      head(chunked, false),
      chunks(leading),
      Buffer.from("0\r\n\r\n"),
    ]),
    // Without a length or chunking, closing the connection ends the body.
    Buffer.concat([head("", true), Buffer.from(text)]),
  ]) {
    const server = await serve(response);
    try {
      const result = await discovery.discover(server.origin);
      assert.equal(result.status, "found");
      result.document.dispose();
    } finally {
      await server.close();
    }
  }
});
