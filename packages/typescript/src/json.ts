/** Exact JSON owners, admission and checked host-value encoding. */
import * as wasm from "./wasm/openbindings_wasm.js";
import {
  EditError,
  JsonEncodingError,
  SdkError,
  decode,
  failure,
  sdkCall,
} from "./diagnostics.js";
import type {
  JsonEncodingErrorCode,
  ParseResult,
  SourceLocation,
} from "./diagnostics.js";
import { Managed, adopt, handle } from "./ownership.js";
import type { Raw } from "./ownership.js";
import { requireReady } from "./runtime.js";

/** Exact JSON source: UTF-8 bytes or JavaScript scalar text encoded as UTF-8. Text containing literal unpaired UTF-16 units throws; represent those units using JSON \u escapes or exact bytes. Parsing copies admitted input into immutable storage; input longer than the byte limit is refused without copying it whole into Wasm. */
export type ExactInput = string | Uint8Array;

/** Admission limits for exact JSON and documents, Rust's `JsonLimits`; each a nonnegative integer no greater than 4294967295, passed as `limits` to validateDocument, Document.parse and parseJson in a plain options object, where anything else throws SdkError invalid-json-limits. A document keeps them (its `limits` getter). Validation memory grows with the document, so raise them only where the process has the memory. */
export interface JsonLimits {
  /** Maximum admitted UTF-8 bytes; default 33,554,432 (32 MiB). */
  maxBytes: number;
  /** Maximum parsed nodes, including object member-name tokens; default 1,000,000. */
  maxNodes: number;
  /** Maximum nested arrays and objects, including a container root; default 10,000. validateDocument without limits sets no nesting cap. */
  maxDepth: number;
}

// Rust's JsonLimits::default(), read once: bytes, nodes and nesting.
let defaultLimits: Uint32Array | undefined;
// Cache the scalar default used on every exact-input transfer.
let defaultMaxBytes: number | undefined;

function jsonDefaults(): Uint32Array {
  return (defaultLimits ??= wasm.jsonLimits());
}

function invalidLimits(message: string): never {
  throw new SdkError("invalid-json-limits", message);
}

// A plain object of enumerable data properties, or invalid-json-limits.
function plainEntries(value: unknown, what: string): [string, unknown][] {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    ![null, Object.prototype].includes(Object.getPrototypeOf(value))
  )
    invalidLimits(`${what} must be a plain object.`);
  const entries: [string, unknown][] = [];
  for (const key of Reflect.ownKeys(value as object)) {
    const d = Object.getOwnPropertyDescriptor(value, key)!;
    if (typeof key === "symbol" || !d.enumerable || !("value" in d))
      invalidLimits(`${what} must hold enumerable data properties.`);
    entries.push([key as string, d.value]);
  }
  return entries;
}

// Complete admission limits for the bridge, unspecified entries from Rust's
// JsonLimits::default(); undefined when the caller passed none.
function admissionLimits(limits: unknown): Uint32Array | undefined {
  if (limits === undefined) return undefined;
  const [maxBytes, maxNodes, maxDepth] = jsonDefaults();
  const complete: Record<string, number> = { maxBytes, maxNodes, maxDepth };
  for (const [key, value] of plainEntries(limits, "JSON limits")) {
    if (
      !Object.hasOwn(complete, key) ||
      typeof value !== "number" ||
      !Number.isSafeInteger(value) ||
      value < 0 ||
      value > 0xffffffff
    )
      invalidLimits(
        "JSON limits must be maxBytes, maxNodes or maxDepth, each a nonnegative integer no greater than 4294967295.",
      );
    complete[key] = value;
  }
  return new Uint32Array([
    complete.maxBytes!,
    complete.maxNodes!,
    complete.maxDepth!,
  ]);
}

// The options of an entry that admits exact JSON: only `limits`, refusing
// other keys as prepare's options do.
/** @internal */
export function limitsOption(options: unknown): Uint32Array | undefined {
  let limits: unknown;
  for (const [key, value] of plainEntries(options, "Options")) {
    if (key !== "limits") invalidLimits("Unknown option; expected limits.");
    limits = value;
  }
  return admissionLimits(limits);
}

// The host's own check for unpaired UTF-16 (ES2024), which every host the
// package is qualified on provides; elsewhere the scan below answers alike.
const hostWellFormed = (
  String.prototype as { isWellFormed?: (this: string) => boolean }
).isWellFormed;

