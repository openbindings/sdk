import { exact_roundtrip, pending_cleanup, logical_owners } from './wasm/openapi_json_path_consumer';
// No source/input/output value is parsed into a JavaScript Number.
const description = '{"openapi":"3.1.2","info":{"title":"Node","version":"1"},"servers":[{"url":"https://node.invalid"}],"paths":{"/echo":{"post":{"requestBody":{"content":{"application/json":{"schema":true}}},"responses":{"200":{"description":"echo","content":{"application/json":{"schema":true}}}}}}}}';
const input = '{"body":{"n":9007199254740993,"label":"雪"}}';
const output = '{"n":9007199254740993,"label":"雪"}';
async function cycle() {
  if (await exact_roundtrip(description, input, output) !== output) throw new Error('exact output changed');
  let failed = false;
  try { await exact_roundtrip(description, input, '{'); } catch { failed = true; }
  if (!failed) throw new Error('malformed success accepted');
  let challenged = false;
  const choices = description.replace('{"url":"https://node.invalid"}', '{"url":"https://node.invalid"},{"url":"https://second.invalid"}');
  try { await exact_roundtrip(choices, input, output); } catch (e) { challenged = String(e) === "CONTEXT_REQUIRED"; }
  if (!challenged) throw new Error("context challenge lost");
  await pending_cleanup(false);
  await pending_cleanup(true);
}
async function main() {
  await cycle();
  const before = logical_owners();
  for (let i=0;i<1000;i++) await cycle();
  if (logical_owners() !== before) throw new Error(`retained owners ${before} -> ${logical_owners()}`);
  console.log(JSON.stringify({cycles:1000, exactText:true, localPendingHost:true, owners:before}));
}
void main().catch(error => { console.error(error); throw error; });
