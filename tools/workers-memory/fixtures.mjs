// Deterministic shapes from the 2026-11-14 Workers measurements.
// Many small operations. Each carries a 600-character description so that a
// 32 MiB document stays under the default 1,000,000-node admission limit.
export function docMany(target) {
  const pad = "d".repeat(600);
  const head = '{"openbindings":"0.2.0","operations":{';
  const tail = "}}";
  const op = (i) =>
    `"op${i}":{"description":"${pad}","input":{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}},"required":["a"]}}`;
  // Add operations while the whole document stays within the target.
  const parts = [];
  let total = head.length + tail.length;
  for (let i = 0; ; i++) {
    const o = op(i);
    const add = o.length + (i ? 1 : 0);
    if (i > 0 && total + add > target) break;
    parts.push(o); total += add;
  }
  return head + parts.join(",") + tail;
}

// One deeply nested schema: up to 2,000 nested object levels (4,000 container
// levels plus the document's own), each with k sibling string properties that
// carry a 200-character description. k is chosen to reach the target size.
export function docNested(target) {
  const pad = "n".repeat(200);
  const field = (j) => `"f${j}":{"type":"string","description":"${pad}"}`;
  const levels = Math.max(1, Math.min(2000, Math.floor(target / 2000)));
  const fieldLen = field(0).length + 1;
  let k = Math.max(0, Math.round((target / levels - 40) / fieldLen));
  // Keep the whole document within the target.
  while (k > 0 && levels * (40 + k * fieldLen) + 120 > target) k--;
  const fields = Array.from({ length: k }, (_, j) => field(j)).join(",");
  let open = "", close = "";
  for (let l = 0; l < levels; l++) {
    open += `{"type":"object","properties":{${fields}${k ? "," : ""}"next":`;
    close = "}}" + close;
  }
  const body = open + '{"type":"integer"}' + close;
  return `{"openbindings":"0.2.0","schemas":{"deep":${body}},"operations":{"op":{"input":{"$ref":"#/schemas/deep"}}}}`;
}

// An object of n eight-property records.
// With distinct, every record's property names carry its index, so no two
// records are equal JSON.
export function recordsSchema(n, distinct) {
  const make = (i) => {
    const r = recBase();
    if (!distinct) return r;
    const props = {};
    for (const [k, v] of Object.entries(r.properties)) props[k + i] = v;
    return { ...r, properties: props, required: r.required.map((k) => k + i) };
  };
  const properties = {};
  for (let i = 0; i < n; i++) properties[`r${i}`] = make(i);
  return { type: "object", properties };
}
function recBase() {
  return {
    type: "object",
    properties: {
      id: { type: "integer" }, name: { type: "string", maxLength: 64 },
      price: { type: "number", minimum: 0 }, active: { type: "boolean" },
      tags: { type: "array", items: { type: "string" } },
      kind: { enum: ["a", "b", "c"] }, note: { type: "string" },
      count: { type: "integer", minimum: 0 },
    },
    required: ["id", "name"],
    additionalProperties: false,
  };
}
export function recordsValue(n, distinct) {
  const v = {};
  for (let i = 0; i < n; i++) {
    const r = { id: i, name: `n${i}`, price: 1.5, active: true, tags: ["x"], kind: "a", note: "", count: 3 };
    v[`r${i}`] = distinct ? Object.fromEntries(Object.entries(r).map(([k, x]) => [k + i, x])) : r;
  }
  return v;
}
export function recordsDoc(n, supplied, distinct) {
  return JSON.stringify({
    openbindings: "0.2.0",
    operations: { op: { input: supplied ? { $ref: "https://records.test/schema" } : recordsSchema(n, distinct) } },
  });
}
// d0 -> d1 -> ... -> d(links), the last an integer (the package test's chain).
export function chainDoc(links) {
  const parts = [];
  for (let i = 0; i < links; i++) parts.push(`"d${i}":{"$ref":"#/schemas/d${i + 1}"}`);
  parts.push(`"d${links}":{"type":"integer"}`);
  return `{"openbindings":"0.2.0","schemas":{${parts.join(",")}},"operations":{"op":{"input":{"$ref":"#/schemas/d0"}}}}`;
}
