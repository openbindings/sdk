// Document editing preserves exact bytes and ownership through the Node Wasm API.
// node crates/openbindings-wasm/tests/document-edit.cjs /absolute/path/openbindings_wasm.js
const assert = require('node:assert/strict');
const wasm = require(require('node:path').resolve(process.argv[2]));
const decode = value => new TextDecoder().decode(value.toBytes());
const parse = text => wasm.WasmDocument.parseBytes(new TextEncoder().encode(text));
const initial = ' {"openbindings":"0.2.0","operations":{},"x-number":1e+003} ';
const document = parse(initial);
const assessment = document.validate();
const proof = assessment.takeConformant();
assert.ok(proof instanceof wasm.WasmConformantDocument);
assert.equal(assessment.takeConformant(), undefined);
assert.equal(proof.set, undefined);
assert.equal(document.valueContracts, undefined);
document.edit('[{"op":"set","pointer":"/operations/one","value":{"input":true}},{"op":"set","pointer":"/operations/two","value":{"output":false}}]');
assert.equal(decode(proof), initial);
assert.ok(decode(document).includes('"x-number":1e+003'));
assert.ok(decode(document).includes('"operations":{"one":{"input":true},"two":{"output":false}}'));
const before = decode(document);
assert.throws(() => document.edit('[{"op":"set","pointer":"/name","value":"changed"},{"op":"remove","pointer":"/absent"}]'), e => JSON.parse(e).code === 'missing-value');
assert.equal(decode(document), before);
const plain = proof.asDocument();
plain.setDescription('independent owner');
assert.equal(decode(proof), initial);
const editable = proof.intoInner();
assert.throws(() => proof.operations());
editable.setDescription('proof consumed');
assert.ok(decode(editable).includes('proof consumed'));
const duplicate = parse('{"openbindings":"0.2.0","operations":{},"x":0,"x":1}');
assert.throws(() => duplicate.setName('ambiguous'), e => JSON.parse(e).code === 'duplicate-members');
const invalid = parse('{"openbindings":"0.2.0","operations":null}');
const resources = new wasm.WasmResources();
// A JS receiver's class is never authority for a proof-requiring raw export.
assert.throws(() => wasm.WasmConformantDocument.prototype.valueContracts.call(invalid, resources, '{}', 'coarse'));
for (const handle of [document, assessment, plain, editable, duplicate, invalid, resources]) handle.free();
console.log('Wasm document editing, retained proof, consumption and raw receiver checks passed');

// OBI-01's five surrogate fixtures and CONFORMANCE-06 cross the raw Wasm API.
for (const [token, member, conforms] of [
  ['"\\ud800"', false, false],
  ['"\\udc00"', true, false],
  ['"\\ude00\\ud83d"', false, false],
  ['"\\ud83d\\ude00"', false, true],
  ['"\\uffff"', false, true],
  ['"\\ud800"', false, false],
]) {
  const text = member
    ? `{"openbindings":"0.2.0","operations":{"op":{"input":{"type":"object","properties":{${token}:{}}}}}}`
    : `{"openbindings":"0.2.0","description":${token},"operations":{}}`;
  const draft = parse(text);
  const checked = draft.validate();
  const report = JSON.parse(checked.report()).report;
  assert.equal(report.conclusion, conforms ? 'conformant' : 'non-conformant');
  assert.equal(decode(draft), text);
  const proof = checked.takeConformant();
  if (conforms) {
    assert.ok(proof);
    assert.deepEqual(report.findings, []);
    proof.free();
  } else {
    assert.equal(proof, undefined);
    assert.equal(report.evidence['OBI-01'], 'violated');
    assert.ok(Object.entries(report.evidence).every(([rule, state]) => rule === 'OBI-01' || state === 'not-applicable'));
    assert.equal(report.findings.length, 1);
    const finding = report.findings[0];
    assert.equal(finding.rule, 'OBI-01');
    assert.equal(finding.code, 'unpaired-surrogate');
    assert.equal(finding.location.pointer, member ? null : '/description');
    assert.equal(finding.location.byte_offset, text.indexOf(token));
    assert.equal(finding.location.line, 1);
    assert.equal(finding.location.byte_column, text.indexOf(token) + 1);
  }
  checked.free();
  draft.free();
}
console.log('Wasm surrogate document boundary: six cases passed');
