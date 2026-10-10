/** Portable OpenBindings APIs. Call initialize once before constructing values. */
import init, * as wasm from "./wasm/openbindings_wasm.js";
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
/** Initialize this module instance once before using SDK APIs. Concurrent calls share one promise; a failed attempt can be retried. Requires host crypto.getRandomValues (request-scoped hosts should call inside a request). Throws SdkError for initialization/entropy failures. Supplying compiled bytes/module avoids a default asset fetch. */
export function initialize(
  input?: WasmInput | Promise<WasmInput>,
): Promise<void> {
  return (initialization ??= (async () => {
    const module = await input;
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
    await init(module === undefined ? undefined : { module_or_path: module });
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
function requireReady(): void {
  if (!ready)
    throw new SdkError(
      "not-initialized",
      "Call and await initialize() before using the SDK.",
    );
}
/** Exact JSON source: UTF-8 bytes or JavaScript scalar text encoded as UTF-8. Text containing literal unpaired UTF-16 units throws; represent those units using JSON \u escapes or exact bytes. Parsing copies input into immutable storage. */
export type ExactInput = string | Uint8Array;
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
    /** Human-readable explanation; use structured discriminants/codes for logic. */
    message: string;
  };
}
/** Parse result only. The parsed branch transfers one disposable owner to the caller; it establishes neither document conformance nor schema readiness. */
export type ParseResult<T> =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "parsed";
      /** Parsed owner or detached converted value, as specified by this branch. */
      value: T;
    }
  | InputFailure;
/** Closed per-rule evidence partition: satisfied establishes the rule; violated establishes a counterexample; inconclusive lacks proof; not-applicable records a failed prerequisite. Findings explain evidence but do not replace it. */
export type Evidence =
  | "satisfied"
  | "violated"
  | "inconclusive"
  | "not-applicable";
/** Coordinates in original UTF-8 source. byteOffset is zero-based; line and byteColumn are one-based. A byte column is not a JavaScript UTF-16 editor column: convert against the original source. Pointers are untrusted display data and repeated keys may share a pointer. */
export interface SourceLocation {
  /** RFC 6901 pointer; empty string means root, null means unavailable. Use byteOffset to distinguish duplicate key occurrences. */
  pointer: string | null;
  /** Zero-based original UTF-8 byte offset. */
  byteOffset: number;
  /** One-based original source line. */
  line: number;
  /** One-based UTF-8 byte column, not a UTF-16 character index. */
  byteColumn: number;
}
/** One explanatory normative-rule finding. Use rule/code/status for logic and render message/pointer as text. Retained coordinates refer to original source; bounded findings do not replace the report evidence map. */
export interface Finding {
  /** Normative rule identifier such as OBI-01. */
  rule: string;
  /** Evidence expressed by this finding; consult the report map for aggregate rule evidence. */
  status: Evidence;
  /** Stable detailed diagnostic identifier. */
  code: string;
  /** Original source coordinate, or null when unavailable. */
  location: SourceLocation | null;
  /** Human-readable explanation; render as text rather than HTML. */
  message: string;
}
/** Normative OBI assessment at an exact specification revision. All rules retain independent evidence even if findings are omitted. Presentation retains at most 4096 findings and 8 MiB aggregate generated-pointer UTF-8 bytes; findingsTruncated records omissions without changing the conclusion. */
export interface ConformanceReport {
  /** Applied specification release. */
  release: string;
  /** Exact applied specification Git revision. */
  revision: string;
  /** Assessment-policy identifier. */
  policy: string;
  /** Aggregate normative conclusion derived from independent rule evidence. */
  conclusion: "conformant" | "non-conformant" | "undetermined";
  /** Entry for every OBI rule, including inconclusive and not-applicable. */
  evidence: Readonly<Record<string, Evidence>>;
  /** Retained explanatory findings, bounded independently of rule evidence. */
  findings: readonly Finding[];
  /** True when the 4096-finding or 8 MiB aggregate generated-pointer byte cap omitted findings. Omission never changes evidence or substitutes a shortened pointer. */
  findingsTruncated: boolean;
}
/** Normative assessment or a distinct unsupported-version refusal. An assessed report can be conformant, non-conformant or undetermined. This plain-data result owns no handles; use ParsedDocument.validate() to obtain a retained proof. */
export type Assessment =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "assessed";
      /** Plain normative report for the exact snapshot. */
      report: ConformanceReport;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "version-refused";
      /** Declared/supported version refusal, distinct from normative evidence. */
      refusal: {
        /** Original unsupported declared version. */
        declared: string;
        /** Supported specification line. */
        supported: string;
      };
    };
/** Closed choice of the normative operation input or output schema field. */
export type Side = "input" | "output";
/** Original schema resource/pointer identity. resource null denotes the OpenBindings document; supplied URIs identify explicit resources and never initiate acquisition. Private evaluator projection identities are not exposed. */
export interface SchemaLocation {
  /** Original supplied-resource URI, or null for the OpenBindings document. */
  resource: string | null;
  /** RFC 6901 pointer within that original source; empty string denotes root. */
  pointer: string;
}
/** Known refusal causes for this facade and its bundled engine. Unsupported capability, conservative preparation, unavailable resources, limits, cancellation and evaluator failure do not establish satisfaction/failure. Undefined is reserved for proved semantic undefinedness; a potential cycle alone is conservative-preparation. Later package versions may add causes. For unavailable resources, ParsedDocument.references() exposes explicit reference spellings and original keyword locations; apply an application disclosure policy before logging them. */
export type NoVerdictReason =
  | "unsupported-capability"
  | "conservative-preparation"
  | "resource-unavailable"
  | "limit-exceeded"
  | "cancelled"
  | "evaluator-failure"
  | "undefined";
/** Structured reason no schema verdict was established. code is the stable detail identifier; message is explanatory and location is an original schema coordinate when known. */
export interface NoVerdict {
  /** Extensible broad refusal category. */
  reason: NoVerdictReason;
  /** Stable evaluator-defined refusal detail code. */
  code: string;
  /** Human-readable explanation; use reason/code for logic. */
  message: string;
  /** Original schema location when known, otherwise null. */
  location: SchemaLocation | null;
}
/** Opt-in schema facts, not rejected instance data. Bounds and enum choices are exact JSON token strings: do not coerce them to Number. Source facts can contain secrets and must be rendered as text under an application disclosure policy. Each variant is atomic; truncated contains no partial facts. */
export type ValueProblemDetails =
  | {
      /** Complete expected JSON type names, drawn from the fixed JSON type vocabulary. */
      kind: "type";
      /** Fixed expected type names. */
      expected: readonly string[];
    }
  | {
      /** One established missing member, verified against the original required array. */
      kind: "required";
      /** Source-controlled missing member name; the problem pointer still identifies its existing parent object. */
      member: string;
    }
  | {
      /** Exact numeric bound; the problem code supplies direction and inclusivity. */
      kind: "numeric-bound";
      /** Original JSON number token, never a rounded JavaScript number. */
      bound: string;
    }
  | {
      /** Exact length/count bound; the problem code identifies the unit and direction. */
      kind: "size-bound";
      /** Original JSON number token. */
      bound: string;
    }
  | {
      /** Complete allowed choices; the array is never a partial prefix. */
      kind: "enum";
      /** Original exact JSON token for every choice, including source-controlled strings/objects and exact numbers. */
      choices: readonly string[];
    }
  | {
      /** Applicable requested facts were omitted for the byte budget; outer problemsComplete is false. */
      kind: "truncated";
    };
/** Established instance failure, with an existing instance pointer and optional original schema coordinate. Failure messages avoid echoing instance values by default; pointers still contain source-controlled strings and must be rendered as text. */
export interface ValueProblem {
  /** RFC 6901 pointer to an existing input location; empty string means root. */
  instancePointer: string;
  /** Original schema keyword coordinate when mapped, otherwise null. */
  schemaLocation: SchemaLocation | null;
  /** Stable evaluator-defined problem code, commonly the failed keyword. */
  code: string;
  /** Explanatory text; present pointers separately and safely. */
  message: string;
  /** Optional schema facts. Absent means disabled or unavailable; truncated explicitly marks byte-budget omission. */
  details?: ValueProblemDetails;
}
/** Closed selected-schema result partition: satisfies, fails, or no-verdict. An established failure may have incomplete diagnostics; problemsComplete does not weaken its verdict. Neither satisfies nor fails proves normative OBI conformance of the surrounding draft. */
export type ValueOutcome =
  | {
      /** Semantic result discriminant; narrow before reading branch-specific data. */
      outcome: "satisfies";
    }
  | {
      /** Semantic result discriminant; narrow before reading branch-specific data. */
      outcome: "fails";
      /** Retained actual failing instance locations. */
      problems: readonly ValueProblem[];
      /** Whether selected failure diagnostics, including requested details, completed; false does not weaken the established failure. */
      problemsComplete: boolean;
    }
  | {
      /** Semantic result discriminant; narrow before reading branch-specific data. */
      outcome: "no-verdict";
      /** Structured no-verdict reason; no satisfaction/failure is claimed. */
      detail: NoVerdict;
    };
/** Name lookup after successful interpretation. Found transfers a disposable OperationView; missing means no occurrence; ambiguous retains distinct primary keys in lexical order, possibly one key for repeated aliases. Invalid typed namespace structure throws SdkError. */
export type OperationSelection =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "found";
      /** Caller-owned selected operation view; dispose independently. */
      operation: OperationView;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "missing";
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "ambiguous";
      /** Distinct primary keys in lexical order; repeated occurrences can yield one key. */
      candidates: readonly string[];
    };
/** Setup partition: ready transfers a disposable PreparedContract; no-contract means the selected side is absent; operation-missing/operation-ambiguous are selection outcomes; no-verdict is preparation refusal. Present false is a contract. None of these setup refusals judges an instance. */
export type ContractPreparation =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "ready";
      /** Caller-owned prepared contract, valid independently of its context. */
      contract: PreparedContract;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "no-contract" | "operation-missing";
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "operation-ambiguous";
      /** Distinct primary keys in lexical order; repeated occurrences can yield one key. */
      candidates: readonly string[];
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "no-verdict";
      /** Structured no-verdict reason; no satisfaction/failure is claimed. */
      detail: NoVerdict;
    };
