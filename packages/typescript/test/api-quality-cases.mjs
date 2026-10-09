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
    const r = sdk.parseDocument(text);
    check(r.status === "parsed", "document admission");
    return own(r.value);
  };
  const exact = (text) => {
    const r = sdk.parseJson(text);
    check(r.status === "parsed", "exact admission");
    return own(r.value);
  };
  const ready = (context, name, side = "input") => {
    const r = context.prepare(name, side);
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
  const warm = sdk.parseDocument(
    '{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"integer"}}}}',
  );
  warm.value.assess();
  const warmContext = warm.value.contracts();
  const warmPrepared = warmContext.prepare("run", "input");
  if (warmPrepared.status !== "ready")
    throw new Error("Warmup did not prepare.");
  warmPrepared.contract.validate(1);
  warmPrepared.contract.dispose();
  warmContext.dispose();
  warm.value.dispose();
  const baseline = sdk.liveStorageOwners();
  try {
    check(
      !("discover" in sdk) && !("DiscoveryPublication" in sdk),
      "root HTTP removals",
    );
    check(
      !("fromParsed" in sdk.ValidatedDocument),
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
      const raw = own(invalid.value);
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
    const context = own(document.contracts());
    check(
      context.prepare("empty", "input").status === "no-contract",
      "absent contract setup",
    );
    check(
      context.prepare("missing", "input").status === "operation-missing",
      "missing operation setup",
    );
    const ambContext = own(ambiguous.contracts());
    check(
      ambContext.prepare("x", "input").status === "operation-ambiguous",
      "ambiguous setup",
    );
    const remote = context.prepare("remote", "input");
    check(
      remote.status === "no-verdict" &&
        remote.detail.reason === "resource-unavailable",
      "unsupplied resource setup",
    );
    const cancelled = new AbortController();
    cancelled.abort();
    const beforeCancel = sdk.liveStorageOwners();
    const cancelledSetup = context.prepare("run", "input", {
      signal: cancelled.signal,
    });
    check(
      cancelledSetup.status === "no-verdict" &&
        cancelledSetup.detail.reason === "cancelled",
      "setup cancellation",
    );
    check(
      sdk.liveStorageOwners() === beforeCancel,
      "no ready owner on cancellation",
    );
    const input = ready(context, "run");
    const never = ready(context, "never");
    check(
      never.validate(null).outcome === "mismatch",
      "false is present contract",
    );
    const nullDoc = parsed(
      '{"openbindings":"0.2.0","operations":{"run":{"input":null}}}',
    );
    let nullContext;
    try {
      nullContext = own(nullDoc.contracts());
    } catch (e) {
      check(e.code === "interpretation", "invalid schema construction refuses");
    }
    if (nullContext) {
      const r = nullContext.prepare("run", "input");
      check(r.status === "no-verdict", "null schema is not absence");
    }
    check(input.validate(7).outcome === "satisfies", "ordinary validation");
    check(
      input.validate("7").outcome === "mismatch",
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
    const object = ready(context, "object");
    const beforeValue = sdk.liveStorageOwners();
    check(
      object.validate({ n: large }).outcome === "satisfies",
      "nested exact token",
    );
    check(large.text === "9007199254740993", "nested owner remains");
    check(
      object.validate({ n: 9007199254740992 }).outcome === "mismatch",
      "rounded ordinary differs",
    );
    check(
      object.validate({ n: large, bad: undefined }).outcome === "input-error",
      "partial ordinary refusal",
    );
    check(
      sdk.liveStorageOwners() === beforeValue,
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
    check(sdk.liveStorageOwners() === beforeValue, "nested refusal cleanup");
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
      sdk.liveStorageOwners() === beforeValue,
      "post admission cancellation cleanup",
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
    const first = own(document.contracts({ resources }));
    const second = own(document.contracts({ resources: others }));
    const a = ready(first, "remote"),
      b = ready(second, "remote");
    check(
      a.validate(7).outcome === "satisfies" &&
        b.validate(7).outcome === "mismatch",
      "same URI isolation",
    );
    const beforeBatch = sdk.liveStorageOwners();
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
      check(sdk.liveStorageOwners() === beforeBatch, "atomic resource cleanup");
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
    check(sdk.liveStorageOwners() === beforeBatch, "iterator cleanup");
    own(new sdk.SchemaResources([["https://schema.test/healthy", intSchema]]));
    const checked = document.validate();
    check(checked.status === "validated", "proof");
    const proof = own(checked.document);
    const publication = own(new http.DiscoveryPublication(proof));
    proof.dispose();
    document.dispose();
    context.dispose();
    resources.dispose();
    check(operation.key === "run", "retained selected owner");
    const child = own(operation.value);
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
      sdk.liveStorageOwners() === baseline,
      `complete owner cleanup: ${sdk.liveStorageOwners()} vs ${baseline}`,
    );
  }
}
