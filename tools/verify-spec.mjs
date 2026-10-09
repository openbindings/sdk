import assert from 'node:assert/strict';
import {readFileSync, readdirSync} from 'node:fs';
import {resolve, join} from 'node:path';
import {execFileSync} from 'node:child_process';

const [sdkArgument, specArgument, expected] = process.argv.slice(2);
assert.match(expected ?? '', /^[0-9a-f]{40}$/, 'An exact applied spec commit is required');
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
console.log(JSON.stringify({appliedSpecification:expected, identicalFiles:count}));