// Whether text holds no unpaired UTF-16 unit, so it crosses the bridge exactly as UTF-8.
/** @internal */
export function wellFormed(text: string): boolean {
  if (hostWellFormed && typeof text === "string")
    return hostWellFormed.call(text);
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
    } else if (c >= 0xdc00 && c <= 0xdfff) return false;
  }
  return true;
}

/** @internal */
export function scalar(text: string): void {
  if (!wellFormed(text))
    throw new TypeError(
      "Unpaired UTF-16 in JSON text; use a JSON \\u escape or exact UTF-8 bytes.",
    );
}

// Exact input as it crosses into the module: text is checked here and the
// bindings encode it straight into module memory, so no JavaScript copy of its
// UTF-8 is made beside the module's own. Input past the byte limit crosses only
// as the prefix that establishes the same refusal.
/** @internal */
export function exact(input: ExactInput, limits?: Uint32Array): ExactInput {
  if (typeof input === "string") scalar(input);
  else if (!(input instanceof Uint8Array))
    throw new TypeError("Expected JSON text or Uint8Array.");
  const maxBytes = limits?.[0] ?? (defaultMaxBytes ??= jsonDefaults()[0]);
  if (input.length <= maxBytes) return input;
  if (typeof input !== "string") return input.subarray(0, maxBytes + 1);
  // UTF-8 has at least as many bytes as scalar text has UTF-16 units. Keep
  // enough units to exceed the byte limit and preserve its entire byte prefix.
  // Include the low surrogate when the cut would split a scalar character.
  let end = maxBytes + 1;
  const last = input.charCodeAt(end - 1);
  if (last >= 0xd800 && last <= 0xdbff) end++;
  return input.slice(0, end);
}

const exactOwners = new WeakSet<object>();

/** @internal */
export function isExact(value: unknown): value is ExactJson {
  return typeof value === "object" && value !== null && exactOwners.has(value);
}

/** @internal */
export const jsonRaw = (owner: ExactJson) => handle<wasm.WasmJson>(owner);

/** Ordinary JSON scalar types; runtime admission additionally requires finite numbers. JavaScript number precision already lost before admission cannot be reconstructed. */
export type JsonPrimitive = null | boolean | number | string;

/** Checked ordinary JSON input, including nested borrowed ExactJson owners for precise subtrees. Arrays must be dense; objects plain with enumerable data properties; cycles, accessors, symbols, bigint and undefined are refused. Admission never invokes toJSON or getters. Ordinary encoding admits at most 33,554,432 UTF-16 code units of serialized text, 1,000,000 nodes counting member names as Rust does, and 10,000 container levels; subsequent exact parsing also enforces 32 MiB UTF-8 and 1,000,000 total nodes, including member-name tokens. A document's edits apply the limits the document was admitted with to the values they write, not to the edit's own structure. Check a larger value as exact JSON from parseJson with larger limits. */
export type JsonInput =
  | JsonPrimitive
  | ExactJson
  | readonly JsonInput[]
  | { readonly [name: string]: JsonInput };

/** Ordinary JavaScript JSON tree returned only after exact conversion checks. Values are detached from Wasm storage and need no disposal. */
export type JsonOutput =
  | JsonPrimitive
  | JsonOutput[]
  | { [name: string]: JsonOutput };

/** Lazy single-pass exact traversal. Every yielded value is an independent owner.
 * Natural exhaustion or return makes later next return done; disposal/failure of a live cursor makes next throw disposed-handle.
 * The first terminal reason wins. Use using on the cursor and on each yielded item.
 */
export interface ExactIterator<T>
  extends IterableIterator<T, undefined>,
    Disposable {
  /** True after any terminal transition. */ readonly disposed: boolean;
  /** Acquire the next independent owner, or report natural completion. */ next(): IteratorResult<
    T,
    undefined
  >;
  /** Close the cursor without releasing previously yielded owners; always returns done. */ return(): IteratorResult<
    T,
    undefined
  >;
  /** Return this single-pass iterator. */ [Symbol.iterator](): this;
  /** Release the cursor; later next throws unless natural exhaustion/return happened first. */ dispose(): void;
}

