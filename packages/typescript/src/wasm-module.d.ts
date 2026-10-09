/** Raw Wasm asset. Compiled-module hosts such as workerd provide this import.
 * Other hosts should load the exported asset URL/bytes and pass them to initialize.
 */
declare const module: WebAssembly.Module;
export default module;
