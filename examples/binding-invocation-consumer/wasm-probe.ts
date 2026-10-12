import { InvocationBridge, bridgeStats } from './wasm/binding_invocation_consumer';

// A qualification adapter over one Rust invocation, not a released TypeScript API.
class InvocationFailure extends Error {
  readonly code: string;
  constructor(readonly exactRecord: string) {
    super('Invocation completed unsuccessfully');
    this.code = (JSON.parse(exactRecord) as { code: string }).code;
  }
}
class Invocation {
  private disposed = false;
  private acquired = false;
  private readonly abort = () => this.raw.cancel();
  readonly outputs: AsyncIterable<string>;
  constructor(private readonly raw: InvocationBridge, private readonly signal?: AbortSignal) {
    if (signal?.aborted) raw.cancel();
    else signal?.addEventListener('abort', this.abort, { once: true });
    this.outputs = {
      [Symbol.asyncIterator]: () => {
        if (this.acquired) throw Error('Output already acquired');
        this.acquired = true;
        let finished = false;
        return {
          next: async (): Promise<IteratorResult<string>> => {
            if (finished || this.disposed) return { done: true, value: undefined };
            try {
              const text: string | undefined = await this.raw.next();
              if (text === undefined) { finished = true; this.detach(); return { done: true, value: undefined }; }
              return { done: false, value: text };
            } catch (error) {
              finished = true;
              this.detach();
              throw new InvocationFailure(String(error));
            }
          },
          return: async (): Promise<IteratorResult<string>> => {
            finished = true;
            this.dispose();
            return { done: true, value: undefined };
          },
        };
      },
    };
  }
  write(exactText: string): Promise<void> { return this.disposed ? Promise.reject('INPUT_CLOSED') : this.raw.write(exactText); }
  close(): void { if (!this.disposed) this.raw.close(); }
  cancel(): void { if (!this.disposed) this.raw.cancel(); }
  private detach(): void { this.signal?.removeEventListener('abort', this.abort); }
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.detach();
    this.raw.dispose();
    this.raw.free();
  }
}

type Stats = { drivers: number; providers: number; commands: number; tokens: number; starts: number };
const stats = (): Stats => JSON.parse(bridgeStats()) as Stats;
const tick = (): Promise<void> => new Promise(resolve => setTimeout(resolve, 0));
function assert(value: unknown, message: string): asserts value { if (!value) throw Error(message); }
let operationPath = false;
function create(mode = 'echo', signal?: AbortSignal, gate: Promise<unknown> = Promise.resolve()): Invocation {
  if (operationPath) return new Invocation(InvocationBridge.forOperation(
    '{"openbindings":"0.2.0","operations":{"echo":{"aliases":["say"]}},"bindings":{"b":{"operation":"echo","source":"s"}},"sources":{"s":{"kind":"example.wasm-local"}}}', 'say', mode, gate, false), signal);
  return new Invocation(new InvocationBridge('{"source":{"kind":"example.wasm-local","content":9007199254740993},"binding":{},"context":{}}', mode, gate), signal);
}
async function released(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await tick();
    const s = stats();
    if (s.drivers + s.providers + s.commands + s.tokens === 0) return;
  }
  throw Error('Bridge owners or commands retained: ' + bridgeStats());
}
async function failure(work: Promise<unknown>, expected: string): Promise<InvocationFailure> {
  try { await work; throw Error('Expected unsuccessful completion'); }
  catch (error) {
    assert(error instanceof InvocationFailure && error.code === expected, 'Wrong terminal failure');
    return error;
  }
}
function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void;
  const promise = new Promise<void>(complete => { resolve = complete; });
  return { promise, resolve };
}