/** Expected refusal while converting an ordinary caller value into exact JSON, before schema evaluation. The code identifies the unsupported shape; instancePointer is a logical ordinary-value path, never invented source coordinates. */
export interface ValueAdmissionFailure {
  /** Stable ordinary-value admission category; returned before schema evaluation. */
  readonly code:
    | "non-finite-number"
    | "unsupported-value"
    | "sparse-array"
    | "array-property"
    | "cyclic-value"
    | "non-plain-object"
    | "accessor-property"
    | "non-enumerable-property"
    | "symbol-key"
    | "input-limit";
  /** Pointer into the ordinary caller value; no source bytes are invented. */
  readonly instancePointer: string | null;
  /** Explanatory ordinary-value admission failure. */
  readonly message: string;
}
/** Ordinary-value validation result: admission may return input-error; after admission, the result is the same selected-schema ValueOutcome as for an ExactJson root. */
export type ValueCheck =
  | ValueOutcome
  | {
      /** Semantic result discriminant; narrow before reading branch-specific data. */
      outcome: "input-error";
      /** Structured expected failure for this branch. */
      error: ValueAdmissionFailure;
    };
/** One reference keyword in a supported schema position. Opaque content is not interpreted as schema; located is static resolution, and dynamicLookup still requires evaluation-time scope handling. */
export interface Reference {
  /** Original reference-keyword location. */
  location: SchemaLocation;
  /** Reference keyword spelling, such as $ref or $dynamicRef. */
  keyword: string;
  /** Original scalar string, or null when not representable. */
  spelling: string | null;
  /** Static resolution or structured refusal; does not acquire resources. */
  resolution:
    | {
        /** Located establishes a static target; unresolved preserves the refusal. */
        outcome: "located";
        /** Original source target coordinate. */
        target: SchemaLocation;
        /** Whether runtime dynamic-anchor scope must still be applied. */
        dynamicLookup: boolean;
      }
    | {
        /** Located establishes a static target; unresolved preserves the refusal. */
        outcome: "unresolved";
        /** Structured unresolved-reference cause. */
        detail: NoVerdict;
      };
}
/** Original-context reference inspection, separate from normative proof and preparation. complete describes traversal completion; a complete report can still contain unresolved reference results. */
export interface ReferenceReport {
  /** Retained reference observations in traversal order. */
  references: readonly Reference[];
  /** Whether traversal completed; unresolved entries can occur even when true. */
  complete: boolean;
  /** Traversal-level limitation when incomplete, otherwise null. */
  limitation: NoVerdict | null;
}
/** Cooperative work control. A pre-aborted signal returns cancellation. Synchronous Wasm does not yield to same-thread JavaScript, so use a worker plus host scheduling to interrupt long synchronous work externally. This is not a wall-clock deadline. */
export interface WorkOptions {
  /** Optional caller-owned cooperative cancellation signal; synchronous Wasm only observes same-thread cancellation at call boundaries. */
  signal?: AbortSignal;
}
/** Default-evaluator finite budgets, each a nonnegative 32-bit integer. Work counts are implementation units, not milliseconds. Zero is literal except maxProblems has a minimum count allowance of one; a byte budget may retain zero problems; exhaustion produces no-verdict unless only post-verdict diagnostics are incomplete. */
export interface EvaluatorLimits {
  /** Work units per verdict/diagnostic pass; default 2,000,000. */
  evaluationSteps: number;
  /** Nested evaluation depth; default 1024. */
  evaluationDepth: number;
  /** Regex evaluation/backtracking work budget; default 2,000,000. */
  regexSteps: number;
  /** Retained failure diagnostics; default 256, minimum count allowance one; byte admission may retain none. Truncation clears problemsComplete. */
  maxProblems: number;
  /** Aggregate retained UTF-8 bytes in failure pointers, resource identifiers, codes, messages and requested detail strings; default 1,048,576. Zero returns an established failure with empty, incomplete diagnostics. Not a heap or wire-byte limit. */
  diagnosticBytes: number;
  /** Projected JSON nesting admitted to compilation; default 512. */
  compileJsonDepth: number;
  /** Maximum UTF-8 bytes per schema regex; default 1,048,576 (1 MiB). */
  patternBytes: number;
  /** Maximum regex parenthesis nesting; default 256. */
  patternDepth: number;
}
/** Immutable context inputs and bounded preparation reuse. The built-in JSON Schema 2020-12 evaluator performs no resource I/O, treats format as annotation, declines unqualified Unicode property-escape matching, and may conservatively refuse potential non-progressing cycles. */
export interface ContractOptions {
  /** Explicit resources retained by the context; omitted means an empty caller set. The passed owner remains caller-owned. */
  resources?: SchemaResources;
  /** Overrides for built-in evaluator budgets; unspecified entries use evaluatorLimits(). */
  limits?: Partial<EvaluatorLimits>;
  /** Most-recently-used preparations retained by the context; default 4, zero disables caching. */
  cacheCapacity?: number;
  /** Opt into source-controlled schema facts (default false). Ready validators then retain original document/resource snapshots until their last owner is released. Details obey diagnosticBytes, may expose secrets, and never include rejected instance values. Invalid non-boolean values throw SdkError. */
  includeSchemaDetails?: boolean;
}
/** Explicit resources and cooperative cancellation for reference inspection; no retrieval or global resource registry is used. */
export interface ReferenceOptions extends WorkOptions {
  /** Explicit resource set borrowed for inspection; no owner is transferred and no URI is fetched. */
  resources?: SchemaResources;
}
/** Return a fresh frozen copy of default evaluator budgets. Requires initialization; values describe work/depth/retention units rather than elapsed-time deadlines. */
export function evaluatorLimits(): Readonly<EvaluatorLimits> {
  requireReady();
  return Object.freeze(decode<EvaluatorLimits>(wasm.evaluatorLimits()));
}
function encodedLimits(limits: Partial<EvaluatorLimits> = {}): string {
  const defaults = evaluatorLimits();
  const encoded: Record<string, number> = {};
  for (const [key, value] of ownEntries(limits)) {
    if (
      !Object.hasOwn(defaults, key) ||
      typeof value !== "number" ||
      !Number.isSafeInteger(value) ||
      value < 0 ||
      value > 0xffffffff
    )
      throw new SdkError(
        "invalid-evaluator-limits",
        "Evaluator limits must be known nonnegative 32-bit integers.",
      );
    encoded[key.replace(/[A-Z]/g, (c) => "_" + c.toLowerCase())] = value;
  }
  return JSON.stringify(encoded);
}
/** Frozen metadata snapshot in lexical primary-key order. It contains no disposable owners and is not evidence of conformance, schema support or binding priority. */
export interface OperationMetadata {
  /** Primary operation key, independent of the alias used to select it. */
  readonly key: string;
  /** Optional decoded description; null means absent. */
  readonly description: string | null;
  /** Frozen aliases preserving declaration order; null means absent. */
  readonly aliases: readonly string[] | null;
  /** Frozen declared tags in order; null means absent. */
  readonly tags: readonly string[] | null;
  /** Declared deprecation annotation; null means absent. */
  readonly deprecated: boolean | null;
  /** Whether an input member is present, without proving it is a supported schema. */
  readonly hasInput: boolean;
  /** Whether an output member is present, without proving it is a supported schema. */
  readonly hasOutput: boolean;
}
/** Frozen binding declaration metadata; annotations do not select or execute a realization. */
export interface BindingMetadata {
  /** Binding declaration key. */ readonly key: string;
  /** Declared primary operation key. */ readonly operation: string;
  /** Declared source key; its target may be missing. */ readonly source: string;
  /** Declared description or null for absence. */ readonly description:
    | string
    | null;
  /** Exact interoperable preference integer or null; no ranking is performed. */ readonly preference:
    | number
    | null;
  /** Declared idempotence or null for absence. */ readonly idempotent:
    | boolean
    | null;
  /** Declared deprecation or null for absence. */ readonly deprecated:
    | boolean
    | null;
  /** Presence of content, including explicit JSON null. */ readonly hasContent: boolean;
}
/** Frozen source declaration metadata, excluding opaque content. */
export interface SourceMetadata {
  /** Source declaration key. */ readonly key: string;
  /** Exact declared kind string; no normalization or execution. */ readonly kind: string;
  /** Declared description or null for absence. */ readonly description:
    | string
    | null;
  /** Presence of content, including explicit JSON null. */ readonly hasContent: boolean;
}
/** Frozen dependency declaration metadata; no provider is selected. */
export interface DependencyMetadata {
  /** Dependency declaration key. */ readonly key: string;
  /** Declared operation key. */ readonly operation: string;
  /** Declared description or null for absence. */ readonly description:
    | string
    | null;
  /** Frozen declared kind strings; null accepts all, an empty list accepts none. */ readonly kinds:
    | readonly string[]
    | null;
}
/** Frozen named-example metadata; values and their truth are separate. */
export interface ExampleMetadata {
  /** Example declaration key. */ readonly key: string;
  /** Declared description or null for absence. */ readonly description:
    | string
    | null;
  /** Presence of input, including explicit null. */ readonly hasInput: boolean;
  /** Presence of output, including explicit null. */ readonly hasOutput: boolean;
}
function freezeMetadata<T extends object>(row: T): T {
  for (const value of Object.values(row))
    if (Array.isArray(value)) Object.freeze(value);
  return Object.freeze(row);
}
function metadataRows<T extends object>(text: string): readonly T[] | null {
  const rows = decode<T[] | null>(text);
  return rows === null ? null : Object.freeze(rows.map(freezeMetadata));
}
function camel(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(camel);
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.entries(value).map(([key, v]) => [
        key.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()),
        camel(v),
      ]),
    );
  return value;
}
function decode<T>(text: string): T {
  return camel(JSON.parse(text)) as T;
}
/** Thrown SDK misuse, configuration or typed-interpretation failure. Expected parse, authoring, conformance and schema outcomes use result unions. code is the broad category; interpretationCode, when present, preserves a specific interpretation cause. location holds original byte coordinates. Error messages are explanatory. */
export class SdkError extends Error {
  /** Construct a structured thrown error with a broad code, explanatory message and optional original-source location. */
  constructor(
    /** Broad stable SDK error category; expected semantic outcomes use result unions. */
    readonly code: string,
    message: string,
    /** Optional frozen original UTF-8 source coordinates. */
    readonly location?: Readonly<SourceLocation>,
    /** Specific interpretation refusal; broad code remains "interpretation". */
    readonly interpretationCode?: string,
  ) {
    super(message);
    if (location) this.location = Object.freeze({ ...location });
    this.name = "SdkError";
  }
}
function sdkCall<T>(call: () => T): T {
  try {
    return call();
  } catch (error) {
    if (typeof error === "string") {
      try {
        const info = decode<{
          code: string;
          message: string;
          location?: SourceLocation | null;
          interpretationCode?: string;
        }>(error);
        if (typeof info.code === "string" && typeof info.message === "string")
          throw new SdkError(
            info.code,
            info.message,
            info.location ?? undefined,
            typeof info.interpretationCode === "string"
              ? info.interpretationCode
              : undefined,
          );
      } catch (decoded) {
        if (decoded instanceof SdkError) throw decoded;
      }
    }
    throw error;
  }
}
function failure(error: unknown): InputFailure {
  let info: InputFailure["error"];
  try {
    info = decode(String(error));
  } catch {
    throw error;
  }
  if (!info || typeof info.code !== "string") throw error;
  return { status: "input-error", error: info };
}
function scalar(text: string): void {
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff))
        throw new TypeError(
          "Unpaired UTF-16 in JSON text; use a JSON \\u escape or exact UTF-8 bytes.",
        );
    } else if (c >= 0xdc00 && c <= 0xdfff)
      throw new TypeError(
        "Unpaired UTF-16 in JSON text; use a JSON \\u escape or exact UTF-8 bytes.",
      );
  }
}
function bytes(input: ExactInput): Uint8Array {
  if (typeof input === "string") {
    scalar(input);
    return new TextEncoder().encode(input);
  }
  if (!(input instanceof Uint8Array))
    throw new TypeError("Expected JSON text or Uint8Array.");
  return input;
}
type Raw = { free(): void };
const finalizer =
  typeof FinalizationRegistry === "undefined"
    ? undefined
    : new FinalizationRegistry<Raw>((raw) => raw.free());
