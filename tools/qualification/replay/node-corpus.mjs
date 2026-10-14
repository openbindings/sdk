// Public TypeScript facade replay without a network listener.
import { readFile } from 'node:fs/promises';
import * as sdk from '../../../packages/typescript/dist/index.js';
import { observe } from './sdk-observe.mjs';
await sdk.initialize(await readFile(new URL('../../../packages/typescript/dist/wasm/openbindings_wasm_bg.wasm', import.meta.url)));
for (const line of (await readFile(process.argv[2], 'utf8')).trim().split('\n')) {
  console.log(JSON.stringify(observe(sdk, JSON.parse(line))));
}
