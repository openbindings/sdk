/** SDK failures, original source coordinates and report-carrier decoding. */

/** Expected JSON syntax/encoding/admission refusal. This is separate from normative document evidence and schema-instance failure; initialization and API misuse still throw. */
export interface InputFailure {
  /** Discriminant for expected exact-input refusal. */
  status: "input-error";
  /** Stable input code/category, optional zero-based byte offset, and explanatory message. */
  error: {
    /** Stable detailed failure identifier. */
    code: string;
    /** Stable category describing the observed value or failure. */
    kind?: string;
    /** Zero-based offset in original UTF-8 source when available. */
    byteOffset?: number;
    /** One-based original source line. */
    line: number;
    /** One-based UTF-8 byte column, not a UTF-16 character index. */
    byteColumn: number;
    /** Human-readable explanation of what the parser expected or found and the byte where, such as `expected a colon after the member name at byte 5`, and for an exceeded limit the `limits` entry that raises it; use `code` and `byteOffset` for logic. */
    message: string;
  };
}

/** Parse result only. The successful parse branch transfers one disposable owner to the caller; it establishes neither document conformance nor schema readiness. */
export type ParseResult<T> =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "parsed";
      /** Acquired owner or detached converted value, as specified by this branch. */
      value: T;
    }
  | InputFailure;

/** Coordinates in original UTF-8 source. byteOffset is zero-based; line and byteColumn are one-based. A byte column is not a JavaScript UTF-16 editor column: convert against the original source. Pointers are untrusted display data and repeated keys may share a pointer. */
export interface SourceLocation {
  /** Exact RFC 6901 pointer: from the value, document or resource a locate() call was asked of, or from the source's root for findings and metadata; empty string means that root. A member name with an unpaired UTF-16 unit keeps it, as a JavaScript string can. null for parser syntax errors and admission limits, where no value pointer is established. Use byteOffset to distinguish duplicate key occurrences. */
  pointer: string | null;
  /** Zero-based original UTF-8 byte offset. */
  byteOffset: number;
  /** One-based original source line. */
  line: number;
  /** One-based UTF-8 byte column, not a UTF-16 character index. */
  byteColumn: number;
}

function camel(value: unknown, keys: Map<string, string>): unknown {
  if (Array.isArray(value)) return value.map((v) => camel(v, keys));
  if (value && typeof value === "object") {
    const result: Record<string, unknown> = {};
    for (const key of Object.keys(value)) {
      let name = keys.get(key);
      if (name === undefined) {
        name = key.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
        keys.set(key, name);
      }
      const member = camel((value as Record<string, unknown>)[key], keys);
      if (name === "__proto__")
        Object.defineProperty(result, name, {
          value: member,
          enumerable: true,
          configurable: true,
          writable: true,
        });
      else result[name] = member;
    }
    return result;
  }
  return value;
}

/** @internal */
export function decode<T>(text: string): T {
  // Field names repeat across large reports. Keep translations for this decode
  // only, without sharing any of the caller's returned objects.
  return camel(JSON.parse(text), new Map()) as T;
}

/** Thrown SDK misuse or configuration failure. Expected parse, conformance and schema outcomes use result unions. Messages explain; codes identify. */
export class SdkError extends Error {
  /** Construct a structured thrown error with a broad code, explanatory message and optional original-source location. */
  constructor(
    /** Broad stable SDK error category; expected semantic outcomes use result unions. */
    readonly code: string,
    message: string,
    /** Optional frozen original UTF-8 source coordinates. */
    readonly location?: Readonly<SourceLocation>,
  ) {
    super(message);
    if (location) this.location = Object.freeze({ ...location });
    this.name = "SdkError";
  }
}

/** @internal */
export function sdkCall<T>(call: () => T): T {
  try {
    return call();
  } catch (error) {
    if (typeof error === "string") {
      try {
        const info = decode<{
          code: string;
          message: string;
          location?: SourceLocation | null;
        }>(error);
        if (typeof info.code === "string" && typeof info.message === "string")
          throw new SdkError(
            info.code,
            info.message,
            info.location ?? undefined,
          );
      } catch (decoded) {
        if (decoded instanceof SdkError) throw decoded;
      }
    }
    throw error;
  }
}

/** @internal */
export function failure(error: unknown): InputFailure {
  let info: InputFailure["error"];
  try {
    info = decode(String(error));
  } catch {
    throw error;
  }
  if (!info || typeof info.code !== "string") throw error;
  return { status: "input-error", error: info };
}

/** Closed host JSON encoding refusal codes; source coordinates are not invented. */
/** @internal */
export type JsonEncodingErrorCode =
  | "non-finite-number"
  | "unsupported-value"
  | "sparse-array"
  | "array-property"
  | "cyclic-value"
  | "non-plain-object"
  | "accessor-property"
  | "non-enumerable-property"
  | "symbol-key"
  | "invalid-json-object"
  | "encoding-limit";

/** An atomic edit was refused. code is stable; inputPointer locates invalid caller JSON input when available. The document remains unchanged. */
export class EditError extends SdkError {
  /** Logical pointer into caller JSON input; null when unavailable, including for an exceeded document limit, which is the edited document's. */
  readonly inputPointer: string | null;
  /** Construct an edit refusal without inventing source byte coordinates. */
  constructor(
    code: string,
    message: string,
    inputPointer: string | null = null,
  ) {
    super(code, message);
    this.name = "EditError";
    this.inputPointer = inputPointer;
  }
}

/** @internal */
export class JsonEncodingError extends EditError {
  constructor(
    code: JsonEncodingErrorCode,
    pointer: string | null,
    message: string,
  ) {
    super(code, message, pointer);
  }
}