const handles = new WeakMap<object, Raw>();
function registerHandle(owner: object, raw: Raw, finalize: boolean): void {
  handles.set(owner, raw);
  if (finalize) finalizer?.register(owner, raw, owner);
}
function handle<T extends Raw>(owner: Managed): T {
  const raw = handles.get(owner);
  if (!raw)
    throw new SdkError("disposed-handle", "This SDK handle has been disposed.");
  return raw as T;
}
abstract class Managed {
  protected constructor(raw: Raw);
  /** @internal */ protected constructor(raw: Raw, finalize: boolean);
  protected constructor(raw: Raw, finalize = true) {
    registerHandle(this, raw, finalize);
  }
  /** Whether this handle has been released. Safe to inspect after disposal. */
  get disposed(): boolean {
    return !handles.has(this);
  }
  /** Release this owner deterministically; idempotent. Other retained owners remain usable. Later handle access throws disposed-handle. Release is not a promise of reduced process RSS or Wasm memory capacity. */
  dispose(): void {
    const raw = handles.get(this);
    if (raw) {
      handles.delete(this);
      finalizer?.unregister(this);
      raw.free();
    }
  }
  /** Explicit resource-management alias for dispose(); use with JavaScript using where supported. */
  [Symbol.dispose](): void {
    this.dispose();
  }
}
const exactOwners = new WeakSet<object>();
function isExact(value: unknown): value is ExactJson {
  return typeof value === "object" && value !== null && exactOwners.has(value);
}
const jsonRaw = (owner: ExactJson) => handle<wasm.WasmJson>(owner);
const documentRaw = (owner: ParsedDocument) => handle<wasm.WasmDocument>(owner);
const resourceRaw = (owner: SchemaResources) =>
  handle<wasm.WasmResources>(owner);
/** Ordinary JSON scalar types; runtime admission additionally requires finite numbers. JavaScript number precision already lost before admission cannot be reconstructed. */
export type JsonPrimitive = null | boolean | number | string;
/** Checked ordinary JSON input, including nested borrowed ExactJson owners for precise subtrees. Arrays must be dense; objects plain with enumerable data properties; cycles, accessors, symbols, bigint and undefined are refused. Admission never invokes toJSON or getters. Ordinary encoding admits at most 67,108,864 UTF-16 code units of serialized text, 1,000,000 visited values and depth 10,000; subsequent exact parsing also enforces 64 MiB UTF-8 and 1,000,000 total nodes, including member-name tokens. */
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
function adopt<R extends Raw, T>(raw: R, construct: (raw: R) => T): T {
  try {
    return construct(raw);
  } catch (error) {
    raw.free();
    throw error;
  }
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
  get name(): ExactJson {
    return new ExactJson(handle<wasm.WasmMember>(this).name());
  }
  /** Acquire a NEW exact value owner, independent of this entry/source/cursor. */
  get value(): ExactJson {
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
    return new ExactJson(wasm.WasmJson.parseText(encodeOrdinary(value)));
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
  /** Resolve a relative RFC 6901 pointer and return a NEW disposable subtree owner, or undefined. Empty selects this value; repeated names select the first occurrence. The owner can retain the entire source. */
  at(pointer: string): ExactJson | undefined {
    scalar(pointer);
    const found = handle<wasm.WasmJson>(this).at(pointer);
    return found ? new ExactJson(found) : undefined;
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
        /** Parsed owner or detached converted value, as specified by this branch. */
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
/** Parse exact JSON using default admission limits (64 MiB UTF-8, 10,000 container depth, 1,000,000 nodes). Returns a caller-owned ExactJson or input-error. Initialization, wrong input types and literal unpaired UTF-16 text throw; JSON escapes preserve such units. */
export function parseJson(input: ExactInput): ParseResult<ExactJson> {
  requireReady();
  const data = bytes(input);
  try {
    return {
      status: "parsed",
      value: new ExactJson(wasm.WasmJson.parseBytes(data)),
    };
  } catch (e) {
    return failure(e);
  }
}
/** Parse an exact immutable document under default admission limits. The parsed branch transfers a disposable ParsedDocument but proves no OBI rules. Use validate() for a retained proof or assess() for plain evidence. Initialization, wrong input types and literal unpaired UTF-16 text throw; JSON escapes preserve such units. */
export function parseDocument(input: ExactInput): ParseResult<ParsedDocument> {
  requireReady();
  const data = bytes(input);
  try {
    return {
      status: "parsed",
      value: new ParsedDocument(wasm.WasmDocument.parseBytes(data)),
    };
  } catch (e) {
    return failure(e);
  }
}
/** Assess all normative document rules directly from exact source, including invalid JSON. Unsupported declared versions return version-refused; parse/limit failures become truthful evidence. Returns plain data with no handle to dispose. Initialization, wrong input types and literal unpaired UTF-16 text throw; represent unpaired units with JSON escapes or exact UTF-8 bytes. */
export function assessDocument(input: ExactInput): Assessment {
  requireReady();
  return decode(wasm.assessBytes(bytes(input)));
}
/** Return authoring default, supported specification line and exact applied specification revision. These identities are independent of the npm package version. */
export function versionPolicy(): {
  /** Default version emitted by typed authoring. */
  authoringVersion: string;
  /** Supported stable specification line. */
  supportedVersions: string;
  /** Exact applied specification Git revision. */
  appliedSpecRevision: string;
} {
  requireReady();
  return decode(wasm.versionPolicy());
}
/** Classify strict SemVer: stable 0.2.x is supported, prereleases/other well-formed lines unsupported, invalid syntax malformed. Build metadata does not affect support and numeric components have no machine-integer ceiling. */
export function checkVersion(
  version: string,
): "supported" | "malformed" | "unsupported" {
  requireReady();
  scalar(version);
  return wasm.checkVersion(version) as ReturnType<typeof checkVersion>;
}
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
  return wasm.liveStorageOwners();
}
/** Disposable immutable exact snapshot without normative proof. retain() creates an independent owner. Assessment may be cached in Rust; operations caches frozen plain metadata. Owner-producing properties create fresh handles on every access and require separate disposal. */
export class ParsedDocument extends Managed {
  #operations?: readonly OperationMetadata[];
  #bindings?: readonly BindingMetadata[] | null;
  #sources?: readonly SourceMetadata[] | null;
  #dependencies?: readonly DependencyMetadata[] | null;
  /** @internal */ constructor(raw: wasm.WasmDocument) {
    super(raw);
  }
  /** Create an independently disposable owner sharing this immutable snapshot and Rust caches. */
  retain(): ParsedDocument {
    return new ParsedDocument(handle<wasm.WasmDocument>(this).retain());
  }
  /** Allocate a detached copy of the complete original UTF-8 source, including formatting. */
  get originalBytes(): Uint8Array {
    return handle<wasm.WasmDocument>(this).originalBytes();
  }
  /** Return a NEW independently disposable ExactJson owner on every access. Cache it locally and dispose it; disposing this document does not release that separate owner. */
  get value(): ExactJson {
    return new ExactJson(handle<wasm.WasmDocument>(this).value());
  }
  /** Return plain normative evidence or version refusal; no proof owner is created and no result needs disposal. */
  assess(): Assessment {
    return decode(handle<wasm.WasmDocument>(this).assess());
  }
  /** Establish normative conformance and, only on validated, return a NEW independent proof owner. The original document remains caller-owned. Other branches preserve plain evidence/refusal. */
  validate(): ValidationResult {
    return validateParsed(this);
  }
  /** Return cached frozen plain metadata in lexical primary-key order. No handles are created. Throws a located interpretation error for malformed typed namespace/metadata; does not assess all OBI rules. */
  get operations(): readonly OperationMetadata[] {
    handle<wasm.WasmDocument>(this);
    return (this.#operations ??= Object.freeze(
      decode<OperationMetadata[]>(
        sdkCall(() => handle<wasm.WasmDocument>(this).operations()),
      ).map((op) =>
        Object.freeze({
          ...op,
          aliases: op.aliases ? Object.freeze(op.aliases) : null,
          tags: op.tags ? Object.freeze(op.tags) : null,
        }),
      ),
    ));
  }
  /** Convert through Rust's typed authoring model into a new registry-owned editable draft. Expected invalid typed data returns located authoring-error; no conformance proof is created. */
  toDraft(): DraftResult {
    let raw: wasm.WasmDraft;
    try {
      raw = documentRaw(this).toDraft();
    } catch (error) {
      if (typeof error === "string") {
        let result: DraftResult | undefined;
        try {
          result = decode<DraftResult>(error);
        } catch {
          /* Unexpected engine failure. */
        }
        if (result?.status === "authoring-error") return result;
      }
      throw error;
    }
    return { status: "drafted", draft: OwnedDocumentDraft.fromRaw(raw) };
  }
  /** Cached frozen binding metadata in lexical key order; null means namespace absent. Checks all rows, never copies opaque payloads, and throws located interpretation errors. */
  get bindings(): readonly BindingMetadata[] | null {
    const raw = documentRaw(this);
    if (this.#bindings === undefined)
      this.#bindings = metadataRows<BindingMetadata>(
        sdkCall(() => raw.bindings()),
      );
    return this.#bindings;
  }
  /** Acquire an independent binding view by declaration key, or undefined when missing. Checks selected shape without interpreting unrelated row metadata. */
  binding(key: string): BindingView | undefined {
    scalar(key);
    const raw = sdkCall(() => documentRaw(this).binding(key));
    return raw ? adopt(raw, BindingView.fromRaw) : undefined;
  }
  /** Cached frozen source metadata in lexical key order; null means namespace absent. Checks all rows, never copies opaque payloads, and throws located interpretation errors. */
  get sources(): readonly SourceMetadata[] | null {
    const raw = documentRaw(this);
    if (this.#sources === undefined)
      this.#sources = metadataRows<SourceMetadata>(
        sdkCall(() => raw.sources()),
      );
    return this.#sources;
  }
  /** Acquire an independent source view by declaration key, or undefined when missing. Checks selected shape without interpreting unrelated row metadata. */
  source(key: string): SourceView | undefined {
    scalar(key);
    const raw = sdkCall(() => documentRaw(this).source(key));
    return raw ? adopt(raw, SourceView.fromRaw) : undefined;
  }
  /** Cached frozen dependency metadata in lexical key order; null means namespace absent. Checks all rows, never copies opaque payloads, and throws located interpretation errors. */
  get dependencies(): readonly DependencyMetadata[] | null {
    const raw = documentRaw(this);
    if (this.#dependencies === undefined)
      this.#dependencies = metadataRows<DependencyMetadata>(
        sdkCall(() => raw.dependencies()),
      );
    return this.#dependencies;
  }
  /** Acquire an independent dependency view by declaration key, or undefined when missing. Checks selected shape without interpreting unrelated row metadata. */
  dependency(key: string): DependencyView | undefined {
    scalar(key);
    const raw = sdkCall(() => documentRaw(this).dependency(key));
    return raw ? adopt(raw, DependencyView.fromRaw) : undefined;
  }
  /** Resolve an exact primary key or alias; found transfers a new disposable view. Repeated occurrences remain ambiguous, including repeats within one operation. Invalid typed namespace throws SdkError. */
  resolveOperation(name: string): OperationSelection {
    scalar(name);
    const selection = sdkCall(() => documentRaw(this).resolveOperation(name));
    try {
      const result = decode<
        Exclude<OperationSelection, { status: "found" }> | { status: "found" }
      >(selection.result());
      if (result.status !== "found") {
        if (result.status === "ambiguous") freezeCandidates(result.candidates);
        else if (result.status !== "missing")
          throw new Error("Invalid operation selection result.");
        return result;
      }
      const raw = selection.takeOperation();
      if (!raw)
        throw new Error("Selection invariant: found operation missing.");
      try {
        return { status: "found", operation: new OperationView(raw) };
      } catch (error) {
        raw.free();
        throw error;
      }
    } finally {
      selection.free();
    }
  }

  /** Return undefined for absent dependency; otherwise test its kind filter. Absent kinds accepts all and empty accepts none. This helper does not establish normative conformance. */
  dependencyAcceptsKind(dependency: string, kind: string): boolean | undefined {
    scalar(dependency);
    scalar(kind);
    return sdkCall(() =>
      handle<wasm.WasmDocument>(this).dependencyAcceptsKind(dependency, kind),
    );
  }
  /** Inspect schema references with explicit resources and cooperative cancellation. Returns plain data, including incomplete traversal and per-reference refusal; no acquisition or normative proof. */
  references(options: ReferenceOptions = {}): ReferenceReport {
    const resources = options.resources,
      empty = resources ? undefined : new SchemaResources();
    try {
      return decode(
        sdkCall(() =>
          handle<wasm.WasmDocument>(this).references(
            resourceRaw(resources ?? empty!),
            options.signal?.aborted ?? false,
          ),
        ),
      );
    } finally {
      empty?.dispose();
    }
  }
  /** Create a NEW disposable immutable evaluator/resource context. Eagerly validates the whole operation namespace, including unrelated aliases, throwing a specific interpretation error for malformed entries. Unrelated metadata may remain a draft. A later value verdict concerns only the selected schema; use validate() first when accepting a normative document. */
  contracts(options: ContractOptions = {}): ValueContracts {
    if (
      options.includeSchemaDetails !== undefined &&
      typeof options.includeSchemaDetails !== "boolean"
    )
      throw new SdkError(
        "invalid-schema-details",
        "includeSchemaDetails must be a boolean.",
      );
    if (
      options.cacheCapacity !== undefined &&
      (!Number.isSafeInteger(options.cacheCapacity) ||
        options.cacheCapacity < 0 ||
        options.cacheCapacity > 0xffffffff)
    )
      throw new SdkError(
        "invalid-cache-capacity",
        "cacheCapacity must be a nonnegative 32-bit integer.",
      );
    const resources = options.resources,
      limits = encodedLimits(options.limits),
      empty = resources ? undefined : new SchemaResources();
    try {
      return new ValueContracts(
        sdkCall(() =>
          handle<wasm.WasmDocument>(this).contracts(
            resourceRaw(resources ?? empty!),
            limits,
            options.cacheCapacity,
            options.includeSchemaDetails ?? false,
          ),
        ),
      );
    } finally {
      empty?.dispose();
    }
  }
}
/** Document proof acquisition: validated transfers an independent ValidatedDocument owner and report. Otherwise the original Assessment explains refusal, nonconformance or uncertainty; no proof owner is created. */
export type ValidationResult =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "validated";
      /** Caller-owned document handle for this branch; dispose independently. */
      document: ValidatedDocument;
      /** Plain normative report for the exact snapshot. */
      report: ConformanceReport;
    }
  | Assessment;
