import { synthesizeDocumentSource } from './wasm/interface_synthesis_consumer';
async function main() {
  const input = '{"openbindings":"0.2.0","operations":{},"sources":{"catalog":{"kind":"urn:example:capability-catalog:v1","content":{"units":[{"id":"空.🚲"}],"exact":9007199254740993,"null":null},"x-note":9007199254740993}}}';
  const result = await synthesizeDocumentSource(input);
  if (!result.includes('9007199254740993') || !result.includes('空.🚲')) throw Error('exact values lost');
  // Keep result unchanged for persistence; this parsed view is display-only.
  const summary = JSON.parse(result).coverage;
  if (!summary.exhaustive || !summary.fullyRepresented || summary.entries.length !== 1) throw Error('incorrect coverage');
  try { await synthesizeDocumentSource('{"openbindings":"0.2.0","operations":{},"sources":{"catalog":{"kind":"unknown"}}}'); throw Error('unsupported accepted'); }
  catch (error) { if (!String(error).includes('unsupported-kind')) throw error; }
  console.log('synthesis local Rc/Promise Wasm, exact values and refusal probe passed');
}
void main().catch(error => { console.error(error); throw error; });