class ExactCursor<R extends Raw, C extends Raw, T>
  extends Managed
  implements ExactIterator<T>
{
  #terminal: "closed" | "disposed" | undefined;
  constructor(
    raw: R,
    private readonly step: (raw: R) => C | undefined,
    private readonly wrap: (raw: C) => T,
  ) {
    super(raw);
  }
  next(): IteratorResult<T, undefined> {
    if (this.#terminal === "closed") return { done: true, value: undefined };
    const raw = handle<R>(this);
    try {
      const child = this.step(raw);
      if (!child) return this.return();
      return { done: false, value: adopt(child, this.wrap) };
    } catch (error) {
      this.dispose();
      throw error;
    }
  }
  return(): IteratorResult<T, undefined> {
    this.#terminal ??= "closed";
    super.dispose();
    return { done: true, value: undefined };
  }
  dispose(): void {
    this.#terminal ??= "disposed";
    super.dispose();
  }
  [Symbol.iterator](): this {
    return this;
  }
}

/** Independently owned object-member occurrence, including exact names and duplicate occurrences. */
export class ExactMember extends Managed {
  private constructor(raw: wasm.WasmMember) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmMember): ExactMember {
    return new ExactMember(raw);
  }
  /** Zero-based occurrence position within its original object. */
  get index(): number {
    return handle<wasm.WasmMember>(this).index();
  }
  /** Acquire a NEW exact string owner, preserving escaped unpaired code units and original location. */
  name(): ExactJson {
    return new ExactJson(handle<wasm.WasmMember>(this).name());
  }
  /** Acquire a NEW exact value owner, independent of this entry/source/cursor. */
  value(): ExactJson {
    return new ExactJson(handle<wasm.WasmMember>(this).value());
  }
  /** Acquire another independently disposable owner for this occurrence. */
  retain(): ExactMember {
    return ExactMember.fromRaw(handle<wasm.WasmMember>(this).retain());
  }
}