const conformanceToken = Symbol("established conformance");
let validateParsed: (document: ParsedDocument) => ValidationResult;
/** Retained proof for one immutable snapshot whose normative OBI rules were established. Obtain via ParsedDocument.validate(); preparation remains evaluator-dependent. It inherits exact inspection methods and deterministic disposal from ParsedDocument. */
export class ValidatedDocument extends ParsedDocument {
  declare private readonly validatedDocumentBrand: void;
  private constructor(raw: wasm.WasmDocument, token: symbol) {
    if (token !== conformanceToken)
      throw new TypeError(
        "Use ParsedDocument.validate() to establish conformance.",
      );
    super(raw);
  }
  static {
    validateParsed = (document: ParsedDocument): ValidationResult => {
      const result = document.assess();
      return result.status === "assessed" &&
        result.report.conclusion === "conformant"
        ? {
            status: "validated",
            document: new ValidatedDocument(
              documentRaw(document).retain(),
              conformanceToken,
            ),
            report: result.report,
          }
        : result;
    };
  }
  /** Create an independently disposable owner preserving the same established normative proof. */
  override retain(): ValidatedDocument {
    return new ValidatedDocument(
      handle<wasm.WasmDocument>(this).retain(),
      conformanceToken,
    );
  }
}
/** Retained operation selected by primary key or alias. It survives parent disposal; dispose it independently. Exact value inspection is not normative proof or a prepared schema. */
export class OperationView extends Managed {
  #metadata?: OperationMetadata;
  #examples?: readonly ExampleMetadata[] | null;
  /** @internal */ constructor(raw: wasm.WasmOperation) {
    super(raw);
  }
  /** Allocate the selected primary key, even when lookup used an alias. */
  get key(): string {
    return handle<wasm.WasmOperation>(this).key();
  }
  /** Return a NEW independently disposable ExactJson owner on every access. It survives operation/document disposal and can retain the original arena. */
  get value(): ExactJson {
    return new ExactJson(handle<wasm.WasmOperation>(this).value());
  }
  /** Cached frozen metadata for this selected operation only; no disposable owner is created. */
  get metadata(): OperationMetadata {
    const raw = handle<wasm.WasmOperation>(this);
    return (this.#metadata ??= freezeMetadata(
      decode<OperationMetadata>(sdkCall(() => raw.metadata())),
    ));
  }
  /** Cached frozen example metadata in lexical key order; null means absent, empty means present-empty. Checks every example row but transports no values. */
  get examples(): readonly ExampleMetadata[] | null {
    const raw = handle<wasm.WasmOperation>(this);
    if (this.#examples === undefined)
      this.#examples = metadataRows<ExampleMetadata>(
        sdkCall(() => raw.examples()),
      );
    return this.#examples;
  }
  /** Acquire a selected independent example view, or undefined for absence; unrelated row metadata is not interpreted. */
  example(key: string): ExampleView | undefined {
    scalar(key);
    const raw = sdkCall(() => handle<wasm.WasmOperation>(this).example(key));
    return raw ? adopt(raw, ExampleView.fromRaw) : undefined;
  }
  /** Allocate a plain array of binding keys in lexical order. No owner disposal, ranking or invocation is involved. */
  get bindings(): readonly string[] {
    return decode(sdkCall(() => handle<wasm.WasmOperation>(this).bindings()));
  }
}
const resourceToken = Symbol("retained resource set");
/** Retained binding declaration view. Metadata is plain; each exact getter acquires a separate owner. Neither reading nor retaining establishes conformance. */
export class BindingView extends Managed {
  #metadata?: BindingMetadata;
  private constructor(raw: wasm.WasmBinding) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmBinding): BindingView {
    return new BindingView(raw);
  }
  /** Acquire a separate owner sharing this immutable selected declaration. */
  retain(): BindingView {
    return BindingView.fromRaw(handle<wasm.WasmBinding>(this).retain());
  }
  /** Cached frozen metadata; checks this selected row and no unrelated row. */
  get metadata(): BindingMetadata {
    const raw = handle<wasm.WasmBinding>(this);
    return (this.#metadata ??= freezeMetadata(
      decode<BindingMetadata>(sdkCall(() => raw.metadata())),
    ));
  }
  /** Acquire a NEW independent exact owner for the complete declaration, including unknown fields. */
  get value(): ExactJson {
    return new ExactJson(handle<wasm.WasmBinding>(this).value());
  }
  /** Acquire a NEW exact content owner; undefined means absent, while present null remains an exact value. */
  get content(): ExactJson | undefined {
    const raw = handle<wasm.WasmBinding>(this).content();
    return raw ? new ExactJson(raw) : undefined;
  }
}
/** Retained source declaration view. Metadata is plain; each exact getter acquires a separate owner. Neither reading nor retaining establishes conformance. */
export class SourceView extends Managed {
  #metadata?: SourceMetadata;
  private constructor(raw: wasm.WasmSource) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmSource): SourceView {
    return new SourceView(raw);
  }
  /** Acquire a separate owner sharing this immutable selected declaration. */
  retain(): SourceView {
    return SourceView.fromRaw(handle<wasm.WasmSource>(this).retain());
  }
  /** Cached frozen metadata; checks this selected row and no unrelated row. */
  get metadata(): SourceMetadata {
    const raw = handle<wasm.WasmSource>(this);
    return (this.#metadata ??= freezeMetadata(
      decode<SourceMetadata>(sdkCall(() => raw.metadata())),
    ));
  }
  /** Acquire a NEW independent exact owner for the complete declaration, including unknown fields. */
  get value(): ExactJson {
    return new ExactJson(handle<wasm.WasmSource>(this).value());
  }
  /** Acquire a NEW exact content owner; undefined means absent, while present null remains an exact value. */
  get content(): ExactJson | undefined {
    const raw = handle<wasm.WasmSource>(this).content();
    return raw ? new ExactJson(raw) : undefined;
  }
}
/** Retained dependency declaration view. Metadata is plain; each exact getter acquires a separate owner. Neither reading nor retaining establishes conformance. */
export class DependencyView extends Managed {
  #metadata?: DependencyMetadata;
  private constructor(raw: wasm.WasmDependency) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmDependency): DependencyView {
    return new DependencyView(raw);
  }
  /** Acquire a separate owner sharing this immutable selected declaration. */
  retain(): DependencyView {
    return DependencyView.fromRaw(handle<wasm.WasmDependency>(this).retain());
  }
  /** Cached frozen metadata; checks this selected row and no unrelated row. */
  get metadata(): DependencyMetadata {
    const raw = handle<wasm.WasmDependency>(this);
    return (this.#metadata ??= freezeMetadata(
      decode<DependencyMetadata>(sdkCall(() => raw.metadata())),
    ));
  }
  /** Acquire a NEW independent exact owner for the complete declaration, including unknown fields. */
  get value(): ExactJson {
    return new ExactJson(handle<wasm.WasmDependency>(this).value());
  }
}
/** Retained example declaration view. Metadata is plain; each exact getter acquires a separate owner. Neither reading nor retaining establishes conformance. */
export class ExampleView extends Managed {
  #metadata?: ExampleMetadata;
  private constructor(raw: wasm.WasmExample) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmExample): ExampleView {
    return new ExampleView(raw);
  }
  /** Acquire a separate owner sharing this immutable selected declaration. */
  retain(): ExampleView {
    return ExampleView.fromRaw(handle<wasm.WasmExample>(this).retain());
  }
  /** Cached frozen metadata; checks this selected row and no unrelated row. */
  get metadata(): ExampleMetadata {
    const raw = handle<wasm.WasmExample>(this);
    return (this.#metadata ??= freezeMetadata(
      decode<ExampleMetadata>(sdkCall(() => raw.metadata())),
    ));
  }
  /** Acquire a NEW independent exact owner for the complete declaration, including unknown fields. */
  get value(): ExactJson {
    return new ExactJson(handle<wasm.WasmExample>(this).value());
  }
  /** Acquire a NEW exact input owner; undefined means absent, while present null remains an exact value. */
  get input(): ExactJson | undefined {
    const raw = handle<wasm.WasmExample>(this).input();
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Acquire a NEW exact output owner; undefined means absent, while present null remains an exact value. */
  get output(): ExactJson | undefined {
    const raw = handle<wasm.WasmExample>(this).output();
    return raw ? new ExactJson(raw) : undefined;
  }
}
/** Immutable explicit resource set. URIs must be absolute without a nonempty fragment; normalized duplicate identities are refused even for equal bytes. Resources are retained, never fetched. Batch failure releases partial state and leaves supplied owners usable. Distinct contexts may associate the same URI with different snapshots. */
export class SchemaResources extends Managed {
  /** Create an empty set or retain explicit [URI, exact document] pairs. Does not consume supplied owners. Invalid/duplicate URI or duplicate-member document throws SdkError; partial construction is released. */
  constructor(entries?: Iterable<readonly [uri: string, document: ExactJson]>);
  /** @internal */ constructor(raw: wasm.WasmResources, token: symbol);
  constructor(
    entries?: Iterable<readonly [string, ExactJson]> | wasm.WasmResources,
    token?: symbol,
  ) {
    requireReady();
    if (token === resourceToken) {
      super(entries as wasm.WasmResources);
      return;
    }
    let raw = new wasm.WasmResources();
    try {
      if (entries !== undefined) {
        for (const [uri, document] of entries as Iterable<
          readonly [string, ExactJson]
        >) {
          scalar(uri);
          const next = sdkCall(() => raw.add(uri, jsonRaw(document)));
          raw.free();
          raw = next;
        }
      }
    } catch (error) {
      raw.free();
      throw error;
    }
    super(raw);
  }

