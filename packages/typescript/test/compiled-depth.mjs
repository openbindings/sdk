import { prepareInput } from "./conformant-contracts.mjs";

// Public API only. A Worker can initialize its own SDK, import this module, and
// call compiledDepthCases(sdk). No Node, DOM, testing entry point or private lane
// switch is needed. beforeCase permits a host probe to record the last attempted
// operation before a trap; a thrown error always escapes and fails the walk.
// Boundaries were read from qualified_depth() in the native release tests at
// 1d8b757 on 2026-11-07. They describe container depth, not recursive links.
const reference = { $ref: "#" };
export const compiledDepthShapes = [
  {
    name: "integer-array",
    compiledDepth: 203,
    schema: {
      $defs: {
        node: {
          anyOf: [
            { type: "integer" },
            { type: "array", items: { $ref: "#/$defs/node" } },
          ],
        },
        unused: { const: false },
      },
      $ref: "#/$defs/node",
    },
    open: "[",
    close: "]",
    leaves: ["0", "false"],
    leafDepth: 0,
    linkDepth: 1,
  },
  ...["named", "enum"].map((name) => ({
    name,
    compiledDepth: 340,
    schema: {
      type: "object",
      properties: {
        name: { type: "string" },
        ...(name === "enum" ? { kind: { enum: ["branch", "leaf"] } } : {}),
        children: { type: "array", items: reference },
      },
      required:
        name === "enum" ? ["name", "kind", "children"] : ["name", "children"],
      additionalProperties: false,
    },
    open:
      name === "enum"
        ? '{"name":"node","kind":"branch","children":['
        : '{"name":"node","children":[',
    close: "]}",
    leaves:
      name === "enum"
        ? [
            '{"name":"node","kind":"leaf","children":[]}',
            '{"name":"node","kind":42,"children":[]}',
          ]
        : ['{"name":"node","children":[]}', '{"name":42,"children":[]}'],
    leafDepth: 2,
    linkDepth: 2,
  })),
  {
    name: "binary",
    compiledDepth: 340,
    schema: {
      type: ["object", "null"],
      properties: { left: reference, right: reference },
      required: ["left", "right"],
      additionalProperties: false,
    },
    open: '{"left":',
    close: ',"right":null}',
    leaves: ["null", "42"],
    leafDepth: 0,
    linkDepth: 1,
  },
  {
    name: "minimum",
    compiledDepth: 340,
    schema: {
      type: ["object", "null"],
      properties: {
        value: { type: "integer", minimum: 0 },
        left: reference,
        right: reference,
      },
      required: ["value", "left", "right"],
      additionalProperties: false,
    },
    open: '{"value":0,"left":',
    close: ',"right":null}',
    leaves: [
      '{"value":0,"left":null,"right":null}',
      '{"value":-1,"left":null,"right":null}',
    ],
    leafDepth: 1,
    linkDepth: 1,
  },
  {
    name: "additional",
    compiledDepth: 340,
    schema: {
      type: "object",
      properties: { name: { type: "string" } },
      required: ["name"],
      additionalProperties: reference,
    },
    open: '{"name":"node","next":',
    close: "}",
    leaves: ['{"name":"leaf"}', '{"name":42}'],
    leafDepth: 1,
    linkDepth: 1,
  },
  {
    name: "union",
    compiledDepth: 203,
    schema: {
      oneOf: [
        {
          type: "object",
          properties: { kind: { const: "leaf" }, value: { type: "integer" } },
          required: ["kind", "value"],
          additionalProperties: false,
        },
        {
          type: "object",
          properties: {
            kind: { const: "branch" },
            children: { type: "array", items: reference },
          },
          required: ["kind", "children"],
          additionalProperties: false,
        },
      ],
    },
    open: '{"kind":"branch","children":[',
    close: "]}",
    leaves: ['{"kind":"leaf","value":0}', '{"kind":"leaf","value":false}'],
    leafDepth: 1,
    linkDepth: 2,
    // The empty branch reaches even container depths, including 10,000.
    alternate: {
      leafDepth: 2,
      leaves: [
        '{"kind":"branch","children":[]}',
        '{"kind":"wrong","children":[]}',
      ],
    },
  },
];

// Each shape's compiledDepth is the native qualified_depth(), pinned by
// compiled_qualification::wasm_depth_walk_bounds_match_native_selection.
// A linear chain's evaluation path at one depth is a prefix of the next, so
// stack use grows with depth and the deepest compiled value is the one that
// can overflow. Every chain depth within `below` of the compiled boundary and
// `margin` past it is walked; shallower depths every `sparse` links, and the
// Original lane, which keeps its frames on the heap, every `stride` links to
// the default parser ceiling.
const below = 64;
const margin = 128;
const sparse = 16;
const stride = 1_024;
const parseDepth = 10_000;