/** Immutable exact JSON owner retaining numeric token spelling and duplicate names. Dispose each owner deterministically (or use Symbol.dispose). Retained/subtree owners share storage and survive parent disposal; they may retain the whole source arena. Text/bytes stay available when ordinary conversion is inexact. */
export class ExactJson extends Managed {
  /** @internal */ constructor(raw: wasm.WasmJson) {
    super(raw);
    exactOwners.add(this);
  }
  /** Admit checked ordinary JSON into a new disposable exact owner. Nested ExactJson inputs are borrowed. Ordinary encoding failures throw TypeError/RangeError; exact parser admission failures also throw. Use parseJson for structured exact-input failures. Handle misuse throws SdkError; precision already lost in JavaScript numbers cannot be recovered. */
  static from(value: JsonInput): ExactJson {
    requireReady();
    const text = encodeOrdinary(value);
    try {
      return new ExactJson(wasm.WasmJson.parseText(text));
    } catch (error) {
      // The checked encoder leaves only a JSON admission limit to refuse it.
      if (inputErrorKind(error) !== "Limit") throw error;
      throw new RangeError(ORDINARY_LIMITS);
    }
  }
  /** Return a new independently disposable owner sharing this exact snapshot; no reparse. */
  retain(): ExactJson {
    return new ExactJson(handle<wasm.WasmJson>(this).retain());
  }
  /** Allocate exact token text in JavaScript, excluding surrounding whitespace; no disposal is needed for the string. */
  get text(): string {
    return handle<wasm.WasmJson>(this).text();
  }
  /** Allocate a detached copy of exact token UTF-8 bytes; mutating the copy does not change this snapshot. */
  get bytes(): Uint8Array {
    return handle<wasm.WasmJson>(this).bytes();
  }
  /** Allocate plain metadata for this value: kind, original byte coordinates and subtree duplicate-name presence. No disposable owner is returned. */
  get metadata(): {
    /** Stable category describing the observed value or failure. */
    kind: string;
    /** Original-source coordinates for this exact value. */
    location: SourceLocation;
    /** Whether this subtree contains repeated decoded object member names. */
    duplicateNames: boolean;
  } {
    return decode(handle<wasm.WasmJson>(this).metadata());
  }
  /** Resolve a relative RFC 6901 pointer and return a NEW disposable subtree owner, or undefined for a malformed pointer or one that names no node. Empty selects this value; repeated names select the first occurrence. A segment may hold an unpaired UTF-16 unit, so a value problem's instance resolves to the node locate() reports. The owner can retain the entire source. */
  at(pointer: string): ExactJson | undefined {
    const raw = handle<wasm.WasmJson>(this);
    // Only a pointer holding an unpaired unit needs the exact JSON route that
    // locate() uses; JSON.stringify escapes that unit.
    const found = wellFormed(pointer)
      ? raw.at(pointer)
      : raw.atPointer(JSON.stringify(pointer));
    return found ? new ExactJson(found) : undefined;
  }
  /** Locate an exact RFC 6901 pointer relative to this value, such as a value problem's instance, in the text it was parsed from: the node's byte offset, line and byte column with the given pointer, or null when it names no node. A segment may hold an unpaired UTF-16 unit. Computed on this call; nothing is retained. A malformed pointer throws SdkError with code invalid-pointer. */
  locate(pointer: string): SourceLocation | null {
    if (typeof pointer !== "string")
      throw new TypeError("A JSON Pointer string is required.");
    const raw = handle<wasm.WasmJson>(this);
    return decode(sdkCall(() => raw.locate(JSON.stringify(pointer))));
  }
  /** Return a NEW disposable owner for the first member with this decoded name, or undefined if absent/not an object. Duplicate names remain available through exact text; this lookup does not prove uniqueness. */
  get(name: string): ExactJson | undefined {
    scalar(name);
    const found = handle<wasm.WasmJson>(this).get(name);
    return found ? new ExactJson(found) : undefined;
  }
  /** Lazily enumerate every member in source order, including duplicate and exact names. Wrong kind returns undefined; an empty object returns an empty cursor. Dispose cursor and each yielded entry independently. */
  members(): ExactIterator<ExactMember> | undefined {
    const raw = jsonRaw(this).members();
    return raw
      ? adopt(
          raw,
          (r) => new ExactCursor(r, (r) => r.next(), ExactMember.fromRaw),
        )
      : undefined;
  }
  /** Lazily enumerate independent exact element owners in array order. Wrong kind returns undefined; empty arrays return an empty cursor. No unvisited child handles are acquired. */
  elements(): ExactIterator<ExactJson> | undefined {
    const raw = jsonRaw(this).elements();
    return raw
      ? adopt(
          raw,
          (r) =>
            new ExactCursor(
              r,
              (r) => r.next(),
              (r) => new ExactJson(r),
            ),
        )
      : undefined;
  }
  /** Compare exact mathematical JSON values without binary64 rounding. Returns undefined when duplicate names make equality ambiguous; borrows both owners. */
  equals(other: ExactJson): boolean | undefined {
    return handle<wasm.WasmJson>(this).equals(jsonRaw(other));
  }
  /** Allocate an ordinary JS value only if exact round-trip comparison succeeds. Returns inexact for duplicate names, numeric rounding or unsupported conversion. Keeps this owner valid and leaves text/bytes available. */
  toValue():
    | {
        /** Result discriminant; narrow this before reading branch-specific fields. */
        status: "converted";
        /** Acquired owner or detached converted value, as specified by this branch. */
        value: JsonOutput;
      }
    | {
        /** Result discriminant; narrow this before reading branch-specific fields. */
        status: "inexact";
        /** Human-readable explanation; use structured discriminants/codes for logic. */
        message: string;
      } {
    if (this.metadata.duplicateNames)
      return {
        status: "inexact",
        message:
          "Repeated JSON names cannot be represented in an ordinary object.",
      };
    let value: unknown;
    try {
      value = JSON.parse(this.text);
    } catch {
      return {
        status: "inexact",
        message: "The host JSON parser cannot represent this value.",
      };
    }
    // Check the host's actual numeric result against the authoritative exact value.
    let roundtrip: ExactJson | undefined;
    try {
      roundtrip = ExactJson.from(value as JsonInput);
      if (this.equals(roundtrip) !== true)
        return {
          status: "inexact",
          message:
            "Ordinary JavaScript conversion would change a JSON value; retain text or bytes.",
        };
    } catch {
      return {
        status: "inexact",
        message:
          "The value exceeds the checked ordinary-value conversion domain.",
      };
    } finally {
      roundtrip?.dispose();
    }
    return { status: "converted", value: value as JsonOutput };
  }
}

