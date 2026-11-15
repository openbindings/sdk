/** Shared checks for caller-selected report retention. */
import { SdkError } from "./diagnostics.js";
import { optionEntries } from "./json.js";

/** @internal */
export function reportLimits<T extends object>(
  defaults: Readonly<T>,
  overrides: unknown,
  code: string,
): T {
  const complete = { ...defaults };
  if (overrides === undefined) return complete;
  for (const [key, value] of optionEntries(overrides, "Report limits", code)) {
    if (
      !Object.hasOwn(complete, key) ||
      typeof value !== "number" ||
      !Number.isSafeInteger(value) ||
      value < 0 ||
      value > 0xffffffff
    )
      throw new SdkError(
        code,
        "Report limits require known nonnegative 32-bit integers.",
      );
    Object.defineProperty(complete, key, {
      value,
      enumerable: true,
      writable: true,
      configurable: true,
    });
  }
  return complete;
}
