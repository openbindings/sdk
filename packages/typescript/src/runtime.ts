/** Explicit host initialization for the Rust/Wasm engine. */
import init from "./wasm/openbindings_wasm.js";
import * as wasm from "./wasm/openbindings_wasm.js";
import signature from "./wasm/openbindings_wasm_signature.js";
import { SdkError } from "./diagnostics.js";

/** Host input for WebAssembly initialization. A compiled module or bytes avoids fetching; URL/string/Request inputs and the default initializer may use host loading. Schema interpretation and evaluation perform no resource I/O. */
export type WasmInput =
  | BufferSource
  | WebAssembly.Module
  | Response
  | Request
  | URL
  | string;

let initialization: Promise<void> | undefined;

let ready = false;

/** Initialize this module instance once before using SDK APIs. Concurrent calls share one promise; a failed attempt can be retried, including one given a WebAssembly module that is not this package's, which is refused before it is instantiated. Requires host crypto.getRandomValues (request-scoped hosts should call inside a request). Throws SdkError for initialization/entropy failures. Without input, Node reads the installed Wasm asset from disk; browsers fetch the module-relative asset. Supplying bytes/module bypasses default loading in every host. */
export function initialize(
  input?: WasmInput | Promise<WasmInput>,
): Promise<void> {
  return (initialization ??= (async () => {
    let module = await input;
    // Check before entering Rust: the engine's randomized hash tables require
    // host entropy, and some hosts only permit it inside a request handler.
    try {
      globalThis.crypto.getRandomValues(new Uint8Array(1));
    } catch {
      throw new SdkError(
        "host-entropy-unavailable",
        "The host must permit crypto.getRandomValues during initialization. In request-scoped hosts such as workerd, call initialize() inside a request handler.",
      );
    }
    if (
      module === undefined &&
      (globalThis as { process?: { versions?: { node?: string } } }).process
        ?.versions?.node
    ) {
      // Keep Node loading lazy so browsers and explicit-input hosts need no
      // Node builtins. Resolve from this installed module, never the caller's cwd.
      const builtin = "node:fs/promises";
      const { readFile } = (await import(builtin)) as {
        readFile(path: URL): Promise<Uint8Array<ArrayBuffer>>;
      };
      module = await readFile(
        new URL("./wasm/openbindings_wasm_bg.wasm", import.meta.url),
      );
    }
    await init({ module_or_path: await packageModule(module) });
    wasm.initializeRuntime();
    ready = true;
  })().catch((error) => {
    initialization = undefined;
    if (error instanceof SdkError) throw error;
    throw new SdkError(
      "wasm-initialization",
      "Unable to initialize the OpenBindings WebAssembly module: " +
        String(error?.message ?? error),
    );
  }));
}

/**
 * Compile initialization input and refuse a module that is not this package's.
 * The generated bindings keep the first instance they finalize, even when its
 * start then fails, so a module that instantiates but is not this one would leave
 * every later attempt unusable. Its imports and exports must be exactly those the
 * bindings were generated for. Loading mirrors the bindings' own: URL, string and
 * Request inputs are fetched, and a Response compiles as a stream when served as
 * application/wasm.
 */
async function packageModule(
  input: WasmInput | undefined,
): Promise<WebAssembly.Module> {
  let source: WasmInput =
    input ?? new URL("./wasm/openbindings_wasm_bg.wasm", import.meta.url);
  if (
    typeof source === "string" ||
    source instanceof URL ||
    (typeof Request === "function" && source instanceof Request)
  )
    source = await fetch(source);
  let module: WebAssembly.Module;
  if (Object.prototype.toString.call(source) === "[object WebAssembly.Module]")
    module = source as WebAssembly.Module;
  else if (typeof Response === "function" && source instanceof Response) {
    if (!source.ok)
      throw new Error(
        `failed to fetch Wasm: ${source.status} ${source.statusText} fetching '${source.url}'`,
      );
    module =
      typeof WebAssembly.compileStreaming === "function" &&
      source.headers.get("Content-Type")?.trim().toLowerCase() ===
        "application/wasm"
        ? await WebAssembly.compileStreaming(source)
        : await WebAssembly.compile(await source.arrayBuffer());
  } else module = await WebAssembly.compile(source as BufferSource);
  const entries = [
    ...WebAssembly.Module.imports(module).map(({ module, name, kind }) =>
      JSON.stringify(["import", kind, module, name]),
    ),
    ...WebAssembly.Module.exports(module).map(({ name, kind }) =>
      JSON.stringify(["export", kind, name]),
    ),
  ].sort();
  if (
    entries.length !== signature.length ||
    entries.some((entry, index) => entry !== JSON.stringify(signature[index]))
  )
    throw new Error(
      "the supplied module is not this package's: its imports and exports differ from those its bindings were generated for",
    );
  return module;
}

/** @internal */
export function requireReady(): void {
  if (!ready)
    throw new SdkError(
      "not-initialized",
      "Call and await initialize() before using the SDK.",
    );
}