  private static fromRaw(raw: wasm.WasmResources): SchemaResources {
    return new SchemaResources(raw, resourceToken);
  }
  /** Return a NEW independently disposable resource-set owner with one added resource; the original and supplied document stay usable. Invalid/duplicate identity throws; no acquisition. */
  with(uri: string, document: ExactJson): SchemaResources {
    scalar(uri);
    return SchemaResources.fromRaw(
      sdkCall(() =>
        handle<wasm.WasmResources>(this).add(uri, jsonRaw(document)),
      ),
    );
  }
  /** Return a NEW independently disposable owner of this immutable set. */
  retain(): SchemaResources {
    return SchemaResources.fromRaw(handle<wasm.WasmResources>(this).retain());
  }
}
/** Disposable immutable document/resource/evaluator context with entry-bounded preparation reuse. Replacing an active context does not mutate existing prepared owners. Dispose the context to release its cache owners; independently retained PreparedContract handles remain valid. */
export class ValueContracts extends Managed {
  /** @internal */ constructor(raw: wasm.WasmContracts) {
    super(raw);
  }
  /** Select the named operation/alias and requested side, then return the complete ContractPreparation partition. Ready transfers a new disposable contract independent of this context/cache. Deterministic preparations can be reused; cancelled/transient failures do not poison retries. Pre-aborted work returns no-verdict. */
  prepare(
    operation: string,
    side: Side,
    options: WorkOptions = {},
  ): ContractPreparation {
    requireReady();
    const raw = handle<wasm.WasmContracts>(this);
    scalar(operation);
    const signal = workSignal(options);
    const preparation = sdkCall(() =>
      raw.prepare(operation, side, isAborted(signal)),
    );
    try {
      const result = decode<
        Exclude<ContractPreparation, { status: "ready" }> | { status: "ready" }
      >(preparation.result());
      if (result.status !== "ready") {
        if (result.status === "operation-ambiguous")
          freezeCandidates(result.candidates);
        else if (
          !["no-contract", "operation-missing", "no-verdict"].includes(
            result.status,
          )
        )
          throw new Error("Invalid preparation result.");
        return result;
      }
      const contract = preparation.takeContract();
      if (!contract)
        throw new Error("Preparation invariant: ready contract missing.");
      try {
        return { status: "ready", contract: new PreparedContract(contract) };
      } catch (error) {
        contract.free();
        throw error;
      }
    } finally {
      preparation.free();
    }
  }
}
function freezeCandidates(candidates: readonly string[]): void {
  if (
    !Array.isArray(candidates) ||
    candidates.some((key) => typeof key !== "string")
  )
    throw new Error("Invalid operation candidates.");
  Object.freeze(candidates);
}
function workSignal(options: WorkOptions): AbortSignal | undefined {
  if (!options || typeof options !== "object" || Array.isArray(options))
    throw new SdkError(
      "invalid-work-options",
      "Work options must be an object containing an optional AbortSignal.",
    );
  let signal: AbortSignal | undefined;
  for (const [key, value] of ownEntries(options)) {
    if (key !== "signal")
      throw new SdkError("invalid-work-options", "Unknown work option.");
    signal = value as AbortSignal | undefined;
  }
  if (signal !== undefined) {
    try {
      isAborted(signal);
    } catch {
      throw new SdkError(
        "invalid-work-options",
        "signal must be an AbortSignal.",
      );
    }
  }
  return signal;
}
function isAborted(signal: AbortSignal | undefined): boolean {
  if (signal === undefined) return false;
  // Use the host brand check, including valid signals from another browser realm.
  return Object.getOwnPropertyDescriptor(
    AbortSignal.prototype,
    "aborted",
  )!.get!.call(signal) as boolean;
}
function cancelledValue(): ValueOutcome {
  return {
    outcome: "no-verdict",
    detail: {
      reason: "cancelled",
      code: "caller-cancelled",
      message: "the caller cancelled this operation",
      location: null,
    },
  };
}
/** Disposable retained selected-schema validator, independent of document/context lifetime and cache eviction. Repeated validation borrows exact input; ordinary inputs are admitted and temporary owners released within the call. No verdict here proves whole-document conformance. */
export class PreparedContract extends Managed {
  /** @internal */ constructor(raw: wasm.WasmPrepared) {
    super(raw);
  }
  /** Borrow an exact root; cancellation cannot preempt synchronous Wasm. */
  validate(value: ExactJson, options?: WorkOptions): ValueOutcome;
  /** Admit ordinary JSON (including nested exact owners), then validate.
   * Expected admission failures return input-error. Caller exceptions and handle misuse throw.
   */
  validate(value: JsonInput, options?: WorkOptions): ValueCheck;
  validate(value: JsonInput, options: WorkOptions = {}): ValueCheck {
    requireReady();
    const prepared = handle<wasm.WasmPrepared>(this);
    const exact = isExact(value) ? jsonRaw(value) : undefined;
    const signal = workSignal(options);
    if (isAborted(signal)) return cancelledValue();
    if (exact) return decode(prepared.validate(exact, isAborted(signal)));
    let temporary: ExactJson | undefined;
    try {
      let text: string;
      try {
        text = encodeOrdinary(value, { origins: new WeakMap() });
      } catch (error) {
        if (!(error instanceof DraftFailure)) throw error;
        return {
          outcome: "input-error",
          error: {
            code: (error.code === "authoring-limit"
              ? "input-limit"
              : error.code) as ValueAdmissionFailure["code"],
            instancePointer: error.draftPointer,
            message: error.message,
          },
        };
      }
      try {
        temporary = new ExactJson(wasm.WasmJson.parseText(text));
      } catch (error) {
        // Only known JSON admission limits can refuse text produced by the checked encoder.
        let info: { kind?: string } | undefined;
        if (typeof error === "string") {
          try {
            info = JSON.parse(error);
          } catch {
            /* unexpected engine exception */
          }
        }
        if (info?.kind !== "Limit") throw error;
        return {
          outcome: "input-error",
          error: {
            code: "input-limit",
            instancePointer: null,
            message: "The ordinary value exceeds the admitted JSON limits.",
          },
        };
      }
      if (isAborted(signal)) return cancelledValue();
      return decode(prepared.validate(jsonRaw(temporary), false));
    } finally {
      temporary?.dispose();
    }
  }

