/** Optional testing entry for ownership assertions after SDK initialization. */
import { requireReady } from "./internal.js";
import { liveStorageOwners as liveOwners } from "./wasm/openbindings_wasm.js";

/**
 * Count live exact-JSON storage arenas in this initialized Wasm instance.
 * Retained handles and views can share one arena, so this is not a handle count.
 * Fixed evaluator arenas can initialize lazily and live for the instance's lifetime.
 * For cleanup checks, record startup retention separately and compare release
 * baselines after warming the complete parse/prepare/validate job. The count is
 * not a byte measurement and does not describe allocator RSS or Wasm capacity.
 */
export function liveStorageOwners(): number {
  requireReady();
  return liveOwners();
}