async function main(): Promise<void> {
  // Exact texts cross the host boundary unchanged, including non-binary64 values.
  const exact = ['9007199254740993', '1e400', '{"x":1,"x":2}', '"\\ud800"', 'null'];
  const echo = create();
  const outputs: string[] = [];
  await Promise.all([
    (async () => { for (let i = 0; i < 20; i++) await echo.write(exact[i % exact.length]); echo.close(); })(),
    (async () => { for await (const text of echo.outputs) outputs.push(text); })(),
  ]);
  assert(outputs.length === 20 && outputs.every((v, i) => v === exact[i % exact.length]), 'Exact values/order changed');
  echo.dispose(); await released();

  const empty = create('no-output');
  const emptyValues: string[] = [];
  for await (const value of empty.outputs) emptyValues.push(value);
  assert(emptyValues.length === 0, 'No-output became null');
  empty.dispose(); await released();

  const unary = create('unary');
  await unary.write('null');
  try { await unary.write('1'); throw Error('Write after provider close accepted'); }
  catch (error) { assert(String(error) === 'INPUT_CLOSED', 'Wrong input closure result'); }
  const unaryValues: string[] = [];
  for await (const value of unary.outputs) unaryValues.push(value);
  assert(unaryValues.length === 1 && unaryValues[0] === 'null', 'Input closure changed output');
  unary.dispose(); await released();

  for (const mode of ['error-null', 'error-absent']) {
    const call = create(mode); const iterator = call.outputs[Symbol.asyncIterator]();
    await call.write('9007199254740993'); call.close();
    assert((await iterator.next()).value === '9007199254740993', 'Pre-failure output changed');
    const error = await failure(iterator.next(), 'example.failure');
    const record = JSON.parse(error.exactRecord) as Record<string, unknown>;
    assert(Object.hasOwn(record, 'data') === (mode === 'error-null'), 'Absent/null error data collapsed');
    if (mode === 'error-null') assert(record.data === null, 'Present error null changed');
    assert((await iterator.next()).done, 'Error terminal was delivered twice');
    call.dispose(); await released();
  }

  const before = new AbortController(); before.abort();
  const starts = stats().starts;
  const unstarted = create('echo', before.signal);
  await failure(unstarted.outputs[Symbol.asyncIterator]().next(), 'ERR_CANCELLED');
  assert(stats().starts === starts, 'Pre-aborted invocation started provider');
  unstarted.dispose(); await released();

  const gate = deferred(); const controller = new AbortController();
  const active = create('echo', controller.signal, gate.promise);
  const pending = active.outputs[Symbol.asyncIterator]().next();
  await tick();
  assert(stats().providers === 1 && stats().commands === 1, 'Provider did not await host Promise');
  controller.abort();
  await failure(pending, 'ERR_CANCELLED');
  active.dispose();
  await released();
  gate.resolve(); await tick(); await released(); // Late host settlement cannot restart Rust work.

  const early = create('burst');
  const earlyIterator = early.outputs[Symbol.asyncIterator]();
  for await (const text of { [Symbol.asyncIterator]: () => earlyIterator }) { assert(text === '0', 'Wrong burst start'); break; }
  assert((await earlyIterator.next()).done, 'Iterator resumed after early return');
  await released(); // Async iterator return cancels and disposes the pending producer.

  const held = deferred(); const disposed = create('echo', undefined, held.promise);
  await disposed.write('0'); // Fill the single native input slot before provider reads.
  const write = disposed.write('1').then(() => 'accepted', error => String(error));
  const read = disposed.outputs[Symbol.asyncIterator]().next();
  await tick();
  assert(stats().commands === 2, 'Expected one pending command per direction');
  disposed.dispose();
  assert(await write === 'INPUT_CLOSED', 'Dispose failed to settle pending write');
  await failure(read, 'ERR_CANCELLED');
  await released();
  held.resolve(); await tick(); await released();

  console.log(JSON.stringify({ result: 'passed', path: operationPath ? 'operation' : 'binding', host: 'Node/Wasm', checks: ['Rc across Promise', 'exact texts', 'no-output versus null', 'input/output closure', 'output then failure', 'absent/null errors', 'pre/mid abort', 'iterator return', 'dispose outstanding commands', 'late Promise settlement'], final: stats() }));
}
const timeout = setTimeout(() => { throw Error('Wasm qualification timed out'); }, 15000);
async function operationValidation(): Promise<void> {
  const document = '{"openbindings":"0.2.0","operations":{"echo":{"input":{"type":"integer"},"output":{"type":"integer"}}},"bindings":{"b":{"operation":"echo","source":"s"}},"sources":{"s":{"kind":"example.wasm-local"}}}';
  const call = new Invocation(InvocationBridge.forOperation(document, 'echo', 'echo', Promise.resolve(), true));
  const out = call.outputs[Symbol.asyncIterator]();
  await call.write('9007199254740993');
  assert((await out.next()).value === '9007199254740993', 'Wasm validation rounded an exact integer');
  await call.write('"bad"');
  await failure(out.next(), 'ERR_OPERATION_VALIDATION_FAILED');
  assert((await out.next()).done, 'Validation terminal repeated');
  call.dispose(); await released();
  const unresolved = new Invocation(InvocationBridge.forOperation(document, 'echo', 'echo', Promise.resolve(), false));
  await unresolved.write('1');
  await failure(unresolved.outputs[Symbol.asyncIterator]().next(), 'ERR_SCHEMA_UNRESOLVED');
  unresolved.dispose(); await released();
  console.log(JSON.stringify({result:'passed',checks:['operation exact integer validation', 'input mismatch terminal', 'missing evaluator no verdict'],final:stats()}));
}
void (async () => { await main(); operationPath = true; await main(); await operationValidation(); })().then(() => clearTimeout(timeout), error => { clearTimeout(timeout); console.error(error); throw error; });