/** Parse exact JSON under admission limits, by default 32 MiB UTF-8, 1,000,000 nodes and 10,000 container levels; `limits` overrides them, unspecified entries keeping those defaults, as Rust's `JsonValue::parse_with_limits` does. Returns a caller-owned ExactJson or input-error. Check a value larger than the defaults this way and pass the ExactJson to a prepared schema. Malformed limits or options throw SdkError invalid-json-limits; initialization, wrong input types and literal unpaired UTF-16 text throw; JSON escapes preserve such units. */
export function parseJson(
  input: ExactInput,
  options: {
    /** Admission limits; unspecified entries keep their defaults. */
    limits?: Partial<JsonLimits>;
  } = {},
): ParseResult<ExactJson> {
  requireReady();
  const limits = limitsOption(options);
  const data = exact(input, limits);
  try {
    return {
      status: "parsed",
      value: new ExactJson(
        typeof data === "string"
          ? wasm.WasmJson.parseText(data, limits)
          : wasm.WasmJson.parseBytes(data, limits),
      ),
    };
  } catch (e) {
    return failure(e);
  }
}

/** @internal */
export interface EditEncoding {
  chunks: string[];
  owners: ExactJson[];
  indices: number[];
  seen: Map<ExactJson, number>;
  // An edit batch's own structure, beside its values: its array, rows,
  // operations and pointers, in UTF-8 bytes and nodes as Rust counts them.
  envelopeBytes: number;
  envelopeNodes: number;
}

// The admission limits encoded text is parsed under, bytes, nodes and nesting.
// A document's edits apply the limits it was admitted with to what they write,
// any other value the defaults to all of it.
interface EncodingLimits {
  limits: ArrayLike<number>;
  document: boolean;
}

// An ordinary value the encoder admitted but JSON admission refused.
/** @internal */
export const ORDINARY_LIMITS =
  "The ordinary value exceeds the admitted JSON limits; parse it with parseJson and larger limits.";

// The kind of an engine input error, or undefined for any other exception.
/** @internal */
export function inputErrorKind(error: unknown): string | undefined {
  if (typeof error !== "string") return undefined;
  try {
    return (JSON.parse(error) as { kind?: string }).kind;
  } catch {
    return undefined;
  }
}

function valueEncoding(): EncodingLimits {
  return { limits: jsonDefaults(), document: false };
}

// Rust's EditError refusal for an exceeded limit, which a document's edits give
// whichever route reaches it.
const EDIT_LIMITS = {
  "byte-limit": ["byte", "max_bytes", "maxBytes"],
  "node-limit": ["node", "max_nodes", "maxNodes"],
  "depth-limit": ["nesting", "max_depth", "maxDepth"],
} as const;

function editLimitMessage(code: keyof typeof EDIT_LIMITS): string {
  const [limit, rust, typescript] = EDIT_LIMITS[code];
  return `document edit refused: the edited document exceeds its ${limit} limit; admit the document with a larger JsonLimits::${rust} (limits.${typescript} in TypeScript)`;
}

// UTF-8 bytes of well-formed encoder output; a surrogate pair takes four.
function utf8Length(text: string): number {
  let bytes = text.length;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0x80) bytes += c < 0x800 || (c >= 0xd800 && c <= 0xdfff) ? 1 : 2;
  }
  return bytes;
}