// Chain depth counts the terminal node as one when it is a container. Empty
// children arrays give named/enum a minimum container depth of two. Their
// satisfying values can only have even depths. Union uses both leaf and empty
// branch terminals to cover odd and even depths. Record both depth units.
function containerDepth(shape, chainDepth) {
  return (
    shape.leafDepth +
    shape.linkDepth * (chainDepth - (shape.leafDepth > 0 ? 1 : 0))
  );
}
function chainLimit(shape, depth) {
  return (
    Math.floor((depth - shape.leafDepth) / shape.linkDepth) +
    (shape.leafDepth > 0 ? 1 : 0)
  );
}
function depths(shape) {
  const maximum = chainLimit(shape, parseDepth);
  const boundary = chainLimit(shape, shape.compiledDepth);
  const denseFrom = Math.max(1, boundary - below);
  const denseThrough = Math.min(maximum, boundary + margin);
  const values = [];
  for (let depth = 1; depth < denseFrom; depth += sparse) values.push(depth);
  for (let depth = denseFrom; depth <= denseThrough; depth++)
    values.push(depth);
  for (let depth = denseThrough + stride; depth < maximum; depth += stride)
    values.push(depth);
  if (values.at(-1) !== maximum) values.push(maximum);
  return { values, denseFrom, denseThrough, maximum };
}

export function compiledDepthCases(
  sdk,
  { shapes = compiledDepthShapes, beforeCase = () => {} } = {},
) {
  const started = performance.now();
  const rows = [];
  for (const shape of shapes) {
    const start = performance.now();
    const owners = [];
    let failed = false;
    let current = { shape: shape.name, operation: "prepare" };
    const own = (value) => {
      owners.push(value);
      return value;
    };
    try {
      const root = "https://qualification.invalid/root";
      const document = own(
        sdk.Document.parse(
          JSON.stringify({
            openbindings: "0.2.0",
            operations: { op: { input: { $ref: root } } },
          }),
        ).value,
      );
      const resource = own(sdk.parseJson(JSON.stringify(shape.schema)).value);
      const resources = own(new sdk.ExternalSchemas([[root, resource]]));
      const setup = prepareInput(document, "op", { resources });
      if (setup.status !== "ready")
        throw Error(`${shape.name}/prepare: ${JSON.stringify(setup)}`);
      const schema = own(setup.schema);
      const walk = depths(shape);
      const operations = Object.fromEntries(
        ["evaluate", "explain"].map((operation) => [
          operation,
          Object.fromEntries(
            ["satisfies", "fails"].map((expected) => [
              expected,
              { verdicts: 0, deepestVerdict: null, noVerdicts: {} },
            ]),
          ),
        ]),
      );
      const terminals = shape.alternate
        ? [shape, { ...shape, ...shape.alternate }]
        : [shape];
      for (const chainDepth of walk.values) {
        for (const terminal of terminals) {
          const depth = containerDepth(terminal, chainDepth);
          const links = chainDepth - (shape.leafDepth > 0 ? 1 : 0);
          for (const [index, expected] of ["satisfies", "fails"].entries()) {
            const context = { shape: shape.name, chainDepth, depth, expected };
            current = { ...context, operation: "parse" };
            beforeCase({ ...context, operation: "parse" });
            const parsed = sdk.parseJson(
              shape.open.repeat(links) +
                terminal.leaves[index] +
                shape.close.repeat(links),
            );
            if (parsed.status !== "parsed")
              throw Error(
                `${JSON.stringify(context)}/parse: ${JSON.stringify(parsed)}`,
              );
            try {
              for (const operation of ["evaluate", "explain"]) {
                current = { ...context, operation };
                beforeCase({ ...context, operation });
                const result = schema[operation](parsed.value);
                const stats = operations[operation][expected];
                if (result.status === expected) {
                  stats.verdicts++;
                  stats.deepestVerdict = { chainDepth, depth };
                } else if (
                  result.status === "no-verdict" &&
                  result.detail.reason === "limit-exceeded"
                ) {
                  const code = result.detail.code;
                  const observed = (stats.noVerdicts[code] ??= {
                    count: 0,
                    firstDepth: depth,
                    lastDepth: depth,
                  });
                  observed.count++;
                  observed.lastDepth = depth;
                } else {
                  throw Error(
                    `${JSON.stringify({ ...context, operation })}: ${JSON.stringify(result)}`,
                  );
                }
              }
            } catch (error) {
              failed = true;
              throw error;
            } finally {
              // A trap can leave Rust borrows and the Wasm stack pointer live.
              // Do not mask that original error by re-entering a broken instance.
              if (!failed) parsed.value.dispose();
            }
          }
        }
      }
      rows.push({
        shape: shape.name,
        compiledDepth: shape.compiledDepth,
        compiledChainDepth: chainLimit(shape, shape.compiledDepth),
        denseFromChainDepth: walk.denseFrom,
        denseThroughChainDepth: walk.denseThrough,
        maximumDepth: Math.max(
          ...terminals.map((terminal) =>
            containerDepth(terminal, walk.maximum),
          ),
        ),
        sampledDepths: walk.values.length * terminals.length,
        operations,
        milliseconds: performance.now() - start,
      });
    } catch (error) {
      failed = true;
      throw new Error(`${JSON.stringify(current)}: ${String(error)}`, {
        cause: error,
      });
    } finally {
      if (!failed) for (const owner of owners.reverse()) owner.dispose();
    }
  }
  return {
    below,
    margin,
    sparse,
    stride,
    parseDepth,
    rows,
    milliseconds: performance.now() - started,
  };
}
