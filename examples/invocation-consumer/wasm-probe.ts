import { describeLocalKinds, pullAdoptAndEcho } from './wasm/invocation_consumer.js';

async function main() {
  type Description = {
    kind: string;
    sourceData?: { properties: { items: { items: { properties: { key: { pattern: string } } } } } };
    bindingData: { properties: { item: { pattern?: string; const?: string } } };
  };
  const described: Description[] = JSON.parse(await describeLocalKinds());
  const embedded = described.find(d => d.kind === 'example.embedded@1')!;
  const fixed = described.find(d => d.kind === 'example.fixed@1')!;
  if (fixed.bindingData.properties.item.const !== 'echo') throw Error('fixed selector description is too broad');
  for (const pattern of [embedded.sourceData!.properties.items.items.properties.key.pattern, embedded.bindingData.properties.item.pattern!]) {
    const expression = new RegExp(pattern, 'u');
    for (const [value, accepted] of [['', true], ['a', true], ['空🚲', true], ['\n', true], ['\ud800', false], ['\udc00', false], ['a\ud800x', false], ['x\udc00', false]] as const) {
      if (expression.test(value) !== accepted) throw Error('description disagrees with Unicode scalar-string carriage');
    }
  }
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
  console.log('current partial Wasm workflow: kind descriptions, exact values and operation validation passed');
}
void main().catch(error => { throw error; });
