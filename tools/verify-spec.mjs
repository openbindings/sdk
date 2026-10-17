import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve, join, dirname, relative, sep} from 'node:path';
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

// The copy the product embeds: OBI-02 applies its derived schema and OBI-10 its meta-schemas.
const schemasPath = 'crates/openbindings-internal-schema/schemas';
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

// A pin only guarantees the bytes the product uses when it names them: every
// bundled schema a crate source embeds is pinned above, and each pinned file is embedded.
const pinned = [derivedPath, ...metaSchemas.map(name => schemasPath + '/' + name)];
function embeddedSchemas(directory, found = new Set()) {
  for (const entry of readdirSync(directory, {withFileTypes:true})) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) embeddedSchemas(path, found);
    else if (entry.name.endsWith('.rs')) {
      for (const [, target] of readFileSync(path, 'utf8').matchAll(/include_(?:str|bytes)!\(\s*"([^"]+\.json)"\s*\)/g)) {
        const file = relative(sdk, resolve(dirname(path), target)).split(sep).join('/');
        if (file.split('/').includes('schemas')) found.add(file);
      }
    }
  }
  return found;
}
const embedded = new Set();
for (const crate of readdirSync(join(sdk, 'crates'), {withFileTypes:true})) {
  const sources = join(sdk, 'crates', crate.name, 'src');
  if (crate.isDirectory() && existsSync(sources)) embeddedSchemas(sources, embedded);
}
for (const file of embedded) assert(pinned.includes(file), 'Embedded schema is not pinned: ' + file);
for (const file of pinned) assert(embedded.has(file), 'Pinned schema is not embedded: ' + file);

function testDrift() {
  // the unmodified copy passes; a one-byte change in each bundled input fails,
  // and so does embedding a schema outside the pin or leaving a pinned one unused.
  const temporary = mkdtempSync(join(tmpdir(), 'openbindings-spec-pinning-'));
  const embedder = 'crates/openbindings-internal-schema/src';
  try {
    const copy = join(temporary, 'sdk');
    for (const name of ['README.md', 'tools/qualification/inputs/spec', schemasPath, embedder]) {
      mkdirSync(dirname(join(copy, name)), {recursive:true});
      cpSync(join(sdk, name), join(copy, name), {recursive:true});
    }
    function run() {
      const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), copy, spec, expected], {encoding:'utf8'});
      assert.ifError(result.error);
      return result;
    }
    const baseline = run();
    assert.equal(baseline.status, 0, 'unmodified temporary inputs must pass: ' + baseline.stderr);
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
      assert(newline >= 0, 'test input must contain a newline: ' + name);
      altered[newline] = 0x20;
      writeFileSync(path, altered);
      const result = run();
      writeFileSync(path, original);
      assert.equal(result.status, 1, 'altered input must fail: ' + name);
      assert(result.stderr.includes(message), 'failure must identify the altered input: ' + result.stderr);
    }
    // a crate embedding another copy fails, as does a pinned file no source embeds.
    const unpinned = 'crates/openbindings/src/copy.rs';
    mkdirSync(dirname(join(copy, unpinned)), {recursive:true});
    writeFileSync(join(copy, unpinned), 'const SCHEMA: &str = include_str!("../schemas/openbindings.schema.json");\n');
    const outside = run();
    rmSync(join(copy, 'crates/openbindings'), {recursive:true});
    assert.equal(outside.status, 1, 'an embedded copy outside the pin must fail');
    assert(outside.stderr.includes('Embedded schema is not pinned: crates/openbindings/schemas/openbindings.schema.json'), 'failure must identify the unpinned copy: ' + outside.stderr);
    const source = join(copy, embedder, 'fixed_schema.rs');
    const embedding = readFileSync(source, 'utf8');
    const last = schemasPath + '/' + metaSchemas.at(-1);
    const include = 'include_str!("../schemas/' + metaSchemas.at(-1) + '")';
    assert(embedding.includes(include), 'test input must embed ' + last);
    writeFileSync(source, embedding.replace(include, '""'));
    const unused = run();
    writeFileSync(source, embedding);
    assert.equal(unused.status, 1, 'a pinned schema no source embeds must fail');
    assert(unused.stderr.includes('Pinned schema is not embedded: ' + last), 'failure must identify the unused pin: ' + unused.stderr);
    // deleting a digest must fail rather than silently stop checking that input.
    const path = join(copy, digestsPath);
    const incomplete = {...digests};
    delete incomplete[metaSchemas[0]];
    writeFileSync(path, JSON.stringify(incomplete));
    const result = run();
    assert.equal(result.status, 1, 'incomplete digest inventory must fail');
    assert(result.stderr.includes('Bundled meta-schema digest inventory'), 'failure must identify the incomplete digest inventory: ' + result.stderr);
    return inputs.length + 3;
  } finally {
    rmSync(temporary, {recursive:true, force:true});
  }
}

const regression = options.length ? {driftRegressionTests:testDrift()} : {};
console.log(JSON.stringify({appliedSpecification:expected, identicalFiles:count, verifiedMetaSchemas:metaSchemas.length, ...regression}));