  /** Return a NEW independently disposable owner sharing compiled state; it remains valid after the context or prior contract owner is disposed. */
  retain(): PreparedContract {
    return new PreparedContract(handle<wasm.WasmPrepared>(this).retain());
  }
}
/** Pinned HTTP discovery companion identity and wire defaults, independent of package/spec core versions. */
export interface DiscoveryPolicy {
  /** Applied companion version. */
  version: string;
  /** Exact companion specification Git revision. */
  revision: string;
  /** Fixed route /.well-known/openbindings. */
  wellKnownPath: string;
  /** OpenBindings JSON response media type. */
  mediaType: string;
  /** Companion request Accept header value. */
  accept: string;
  /** Default decoded-body byte limit: 1,048,576 (1 MiB). */
  defaultMaxDocumentBytes: number;
}
/** Return a fresh plain-data copy of the pinned companion version/revision, route, media types and default decoded-body limit; requires initialization. */
export function discoveryPolicy(): DiscoveryPolicy {
  requireReady();
  return decode(wasm.discoveryPolicy());
}
/** Construct the fixed discovery URL from an absolute HTTP(S) origin. Refuses credentials, resource paths, query and fragment; accepts an optional root slash. Does not fetch. The transport owns connection and redirect policy. */
export function discoveryEndpoint(origin: string): string {
  requireReady();
  scalar(origin);
  return sdkCall(() => wasm.discoveryEndpoint(origin));
}
/** A local asynchronous Fetch callback; native function overloads are not required. */
export type DiscoveryFetch = (
  url: string,
  init: RequestInit,
) => Promise<Response>;
/** One discovery attempt: decoded-body bound, cancellation and optional local Fetch callback. The SDK creates no global fetch override or response cache; the application owns credentials, redirects and network policy. */
export interface DiscoveryOptions {
  /** Decoded-body byte limit; omitted or zero selects 1 MiB. Must be a nonnegative safe integer. Bounds accepted/retained document bytes; Fetch may deliver a larger chunk before overflow is detected, so this does not bound host buffering. */
  maxDocumentBytes?: number;
  /** Caller cancellation; aborts I/O and is checked around synchronous assessment. */
  signal?: AbortSignal;
  /** Local asynchronous Fetch-compatible callback; omitted uses host fetch. Configure credentials, redirects and network restrictions here. */
  fetch?: DiscoveryFetch;
}
/** Observed response facts retained even when body reading or document assessment fails. The final URL is metadata only, not an implicit schema base. */
export interface DiscoveryMetadata {
  /** Constructed discovery URL requested from the transport. */
  requestedUrl: string;
  /** Final URL when exposed by the host, otherwise null; not a schema base. */
  finalUrl: string | null;
  /** Observed HTTP status. */
  status: number;
  /** Host Fetch response visibility/type. */
  responseType: ResponseType;
  /** Frozen observed header strings under host Fetch combining/visibility rules. */
  headers: Readonly<Record<string, string>>;
}
/** Assessment of a complete bounded discovery body. Found owns proof; other document handles, when present, remain unproved parsed snapshots. */
type DiscoveryBodyResult =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "found";
      /** Caller-owned document handle for this branch; dispose independently. */
      document: ValidatedDocument;
      /** Plain normative report for the exact snapshot. */
      report: ConformanceReport;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "non-conformant" | "undetermined";
      /** Caller-owned document handle for this branch; dispose independently. */
      document?: ParsedDocument;
      /** Plain normative report for the exact snapshot. */
      report: ConformanceReport;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "version-refused";
      /** Declared/supported version refusal, distinct from normative evidence. */
      refusal: {
        /** Original unsupported declared version. */
        declared: string;
        /** Supported specification line. */
        supported: string;
      };
    };
/** Finite discovery receipt. Only found carries normative proof. Only HTTP 404 means absent; 401/403 are gated; other non-200 statuses are http-status. Complete bounded 200 bytes can accompany refused/invalid documents; partial bodies are never returned. Dispose any returned document owner independently. */
export type DiscoveryResult = (
  | DiscoveryBodyResult
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "absent" | "gated" | "http-status";
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "body-limit";
      /** Effective decoded-body byte limit. */
      limit: number;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "cancelled";
      /** Observed cancellation classification. */
      reason: "aborted" | "timeout";
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "transport-error" | "body-error";
      /** Structured expected failure for this branch. */
      error: {
        /** Stable category describing the observed value or failure. */
        kind: "network" | "timeout" | "other";
        /** Human-readable explanation; use structured discriminants/codes for logic. */
        message: string;
      };
    }
) & {
  /** Observed response metadata, present once a response was obtained. */
  metadata?: DiscoveryMetadata;
  /** Complete bounded decoded 200 body copy when available; never a partial prefix. */
  body?: Uint8Array;
};
const aborted = Symbol("discovery cancelled");
function interruptible<T>(
  promise: Promise<T>,
  signal: AbortSignal,
): Promise<T> {
  if (signal.aborted) {
    void promise.catch(() => {});
    return Promise.reject(aborted);
  }
  return new Promise((resolve, reject) => {
    const stop = () => {
      signal.removeEventListener("abort", stop);
      reject(aborted);
    };
    signal.addEventListener("abort", stop, { once: true });
    promise.then(
      (value) => {
        signal.removeEventListener("abort", stop);
        resolve(value);
      },
      (error) => {
        signal.removeEventListener("abort", stop);
        reject(error);
      },
    );
  });
}
/** Retrieve decoded bytes, then assess them in Rust. Only 404 means absence.
 * Returned document handles belong to the caller and must be disposed.
 * AbortSignal cancels I/O; synchronous Wasm assessment requires a worker to be interruptible.
 */
export async function discover(
  origin: string,
  options: DiscoveryOptions = {},
): Promise<DiscoveryResult> {
  requireReady();
  const requestedUrl = discoveryEndpoint(origin),
    policy = discoveryPolicy();
  const configured = options.maxDocumentBytes ?? 0;
  if (!Number.isSafeInteger(configured) || configured < 0)
    throw new SdkError(
      "invalid-byte-limit",
      "maxDocumentBytes must be a nonnegative safe integer.",
    );
  const limit = configured || policy.defaultMaxDocumentBytes,
    controller = new AbortController(),
    signal = controller.signal;
  const cancel = () => controller.abort(options.signal?.reason);
  const cancellation = (): DiscoveryResult => ({
    status: "cancelled",
    reason:
      options.signal?.reason?.name === "TimeoutError" ? "timeout" : "aborted",
  });
  if (options.signal?.aborted) return cancellation();
  options.signal?.addEventListener("abort", cancel, { once: true });
  const fetcher = options.fetch ?? globalThis.fetch?.bind(globalThis);
  if (!fetcher) {
    options.signal?.removeEventListener("abort", cancel);
    throw new SdkError(
      "fetch-unavailable",
      "Supply a Fetch callback in this host.",
    );
  }
  let metadata: DiscoveryMetadata | undefined,
    body: Uint8Array | undefined,
    reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  let phase: "transport" | "body" | "assessment" = "transport";
  try {
    const pending = fetcher(requestedUrl, {
      method: "GET",
      headers: { Accept: policy.accept },
      signal,
    });
    const response = await interruptible(
      Promise.resolve(pending).then((response) => {
        if (signal.aborted) void response.body?.cancel().catch(() => {});
        return response;
      }),
      signal,
    );
    metadata = {
      requestedUrl,
      finalUrl: ["opaque", "opaqueredirect"].includes(response.type)
        ? null
        : response.url || null,
      status: response.status,
      responseType: response.type,
      headers: Object.freeze(Object.fromEntries(response.headers.entries())),
    };
    if (response.status !== 200) {
      void response.body?.cancel().catch(() => {});
      return {
        status:
          response.status === 404
            ? "absent"
            : response.status === 401 || response.status === 403
              ? "gated"
              : "http-status",
        metadata,
      };
    }
    phase = "body";
    const chunks: Uint8Array[] = [];
    let length = 0;
    if (response.body) {
      reader = response.body.getReader();
      while (true) {
        const part = await interruptible(reader.read(), signal);
        if (part.done) break;
        const chunk = part.value;
        if (!(chunk instanceof Uint8Array))
          throw new TypeError("Fetch body must yield Uint8Array chunks.");
        if (chunk.byteLength > limit - length)
          return { status: "body-limit", limit, metadata };
        chunks.push(chunk.slice());
        length += chunk.byteLength;
      }
    }
    body = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      body.set(chunk, offset);
      offset += chunk.byteLength;
    }
    if (signal.aborted) return { ...cancellation(), metadata, body };
    phase = "assessment";
    const assessed = new wasm.WasmDiscovery(body);
    try {
      const result = decode<DiscoveryBodyResult>(assessed.result()),
        raw = assessed.document();
      let document: ParsedDocument | undefined = raw
        ? new ParsedDocument(raw)
        : undefined;
      if (result.status === "found") {
        if (!document)
          throw new Error("Discovery invariant: conformant document missing.");
        const validation = document.validate();
        document.dispose();
        document = undefined;
        if (validation.status !== "validated")
          throw new Error("Discovery invariant: conformance changed.");
        return { ...result, document: validation.document, metadata, body };
      }
      return { ...result, ...(document ? { document } : {}), metadata, body };
    } finally {
      assessed.free();
    }
  } catch (error) {
    if (error === aborted || signal.aborted)
      return { ...cancellation(), metadata, body };
    if (phase === "assessment") throw error;
    return {
      status: metadata ? "body-error" : "transport-error",
      metadata,
      body,
      error: {
        kind:
          error instanceof Error && error.name === "TimeoutError"
            ? "timeout"
            : "network",
        message: error instanceof Error ? error.message : "Fetch failed",
      },
    };
  } finally {
    options.signal?.removeEventListener("abort", cancel);
    if (reader) {
      void reader.cancel().catch(() => {});
      reader.releaseLock();
    }
    controller.abort();
  }
}
/** Disposable immutable copy of a proved discovery document. Creates no listener or server; authorization and request routing remain caller-owned. Existing publication remains usable after the original proof is disposed. */
export class DiscoveryPublication extends Managed {
  /** Copy a ValidatedDocument into an independently owned publication. allowOrigin defaults to omission; accepts *, null or one ASCII URI origin with a scheme and host, without credentials/path/query/fragment. Schemes are not restricted to HTTP(S). Invalid configuration throws; the source proof remains caller-owned. */
  constructor(
    document: ValidatedDocument,
    options: {
      /** Optional CORS allow-origin spelling; empty/omitted omits the header. Authentication, credentials and preflight remain application-owned. */
      allowOrigin?: string;
    } = {},
  ) {
    requireReady();
    const allow = options.allowOrigin ?? "";
    scalar(allow);
    super(
      sdkCall(() => new wasm.WasmPublication(documentRaw(document), allow)),
    );
  }
  /** Create a detached Response for the decoded route and method. GET returns exact bytes; HEAD returns no body with the original Content-Length; unsupported route is 404 and unsupported method 405. Creates no server and does not perform authentication or preflight handling. */
  respond(request: Request): Response {
    const url = new URL(request.url);
    let path: string;
    try {
      path = decodeURIComponent(url.pathname);
    } catch {
      path = "";
    }
    const result = decode<{
      status: number;
      headers: { name: string; value: number[] }[];
    }>(handle<wasm.WasmPublication>(this).metadata(request.method, path));
    const headers = new Headers(
      result.headers.map((h): [string, string] => [
        h.name,
        new TextDecoder().decode(new Uint8Array(h.value)),
      ]),
    );
    const body =
      request.method === "HEAD"
        ? null
        : handle<wasm.WasmPublication>(this).body(request.method, path).slice()
            .buffer;
    return new Response(body, { status: result.status, headers });
  }
}
/** Iterative, checked serialization. No getters, toJSON hooks, omission, or implicit coercion. */
function encodeOrdinary(value: unknown, authoring?: AuthorContext): string {
  const childPath = (owner: object, name: string, parent: string) =>
    authoring
      ? (authoring.origins.get(owner)?.get(name) ?? draftPath(parent, name))
      : "";
  function reject(
    code: AuthoringErrorCode,
    pointer: string,
    message: string,
    limit = false,
  ): never {
    if (authoring) throw new DraftFailure(code, pointer, message);
    if (limit) throw new RangeError(message);
    throw new TypeError(message);
  }
  type Job =
    | { value: unknown; depth: number; pointer: string }
    | { text: string; pointer: string }
    | { leave: object };
  const jobs: Job[] = [{ value, depth: 0, pointer: "" }],
    active = new Set<object>(),
    parts: string[] = [];
  let size = 0,
    nodes = 0;
  const add = (s: string, pointer: string) => {
    size += s.length;
    if (size > 64 * 1024 * 1024)
      reject(
        "authoring-limit",
        pointer,
        "Ordinary value exceeds the 64 MiB character limit.",
        true,
      );
    parts.push(s);
  };
  while (jobs.length) {
    const job = jobs.pop()!;
    if ("text" in job) {
      add(job.text, job.pointer);
      continue;
    }
    if ("leave" in job) {
      active.delete(job.leave);
      continue;
    }
    const { value, pointer } = job;
    if (++nodes > 1_000_000 || job.depth > 10_000)
      reject(
        "authoring-limit",
        pointer,
        "Ordinary value exceeds the JSON node or nesting limit.",
        true,
      );
    if (isExact(value)) {
      add(value.text, pointer);
      continue;
    }
    if (
      value === null ||
      typeof value === "boolean" ||
      typeof value === "string"
    ) {
      add(JSON.stringify(value), pointer);
      continue;
    }
    if (typeof value === "number") {
      if (!Number.isFinite(value))
        reject("non-finite-number", pointer, "JSON numbers must be finite.");
      add(JSON.stringify(value), pointer);
      continue;
    }
    if (typeof value !== "object")
      reject(
        "unsupported-value",
        pointer,
        "JSON cannot contain undefined, bigint, functions, or symbols.",
      );
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
      jobs.push({ text: "]", pointer });
      for (let i = value.length - 1; i >= 0; i--) {
        const d = descriptors[String(i)];
        if (!d || !("value" in d))
          reject(
            "accessor-property",
            childPath(value, String(i), pointer),
            "JSON arrays cannot contain accessors or holes.",
          );
        if (i < value.length - 1) jobs.push({ text: ",", pointer });
        jobs.push({
          value: d.value,
          depth: job.depth + 1,
          pointer: childPath(value, String(i), pointer),
        });
      }
      jobs.push({ text: "[", pointer });
    } else {
      const entries = Object.entries(descriptors);
      jobs.push({ text: "}", pointer });
      for (let i = entries.length - 1; i >= 0; i--) {
        const [key, d] = entries[i];
        if (!d.enumerable || !("value" in d))
          reject(
            "value" in d ? "non-enumerable-property" : "accessor-property",
            childPath(value, key, pointer),
            "JSON objects must contain enumerable data properties.",
          );
        if (i < entries.length - 1) jobs.push({ text: ",", pointer });
        jobs.push({
          value: d.value,
          depth: job.depth + 1,
          pointer: childPath(value, key, pointer),
        });
        jobs.push({
          text: JSON.stringify(key) + ":",
          pointer: childPath(value, key, pointer),
        });
      }
      jobs.push({ text: "{", pointer });
    }
  }
  return parts.join("");
}

