// One fresh-process observation. No server, network access or SDK modification.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";
import { jsonLimits, preparationLimits, prepareOptions } from "./guide.mjs";
import { docMany, docNested, recordsDoc, recordsSchema, recordsValue, chainDoc } from "./fixtures.mjs";

const [packagePath, params] = process.argv.slice(2);
const p = JSON.parse(params);
const base = pathToFileURL(resolve(packagePath) + "/dist/");
const sdk = await import(new URL("index.js", base));
await sdk.initialize(await readFile(new URL("wasm/openbindings_wasm_bg.wasm", base)));
const bridge = await import(new URL("wasm/openbindings_wasm.js", base));
const wasm = await bridge.default();
const memory = () => wasm.memory.buffer.byteLength;
const owners = [];
const own = value => (owners.push(value), value);
const parsed = result => (assert.equal(result.status, "parsed"), own(result.value));
const proof = doc => {
  const result = doc.validate();
  assert.equal(result.status, "conformant");
  return own(result.document);
};

function text() {
  if (p.kind === "many") return docMany(p.bytes);
  if (p.kind === "nested") return docNested(p.bytes);
  if (p.kind === "findings")
    return `{"openbindings":"0.2.0","operations":{${Array.from({ length: p.n }, (_, i) => `"o${i}":{"input":5}`).join(",")}}}`;
  if (p.kind === "deep")
    return `{"openbindings":"0.2.0","operations":{},"x-deep":${"[".repeat(p.n)}${"]".repeat(p.n)}}`;
  if (p.kind === "array" || p.kind === "object") {
    const object = p.kind === "object";
    const values = Array.from({ length: p.n }, (_, i) => object ? `"k${i}":0` : "0").join(",");
    return `{"openbindings":"0.2.0","operations":{},"x-data":${object ? "{" : "["}${values}${object ? "}" : "]"}}`;
  }
  assert.equal(p.kind, "ids");
  const element = (i, n) => '{"$id":"s:\\/\\/' + i.toString(16) + "/q/a/../b/" + "c".repeat(n) + "/" + (p.pieces ?? "") + '"}';
  const prefix = '{"openbindings":"0.2.0","operations":{},"schemas":{"s":{"allOf":[';
  const suffix = "]}}}";
  let bytes = prefix.length + suffix.length;
  for (let i = 0; i < p.n; i++) bytes += element(i, 0).length + 1;
  const width = Math.floor((p.bytes - bytes) / p.n);
  return prefix + Array.from({ length: p.n }, (_, i) => element(i, width)).join(",") + suffix;
}

function validate() {
  const input = new TextEncoder().encode(text());
  const beforeBytes = memory();
  const start = performance.now();
  const result = p.kind === "findings"
    ? parsed(sdk.Document.parse(input)).validate({ limits: p.limits })
    : sdk.validateDocument(input, { limits: p.limits });
  return { inputBytes: input.length, beforeBytes, afterBytes: memory(),
    growthBytes: memory() - beforeBytes, ms: performance.now() - start,
    outcome: result.report.conclusion, code: result.report.findings[0]?.code ?? null };
}

function prepare() {
  const source = p.kind === "chain" ? chainDoc(p.n) : recordsDoc(p.n, p.supplied, p.distinct);
  const doc = parsed(sdk.Document.parse(source));
  const schema = own(proof(doc).inputSchema("op"));
  let resources;
  if (p.supplied) {
    const value = parsed(sdk.parseJson(JSON.stringify(recordsSchema(p.n, p.distinct))));
    const registrationBefore = memory();
    const registrationStart = performance.now();
    try {
      resources = own(new sdk.ExternalSchemas([["https://records.test/schema", value]], { limits: p.preparationLimits }));
    } catch (e) {
      assert.equal(e.code, "resource");
      const afterBytes = memory(), ms = performance.now() - registrationStart;
      const small = own(new sdk.ExternalSchemas([["urn:retry", parsed(sdk.parseJson("true"))]]));
      assert.equal(own(small.prepare("urn:retry").schema).evaluate(7).status, "satisfies");
      return { status: "refused", stage: "register", code: e.code, beforeBytes: registrationBefore,
        afterBytes, growthBytes: afterBytes - registrationBefore, ms };
    }
    value.dispose();
  }
  const beforeBytes = memory();
  const start = performance.now();
  const result = schema.prepare({ resources, preparationLimits: p.preparationLimits, limits: p.limits });
  const ms = performance.now() - start;
  const afterBytes = memory();
  let outcome = null, code = result.failure?.details?.resource ?? null, evaluateMs = null;
  if (result.status === "ready") {
    const prepared = own(result.schema);
    const value = p.kind === "chain" ? 7 : recordsValue(p.n, p.distinct);
    const t = performance.now();
    const check = prepared.evaluate(value);
    evaluateMs = performance.now() - t;
    outcome = check.status;
    code = check.detail?.code ?? null;
  } else {
    assert.equal(result.failure.kind, "limit");
    assert.ok(["preparation-sdk-reservation-bytes", "preparation-sdk-work-units"].includes(code));
  }
  const highWaterBytes = memory();
  // A refusal is recoverable, unlike an allocation trap.
  const small = own(new sdk.ExternalSchemas([["urn:small", parsed(sdk.parseJson('{"type":"integer"}'))]]));
  const retry = small.prepare("urn:small");
  assert.equal(retry.status, "ready");
  assert.equal(own(retry.schema).evaluate(7).status, "satisfies");
  return { status: result.status, code, outcome, beforeBytes, afterBytes,
    growthBytes: afterBytes - beforeBytes, highWaterBytes, ms, evaluateMs };
}

