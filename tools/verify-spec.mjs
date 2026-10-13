import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve, join, dirname} from 'node:path';
import {execFileSync, spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const [sdkArgument, specArgument, expected, ...options] = process.argv.slice(2);
assert.match(expected ?? '', /^[0-9a-f]{40}$/, 'An exact applied spec commit is required');
assert(options.length === 0 || (options.length === 1 && options[0] === '--self-test'), 'Only --self-test is supported after the applied spec commit');
const sdk = resolve(sdkArgument), spec = resolve(specArgument);
assert.equal(execFileSync('git', ['rev-parse', 'HEAD'], {cwd:spec, encoding:'utf8'}).trim(), expected);
assert(readFileSync(join(sdk, 'README.md'), 'utf8').includes('Applied specification revision: `' + expected + '`'), 'SDK declared spec differs from selected qualification input');
const frozen = join(sdk, 'tools/qualification/inputs/spec');
let count = 0;
function compare(directory = '') {
  for (const entry of readdirSync(join(frozen, directory), {withFileTypes:true})) {
    const name = join(directory, entry.name);
    if (entry.isDirectory()) compare(name);
    else {
      assert(entry.isFile(), 'Only ordinary frozen input files are supported');
      assert(readFileSync(join(frozen, name)).equals(readFileSync(join(spec, name))), 'Frozen spec input differs: ' + name);
      count++;
    }
  }
}
compare();
assert(count > 0, 'Frozen specification evidence is missing');

const schemasPath = 'crates/openbindings/schemas';
const derivedPath = schemasPath + '/openbindings.schema.json';
assert(readFileSync(join(sdk, derivedPath)).equals(readFileSync(join(spec, 'openbindings.schema.json'))), 'Bundled derived schema differs from pinned spec: ' + derivedPath);
count++;

const metaSchemas = [
  'draft2020-12/schema.json',
  'draft2020-12/meta/core.json',
  'draft2020-12/meta/applicator.json',
  'draft2020-12/meta/unevaluated.json',
  'draft2020-12/meta/validation.json',
  'draft2020-12/meta/meta-data.json',
  'draft2020-12/meta/format-annotation.json',
  'draft2020-12/meta/content.json',
];
const digestsPath = schemasPath + '/DIGESTS.json';
const digests = JSON.parse(readFileSync(join(sdk, digestsPath), 'utf8'));
assert.deepEqual(Object.keys(digests).sort(), [...metaSchemas].sort(), 'Bundled meta-schema digest inventory must contain the dialect and seven vocabulary meta-schemas');
for (const name of metaSchemas) {
  assert.match(digests[name], /^[0-9a-f]{64}$/, 'Invalid SHA-256 digest: ' + name);
  const digest = createHash('sha256').update(readFileSync(join(sdk, schemasPath, name))).digest('hex');
  assert.equal(digest, digests[name], 'Bundled meta-schema SHA-256 differs: ' + schemasPath + '/' + name);
}

function testDrift() {
  // CR-7: the unmodified copy passes; a one-byte change in each bundled input fails.
  const temporary = mkdtempSync(join(tmpdir(), 'openbindings-spec-pinning-'));
  try {
    const copy = join(temporary, 'sdk');
    for (const name of ['README.md', 'tools/qualification/inputs/spec', schemasPath]) {
      mkdirSync(dirname(join(copy, name)), {recursive:true});
      cpSync(join(sdk, name), join(copy, name), {recursive:true});
    }
    function run() {
      const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), copy, spec, expected], {encoding:'utf8'});
      assert.ifError(result.error);
      return result;
    }
    const baseline = run();
    assert.equal(baseline.status, 0, 'CR-7 unmodified temporary inputs must pass: ' + baseline.stderr);
    const inputs = [
      [derivedPath, 'Bundled derived schema differs from pinned spec: ' + derivedPath],
      ...metaSchemas.map(name => [schemasPath + '/' + name, 'Bundled meta-schema SHA-256 differs: ' + schemasPath + '/' + name]),
    ];
    for (const [name, message] of inputs) {
      const path = join(copy, name);
      const original = readFileSync(path);
      const altered = Buffer.from(original);
      // Preserve the JSON value to prove that verification pins exact bytes.
      const newline = original.indexOf('\n');
      assert(newline >= 0, 'CR-7 test input must contain a newline: ' + name);
      altered[newline] = 0x20;
      writeFileSync(path, altered);
      const result = run();
      writeFileSync(path, original);
      assert.equal(result.status, 1, 'CR-7 altered input must fail: ' + name);
      assert(result.stderr.includes(message), 'CR-7 failure must identify the altered input: ' + result.stderr);
    }
    // CR-7: deleting a digest must fail rather than silently stop checking that input.
    const path = join(copy, digestsPath);
    const incomplete = {...digests};
    delete incomplete[metaSchemas[0]];
    writeFileSync(path, JSON.stringify(incomplete));
    const result = run();
    assert.equal(result.status, 1, 'CR-7 incomplete digest inventory must fail');
    assert(result.stderr.includes('Bundled meta-schema digest inventory'), 'CR-7 failure must identify the incomplete digest inventory: ' + result.stderr);
    return inputs.length + 1;
  } finally {
    rmSync(temporary, {recursive:true, force:true});
  }
}

const regression = options.length ? {driftRegressionTests:testDrift()} : {};
console.log(JSON.stringify({appliedSpecification:expected, identicalFiles:count, verifiedMetaSchemas:metaSchemas.length, ...regression}));
