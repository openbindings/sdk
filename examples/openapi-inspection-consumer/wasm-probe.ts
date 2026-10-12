import { inspectSource, authorSource, liveClientOwners, liveSdkOwners } from './wasm/openapi_inspection_consumer.js';
function assert(value: unknown, message: string): asserts value { if (!value) throw new Error(message); }
const content=`{"document":{"openapi":"3.1.2","x-exact":9007199254740993,"paths":{"/雪~a/b":{"get":{"operationId":"雪.query"}}}}}`;
async function main() {
  const result = await inspectSource(content, false);
  const inventory = JSON.parse(result) as { exhaustive: boolean; targets: {sourceRef: string}[] };
  assert(inventory.exhaustive && inventory.targets[0].sourceRef==='/paths/~1雪~0a~1b/get','target changed');
  // Exact values cross as text; JSON.parse is used only on representable metadata.
  const authored = await authorSource(content, '/paths/~1雪~0a~1b/get');
  assert(authored.includes('9007199254740993'),'exact value rounded');
  assert(authored.includes('catalog.lookup'),'author key lost');
  try { await inspectSource(content,true); throw new Error('cancellation ignored'); }
  catch (e: unknown) { assert(String(e).includes('cancelled'),'wrong refusal'); }
  const before=[liveClientOwners(),liveSdkOwners()];
  for(let i=0;i<1000;i++) { const value=await inspectSource(content,false); assert(value.includes('/paths/'),'lost output after owners released'); }
  assert(liveClientOwners()===before[0] && liveSdkOwners()===before[1], 'storage owner growth');
  console.log(JSON.stringify({host:'Node/Wasm',cycles:1000,clientOwnerDelta:liveClientOwners()-before[0],sdkOwnerDelta:liveSdkOwners()-before[1],exactAuthoring:true,preCancelled:true}));
}
void main().catch((e: unknown)=>{throw e;});
