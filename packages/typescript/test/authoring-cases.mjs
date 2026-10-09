/** Public owner regression vectors, usable unchanged in Node/browser/workerd. */
export async function runCases(sdk, ids, http) {
  const rows = [];
  for (const id of ids) {
    const assertions = [];
    const equal = (label, actual, expected) => {
      assertions.push({ label, actual, expected });
      if (JSON.stringify(actual) !== JSON.stringify(expected))
        throw Error(label);
    };
    const bad = (label, draft, code, draftPointer) => {
      const result = sdk.authorDocument(draft);
      equal(label + ": status", result.status, "authoring-error");
      equal(label + ": code", result.error.code, code);
      equal(label + ": draft pointer", result.error.draftPointer, draftPointer);
      equal(label + ": no byte location", "byteOffset" in result.error, false);
      equal(label + ": explained", result.error.message.length > 0, true);
      return result.error;
    };
    const good = (draft = { operations: { run: {} } }) => {
      const result = sdk.authorDocument(draft);
      equal("author success", result.status, "authored");
      return result.document;
    };
    const content = (value) => ({
      operations: { run: {} },
      sources: { s: { kind: "test", content: value } },
    });
    const field = (document, pointer, text) => {
      const root = document.value;
      const child = root.at(pointer);
      try {
        equal(pointer, child?.text, text);
      } finally {
        child?.dispose();
        root.dispose();
      }
    };
    try {
      switch (id) {
        case "A01":
          bad(
            "root collision",
            { operations: { run: {} }, additionalFields: { operations: {} } },
            "field-collision",
            "/additionalFields/operations",
          );
          break;
        case "A02": {
          for (const [map, object, collision] of [
            ["operations", {}, "input"],
            ["sources", { kind: "test" }, "kind"],
            ["bindings", { operation: "run", source: "s" }, "source"],
            ["dependencies", { operation: "run" }, "operation"],
          ])
            bad(
              map,
              {
                operations: { run: {} },
                [map]: {
                  "a/~b": {
                    ...object,
                    additionalFields: { [collision]: null },
                  },
                },
              },
              "field-collision",
              `/${map}/a~1~0b/additionalFields/${collision}`,
            );
          bad(
            "example",
            {
              operations: {
                "a/~b": {
                  examples: { "e/~f": { additionalFields: { input: null } } },
                },
              },
            },
            "field-collision",
            "/operations/a~1~0b/examples/e~1~0f/additionalFields/input",
          );
          break;
        }
        case "A03":
          for (const [name, value] of [
            ["nan", NaN],
            ["positive", Infinity],
            ["negative", -Infinity],
          ])
            bad(
              name,
              content({ [name]: value }),
              "non-finite-number",
              `/sources/s/content/${name}`,
            );
          break;
        case "A04": {
          for (const [name, value] of [
            ["undefined", undefined],
            ["bigint", 1n],
            ["symbol", Symbol("opaque")],
            ["function", () => 1],
          ])
            bad(
              name,
              content({ [name]: value }),
              "unsupported-value",
              `/sources/s/content/${name}`,
            );
          const d = good({
            operations: { run: { description: undefined, aliases: undefined } },
            description: undefined,
          });
          field(d, "/description", undefined);
          field(d, "/operations/run/aliases", undefined);
          d.dispose();
          break;
        }
        case "A05":
          bad(
            "hole",
            content([7, , 9]),
            "sparse-array",
            "/sources/s/content/1",
          );
          break;
        case "A06": {
          const value = {};
          value.loop = value;
          bad(
            "cycle",
            content(value),
            "cyclic-value",
            "/sources/s/content/loop",
          );
          const d = good();
          const v = d.validate();
          equal("healthy after cycle", v.status, "validated");
          v.document.dispose();
          d.dispose();
          break;
        }
        case "A07": {
          let calls = 0;
          class Opaque {
            toJSON() {
              calls++;
              return {};
            }
          }
          bad(
            "date",
            content(new Date(0)),
            "non-plain-object",
            "/sources/s/content",
          );
          bad(
            "instance",
            content(new Opaque()),
            "non-plain-object",
            "/sources/s/content",
          );
          equal("implicit conversions", calls, 0);
          break;
        }
        case "A08": {
          let calls = 0;
          bad(
            "typed root getter",
            {
              get operations() {
                calls++;
                return { run: {} };
              },
            },
            "accessor-property",
            "/operations",
          );
          bad(
            "typed child getter",
            {
              operations: {
                "a/~b": {
                  get description() {
                    calls++;
                    return "secret";
                  },
                },
              },
            },
            "accessor-property",
            "/operations/a~1~0b/description",
          );
          equal("typed getter calls", calls, 0);
          break;
        }
        case "A09": {
          let calls = 0;
          bad(
            "opaque getter",
            content({
              get hidden() {
                calls++;
                return "secret";
              },
            }),
            "accessor-property",
            "/sources/s/content/hidden",
          );
          bad(
            "additional getter",
            {
              operations: { run: {} },
              additionalFields: {
                "x/~a": {
                  get hidden() {
                    calls++;
                    return "secret";
                  },
                },
              },
            },
            "accessor-property",
            "/additionalFields/x~1~0a/hidden",
          );
          equal("opaque getter calls", calls, 0);
          break;
        }
        case "A10": {
          let calls = 0;
          bad(
            "toJSON",
            content({
              toJSON() {
                calls++;
                return "replacement";
              },
            }),
            "unsupported-value",
            "/sources/s/content/toJSON",
          );
          equal("toJSON calls", calls, 0);
          break;
        }
        case "A11": {
          const exact = sdk.parseJson("9007199254740993");
          equal("exact parsed", exact.status, "parsed");
          const d = good({
            operations: {
              run: {
                description: undefined,
                input: { type: "integer" },
                output: false,
                aliases: [],
                examples: { e: { input: exact.value, output: null } },
              },
            },
            sources: { s: { kind: "test", content: null } },
            bindings: { b: { operation: "run", source: "s", content: {} } },
            dependencies: { d: { operation: "run", kinds: [] } },
            schemas: {},
            additionalFields: {
              "x-extra": { large: exact.value, empty: [], text: "" },
              future: { n: 7 },
            },
          });
          exact.value.dispose();
          field(d, "/operations/run/description", undefined);
          field(d, "/operations/run/aliases", "[]");
          field(d, "/sources/s/content", "null");
          field(d, "/bindings/b/content", "{}");
          field(d, "/schemas", "{}");
          field(d, "/operations/run/output", "false");
          field(d, "/x-extra/large", "9007199254740993");
          field(d, "/x-extra/empty", "[]");
          field(d, "/x-extra/text", '""');
          field(d, "/future/n", "7");
          field(d, "/operations/run/examples/e/output", "null");
          equal(
            "unknown retained, conformance separate",
            d.assess().report.conclusion,
            "non-conformant",
          );
          d.dispose();
          break;
        }
        case "A12": {
          const secret = "opaque-secret-never-in-diagnostic";
          const draft = {
            operations: { run: {} },
            sources: {
              s: {
                kind: "test",
                content: { secret },
                additionalFields: { kind: secret },
              },
            },
          };
          const diagnostic = bad(
            "editor",
            draft,
            "field-collision",
            "/sources/s/additionalFields/kind",
          );
          equal(
            "secret omitted",
            JSON.stringify(diagnostic).includes(secret),
            false,
          );
          const segments = diagnostic.draftPointer
            .slice(1)
            .split("/")
            .map((x) => x.replace(/~1/g, "/").replace(/~0/g, "~"));
          let parent = draft;
          for (const key of segments.slice(0, -1)) parent = parent[key];
          delete parent[segments.at(-1)];
          const d = good(draft);
          const v = d.validate();
          equal("editor retry", v.status, "validated");
          v.document.dispose();
          d.dispose();
          break;
        }
        case "P01": {
          const draft = { operations: { run: {} } };
          const d = good(draft);
          const v = d.validate();
          equal("narrow", v.status, "validated");
          const retained = v.document.retain();
          v.document.dispose();
          const expected = Array.from(retained.originalBytes);
          const publication = new http.DiscoveryPublication(retained);
          const changed = retained.originalBytes;
          changed.fill(0);
          draft.operations = {};
          retained.dispose();
          d.dispose();
          const response = publication.respond(
            new Request("https://host.test/.well-known/openbindings"),
          );
          equal("published", response.status, 200);
          equal(
            "immutable bytes",
            Array.from(new Uint8Array(await response.arrayBuffer())),
            expected,
          );
          publication.dispose();
          break;
        }
        case "P02": {
          const d = sdk.parseDocument('{"openbindings":"0.2.0"}');
          equal("unsafe route parses", d.status, "parsed");
          let code;
          try {
            new http.DiscoveryPublication(d.value).dispose();
          } catch (error) {
            equal(
              "structured runtime rejection",
              error instanceof sdk.SdkError,
              true,
            );
            code = error.code;
          } finally {
            d.value.dispose();
          }
          equal("runtime conformance", code, "publication-conformance");
          break;
        }
        case "X01": {
          bad("invalid root", null, "invalid-authoring-object", "");
          bad(
            "typed Rust refusal",
            { operations: { run: {} }, description: null },
            "invalid-draft",
            null,
          );
          const exact = sdk.ExactJson.from(7);
          exact.dispose();
          let code;
          try {
            sdk.authorDocument(content(exact));
          } catch (error) {
            equal("disposed structured", error instanceof sdk.SdkError, true);
            code = error.code;
          }
          equal("disposed remains thrown", code, "disposed-handle");
          const error = new Error("proxy sentinel");
          let caught;
          try {
            sdk.authorDocument(
              new Proxy(
                {},
                {
                  ownKeys() {
                    throw error;
                  },
                },
              ),
            );
          } catch (e) {
            caught = e;
          }
          equal("unexpected failure identity", caught === error, true);
          break;
        }
        case "X02": {
          const obj = {};
          Object.defineProperty(obj, "hidden", { value: 7 });
          bad(
            "non enumerable",
            content(obj),
            "non-enumerable-property",
            "/sources/s/content/hidden",
          );
          bad(
            "symbol key",
            content({ [Symbol("x")]: 7 }),
            "symbol-key",
            "/sources/s/content",
          );
          bad(
            "extra array property",
            content(Object.assign([1], { extra: 2 })),
            "array-property",
            "/sources/s/content",
          );
          bad(
            "duplicate field",
            {
              operations: { run: {} },
              additionalFields: { "x-extra": 1 },
              "x-extra": 2,
            },
            "duplicate-field",
            "/x-extra",
          );
          let deep = 0;
          for (let i = 0; i < 10002; i++) deep = [deep];
          const limited = sdk.authorDocument(content(deep));
          equal("depth limit status", limited.status, "authoring-error");
          equal("depth limit code", limited.error.code, "authoring-limit");
          equal(
            "depth limit draft path",
            limited.error.draftPointer.startsWith("/sources/s/content/0"),
            true,
          );
          break;
        }
      }
      rows.push({ id, status: "pass", assertions });
    } catch (error) {
      rows.push({
        id,
        status: "fail",
        assertions,
        error: {
          name: error.name,
          message: error.message,
          code: error.code ?? null,
        },
      });
    }
  }
  return rows;
}
