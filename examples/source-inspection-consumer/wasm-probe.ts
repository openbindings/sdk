import { inspectDocumentSource } from './wasm/source_inspection_consumer.js';

function assert(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

const kind = 'urn:example:inspection-catalog:v1';
const document = (source: string) =>
  `{"openbindings":"0.2.0","operations":{},"sources":{"catalog":${source}}}`;
const carrier = (content: string) => document(`{"kind":"${kind}","content":${content}}`);

async function rejects(input: string, expected: string) {
  try {
    await inspectDocumentSource(input);
  } catch (error: unknown) {
    assert(String(error).includes(expected), `unexpected refusal: ${String(error)}`);
    return;
  }
  throw new Error(`expected refusal containing ${expected}`);
}

async function main() {
  const pending: Promise<string> = inspectDocumentSource(carrier(`[
    {"id":"空.🚲","status":"admitted","binding":{"整数":9007199254740993}},
    {"id":"null","status":"admitted","binding":null},
    {"id":"absent","status":"admitted"},
    {"id":"excluded","status":"excluded"}
  ]`));
  assert(pending instanceof Promise, 'Rust async export must return a Promise');
  const exactText = await pending;
  assert(exactText.includes('9007199254740993'), 'exact integer was rounded');
  // Ordinary JSON.parse is only used for these representable metadata checks.
  // The exact content representation remains the returned JSON text.
  const inventory = JSON.parse(exactText) as {
    exhaustive: boolean;
    targets: { sourceRef: string; binding: Record<string, unknown> }[];
  };
  assert(inventory.exhaustive && inventory.targets.length === 3, 'wrong inventory');
  assert(inventory.targets[0].sourceRef === '空.🚲', 'Unicode identity changed');
  assert(inventory.targets[1].binding.content === null, 'null was lost');
  assert(!('content' in inventory.targets[2].binding), 'absence was lost');
  assert(inventory.targets.every(t => !('operation' in t.binding) && !('source' in t.binding)), 'provider assigned author keys');
  const [empty, partial] = await Promise.all([
    inspectDocumentSource(carrier('[]')),
    inspectDocumentSource(carrier('[{"id":"unavailable","status":"unavailable"}]')),
  ]);
  assert(JSON.parse(empty).exhaustive === true, 'empty inventory was not complete');
  assert(JSON.parse(partial).exhaustive === false && JSON.parse(partial).limitation.code === 'example.catalog.unavailable', 'partiality was not disclosed');
  await rejects(carrier('null'), 'example.catalog.unreadable');
  await rejects(document(`{"kind":"${kind}"}`), 'example.catalog.unreadable');
  await rejects(document(`{"kind":"${kind}","content":[],"selector":"legacy"}`), 'invalid-input');
  await rejects(document(`{"kind":"${kind}","content":[],"description":7}`), 'string');
  await rejects(document('{"kind":7,"content":[]}'), 'string');
  await rejects(document(`{"kind":"${kind}/future","content":[]}`), 'unsupported-kind');
  console.log('source inspection: TypeScript types and real local async Wasm probe passed');
}
void main().catch((error: unknown) => { throw error; });
