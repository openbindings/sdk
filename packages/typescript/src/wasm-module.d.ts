/** Raw Wasm asset. Compiled-module hosts such as workerd provide this import.
 * Node and browsers can call initialize() for default loading, or pass explicit bytes.
 */
declare const module: WebAssembly.Module;
export default module;
