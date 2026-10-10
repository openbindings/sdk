// Public package jobs; all durations are watchdogs, never benchmark samples.
const eq = (a, b, message = 'equality') => { if (a !== b) throw Error(`${message}: ${a} != ${b}`); };
const ok = (value, message) => { if (!value) throw Error(message); };
const hash = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), x => x.toString(16).padStart(2, '0')).join('');
const yieldTasks = async () => { await new Promise(r => setTimeout(r, 0)); await new Promise(r => setTimeout(r, 0)); };
const acknowledgments = new Map();
const memories = new Set();
let sdk, sequence = 0;
async function mark(phase, extra = {}) {
  const row = { phase, storageOwners: sdk?.liveStorageOwners() ?? null,
    wasmCapacityBytes: [...memories].reduce((n, m) => n + m.buffer.byteLength, 0), ...extra };
  const id = ++sequence;
  const acknowledged = new Promise(resolve => acknowledgments.set(id, resolve));
  postMessage({ type: 'phase', id, row });
  await acknowledged;
}
async function fetchChecked(path, expected) {
  const response = await fetch(path); ok(response.ok, 'asset fetch failed');
  const bytes = await response.arrayBuffer(); eq(await hash(bytes), expected, 'asset identity');
  return bytes;
}
function exercise(contract, value) {
  eq(contract.validate(value).outcome, 'satisfies');
  eq(contract.validate(1 - value).outcome, 'fails');
  const signal = new AbortController(); signal.abort();
  const aborted = contract.validate(value, { signal: signal.signal });
  eq(aborted.outcome, 'no-verdict'); eq(aborted.detail.reason, 'cancelled');
  eq(contract.validate(value).outcome, 'satisfies');
}
function complete(source, facts) {
  const owners = [], own = value => (owners.push(value), value);
  try {
    const parsed = sdk.parseDocument(source); eq(parsed.status, 'parsed');
    const doc = own(parsed.value), assessment = doc.assess();
    eq(assessment.status, 'assessed'); eq(assessment.report.conclusion, 'conformant');
    eq(doc.operations.length, facts.operations, 'operation cardinality');
    const context = own(doc.contracts());
    const prepared = context.prepare(facts.selectedOperation, facts.selectedSide); eq(prepared.status, 'ready');
    const contract = own(prepared.contract);
    const valid = sdk.parseJson(facts.validText), invalid = sdk.parseJson(facts.invalidText);
    eq(valid.status, 'parsed'); const yes = own(valid.value);
    eq(invalid.status, 'parsed'); const no = own(invalid.value);
    const accepted = contract.validate(yes), rejected = contract.validate(no);
    eq(accepted.outcome, 'satisfies'); eq(rejected.outcome, 'fails');
    ok(rejected.problems.length > 0, 'invalid diagnostics missing');
    const serialized = JSON.stringify(rejected);
    return { assessment: assessment.report.conclusion, operations: doc.operations.length,
      valid: accepted.outcome, invalid: rejected.outcome, diagnosticBytes: new TextEncoder().encode(serialized).length,
      problems: rejected.problems.length, problemsComplete: rejected.problemsComplete };
  } finally { for (const owner of owners.reverse()) owner.dispose(); }
}
function prepareReplacement(source, value) {
  const owners = [], own = value => (owners.push(value), value);
  try {
    const parsed = sdk.parseDocument(source); eq(parsed.status, 'parsed');
    const doc = own(parsed.value); eq(doc.assess().report.conclusion, 'conformant');
    const resource = own(sdk.ExactJson.from({ const: value }));
    const resources = own(new sdk.SchemaResources([['https://example.test/value', resource]]));
    const context = own(doc.contracts({ resources })); // shipped default cache
    const signal = new AbortController(); signal.abort();
    const before = sdk.liveStorageOwners();
    const aborted = context.prepare('check', 'input', { signal: signal.signal });
    eq(aborted.status, 'no-verdict'); eq(aborted.detail.reason, 'cancelled'); eq(sdk.liveStorageOwners(), before);
    const prepared = context.prepare('check', 'input'); eq(prepared.status, 'ready');
    try {
      for (let n = 0; n < 6; n++) {
        const other = context.prepare('op' + n, 'input'); eq(other.status, 'ready');
        try { eq(other.contract.validate(7).outcome, 'satisfies'); } finally { other.contract.dispose(); }
      }
      exercise(prepared.contract, value);
      return prepared.contract;
    } catch (error) { prepared.contract.dispose(); throw error; }
  } finally { for (const owner of owners.reverse()) owner.dispose(); }
}
async function run(job) {
  const report = { status: 'failed', forcedGc: false, timed: false, sdkLoaded: false,
    liveHeapClaim: false, platform: navigator.platform, userAgent: navigator.userAgent };
  try {
    eq(typeof globalThis.gc, 'undefined', 'forced GC unavailable');
    if (job.mode === 'control') {
      await yieldTasks(); await mark('idle', { payloadBytes: 0 });
      let buffer = new ArrayBuffer(64 * 1024 * 1024), view = new Uint8Array(buffer);
      view.fill(17);
      let checksum = 0; for (let n = 0; n < view.length; n += 4096) checksum += view[n];
      eq(checksum, 278528);
      await mark('held-64MiB', { payloadBytes: buffer.byteLength, checksum });
      view = null; buffer = null; await yieldTasks();
      await mark('dropped-references', { payloadBytes: 0, physicalCollectionObserved: false });
    } else {
      const asset = await fetchChecked('/sdk/dist/wasm/openbindings_wasm_bg.wasm', job.wasmSha256);
      const original = WebAssembly.instantiate;
      report.instantiations = [];
      WebAssembly.instantiate = async function(...args) {
        const result = await Reflect.apply(original, WebAssembly, args);
        const instance = result instanceof WebAssembly.Instance ? result : result.instance;
        const input = args[0];
        const bytes = input instanceof ArrayBuffer ? input : ArrayBuffer.isView(input) ? input.buffer.slice(input.byteOffset, input.byteOffset + input.byteLength) : null;
        const digest = bytes ? await hash(bytes) : null;
        report.instantiations.push({ sha256: digest, selected: digest === job.wasmSha256 });
        if (digest === job.wasmSha256) for (const item of Object.values(instance.exports))
          if (item instanceof WebAssembly.Memory) memories.add(item);
        return result;
      };
      try { sdk = await import('/sdk/dist/index.js'); await sdk.initialize(new Uint8Array(asset)); }
      finally { WebAssembly.instantiate = original; }
      eq(memories.size, 1, 'exactly one attributed SDK Wasm memory');
      report.sdkLoaded = true;
      const source = new TextDecoder().decode(await fetchChecked('/fixture', job.fixtureSha256));
      if (job.mode === 'ordinary') {
        complete(source, job.facts); complete(source, job.facts);
        const baseline = sdk.liveStorageOwners(); report.fixedWarmedOwners = baseline;
        await yieldTasks(); await mark('warm-released');
        report.semantic = complete(source, job.facts);
        eq(sdk.liveStorageOwners(), baseline, 'complete job owners');
        await yieldTasks(); await mark('all-released');
      } else {
        for (let n = 0; n < 20; n++) {
          const p = prepareReplacement(source, n % 2); try { exercise(p, n % 2); } finally { p.dispose(); }
        }
        const baseline = sdk.liveStorageOwners(); report.fixedWarmedOwners = baseline;
        await yieldTasks(); await mark('warm-released');
        let retained = prepareReplacement(source, 0);
        const retainedOwners = sdk.liveStorageOwners();
        try {
          for (let n = 0; n < 1000; n++) eq(retained.validate(0).outcome, 'satisfies');
          eq(sdk.liveStorageOwners(), retainedOwners); await mark('1000-retained-calls');
          for (let n = 1; n <= 400; n++) {
            const current = prepareReplacement(source, n % 2);
            try { exercise(current, n % 2); exercise(retained, 0); } finally { current.dispose(); }
            eq(sdk.liveStorageOwners(), retainedOwners, 'retained-owner independence');
            if (n === 200 || n === 400) {
              retained.dispose(); retained = null;
              eq(sdk.liveStorageOwners(), baseline, 'fully released checkpoint');
              await yieldTasks(); await mark('released-' + n);
              if (n === 200) { retained = prepareReplacement(source, 0); eq(sdk.liveStorageOwners(), retainedOwners); }
            }
          }
        } finally { retained?.dispose(); }
        eq(sdk.liveStorageOwners(), baseline); await yieldTasks(); await mark('all-released');
        report.retainedCalls = 1000; report.replacements = 400;
        report.retentionEpochs = 'original const0 retained through replacements1..200; all handles released; equal const0 retained through201..400';
      }
    }
    report.status = 'passed';
  } catch (error) { report.error = { name: error.name, message: error.message, stack: error.stack }; }
  postMessage({ type: 'result', report });
}
self.onmessage = ({ data }) => {
  if (data.ack) { acknowledgments.get(data.ack)?.(); acknowledgments.delete(data.ack); }
  else if (data.run) run(data.run);
};