/** Editable normative operation fields. Typed optional undefined means absence; opaque JSON fields preserve explicit null and reject undefined. A built draft still requires normative assessment and separate schema preparation. */
export interface OperationDraft {
  /** Optional operation description. */
  description?: string;
  /** Optional deprecation annotation. */
  deprecated?: boolean;
  /** Optional ordered application-facing tags. */
  tags?: readonly string[];
  /** Optional names in the shared operation namespace. */
  aliases?: readonly string[];
  /** Optional exact input schema; absence differs from present false or null. */
  input?: JsonInput;
  /** Optional exact output schema; absence differs from present false or null. */
  output?: JsonInput;
  /** Named exact example instances. */
  examples?: Readonly<Record<string, ExampleDraft>>;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a logical draft error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Exact input/output example instances, not schemas or proof that an operation accepts them. */
export interface ExampleDraft {
  /** Optional example explanation. */
  description?: string;
  /** Optional exact input instance, including explicit null. */
  input?: JsonInput;
  /** Optional exact output instance, including explicit null. */
  output?: JsonInput;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a logical draft error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Binding source vocabulary; kind-specific content is exact opaque JSON and is never executed or acquired by core. */
export interface SourceDraft {
  /** Source-kind identifier selecting external interpretation. */
  kind: string;
  /** Optional exact kind-specific content; not interpreted by core. */
  content?: JsonInput;
  /** Optional source explanation. */
  description?: string;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a logical draft error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Link from an operation to a source with opaque kind-specific content and annotations. Core does not rank or invoke bindings. */
export interface BindingDraft {
  /** Referenced primary operation name. */
  operation: string;
  /** Referenced source key. */
  source: string;
  /** Optional exact kind-specific binding content. */
  content?: JsonInput;
  /** Optional interoperable integer in inclusive ±9,007,199,254,740,991; no ranking is performed. */
  preference?: number;
  /** Optional idempotence annotation; does not trigger retries. */
  idempotent?: boolean;
  /** Optional binding deprecation annotation. */
  deprecated?: boolean;
  /** Optional binding explanation. */
  description?: string;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a logical draft error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Dependency declaration with an optional exact source-kind filter; absent kinds accepts all and an empty array accepts none. */
export interface DependencyDraft {
  /** Referenced operation name. */
  operation: string;
  /** Exact permitted source kinds; absence accepts all, empty array accepts none. */
  kinds?: readonly string[];
  /** Optional dependency explanation. */
  description?: string;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a logical draft error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Editable normative document shape. authorDocument supplies the default version when openbindings is omitted, checks typed representability and returns a new independent parsed snapshot. It does not establish conformance. */
export interface DocumentDraft {
  /** Declared specification version; omission defaults to 0.2.0. */
  openbindings?: string;
  /** Primary operation map; namespace/shape checks and normative assessment remain separate stages. */
  operations: Readonly<Record<string, OperationDraft>>;
  /** Optional interface name. */
  name?: string;
  /** Optional interface-defined version, independent of openbindings. */
  version?: string;
  /** Optional interface explanation. */
  description?: string;
  /** Named exact JSON Schema values. */
  schemas?: Readonly<Record<string, JsonInput>>;
  /** Named source declarations. */
  sources?: Readonly<Record<string, SourceDraft>>;
  /** Named operation-to-source bindings. */
  bindings?: Readonly<Record<string, BindingDraft>>;
  /** Named operation dependencies. */
  dependencies?: Readonly<Record<string, DependencyDraft>>;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a logical draft error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Finite writable projection of normative draft containers and scalar lists. Stops at opaque JsonInput values and preserves optional fields and ExactJson ownership. */
type EditableNormativeDraft<T> = {
  -readonly [K in keyof T]: K extends "operations"
    ? Record<string, EditableNormativeDraft<OperationDraft>>
    : K extends "sources"
      ? Record<string, EditableNormativeDraft<SourceDraft>>
      : K extends "bindings"
        ? Record<string, EditableNormativeDraft<BindingDraft>>
        : K extends "dependencies"
          ? Record<string, EditableNormativeDraft<DependencyDraft>>
          : K extends "examples"
            ? Record<string, EditableNormativeDraft<ExampleDraft>>
            : K extends "schemas" | "additionalFields"
              ? Record<string, JsonInput>
              : K extends "tags" | "aliases" | "kinds"
                ? string[]
                : T[K];
};
/** Writable normative output from parsed conversion. Opaque JsonInput interiors and ExactJson ownership remain unchanged; readonly-friendly DocumentDraft inputs remain accepted. */
export type EditableDocumentDraft = EditableNormativeDraft<DocumentDraft>;
/** Parsed-to-draft conversion result; drafted transfers one aggregate owner, while authoring-error leaves the parsed source usable and owns nothing. */
export type DraftResult =
  | {
      /** Successful typed conversion, not conformance. */ status: "drafted";
      /** Caller-owned editing scope. */ draft: OwnedDocumentDraft;
    }
  | {
      /** Expected source conversion refusal. */ status: "authoring-error";
      /** Structured source error; never an invented draft coordinate. */ error: AuthoringFailure;
    };
const scopedDrafts = new WeakMap<object, OwnedDocumentDraft>();
type DraftWire =
  | { kind: "plain"; value: JsonPrimitive | string[] }
  | { kind: "exact"; value: number }
  | { kind: "object"; value: [string, DraftWire][] };
/** Disposable scope owning the exact leaves created during native typed conversion, including leaves later removed or replaced. Caller-inserted external handles remain caller-owned. Retain a borrowed leaf before explicitly disposing this scope. Abandoning an undisposed scope does not invalidate reachable leaves; unreachable leaves have their own best-effort finalizers. */
export class OwnedDocumentDraft extends Managed {
  private constructor(
    private readonly graph: EditableDocumentDraft,
    registry: ExactJson[],
  ) {
    super(
      {
        free() {
          for (const owner of registry.splice(0)) owner.dispose();
        },
      },
      // This aggregate owns no native allocation. Its leaves have individual
      // finalizers; a scope finalizer would revoke still-reachable leaf wrappers.
      false,
    );
    scopedDrafts.set(graph, this);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmDraft): OwnedDocumentDraft {
    const registry: ExactJson[] = [];
    try {
      function materialize(node: DraftWire): unknown {
        switch (node.kind) {
          case "plain":
            return node.value;
          case "exact": {
            const value = raw.takeExact(node.value);
            if (!value)
              throw new Error("Draft transfer invariant: exact leaf missing.");
            const owner = adopt(value, (r) => new ExactJson(r));
            registry.push(owner);
            return owner;
          }
          case "object": {
            const result: Record<string, unknown> = {};
            for (const [key, value] of node.value)
              Object.defineProperty(result, key, {
                value: materialize(value),
                writable: true,
                enumerable: true,
                configurable: true,
              });
            return result;
          }
          default:
            throw new Error("Draft transfer invariant: invalid node kind.");
        }
      }
      // This is a tagged typed-model transfer, never serialized opaque JSON.
      // Do not camel-case application map keys or coerce opaque numeric values.
      const graph = materialize(
        JSON.parse(raw.shape()) as DraftWire,
      ) as EditableDocumentDraft;
      return new OwnedDocumentDraft(graph, registry);
    } catch (error) {
      for (const owner of registry) owner.dispose();
      throw error;
    } finally {
      raw.free();
    }
  }
  /** The same writable plain typed graph on every access; no ownership is acquired. Exact leaves made by conversion are borrowed from this scope. Throws after disposal. */
  get value(): EditableDocumentDraft {
    handle(this);
    return this.graph;
  }
}
/**
 * Expected-invalid-draft codes for this package. This is a closed TypeScript union; later releases may add codes. Messages explain; codes identify.
 * - field-collision: additionalFields shadows a typed member.
 * - duplicate-field: a draft represents the same emitted field more than once.
 * - non-finite-number: NaN or infinity cannot enter JSON.
 * - unsupported-value: undefined, bigint, function or symbol appears in opaque JSON.
 * - sparse-array / array-property: an array has holes or extra named properties.
 * - cyclic-value: an object refers to an active ancestor.
 * - non-plain-object: an object has a custom prototype.
 * - accessor-property / non-enumerable-property / symbol-key: admission would require
 *   executing or silently dropping a property, so the value is refused.
 * - invalid-authoring-object: a typed draft container or known field is not representable.
 * - authoring-limit: encoded output exceeds a finite character/byte/node/depth limit.
 * - invalid-draft: the checked Rust authoring boundary refuses the encoded draft.
 * - invalid-field / duplicate-members: native parsed-to-draft conversion refuses a typed field or repeated member names, with original source coordinates.
 * A draftPointer addresses the caller's draft, including additionalFields, rather
 * than the emitted JSON. Initialization/owner misuse and unexpected exceptions throw.
 */
export type AuthoringErrorCode =
  | "field-collision"
  | "duplicate-field"
  | "non-finite-number"
  | "unsupported-value"
  | "sparse-array"
  | "array-property"
  | "cyclic-value"
  | "non-plain-object"
  | "accessor-property"
  | "non-enumerable-property"
  | "symbol-key"
  | "invalid-authoring-object"
  | "authoring-limit"
  | "invalid-draft"
  | "invalid-field"
  | "duplicate-members";
/** Expected invalid-draft diagnostic. code supports branching, draftPointer locates the original caller draft (including additionalFields), and message is explanatory. No generated JSON byte location is substituted. */
export interface AuthoringFailure {
  /** Original parsed-source coordinates for conversion failures; absent/null for caller-created drafts. */
  sourceLocation?: SourceLocation | null;
  /** Stable expected-draft failure category. */
  code: AuthoringErrorCode;
  /** JSON Pointer into the caller's draft; empty means root, null means unavailable. Never a byte offset. */
  draftPointer: string | null;
  /** Human-readable guidance; use code and draftPointer for logic. */
  message: string;
}
/** Authoring admission result. Authored transfers one disposable ParsedDocument without normative proof; authoring-error returns a logical draft diagnostic and owns no handle. */
export type AuthoringResult =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "authored";
      /** Caller-owned document handle for this branch; dispose independently. */
      document: ParsedDocument;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "authoring-error";
      /** Structured expected failure for this branch. */
      error: AuthoringFailure;
    };
class DraftFailure extends Error {
  constructor(
    readonly code: AuthoringErrorCode,
    readonly draftPointer: string | null,
    message: string,
  ) {
    super(message);
  }
  diagnostic(): AuthoringFailure {
    return {
      code: this.code,
      draftPointer: this.draftPointer,
      message: this.message,
    };
  }
}
interface AuthorContext {
  origins: WeakMap<object, Map<string, string>>;
}
function draftPath(parent: string, name: string): string {
  return parent + "/" + name.replace(/~/g, "~0").replace(/\//g, "~1");
}
function bridgeDraftFailure(error: unknown): never {
  // Only known expected refusals from the two authoring ABI calls are normalized.
  if (typeof error === "string") {
    let info: { code?: string; kind?: string } | undefined;
    try {
      info = JSON.parse(error);
    } catch {
      /* Unexpected non-JSON engine failure. */
    }
    if (info?.code === "authoring")
      throw new DraftFailure(
        "invalid-draft",
        null,
        "Draft fields do not satisfy the typed authoring model.",
      );
    if (info?.kind === "Limit")
      throw new DraftFailure(
        "authoring-limit",
        null,
        "The encoded draft exceeds the admitted JSON limits.",
      );
  }
  throw error;
}
/** Expected invalid data returns a draft diagnostic; initialization, disposed handles and unexpected failures throw.
 * Typed optional fields use undefined for absence; opaque JSON never silently omits it.
 */
export function authorDocument(draft: DocumentDraft): AuthoringResult {
  requireReady();
  const scope = scopedDrafts.get(draft);
  if (scope) handle(scope);
  let exact: ExactJson | undefined;
  try {
    const context: AuthorContext = { origins: new WeakMap() };
    const object = authorObject(draft, "document", "", context);
    if (!("openbindings" in object)) object.openbindings = "0.2.0";
    const text = encodeOrdinary(object, context);
    try {
      exact = new ExactJson(wasm.WasmJson.parseText(text));
    } catch (error) {
      return bridgeDraftFailure(error);
    }
    try {
      return {
        status: "authored",
        document: new ParsedDocument(jsonRaw(exact).authorDocument()),
      };
    } catch (error) {
      return bridgeDraftFailure(error);
    }
  } catch (error) {
    if (error instanceof DraftFailure)
      return { status: "authoring-error", error: error.diagnostic() };
    throw error;
  } finally {
    exact?.dispose();
  }
}

type AuthorKind =
  | "document"
  | "operation"
  | "example"
  | "source"
  | "binding"
  | "dependency";
const typedFields: Record<AuthorKind, readonly string[]> = {
  document: [
    "openbindings",
    "operations",
    "name",
    "version",
    "description",
    "schemas",
    "sources",
    "bindings",
    "dependencies",
  ],
  operation: [
    "description",
    "deprecated",
    "tags",
    "aliases",
    "input",
    "output",
    "examples",
  ],
  example: ["description", "input", "output"],
  source: ["kind", "content", "description"],
  binding: [
    "operation",
    "source",
    "content",
    "preference",
    "idempotent",
    "deprecated",
    "description",
  ],
  dependency: ["operation", "kinds", "description"],
};
const requiredFields: Record<AuthorKind, readonly string[]> = {
  document: ["operations"],
  operation: [],
  example: [],
  source: ["kind"],
  binding: ["operation", "source"],
  dependency: ["operation"],
};
function ownEntries(object: unknown, pointer?: string): [string, unknown][] {
  function invalid(
    code: AuthoringErrorCode,
    at: string,
    message: string,
  ): never {
    if (pointer === undefined) throw new TypeError(message);
    throw new DraftFailure(code, at, message);
  }
  if (
    !object ||
    typeof object !== "object" ||
    Array.isArray(object) ||
    ![null, Object.prototype].includes(Object.getPrototypeOf(object))
  )
    invalid(
      "invalid-authoring-object",
      pointer ?? "",
      "Authoring objects and maps must be plain objects.",
    );
  if (Reflect.ownKeys(object).some((key) => typeof key === "symbol"))
    invalid(
      "symbol-key",
      pointer ?? "",
      "Authoring objects cannot contain symbol keys.",
    );
  return Object.entries(Object.getOwnPropertyDescriptors(object)).map(
    ([key, d]) => {
      if (!d.enumerable || !("value" in d))
        invalid(
          "value" in d ? "non-enumerable-property" : "accessor-property",
          draftPath(pointer ?? "", key),
          "Authoring requires enumerable data properties.",
        );
      return [key, d.value];
    },
  );
}
function authorObject(
  object: unknown,
  kind: AuthorKind,
  pointer: string,
  context: AuthorContext,
): Record<string, JsonInput> {
  const out = Object.create(null) as Record<string, JsonInput>;
  const origins = new Map<string, string>();
  context.origins.set(out, origins);
  for (const [key, value] of ownEntries(object, pointer)) {
    const fieldPath = draftPath(pointer, key);
    if (key === "additionalFields") {
      if (value === undefined) continue;
      for (const [name, content] of ownEntries(value, fieldPath)) {
        const contentPath = draftPath(fieldPath, name);
        if (typedFields[kind].includes(name))
          throw new DraftFailure(
            "field-collision",
            contentPath,
            "Additional field shadows a typed member.",
          );
        if (Object.hasOwn(out, name))
          throw new DraftFailure(
            "duplicate-field",
            contentPath,
            "The draft supplies the same document member twice.",
          );
        out[name] = content as JsonInput;
        origins.set(name, contentPath);
      }
      continue;
    }
    if (
      value === undefined &&
      typedFields[kind].includes(key) &&
      !requiredFields[kind].includes(key)
    )
      continue;
    const child: AuthorKind | undefined =
      kind === "document"
        ? key === "operations"
          ? "operation"
          : key === "sources"
            ? "source"
            : key === "bindings"
              ? "binding"
              : key === "dependencies"
                ? "dependency"
                : undefined
        : kind === "operation" && key === "examples"
          ? "example"
          : undefined;
    if (Object.hasOwn(out, key))
      throw new DraftFailure(
        "duplicate-field",
        fieldPath,
        "The draft supplies the same document member twice.",
      );
    out[key] = child
      ? Object.fromEntries(
          ownEntries(value, fieldPath).map(([name, v]) => [
            name,
            authorObject(v, child, draftPath(fieldPath, name), context),
          ]),
        )
      : (value as JsonInput);
  }
  return out;
}
