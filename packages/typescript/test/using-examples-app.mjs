import * as sdk from "@openbindings/sdk";

function check(condition, message) {
  if (!condition) throw new Error(message);
}

check(typeof Symbol.dispose === "symbol", "runtime disposal symbol missing");
check(
  globalThis.originalDispose === undefined ||
    globalThis.originalDispose === Symbol.dispose,
  "bootstrap replaced a native symbol",
);
await sdk.initialize();
const coldOwners = sdk.liveStorageOwners();
// Assessment/preparation retain instance-lifetime meta-schema state. Establish
// that fixed baseline before checking whether the examples release their owners.
const warm = sdk.parseDocument(
  '{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"integer"}}}}',
);
check(warm.status === "parsed", "warm document");
try {
  warm.value.assess();
  const context = warm.value.contracts();
  try {
    const setup = context.prepare("run", "input");
    check(setup.status === "ready", "warm contract");
    try {
      setup.contract.validate(1);
    } finally {
      setup.contract.dispose();
    }
  } finally {
    context.dispose();
  }
} finally {
  warm.value.dispose();
}
const baseline = sdk.liveStorageOwners();
const results = [];
const log = console.log;
try {
  console.log = (value) => results.push(value);
  await import("./first-use.js");
} finally {
  console.log = log;
}
check(results.length === 2, "README must produce both results");
check(results[0].result.outcome === "satisfies", "README passing value");
check(results[1].result.outcome === "fails", "README failing value");
check(sdk.liveStorageOwners() === baseline, "README owners not released");

const example = await import("./inspect-edit.js");
const text = String.raw`{"openbindings":"0.2.0","operations":{},"x-editor":{
  "annotation":null,"large":900719925474099312345,"tiny":1e-1000,
  "negative":-0,"é":1e500,"e\u0301":2,"__proto__":{"value":3.00}}}`;
for (let cycle = 0; cycle < 25; cycle++) {
  const changed = example.editExtensionMember(
    text,
    "x-editor",
    "annotation",
    "Reviewed",
  );
  check(changed.status === "edited", "edit did not produce bytes");
  check(changed.report.conclusion === "conformant", "edit conformance");
  const parsed = sdk.parseDocument(changed.bytes);
  check(parsed.status === "parsed", "reopening edited document");
  const root = parsed.value.value;
  try {
    for (const [key, token] of [
      ["large", "900719925474099312345"],
      ["tiny", "1e-1000"],
      ["negative", "-0"],
      ["é", "1e500"],
      ["e\u0301", "2"],
      ["__proto__/value", "3.00"],
      ["annotation", '"Reviewed"'],
    ]) {
      const leaf = root.at(`/x-editor/${key}`);
      try {
        check(leaf?.text === token, `exact token changed: ${key}`);
      } finally {
        leaf?.dispose();
      }
    }
  } finally {
    root.dispose();
    parsed.value.dispose();
  }
  for (const original of ['{"other":0}', '{"x":0,"nested":{"a":1,"a":2}}']) {
    const parsed = sdk.parseJson(original);
    check(parsed.status === "parsed", "rejection input");
    try {
      let rejected = false;
      try {
        example.replaceExactObjectMember(parsed.value, "x", true);
      } catch (error) {
        rejected = error instanceof TypeError;
      }
      check(rejected, "recipe must reject invalid edit");
      check(parsed.value.text === original, "rejection changed original");
    } finally {
      parsed.value.dispose();
    }
  }
  check(
    sdk.liveStorageOwners() === baseline,
    `owners retained after cycle ${cycle}`,
  );
}
globalThis.usingResult = {
  status: "passed",
  readmeVerdicts: results.map((row) => row.result.outcome),
  editCycles: 25,
  coldOwners,
  liveOwnersBefore: baseline,
  liveOwnersAfter: sdk.liveStorageOwners(),
  nativeDisposePreserved: globalThis.originalDispose === Symbol.dispose,
  suppliedDisposeSymbol: globalThis.originalDispose === undefined,
};