function hostile() {
  const doc = parsed(sdk.Document.parse(JSON.stringify({ openbindings: "0.2.0",
    operations: { op: { input: { type: "string", pattern: "^(?:a{1,200}){1,200}$" } } } })));
  const schema = own(proof(doc).inputSchema("op"));
  const result = schema.prepare({ limits: p.limits });
  assert.equal(result.status, "ready");
  const prepared = own(result.schema);
  const controller = new AbortController();
  if (p.abortAfterMs) setTimeout(() => controller.abort(), p.abortAfterMs);
  if (p.preAbort) controller.abort();
  const start = performance.now();
  const check = prepared.evaluate("a".repeat(2001) + "b", { signal: controller.signal });
  return { outcome: check.status, code: check.detail?.code ?? null, ms: performance.now() - start };
}

// Run the guide's complete recipe with all owners alive until the synchronous
// request finishes. Stop at the first typed refusal and record its stage.
function recipe() {
  const source = p.kind === "records" ? recordsDoc(p.n, p.supplied, p.distinct)
    : p.kind === "chain" ? chainDoc(p.n)
    : p.kind === "hostile" ? JSON.stringify({ openbindings: "0.2.0", operations: {
      op: { input: { type: "string", pattern: "^(?:a{1,200}){1,200}$" } } } }) : text();
  const beforeBytes = memory(), start = performance.now(), stages = {};
  let stage = "parse document";
  const timed = (name, run) => {
    stage = name;
    const t = performance.now();
    const result = run();
    stages[name] = performance.now() - t;
    return result;
  };
  const finish = (status, code = null) => ({ stage, status, code, stages, inputBytes: Buffer.byteLength(source),
    beforeBytes, afterBytes: memory(), growthBytes: memory() - beforeBytes, ms: performance.now() - start });
  const admit = (name, source, document = false) => {
    const result = timed(name, () => document ? sdk.Document.parse(source, { limits: jsonLimits })
      : sdk.parseJson(source, { limits: jsonLimits }));
    if (result.status !== "parsed") return result;
    return { status: "parsed", value: own(result.value) };
  };
  const document = admit("parse document", source, true);
  if (document.status !== "parsed") return finish(document.status, document.error?.code);
  const checked = timed("validate", () => document.value.validate({ limits: { maxFindings: 100 } }));
  if (checked.status !== "conformant") return finish(checked.status, checked.report.findings[0]?.code);
  const proof = own(checked.document);
  const uri = "https://records.test/schema";
  const resource = admit("parse resource", p.supplied ? JSON.stringify(recordsSchema(p.n, p.distinct)) : "true");
  if (resource.status !== "parsed") return finish(resource.status, resource.error?.code);
  const limits = { ...preparationLimits, ...p.preparationLimits };
  let resources;
  try {
    resources = timed("register", () => p.registration === "with"
      ? own(own(new sdk.ExternalSchemas([], { limits })).with(uri, resource.value, { limits }))
      : own(new sdk.ExternalSchemas([[uri, resource.value]], { limits })));
  } catch (e) { assert.equal(e.code, "resource"); return finish("refused", e.code); }
  const key = p.kind === "many" ? "op0" : "op";
  const schema = proof.inputSchema(key) ?? (p.kind === "ids" ? proof.schema("s") : undefined);
  if (schema) own(schema);
  const options = { ...prepareOptions, limits: { ...prepareOptions.limits, ...p.limits }, preparationLimits: limits };
  const prepared = timed("prepare", () => schema ? schema.prepare({ ...options, resources }) : resources.prepare(uri, options));
  if (prepared.status !== "ready") return finish(prepared.status, prepared.failure?.details?.resource);
  const ready = own(prepared.schema);
  const valueText = p.kind === "records" ? JSON.stringify(recordsValue(p.n, p.distinct))
    : p.kind === "hostile" ? JSON.stringify("a".repeat(2001) + "b")
    : ["many", "nested"].includes(p.kind) ? '{"a":"x","b":1}' : "7";
  const value = admit("parse value", valueText);
  if (value.status !== "parsed") return finish(value.status, value.error?.code);
  const outcome = timed("evaluate", () => ready.evaluate(value.value));
  return finish(outcome.status, outcome.detail?.code);
}

try {
  const result = p.recipe ? recipe() : p.kind === "hostile" ? hostile()
    : ["records", "chain"].includes(p.kind) ? prepare() : validate();
  console.log(JSON.stringify(result));
} finally {
  for (const owner of owners.reverse()) owner.dispose();
}
