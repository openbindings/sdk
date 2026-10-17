import * as testing from "../dist/testing.js";
import { prepareInput } from "./conformant-contracts.mjs";
/** Public-API checks shared by repository and freshly installed host consumers. */
export async function apiQualityCases(sdk, http) {
  let assertions = 0;
  const check = (value, message) => {
    assertions++;
    if (!value) throw new Error(message);
  };
  const owners = [];
  const own = (value) => {
    owners.push(value);
    return value;
  };
  const parsed = (text) => {
    const r = sdk.Document.parse(text);
    check(r.status === "parsed", "document admission");
    return own(r.value);
  };
  const exact = (text) => {
    const r = sdk.parseJson(text);
    check(r.status === "parsed", "exact admission");
    return own(r.value);
  };
  const ready = (proof, name, options) => {
    const r = prepareInput(proof, name, options);
    check(r.status === "ready", "ready " + name);
    return own(r.contract);
  };
  const throws = (fn, predicate, label) => {
    let error;
    try {
      fn();
    } catch (e) {
      error = e;
    }
    check(error && predicate(error), label);
  };
  // Warm instance-lifetime meta-schema state before checking live storage deltas.
  const warm = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"integer"}}}}',
  );
  warm.value.validate().document?.dispose();
  const warmPrepared = prepareInput(warm.value, "run");
  if (warmPrepared.status !== "ready")
    throw new Error("Warmup did not prepare.");
  warmPrepared.contract.validate(1);
  warmPrepared.contract.dispose();
  warm.value.dispose();
  const baseline = testing.liveStorageOwners();
  try {
    // OBI-01's five new cases plus CONFORMANCE-06, also run after npm packing.
    for (const [token, member, conforms] of [
      [String.raw`"\ud800"`, false, false],
      [String.raw`"\udc00"`, true, false],
      [String.raw`"\ude00\ud83d"`, false, false],
      [String.raw`"\ud83d\ude00"`, false, true],
      [String.raw`"\uffff"`, false, true],
      [String.raw`"\ud800"`, false, false],
    ]) {
      const text = member
        ? `{"openbindings":"0.2.0","operations":{"op":{"input":{"type":"object","properties":{${token}:{}}}}}}`
        : `{"openbindings":"0.2.0","description":${token},"operations":{}}`;
      const document = parsed(text);
      const checked = document.validate();
      if (checked.status === "conformant") own(checked.document);
      check(
        (checked.status === "conformant") === conforms,
        "surrogate proof boundary",
      );
      check(
        new TextDecoder().decode(document.toBytes()) === text,
        "exact draft bytes",
      );
      for (const validation of [checked, sdk.validateDocument(text)]) {
        check(validation.status !== "version-refused", "supported version");
        const report = validation.report;
        check(
          report.conclusion === (conforms ? "conformant" : "non-conformant"),
          "surrogate conclusion",
        );
        check(
          report.findings.length === (conforms ? 0 : 1),
          "surrogate findings count",
        );
        for (const [rule, evidence] of Object.entries(report.evidence))
          check(
            evidence ===
              (conforms
                ? "met"
                : rule === "OBI-01"
                  ? "violated"
                  : "not-applicable"),
            "rule prerequisites",
          );
        if (!conforms) {
          const finding = report.findings[0];
          check(
            finding.rule === "OBI-01" && finding.code === "unpaired-surrogate",
            "surrogate rule and code",
          );
          check(
            finding.location.pointer === (member ? null : "/description"),
            "surrogate pointer",
          );
          check(
            finding.location.byteOffset === text.indexOf(token),
            "surrogate token offset",
          );
          check(
            finding.location.line === 1 &&
              finding.location.byteColumn === text.indexOf(token) + 1,
            "surrogate line and column",
          );
        }
      }
    }
    check(
      !("discover" in sdk) && !("DiscoveryPublication" in sdk),
      "root HTTP removals",
    );
    check(
      !("fromParsed" in sdk.ConformantDocument),
      "proof constructor removal",
    );
    const document = parsed(
      '{"openbindings":"0.2.0","operations":{"run":{"aliases":["execute"],"input":{"type":"integer"}},"empty":{},"never":{"input":false},"object":{"input":{"const":{"n":9007199254740993}}},"remote":{"input":{"$ref":"https://schema.test/id"}}}}',
    );
    check(document.operations === document.operations, "metadata cache");
    throws(
      () => {
        document.operations[0].key = "edit";
      },
      (e) => e instanceof TypeError,
      "runtime readonly",
    );
    const selected = document.resolveOperation("execute");
    check(selected.status === "found", "found selection");
    const operation = own(selected.operation);
    check(
      document.resolveOperation("missing").status === "missing",
      "missing selection",
    );
    const ambiguous = parsed(
      '{"openbindings":"0.2.0","operations":{"z":{"aliases":["x"]},"a":{"aliases":["x"]}}}',
    );
    const ambiguity = ambiguous.resolveOperation("x");
    check(
      ambiguity.status === "ambiguous" &&
        ambiguity.candidates.join(",") === "a,z",
      "lexical ambiguous keys",
    );
    check(Object.isFrozen(ambiguity.candidates), "immutable candidates");
    const duplicate = parsed(
      '{"openbindings":"0.2.0","operations":{"a":{"aliases":["x","x"]}}}',
    );
    const repeated = duplicate.resolveOperation("x");
    check(
      repeated.status === "ambiguous" && repeated.candidates.join(",") === "a",
      "same operation repetition unresolved",
    );
    for (const text of [
      '{"openbindings":"0.2.0"}',
      '{"openbindings":"0.2.0","operations":[]}',
      '{"openbindings":"0.2.0","operations":{"run":7}}',
    ]) {
      const invalid = parsed(text);
      throws(
        () => invalid.operations,
        (e) =>
          e instanceof sdk.SdkError &&
          e.code === "interpretation" &&
          e.location &&
          Number.isInteger(e.location.byteOffset),
        "located structure refusal",
      );
      const raw = own(invalid.value());
      check(raw.text === text, "raw invalid evidence preserved");
    }
    const aliases = parsed(
      '{"openbindings":"0.2.0","operations":{"run":{"aliases":7}}}',
    );
    throws(
      () => aliases.resolveOperation("run"),
      (e) =>
        e.code === "interpretation" &&
        e.location?.pointer === "/operations/run/aliases",
      "malformed alias primary lookup",
    );
    const validated = document.validate();
    check(validated.status === "conformant", "document proof");
    const conformant = own(validated.document);
    check(
      own(conformant.resolveOperation("empty")).input() === undefined,
      "absent contract setup",
    );
    check(
      conformant.resolveOperation("missing") === undefined,
      "missing operation setup",
    );
    check(
      ambiguous.validate().status === "report",
      "ambiguous documents cannot construct contracts",
    );
    const remote = prepareInput(conformant, "remote");
    check(
      remote.status === "ready",
      "qualified missing-resource contract prepares ready",
    );
    own(remote.contract);
    const remoteResult = remote.contract.validate(null);
    check(
      remoteResult.outcome === "no-verdict" &&
        remoteResult.detail.reason === "resource-unavailable",
      "bare missing resource remains undecidable at validation",
    );
    const cancelled = new AbortController();
    cancelled.abort();
    const runContract = own(own(conformant.resolveOperation("run")).input());
    const beforeCancel = testing.liveStorageOwners();
    const cancelledSetup = runContract.prepare({ signal: cancelled.signal });
    check(
      cancelledSetup.status === "no-verdict" &&
        cancelledSetup.detail.reason === "cancelled",
      "setup cancellation",
    );
    check(
      testing.liveStorageOwners() === beforeCancel,
      "no ready owner on cancellation",
    );
    const input = ready(conformant, "run");
    const never = ready(conformant, "never");
    check(
      never.validate(null).outcome === "fails",
      "false is present contract",
    );
    const nullDoc = parsed(
      '{"openbindings":"0.2.0","operations":{"run":{"input":null}}}',
    );
    check(
      nullDoc.validate().status === "report",
      "null schema rejects the document; it is not absence",
    );
    check(input.validate(7).outcome === "satisfies", "ordinary validation");
    check(
      input.validate("7").outcome === "fails",
      "ordinary string not JSON text",
    );
    check(
      input.validate(NaN).error.code === "non-finite-number",
      "ordinary admission distinct",
    );
    check(
      input.validate({ "a/~": undefined }).error.instancePointer === "/a~1~0",
      "ordinary escaped pointer",
    );
    const getter = {
      get secret() {
        throw new Error("getter must not execute");
      },
    };
    check(
      input.validate(getter).error.code === "accessor-property",
      "getter not invoked",
    );
    const cycle = {};
    cycle.loop = cycle;
    check(
      input.validate(cycle).error.instancePointer === "/loop",
      "cycle edge pointer",
    );
    check(input.validate([,]).error.code === "sparse-array", "sparse refusal");
    for (const value of ["\ud800", { v: "\ud800" }, { "\ud800": 1 }])
      check(
        input.validate(value).outcome !== "input-error",
        "escaped surrogate admission preserved",
      );
    const trap = new Error("caller trap");
    const proxy = new Proxy(
      {},
      {
        getPrototypeOf() {
          throw trap;
        },
      },
    );
    check(
      input.validate(proxy, { signal: cancelled.signal }).detail.reason ===
        "cancelled",
      "cancel before ordinary traversal",
    );
    throws(
      () => input.validate(proxy),
      (e) => e === trap,
      "caller exception retained",
    );
    check(
      input.validate(NaN, { signal: cancelled.signal }).detail.reason ===
        "cancelled",
      "cancellation precedes admission",
    );
    throws(
      () => input.validate(7, { signal: { aborted: true } }),
      (e) => e.code === "invalid-work-options",
      "options validated",
    );
    const large = exact("9007199254740993");
    const object = ready(conformant, "object");
    const beforeValue = testing.liveStorageOwners();
    check(
      object.validate({ n: large }).outcome === "satisfies",
      "nested exact token",
    );
    check(large.text === "9007199254740993", "nested owner remains");
    check(
      object.validate({ n: 9007199254740992 }).outcome === "fails",
      "rounded ordinary differs",
    );
    check(
      object.validate({ n: large, bad: undefined }).outcome === "input-error",
      "partial ordinary refusal",
    );
    check(
      testing.liveStorageOwners() === beforeValue,
      "ordinary temporary cleanup",
    );
    const dead = exact("7");
    dead.dispose();
    throws(
      () => input.validate(dead, { signal: cancelled.signal }),
      (e) => e.code === "disposed-handle",
      "root misuse before cancellation",
    );
    throws(
      () => object.validate({ n: dead }),
      (e) => e.code === "disposed-handle",
      "nested misuse",
    );
    check(
      testing.liveStorageOwners() === beforeValue,
      "nested refusal cleanup",
    );
    const during = new AbortController();
    const duringProxy = new Proxy(
      { n: large },
      {
        ownKeys(target) {
          during.abort();
          return Reflect.ownKeys(target);
        },
      },
    );
    check(
      object.validate(duringProxy, { signal: during.signal }).detail.reason ===
        "cancelled",
      "post admission control check",
    );
    check(
      testing.liveStorageOwners() === beforeValue,
      "post admission cancellation cleanup",
    );
    // The bridge's actual pinned input-error kind is "Limit" (capital L).
    // A large exact fragment is admitted by itself; embedding it can exceed the
    // aggregate parse limit even though the ordinary encoder borrows its tokens.
    const deep = exact("[".repeat(10000) + "0" + "]".repeat(10000));
    const beforeDepthRefusal = testing.liveStorageOwners();
    const depthRefusal = input.validate({ nested: deep });
    check(
      depthRefusal.outcome === "input-error",
      "real aggregate admission limit",
    );
    check(
      depthRefusal.error.code === "input-limit" &&
        depthRefusal.error.instancePointer === null,
      "aggregate limit truthful unknown pointer",
    );
    const editing = sdk.Document.create();
    let editFailure;
    try {
      editing.set("/x-deep", deep);
    } catch (error) {
      editFailure = error;
    } finally {
      editing.dispose();
    }
    check(
      editFailure instanceof sdk.EditError &&
        editFailure.code === "depth-limit" &&
        editFailure.partPointer === null,
      "real encoded edit limit",
    );
    check(
      deep.text.length === 20001,
      "supplied exact owner survives aggregate limit",
    );
    check(
      testing.liveStorageOwners() === beforeDepthRefusal,
      "aggregate limit cleanup",
    );
    check(input.validate(8).outcome === "satisfies", "healthy reuse");
    const intSchema = exact('{"type":"integer"}');
    const stringSchema = exact('{"type":"string"}');
    const resources = own(
      new sdk.SchemaResources([["https://schema.test/id", intSchema]]),
    );
    const others = own(
      new sdk.SchemaResources([["https://schema.test/id", stringSchema]]),
    );
    const a = ready(conformant, "remote", { resources }),
      b = ready(conformant, "remote", { resources: others });
    check(
      a.validate(7).outcome === "satisfies" &&
        b.validate(7).outcome === "fails",
      "same URI isolation",
    );
    const beforeBatch = testing.liveStorageOwners();
    for (const entries of [
      [
        ["https://schema.test/id", intSchema],
        ["https://schema.test/id#", intSchema],
      ],
      [
        ["https://schema.test/id", intSchema],
        ["relative", intSchema],
      ],
      [
        ["https://schema.test/id", intSchema],
        ["https://schema.test/other", dead],
      ],
    ]) {
      throws(
        () => new sdk.SchemaResources(entries),
        () => true,
        "batch refusal",
      );
      check(
        testing.liveStorageOwners() === beforeBatch,
        "atomic resource cleanup",
      );
      check(intSchema.text === '{"type":"integer"}', "input schema survives");
    }
    function* throwing() {
      yield ["https://schema.test/id", intSchema];
      throw trap;
    }
    throws(
      () => new sdk.SchemaResources(throwing()),
      (e) => e === trap,
      "iterator exception preserved",
    );
    check(testing.liveStorageOwners() === beforeBatch, "iterator cleanup");
    own(new sdk.SchemaResources([["https://schema.test/healthy", intSchema]]));
    const checked = document.validate();
    check(checked.status === "conformant", "proof");
    const proof = own(checked.document);
    const publication = own(new http.DiscoveryPublication(proof));
    proof.dispose();
    document.dispose();
    conformant.dispose();
    runContract.dispose();
    resources.dispose();
    check(operation.key === "run", "retained selected owner");
    const child = own(operation.value());
    operation.dispose();
    check(child.text.includes("execute"), "retained exact child");
    check(a.validate(7).outcome === "satisfies", "retained ready resource");
    const held = own(input.retain());
    input.dispose();
    await Promise.resolve();
    check(held.validate(9).outcome === "satisfies", "retain across await");
    throws(
      () => input.validate(7, { signal: cancelled.signal }),
      (e) => e.code === "disposed-handle",
      "disposed prepared before cancellation",
    );
    const response = publication.respond(
      new Request("https://host.test/.well-known/openbindings"),
    );
    check(response.status === 200, "shared realm publication");
    check(
      (await response.text()).includes("9007199254740993"),
      "copied publication exact",
    );
    const notConformant = parsed(
      '{"openbindings":"0.2.0","operations":{},"unknown":1}',
    );
    throws(
      () => new http.DiscoveryPublication(notConformant),
      () => true,
      "runtime forged proof refused",
    );
    return { assertions, baseline };
  } finally {
    for (const owner of owners.reverse()) owner.dispose();
    check(
      testing.liveStorageOwners() === baseline,
      `complete owner cleanup: ${testing.liveStorageOwners()} vs ${baseline}`,
    );
  }
}