/** Iterative, checked serialization. No getters, toJSON hooks, omission, or implicit coercion. */
/** @internal */
export function encodeOrdinary(
  value: unknown,
  context?: EncodingContext,
  exact?: EditEncoding,
  encoding: EncodingLimits = valueEncoding(),
): string {
  const [maxBytes, maxNodes, maxDepth] = Array.from(encoding.limits) as [
    number,
    number,
    number,
  ];
  const childPath = (_owner: object, name: string, parent: string) =>
    context ? pointerChild(parent, name) : "";
  function reject(
    code: JsonEncodingErrorCode,
    pointer: string,
    message: string,
    limit = false,
  ): never {
    if (context) throw new JsonEncodingError(code, pointer, message);
    if (limit) throw new RangeError(message);
    throw new TypeError(message);
  }
  // A limit the encoded value already exceeds. A document's edit refuses as
  // Rust's edit would; another value names parsing it with limits instead.
  function exceeded(code: keyof typeof EDIT_LIMITS, pointer: string): never {
    // As in Rust, a document limit is the edited document's, with no part pointer.
    if (encoding.document) throw new EditError(code, editLimitMessage(code));
    const setting = EDIT_LIMITS[code][2];
    const limit = { maxBytes, maxNodes, maxDepth }[setting];
    const what = {
      maxBytes: `${limit}-character`,
      maxNodes: `${limit}-node`,
      maxDepth: `${limit}-level nesting`,
    }[setting];
    reject(
      "encoding-limit",
      pointer,
      `Ordinary value exceeds the ${what} limit; parse it with parseJson and a larger limits.${setting}.`,
      true,
    );
  }
  // An edit batch is the envelope around its rows' values; only the values
  // count against the limits, from their own root, as Rust counts the edit.
  // `base` is the depth of the value root a job belongs to.
  type Job =
    | {
        value: unknown;
        depth: number;
        pointer: string;
        content: boolean;
        base: number;
      }
    | { text: string; pointer: string; content: boolean }
    | { leave: object };
  const batch = context?.editBatch === true;
  const jobs: Job[] = [
      { value, depth: 0, pointer: "", content: !batch, base: 0 },
    ],
    active = new Set<object>(),
    parts: string[] = [];
  let size = 0,
    nodes = 0;
  const envelope = (bytes: number, count: number) => {
    if (exact) {
      exact.envelopeBytes += bytes;
      exact.envelopeNodes += count;
    }
  };
  const add = (s: string, pointer: string, content: boolean) => {
    if (content) {
      size += s.length;
      if (size > maxBytes) exceeded("byte-limit", pointer);
    } else envelope(utf8Length(s), 0);
    parts.push(s);
  };
  const counted = (count: number, pointer: string, content: boolean) => {
    if (!content) return envelope(0, count);
    nodes += count;
    if (nodes > maxNodes) exceeded("node-limit", pointer);
  };
  while (jobs.length) {
    const job = jobs.pop()!;
    if ("text" in job) {
      add(job.text, job.pointer, job.content);
      continue;
    }
    if ("leave" in job) {
      active.delete(job.leave);
      continue;
    }
    const { pointer, content } = job;
    const value =
      batch && job.depth === 1 ? normalizeEdit(job.value) : job.value;
    counted(1, pointer, content);
    if (isExact(value)) {
      if (exact) {
        jsonRaw(value); // Check lifecycle before any owner crosses the bridge.
        let index = exact.seen.get(value);
        if (index === undefined) {
          index = exact.owners.length;
          exact.seen.set(value, index);
          exact.owners.push(value);
        }
        exact.chunks.push(parts.join(""));
        parts.length = 0;
        exact.indices.push(index);
      } else add(value.text, pointer, content);
      continue;
    }
    if (
      value === null ||
      typeof value === "boolean" ||
      typeof value === "string"
    ) {
      add(JSON.stringify(value), pointer, content);
      continue;
    }
    if (typeof value === "number") {
      if (!Number.isFinite(value))
        reject("non-finite-number", pointer, "JSON numbers must be finite.");
      add(JSON.stringify(value), pointer, content);
      continue;
    }
    if (typeof value !== "object")
      reject(
        "unsupported-value",
        pointer,
        "JSON cannot contain undefined, bigint, functions, or symbols.",
      );
    // A container nests one level below its value's root, the root included.
    if (content && job.depth - job.base + 1 > maxDepth)
      exceeded("depth-limit", pointer);
    if (active.has(value))
      reject("cyclic-value", pointer, "JSON cannot contain cycles.");
    active.add(value);
    jobs.push({ leave: value });
    const array = Array.isArray(value),
      prototype = Object.getPrototypeOf(value);
    if (!array && prototype !== Object.prototype && prototype !== null)
      reject(
        "non-plain-object",
        pointer,
        "JSON objects must be plain objects.",
      );
    const descriptors = Object.getOwnPropertyDescriptors(value);
    if (Reflect.ownKeys(value).some((k) => typeof k === "symbol"))
      reject("symbol-key", pointer, "JSON objects cannot contain symbol keys.");
    if (array) {
      const names = Object.keys(descriptors).filter((k) => k !== "length");
      if (
        names.length !== value.length ||
        names.some((k, i) => k !== String(i))
      ) {
        // Stop at the first missing index without scanning an arbitrarily sparse length.
        let missing = 0;
        while (Object.hasOwn(descriptors, String(missing))) missing++;
        const hole = missing < value.length;
        reject(
          hole ? "sparse-array" : "array-property",
          hole ? childPath(value, String(missing), pointer) : pointer,
          "JSON arrays must be dense and have no extra properties.",
        );
      }
      jobs.push({ text: "]", pointer, content });
      for (let i = value.length - 1; i >= 0; i--) {
        const d = descriptors[String(i)];
        if (!d || !("value" in d))
          reject(
            "accessor-property",
            childPath(value, String(i), pointer),
            "JSON arrays cannot contain accessors or holes.",
          );
        if (i < value.length - 1) jobs.push({ text: ",", pointer, content });
        jobs.push({
          value: d.value,
          depth: job.depth + 1,
          pointer: childPath(value, String(i), pointer),
          content,
          base: job.base,
        });
      }
      jobs.push({ text: "[", pointer, content });
    } else {
      const entries = Object.entries(descriptors);
      // Member names are nodes, as Rust counts them.
      counted(entries.length, pointer, content);
      jobs.push({ text: "}", pointer, content });
      for (let i = entries.length - 1; i >= 0; i--) {
        const [key, d] = entries[i];
        if (!d.enumerable || !("value" in d))
          reject(
            "value" in d ? "non-enumerable-property" : "accessor-property",
            childPath(value, key, pointer),
            "JSON objects must contain enumerable data properties.",
          );
        // A batch row's value is what the edit writes.
        const child = content || (batch && job.depth === 1 && key === "value");
        if (i < entries.length - 1) jobs.push({ text: ",", pointer, content });
        jobs.push({
          value: d.value,
          depth: job.depth + 1,
          pointer: childPath(value, key, pointer),
          content: child,
          base: content ? job.base : job.depth + 1,
        });
        jobs.push({
          text: JSON.stringify(key) + ":",
          pointer: childPath(value, key, pointer),
          content,
        });
      }
      jobs.push({ text: "{", pointer, content });
    }
  }
  return parts.join("");
}

