import { pullCatalog } from './wasm/source_pull_consumer.js';

function assert(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

const request = (data: string) => `{"interface":{"openbindings":"0.2.0","operations":{},"sources":{"catalog":{"kind":"example.embedded@1","data":${data}}}},"source":"catalog"}`;

async function main() {
  const source = `{"items":[{"key":"item0","operation":{"input":true},"data":{"item":"item0","id":"空.🚲","exact":9007199254740993,"present":null}}]}`;
  const pending: Promise<string> = pullCatalog(request(source));
  assert(pending instanceof Promise, 'Rust async consumer did not return a Promise');
  const text = await pending;
  assert(text.includes('9007199254740993'), 'exact number changed');
  const result = JSON.parse(text) as {
    additions: {bindings: Record<string, {data: Record<string, unknown>}>};
    report: {exhaustive: boolean; added: {binding: string; operation: string}[]};
  };
  assert(result.report.exhaustive && result.report.added.length === 1, 'incorrect report');
  const binding = result.additions.bindings[result.report.added[0].binding];
  assert(binding.data.id === '空.🚲', 'kind-owned identity changed');
  assert(binding.data.present === null && !('absent' in binding.data), 'null/absence changed');
  const partial = JSON.parse(await pullCatalog(request('{"items":[],"partial":true}')));
  assert(!partial.report.exhaustive && partial.report.limitation, 'partiality was not disclosed');
  try {
    await pullCatalog(request('null'));
    throw new Error('malformed source became success');
  } catch (error) {
    assert(String(error).includes('Operational'), 'wrong native refusal');
  }
  console.log('source pulling: external async Wasm consumer, exact values and explicit refusal passed');
}
void main().catch(error => { throw error; });
