import { pullAdoptAndEcho } from './wasm/invocation_consumer.js';

async function main() {
  const pending: Promise<string> = pullAdoptAndEcho('9007199254740993', false);
  if (!(pending instanceof Promise)) throw Error('expected an async Wasm call');
  if (await pending !== '9007199254740993') throw Error('binding value was rounded');
  if (await pullAdoptAndEcho('null', false) !== 'null') throw Error('null changed');
  if (await pullAdoptAndEcho('"hello"', true) !== '"hello"') throw Error('operation value changed');
  try {
    await pullAdoptAndEcho('42', true);
    throw Error('invalid operation input was forwarded');
  } catch (error) {
    if (!String(error).includes('ERR_OPERATION_VALIDATION_FAILED')) throw error;
  }
  console.log('current pull/adopt/invoke workflow: exact values and operation validation passed');
}
void main().catch(error => { throw error; });
