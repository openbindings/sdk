/** Private disposable-handle ownership, with no domain-policy authority. */
import {
  SdkError,
  finalizeWasm,
  freeRaw,
  requireUsable,
} from "./diagnostics.js";
/** @internal */
export { freeRaw } from "./diagnostics.js";

export type Raw = { free(): void };

/** @internal */
export const finalizer =
  typeof FinalizationRegistry === "undefined"
    ? undefined
    : new FinalizationRegistry<Raw>((raw) => finalizeWasm(() => raw.free()));

/** @internal */
export const handles = new WeakMap<object, Raw>();

/** @internal */
export function registerHandle(owner: object, raw: Raw): void {
  handles.set(owner, raw);
  finalizer?.register(owner, raw, owner);
}

/** @internal */
export function handle<T extends Raw>(owner: Managed): T {
  requireUsable();
  const raw = handles.get(owner);
  if (!raw)
    throw new SdkError("disposed-handle", "This SDK handle has been disposed.");
  return raw as T;
}

export abstract class Managed {
  protected constructor(raw: Raw) {
    registerHandle(this, raw);
  }
  /** Whether this handle has been released. Safe to inspect after disposal. */
  get disposed(): boolean {
    return !handles.has(this);
  }
  /** Release this owner deterministically; idempotent. Other retained owners remain usable unless the instance trapped. After a trap, forget the owner without reentering Wasm or throwing. Later handle access throws disposed-handle, or wasm-trap for a trapped instance. Release is not a promise of reduced process RSS or Wasm memory capacity. */
  dispose(): void {
    const raw = handles.get(this);
    if (raw) {
      handles.delete(this);
      finalizer?.unregister(this);
      freeRaw(raw);
    }
  }
  /** Explicit resource-management alias for dispose(); use with JavaScript using where supported. */
  [Symbol.dispose](): void {
    this.dispose();
  }
}

/** @internal */
export function canonicalRaw<R extends Raw>(
  owner: Managed,
  type: Function & { prototype: R },
): R {
  const raw = handle<R>(owner);
  if (!(raw instanceof type))
    throw new SdkError(
      "invalid-domain-owner",
      "This method requires the matching canonical domain owner.",
    );
  return raw;
}

/** @internal */
export function adopt<R extends Raw, T>(raw: R, construct: (raw: R) => T): T {
  try {
    return construct(raw);
  } catch (error) {
    freeRaw(raw);
    throw error;
  }
}

/** @internal */
export function adoptMany<R extends Raw, T extends Managed>(
  raws: R[],
  construct: (raw: R) => T,
): readonly T[] {
  const result: T[] = [];
  let index = 0;
  try {
    for (; index < raws.length; index++)
      result.push(adopt(raws[index]!, construct));
    return Object.freeze(result);
  } catch (error) {
    for (const owner of result) owner.dispose();
    // adopt frees the current raw owner if construction fails.
    for (let remaining = index + 1; remaining < raws.length; remaining++)
      freeRaw(raws[remaining]);
    throw error;
  }
}