interface EncodingContext {
  editBatch?: boolean;
}

function normalizeEdit(value: unknown): unknown {
  const entries = ownEntries(value, "");
  const fields = Object.fromEntries(entries);
  const allowed =
    fields.op === "set"
      ? ["op", "pointer", "value"]
      : fields.op === "remove"
        ? ["op", "pointer"]
        : undefined;
  if (!allowed)
    throw new EditError(
      "invalid-edits",
      "Expected an edit op of set or remove.",
    );
  if (entries.some(([key]) => !allowed.includes(key)))
    throw new EditError("invalid-edits", "Unexpected edit member.");
  if (typeof fields.pointer !== "string")
    throw new EditError("invalid-pointer", "Edit pointers must be strings.");
  if (fields.op === "set" && !Object.hasOwn(fields, "value"))
    throw new EditError("invalid-edits", "Set edits require a value.");
  return value;
}

function pointerChild(parent: string, name: string): string {
  return parent + "/" + name.replace(/~/g, "~0").replace(/\//g, "~1");
}

/** @internal */
export function ownEntries(
  object: unknown,
  pointer?: string,
): [string, unknown][] {
  function invalid(
    code: JsonEncodingErrorCode,
    at: string,
    message: string,
  ): never {
    if (pointer === undefined) throw new TypeError(message);
    throw new JsonEncodingError(code, at, message);
  }
  if (
    !object ||
    typeof object !== "object" ||
    Array.isArray(object) ||
    ![null, Object.prototype].includes(Object.getPrototypeOf(object))
  )
    invalid(
      "invalid-json-object",
      pointer ?? "",
      "JSON objects must be plain objects.",
    );
  if (Reflect.ownKeys(object).some((key) => typeof key === "symbol"))
    invalid(
      "symbol-key",
      pointer ?? "",
      "JSON objects cannot contain symbol keys.",
    );
  return Object.entries(Object.getOwnPropertyDescriptors(object)).map(
    ([key, d]) => {
      if (!d.enumerable || !("value" in d))
        invalid(
          "value" in d ? "non-enumerable-property" : "accessor-property",
          pointerChild(pointer ?? "", key),
          "JSON encoding requires enumerable data properties.",
        );
      return [key, d.value];
    },
  );
}
