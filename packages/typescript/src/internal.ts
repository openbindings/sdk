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
/** Initialize this module instance once before using SDK APIs. Concurrent calls share one promise; a failed attempt can be retried. Requires host crypto.getRandomValues (request-scoped hosts should call inside a request). Throws SdkError for initialization/entropy failures. Without input, Node reads the installed Wasm asset from disk; browsers fetch the module-relative asset. Supplying bytes/module bypasses default loading in every host. */
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
/** @internal */
export function requireReady(): void {
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
/** Parse result only. The successful parse branch transfers one disposable owner to the caller; it establishes neither document conformance nor schema readiness. */
export type ParseResult<T> =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "parsed";
      /** Acquired owner or detached converted value, as specified by this branch. */
      value: T;
    }
  | InputFailure;
/** Closed per-rule evidence partition: met establishes the rule; violated establishes a counterexample; inconclusive lacks proof; not-applicable records a failed prerequisite. Findings explain evidence but do not replace it. */
export type Evidence = "met" | "violated" | "inconclusive" | "not-applicable";
/** Coordinates in original UTF-8 source. byteOffset is zero-based; line and byteColumn are one-based. A byte column is not a JavaScript UTF-16 editor column: convert against the original source. Pointers are untrusted display data and repeated keys may share a pointer. */
export interface SourceLocation {
  /** Exact RFC 6901 pointer: from the value, document or resource a locate() call was asked of, or from the source's root for findings and metadata; empty string means that root. A member name with an unpaired UTF-16 unit keeps it, as a JavaScript string can. null only when the location precedes any value, as a syntax error does. Use byteOffset to distinguish duplicate key occurrences. */
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
  /** Original source coordinate, null only for a finding about no particular position, such as an input limit. */
  location: SourceLocation | null;
  /** Human-readable explanation; render as text rather than HTML. */
  message: string;
}
/** Normative OBI assessment at an exact specification revision. All rules retain independent evidence even if findings are omitted. Presentation retains at most 4096 findings and 8 MiB aggregate generated-pointer bytes (UTF-8, or two per UTF-16 unit for a pointer through a name with an unpaired surrogate); findingsTruncated records omissions without changing the conclusion. */
export interface ValidationReport {
  /** Applied specification release. */
  release: string;
  /** Exact applied specification Git revision. */
  revision: string;
  /** Aggregate normative conclusion derived from independent rule evidence. */
  conclusion: "conformant" | "non-conformant" | "undetermined";
  /** Entry for every OBI rule, including inconclusive and not-applicable. */
  evidence: Readonly<Record<string, Evidence>>;
  /** Retained explanatory findings, bounded independently of rule evidence. */
  findings: readonly Finding[];
  /** True when the 4096-finding or 8 MiB aggregate generated-pointer byte cap omitted findings. Omission never changes evidence or substitutes a shortened pointer. */
  findingsTruncated: boolean;
}
/** Normative assessment or a distinct unsupported-version refusal. An assessed report can be conformant, non-conformant or undetermined. This plain-data result owns no handles; use Document.validate() to obtain a retained proof. */
export type DocumentValidation =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "report";
      /** Plain normative report for the exact snapshot. */
      report: ValidationReport;
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
/** A keyword or schema in an original schema source, by resource and exact pointer. resource null denotes the OpenBindings document; supplied URIs identify explicit resources and never initiate acquisition. Private evaluator projection identities are not exposed. Line and column come from the source's holder: ConformantDocument.locate() for the document and SchemaResources.locate() for a supplied resource; a prepared value contract keeps no source text. */
export interface SchemaLocation {
  /** Original supplied-resource URI, or null for the OpenBindings document. */
  resource: string | null;
  /** Exact RFC 6901 pointer from that source's root; empty string denotes root. */
  pointer: string;
}
/** Known refusal causes for this facade and its bundled engine. Unsupported capability, conservative preparation, unavailable resources, limits, cancellation and evaluator failure do not establish satisfaction/failure. A potential cycle alone is conservative-preparation. Later package versions may add causes. Qualified partial value contracts can prepare ready; values whose result depends on missing content return resource-unavailable during validation. For unavailable resources, Document.references() exposes document-wide reference spellings and original keyword locations, not an exact per-value-contract missing-resource list; apply an application disclosure policy before logging them. */
export type NoVerdictReason =
  | "unsupported-capability"
  | "conservative-preparation"
  | "resource-unavailable"
  | "limit-exceeded"
  | "cancelled"
  | "evaluator-failure";
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
/** Established undefined result under the value-contract rules, separate from an evaluator's inability to answer. */
export interface UndefinedResult {
  /** Stable evaluator-defined detail code. */
  code: string;
  /** Explanatory text; custom evaluators should avoid echoing source or instance data. */
  message: string;
  /** Original schema location when known, otherwise null. */
  location: SchemaLocation | null;
}
/** Opt-in schema facts, not rejected instance data. Bounds and enum choices are exact JSON token strings: do not coerce them to Number. Source facts can contain secrets and must be rendered as text under an application disclosure policy. Each variant contains complete facts. */
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
    };
/** Established instance failure, located in the value and in the schema. Failure messages avoid echoing instance values by default; pointers still contain source-controlled strings and must be rendered as text. */
export interface ValueProblem {
  /** Exact RFC 6901 pointer to the failing node, from the value passed to validate/explain (also when that value was acquired from a larger text); it can hold an unpaired UTF-16 unit. ExactJson.locate(instance) gives its byte offset, line and column when asked; an ordinary value is located in its exact JSON serialization. */
  instance: string;
  /** The failing keyword in its original schema source, from that document's or resource's root: ConformantDocument.locate() or SchemaResources.locate() gives its coordinates. When the evaluator cannot map a keyword to the original source, the selected schema itself rather than a guessed keyword. */
  schema: SchemaLocation;
  /** Stable evaluator-defined problem code, commonly the failed keyword. */
  code: string;
  /** Explanatory text; present pointers separately and safely. */
  message: string;
  /** Optional schema facts. Absent means disabled or unavailable. */
  details?: ValueProblemDetails;
}
/** Closed selected-schema result partition: satisfies, fails, undefined, or no-verdict. An established failure may have incomplete diagnostics; problemsComplete does not weaken its verdict. Neither satisfies nor fails proves normative OBI conformance of the surrounding document. */
export type ValueOutcome =
  | {
      /** Semantic result discriminant; narrow before reading branch-specific data. */
      outcome: "satisfies";
    }
  | {
      /** Semantic result discriminant; narrow before reading branch-specific data. */
      outcome: "fails";
      /** Value-free explanation of the established failure. */
      message: string;
      /** Retained actual failing instance locations. */
      problems: readonly ValueProblem[];
      /** Whether selected failure diagnostics, including requested details, completed; false does not weaken the established failure. */
      problemsComplete: boolean;
    }
  | {
      /** Semantic result discriminant; narrow before reading branch-specific data. */
      outcome: "undefined";
      /** Established undefined result, with an original schema coordinate when known. */
      detail: UndefinedResult;
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
/** Preparation result: ready transfers a disposable PreparedValueContract; no-verdict is a preparation refusal that judges no instance. Ready means prepared, not decidable: a prepared value contract can return no-verdict for every value. A missing operation or an absent value contract is undefined before preparation, never a preparation result. */
export type ValueContractPreparation =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "ready";
      /** Caller-owned prepared value contract, valid independently of its document, resources and operation owners. */
      valueContract: PreparedValueContract;
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
/** Exact JSON text validation result: input-error when the text is not admitted as JSON, with the same error parseJson reports; after admission, the same selected-schema ValueOutcome as for an ExactJson root. */
export type JsonCheck =
  | ValueOutcome
  | {
      /** Semantic result discriminant; narrow before reading branch-specific data. */
      outcome: "input-error";
      /** Exact-input refusal: stable code, optional kind and zero-based byte offset, and message. */
      error: InputFailure["error"];
    };
/** Known causes of a static reference-inspection refusal in this SDK version. */
export type ReferenceRefusalReason =
  | "resource-unavailable"
  | "invalid-reference"
  | "missing-target"
  | "ambiguous-target"
  | "identity-conflict"
  | "unsupported-capability"
  | "limit-exceeded"
  | "cancelled"
  | "resolver-failure";
/** Static inspection failure, separate from a selected-schema value outcome. */
export interface ReferenceRefusal {
  /** Broad refusal cause. */
  reason: ReferenceRefusalReason;
  /** Stable inspection-specific code; use with the broad reason when branching. */
  code: string;
  /** Explanation of the inspection refusal; generated messages omit source-controlled details. */
  message: string;
  /** Original location when established. */
  location: SchemaLocation | null;
}
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
        detail: ReferenceRefusal;
      };
}
/** Original-context reference inspection, separate from normative proof and preparation. complete describes traversal completion; a complete report can still contain unresolved reference results. */
export interface ReferenceReport {
  /** Retained reference observations in traversal order. */
  references: readonly Reference[];
  /** Whether traversal completed; unresolved entries can occur even when true. */
  complete: boolean;
  /** Traversal-level limitation when incomplete, otherwise null. */
  limitation: ReferenceRefusal | null;
}
/** Cooperative work control. A pre-aborted signal returns cancellation. Synchronous Wasm does not yield to same-thread JavaScript, so use a worker plus host scheduling to interrupt long synchronous work externally. This is not a wall-clock deadline. */
export interface WorkOptions {
  /** Optional caller-owned cooperative cancellation signal; synchronous Wasm only observes same-thread cancellation at call boundaries. */
  signal?: AbortSignal;
}
/** Default-evaluator finite budgets, each a nonnegative 32-bit integer; `evaluationSteps` and `schemaResources` may also be `null` for their defaults. Work counts are implementation units, not milliseconds. Zero is literal; the allocation and stack use of an explanation's library diagnostics are library-bounded; exhaustion produces no-verdict unless only post-verdict diagnostics are incomplete. */
export interface EvaluatorLimits {
  /** Work units per verdict/diagnostic pass. `null`, the default, grows with the value: 2,000,000 units for a value of up to 65,536 nodes (member names count) and proportionally more for a larger one. A number caps every pass at exactly that many units, whatever the value's size. */
  evaluationSteps: number | null;
  /** Nested evaluation depth; default 1024. It bounds stack use; a deeper value yields the `evaluation-depth-limit` no-verdict. */
  evaluationDepth: number;
  /** Regex evaluation/backtracking work budget; default 2,000,000. */
  regexSteps: number;
  /** Projected JSON nesting admitted to compilation; default 512. */
  compileJsonDepth: number;
  /** Maximum UTF-8 bytes per schema regex; default 1,048,576 (1 MiB). */
  patternBytes: number;
  /** Maximum regex parenthesis nesting; default 32, which keeps preparation within a 256 KiB stack. */
  patternDepth: number;
  /** Schema resources one contract may reach: the document resource holding its schema and each embedded `$id`, supplied and packaged meta-schema resource it reaches. Ordinary contracts reach one or a few. `null`, the default, sets no cap. Registering resources before compilation takes time quadratic in their number, outside every other budget: natively, 1,000 take 7 ms, 10,000 take 0.67 s and 20,000 take 2.8 s, and Wasm takes about three times as long. A number refuses a contract reaching more with the `schema-resource-limit` no-verdict before that setup, so an application preparing untrusted documents or resources can bound it at the cap it chooses. */
  schemaResources: number | null;
}
/** Inputs for one preparation with the built-in JSON Schema 2020-12 evaluator, which performs no resource I/O, treats format as annotation, declines unqualified Unicode property-escape matching, and may conservatively refuse potential non-progressing cycles. A pre-aborted signal returns no-verdict. */
export interface PrepareOptions extends WorkOptions {
  /** Explicit resources borrowed for this preparation; omitted means none. No URI is fetched and the passed owner remains caller-owned. */
  resources?: SchemaResources;
  /** Overrides for built-in evaluator budgets; unspecified entries use evaluatorLimits(). */
  limits?: Partial<EvaluatorLimits>;
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
  const encoded: Record<string, number | null> = {};
  for (const [key, value] of ownEntries(limits)) {
    // A null limit is accepted only where the default itself is null.
    const budget =
      typeof value === "number" &&
      Number.isSafeInteger(value) &&
      value >= 0 &&
      value <= 0xffffffff;
    if (
      !Object.hasOwn(defaults, key) ||
      !(
        budget ||
        (value === null && defaults[key as keyof EvaluatorLimits] === null)
      )
    )
      throw new SdkError(
        "invalid-evaluator-limits",
        "Evaluator limits must be known budgets, each a nonnegative integer no greater than 4294967295; evaluationSteps and schemaResources may also be null for their defaults.",
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
  /** Declared key into the `sources` map; no source need be present under it. */ readonly source: string;
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
  /** Frozen declared kind strings; null accepts all. An empty list matches none but is nonconformant: a present kinds list must be nonempty. */ readonly kinds:
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
// Whether text holds no unpaired UTF-16 unit, so it crosses the bridge exactly as UTF-8.
function wellFormed(text: string): boolean {
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
    } else if (c >= 0xdc00 && c <= 0xdfff) return false;
  }
  return true;
}
function scalar(text: string): void {
  if (!wellFormed(text))
    throw new TypeError(
      "Unpaired UTF-16 in JSON text; use a JSON \\u escape or exact UTF-8 bytes.",
    );
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
function registerHandle(owner: object, raw: Raw): void {
  handles.set(owner, raw);
  finalizer?.register(owner, raw, owner);
}
function handle<T extends Raw>(owner: Managed): T {
  const raw = handles.get(owner);
  if (!raw)
    throw new SdkError("disposed-handle", "This SDK handle has been disposed.");
  return raw as T;
}
abstract class Managed {
  protected constructor(raw: Raw) {
    registerHandle(this, raw);
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
const documentRaw = (owner: DocumentReads) =>
  handle<wasm.WasmDocument | wasm.WasmConformantDocument>(owner);
function editableRaw(owner: DocumentReads): wasm.WasmDocument {
  const raw = handle<wasm.WasmDocument>(owner);
  if (!(raw instanceof wasm.WasmDocument))
    throw new SdkError(
      "immutable-document",
      "A conformant document cannot be edited.",
    );
  return raw;
}
function conformantRaw(
  owner: ConformantDocument,
  code = "document-conformance",
): wasm.WasmConformantDocument {
  const raw = handle<wasm.WasmConformantDocument>(owner);
  if (!(raw instanceof wasm.WasmConformantDocument))
    throw new SdkError(code, "A conformant document is required.");
  return raw;
}
const resourceRaw = (owner: SchemaResources) =>
  handle<wasm.WasmResources>(owner);
// A JavaScript receiver's class is never authority for a proof-requiring call.
function conformantOperationRaw(
  owner: ConformantOperationView,
): wasm.WasmConformantOperation {
  const raw = handle<wasm.WasmConformantOperation>(owner);
  if (!(raw instanceof wasm.WasmConformantOperation))
    throw new SdkError(
      "document-conformance",
      "A conformant operation is required.",
    );
  return raw;
}
function contractRaw(owner: ValueContract): wasm.WasmValueContract {
  const raw = handle<wasm.WasmValueContract>(owner);
  if (!(raw instanceof wasm.WasmValueContract))
    throw new SdkError(
      "document-conformance",
      "A value contract of a conformant operation is required.",
    );
  return raw;
}
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
/** Assess all normative document rules directly from exact source, including invalid JSON. Unsupported declared versions return version-refused; parse/limit failures become truthful evidence. Returns plain data with no handle to dispose. Initialization, wrong input types and literal unpaired UTF-16 text throw; represent unpaired units with JSON escapes or exact UTF-8 bytes. */
export function validateDocument(input: ExactInput): DocumentValidation {
  requireReady();
  return decode(wasm.validateBytes(bytes(input)));
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
/** Shared reads for a document revision. Document reads can refuse malformed typed fields; ConformantDocument already establishes their shape. Owner-producing methods create fresh handles and require separate disposal. */
abstract class DocumentReads extends Managed {
  #operations?: readonly OperationMetadata[];
  #bindings?: readonly BindingMetadata[] | null;
  #sources?: readonly SourceMetadata[] | null;
  #dependencies?: readonly DependencyMetadata[] | null;
  protected constructor(raw: wasm.WasmDocument | wasm.WasmConformantDocument) {
    super(raw);
  }
  protected invalidateMetadata(): void {
    this.#operations = undefined;
    this.#bindings = undefined;
    this.#sources = undefined;
    this.#dependencies = undefined;
  }
  /** Allocate detached exact UTF-8 bytes of the current revision, including formatting. */
  toBytes(): Uint8Array {
    return handle<wasm.WasmDocument>(this).toBytes();
  }
  /** Return a NEW independently disposable ExactJson owner on every call. Cache it locally and dispose it; disposing this document does not release that separate owner. */
  value(): ExactJson {
    return new ExactJson(handle<wasm.WasmDocument>(this).value());
  }
  /** Read the declaration as exact JavaScript UTF-16 text. Invalid typed fields throw. */
  get openbindings(): string | undefined {
    return (
      decode<string | null>(sdkCall(() => documentRaw(this).openbindings())) ??
      undefined
    );
  }
  /** Read optional exact interface name; undefined means absent. */
  get name(): string | undefined {
    return (
      decode<string | null>(sdkCall(() => documentRaw(this).name())) ??
      undefined
    );
  }
  /** Read the optional opaque interface version label. */
  get version(): string | undefined {
    return (
      decode<string | null>(sdkCall(() => documentRaw(this).version())) ??
      undefined
    );
  }
  /** Read optional exact description, including lone UTF-16 units. */
  get description(): string | undefined {
    return (
      decode<string | null>(sdkCall(() => documentRaw(this).description())) ??
      undefined
    );
  }
  /** Acquire a NEW independent ExactJson owner for each declared schema on every call. Dispose each value. Undefined means absent; an empty map means present-empty. */
  schemas(): ReadonlyMap<string, ExactJson> | undefined {
    const raw = documentRaw(this);
    const names = decode<string[] | null>(sdkCall(() => raw.schemaNames()));
    if (names === null) return undefined;
    const values = new Map<string, ExactJson>();
    try {
      for (const name of names) {
        const value = sdkCall(() => raw.schema(name));
        if (value) values.set(name, new ExactJson(value));
      }
      return values;
    } catch (error) {
      for (const value of values.values()) value.dispose();
      throw error;
    }
  }
  /** Return cached frozen plain metadata in lexical primary-key order. No handles are created. Document throws a located error for malformed typed fields; ConformantDocument reads are infallible while live. */
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
  /** Cached frozen binding metadata in lexical key order; null means absent. Never copies opaque payloads. Document can throw located interpretation errors; a live ConformantDocument cannot. */
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
  /** Cached frozen source metadata in lexical key order; null means absent. Never copies opaque payloads. Document can throw located interpretation errors; a live ConformantDocument cannot. */
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
  /** Cached frozen dependency metadata in lexical key order; null means absent. Never copies opaque payloads. Document can throw located interpretation errors; a live ConformantDocument cannot. */
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
  /** Return undefined for absent dependency; otherwise test its kind filter. Absent kinds accepts all; an empty list matches none but is nonconformant. This helper does not establish normative conformance. */
  dependencyAcceptsKind(dependency: string, kind: string): boolean | undefined {
    scalar(dependency);
    return sdkCall(() =>
      handle<wasm.WasmDocument>(this).dependencyAcceptsKind(
        dependency,
        JSON.stringify(kind),
      ),
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
}
/** One exact pointer edit or typed namespace replacement. All pointers address the revision before the atomic batch. */
export type Edit =
  | {
      /** Replace or insert a JSON value at the specified pointer. */
      op: "set";
      /** RFC 6901 path into the revision before the batch; a segment may hold an unpaired UTF-16 unit, as a finding's pointer can. */
      pointer: string;
      /** Borrowed exact or checked ordinary JSON; never consumed. */
      value: JsonInput;
    }
  | {
      /** Remove an existing member or array element. */
      op: "remove";
      /** RFC 6901 path, which may hold an unpaired UTF-16 unit; removing the root is refused. */
      pointer: string;
    }
  | {
      /** Replace the complete operations map using typed parts. */
      op: "set-operations";
      /** Borrowed typed inputs; opaque exact values retain their spelling. */
      value: Readonly<Record<string, Operation>>;
    }
  | {
      /** Replace the complete sources map using typed parts. */
      op: "set-sources";
      /** Borrowed typed inputs; opaque exact values retain their spelling. */
      value: Readonly<Record<string, Source>>;
    }
  | {
      /** Replace the complete bindings map using typed parts. */
      op: "set-bindings";
      /** Borrowed typed inputs; opaque exact values retain their spelling. */
      value: Readonly<Record<string, Binding>>;
    }
  | {
      /** Replace the complete dependencies map using typed parts. */
      op: "set-dependencies";
      /** Borrowed typed inputs; opaque exact values retain their spelling. */
      value: Readonly<Record<string, Dependency>>;
    };
let adoptDocument: (raw: wasm.WasmDocument) => Document;
const documentToken = Symbol("document owner");
/** One exact, editable OBI. Reads may refuse malformed typed fields. Clones, views and proofs retain their revision after editing. */
export class Document extends DocumentReads {
  private constructor(raw: wasm.WasmDocument, token: symbol) {
    if (token !== documentToken)
      throw new TypeError("Use Document.parse() or Document.create().");
    super(raw);
  }
  static {
    adoptDocument = (raw) => new Document(raw, documentToken);
  }
  /** Admit exact JSON, including broken documents, without conformance proof. Expected input refusal uses input-error; initialization and API misuse throw. */
  static parse(input: ExactInput): ParseResult<Document> {
    requireReady();
    const data = bytes(input);
    try {
      return {
        status: "parsed",
        value: adoptDocument(wasm.WasmDocument.parseBytes(data)),
      };
    } catch (error) {
      return failure(error);
    }
  }
  /** Create an empty OBI declaring the supported authoring version. Validate separately to obtain proof. */
  static create(): Document {
    requireReady();
    return adoptDocument(wasm.WasmDocument.create());
  }
  /** Retain a separate editable owner sharing this revision until either owner is edited. */
  retain(): Document {
    return adoptDocument(handle<wasm.WasmDocument>(this).retain());
  }
  /** Resolve an exact primary key or alias; found transfers a new disposable view. Repeated occurrences remain ambiguous, including repeats within one operation. Invalid typed namespace throws SdkError. */
  resolveOperation(name: string): OperationSelection {
    scalar(name);
    const selection = sdkCall(() =>
      handle<wasm.WasmDocument>(this).resolveOperation(name),
    );
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
  /** Assess all rules; the conformant branch owns a new ConformantDocument. The original owner stays editable. Other branches retain evidence or a version refusal. */
  validate(): ValidationResult {
    return assessDocument(this);
  }
  #commit(call: (raw: wasm.WasmDocument) => void): void {
    const raw = editableRaw(this);
    try {
      sdkCall(() => call(raw));
    } catch (error) {
      if (error instanceof SdkError && error.code !== "disposed-handle")
        throw new EditError(error.code, error.message);
      throw error;
    }
    this.invalidateMetadata();
  }
  #encoded(
    value: unknown,
    context?: AuthorContext,
    destination?: { namespace: string; key: string },
    prefix = "",
    suffix = "",
  ): void {
    handle(this);
    const exact: EditEncoding = {
      chunks: [],
      owners: [],
      indices: [],
      seen: new Map(),
    };
    const tail = encodeOrdinary(value, context, exact);
    exact.chunks.push(tail + suffix);
    exact.chunks[0] = prefix + exact.chunks[0];
    const values = exact.owners.map(jsonRaw);
    this.#commit((raw) => {
      const result = raw.editEncoded(
        exact.chunks,
        new Uint32Array(exact.indices),
        values,
        destination?.namespace,
        destination?.key,
      );
      // The native call returns every temporarily transferred owner on failure too.
      // Reattach them before reporting the error; aliases and finalizers stay valid.
      for (let i = 0; i < exact.owners.length; i++) {
        const owner = exact.owners[i]!;
        finalizer?.unregister(owner);
        registerHandle(owner, result[i + 1] as wasm.WasmJson);
      }
      if (result[0] !== null) throw result[0];
    });
  }
  /** Atomically apply pointer edits and typed namespace replacements in request order at shared insertion boundaries, with one bridge call including exact inputs. Untouched bytes stay exact. Invalid paths, duplicates, overlaps and admission limits throw EditError and leave the document unchanged. */
  edit(edits: readonly Edit[]): void {
    handle(this);
    editableRaw(this);
    this.#encoded(edits, { origins: new WeakMap(), editBatch: true });
  }
  /** Replace or insert a value at an RFC 6901 pointer. A segment may hold an unpaired UTF-16 unit, so a finding's pointer addresses the member it reports. Exact owners are borrowed; ordinary values are checked without invoking getters/toJSON. */
  set(pointer: string, value: JsonInput): void {
    // Only a pointer holding an unpaired unit needs the batch route, whose JSON
    // escapes keep it exact across the bridge.
    if (isExact(value) && wellFormed(pointer))
      this.#commit((raw) => raw.set(pointer, jsonRaw(value)));
    else this.edit([{ op: "set", pointer, value }]);
  }
  /** Remove an existing member/array element at an RFC 6901 pointer, which may hold an unpaired UTF-16 unit as a finding's can. Throws EditError on refusal; removing the root is not supported. */
  remove(pointer: string): void {
    if (wellFormed(pointer)) this.#commit((raw) => raw.remove(pointer));
    else this.edit([{ op: "remove", pointer }]);
  }
  /** Set the version declaration; conformance is assessed separately. */
  setOpenbindings(value: string): void {
    this.set("/openbindings", checkedText(value));
  }
  /** Set the interface name. */
  setName(value: string): void {
    this.set("/name", checkedText(value));
  }
  /** Set the interface version label. */
  setVersion(value: string): void {
    this.set("/version", checkedText(value));
  }
  /** Set descriptive text, including escaped unpaired UTF-16 units. */
  setDescription(value: string): void {
    this.set("/description", checkedText(value));
  }
  /** Set a description on an existing operation, escaping its key. */
  setOperationDescription(key: string, value: string): void {
    this.set(
      pointerChild("/operations", key) + "/description",
      checkedText(value),
    );
  }
  /** Set an existing operation's input schema. Absence and explicit null remain distinct. */
  setOperationInput(key: string, value: JsonInput): void {
    this.set(pointerChild("/operations", key) + "/input", value);
  }
  /** Set an existing operation's output schema. */
  setOperationOutput(key: string, value: JsonInput): void {
    this.set(pointerChild("/operations", key) + "/output", value);
  }
  #part(
    namespace: string,
    key: string,
    kind: AuthorKind,
    value: unknown,
  ): void {
    scalar(key);
    handle(this);
    const context: AuthorContext = { origins: new WeakMap() };
    const object = authorObject(
      value,
      kind,
      pointerChild("/" + namespace, key),
      context,
    );
    this.#encoded(object, context, { namespace: "/" + namespace, key });
  }
  #parts(namespace: string, values: unknown): void {
    this.edit([{ op: "set-" + namespace, value: values } as Edit]);
  }
  /** Set a whole typed operation, creating its namespace if absent. Collisions and invalid typed input throw EditError with a logical partPointer. */
  setOperation(key: string, value: Operation): void {
    this.#part("operations", key, "operation", value);
  }
  /** Commit a whole operations map once, for bulk synthesis. */
  setOperations(value: Readonly<Record<string, Operation>>): void {
    this.#parts("operations", value);
  }
  /** Set a whole typed binding, creating its namespace if absent. */
  setBinding(key: string, value: Binding): void {
    this.#part("bindings", key, "binding", value);
  }
  /** Commit a whole bindings map once. */
  setBindings(value: Readonly<Record<string, Binding>>): void {
    this.#parts("bindings", value);
  }
  /** Set a whole typed source, creating its namespace if absent. */
  setSource(key: string, value: Source): void {
    this.#part("sources", key, "source", value);
  }
  /** Commit a whole sources map once. */
  setSources(value: Readonly<Record<string, Source>>): void {
    this.#parts("sources", value);
  }
  /** Set a whole typed dependency, creating its namespace if absent. */
  setDependency(key: string, value: Dependency): void {
    this.#part("dependencies", key, "dependency", value);
  }
  /** Commit a whole dependencies map once. */
  setDependencies(value: Readonly<Record<string, Dependency>>): void {
    this.#parts("dependencies", value);
  }
  /** Set a typed example on an existing operation, creating its examples map if absent. */
  setExample(operation: string, key: string, value: Example): void {
    scalar(operation);
    scalar(key);
    handle(this);
    const context: AuthorContext = { origins: new WeakMap() };
    const object = authorObject(
      value,
      "example",
      pointerChild(pointerChild("/operations", operation) + "/examples", key),
      context,
    );
    this.#encoded(object, context, {
      namespace: pointerChild("/operations", operation) + "/examples",
      key,
    });
  }
}
function checkedText(value: string): string {
  if (typeof value !== "string") throw new TypeError("Expected a string.");
  return value;
}
function checkPartField(
  kind: AuthorKind,
  key: string,
  value: unknown,
  path: string,
): void {
  if (!typedFields[kind].includes(key)) return;
  let valid = true;
  if (["description", "kind", "operation", "source"].includes(key))
    valid = typeof value === "string";
  if (["deprecated", "idempotent"].includes(key))
    valid = typeof value === "boolean";
  if (key === "preference")
    valid = typeof value === "number" && Number.isSafeInteger(value);
  if (["tags", "aliases", "kinds"].includes(key)) {
    // Read descriptors, never getters. The ordinary encoder checks dense arrays.
    valid = Array.isArray(value);
    if (valid)
      for (const [name, d] of Object.entries(
        Object.getOwnPropertyDescriptors(value),
      )) {
        if (name === "length") continue;
        if (!("value" in d))
          throw new PartSerializationError(
            "accessor-property",
            pointerChild(path, name),
            "Typed lists cannot contain accessors.",
          );
        if (typeof d.value !== "string") valid = false;
      }
  }
  if (!valid)
    throw new PartSerializationError(
      "invalid-authoring-object",
      path,
      "Field does not have its typed representation.",
    );
}

/** Document proof acquisition: conformant transfers an independent ConformantDocument owner and report. Otherwise the original DocumentValidation explains refusal, nonconformance or uncertainty; no proof owner is created. */
export type ValidationResult =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "conformant";
      /** Caller-owned document handle for this branch; dispose independently. */
      document: ConformantDocument;
      /** Plain normative report for the exact snapshot. */
      report: ValidationReport;
    }
  | DocumentValidation;
const conformanceToken = Symbol("established conformance");
let assessDocument: (document: DocumentReads) => ValidationResult;
/** Retained proof for one immutable snapshot whose normative OBI rules were established. Obtain via Document.validate(); preparation remains evaluator-dependent. It shares read-only inspection and deterministic disposal, without inheriting Document or its setters. */
export class ConformantDocument extends DocumentReads {
  declare private readonly conformantDocumentBrand: void;
  private constructor(raw: wasm.WasmConformantDocument, token: symbol) {
    if (token !== conformanceToken)
      throw new TypeError("Use Document.validate() to establish conformance.");
    super(raw);
  }
  static {
    assessDocument = (document: DocumentReads): ValidationResult => {
      const transfer = documentRaw(document).validate();
      try {
        const result = decode<DocumentValidation>(transfer.report());
        const proof = transfer.takeConformant();
        if (!proof) return result;
        if (result.status !== "report") {
          proof.free();
          throw new Error("Validation transfer invariant.");
        }
        return {
          status: "conformant",
          document: new ConformantDocument(proof, conformanceToken),
          report: result.report,
        };
      } finally {
        transfer.free();
      }
    };
  }
  /** Retain an independent Rust proof for this same immutable snapshot. */
  retain(): ConformantDocument {
    return new ConformantDocument(
      sdkCall(() => conformantRaw(this).retain()),
      conformanceToken,
    );
  }
  /** Acquire an independent editable document owner; its changes do not affect this proof. */
  asDocument(): Document {
    return adoptDocument(conformantRaw(this).asDocument());
  }
  /** Consume this proof and return an independent editable owner. Every alias of this proof becomes disposed; independently retained owners survive. */
  intoDocument(): Document {
    const raw = sdkCall(() => conformantRaw(this).intoInner());
    handles.delete(this);
    finalizer?.unregister(this);
    return adoptDocument(raw);
  }
  /** The supported scalar specification declaration established by OBI-03. */
  override get openbindings(): string {
    conformantRaw(this);
    return super.openbindings!;
  }
  /** Select a primary key or alias; undefined means missing, because OBI-05 excludes ambiguity. Transfers a new disposable ConformantOperationView whose input() and output() hand out value contracts. */
  resolveOperation(name: string): ConformantOperationView | undefined {
    scalar(name);
    const raw = sdkCall(() => conformantRaw(this).resolveOperation(name));
    return raw ? adopt(raw, ConformantOperationView.fromRaw) : undefined;
  }
  /** Locate a schema location in this document: the node its pointer names, with line and column in the document's text. null for a location in a supplied resource, or a pointer that names no node in this revision. Positions are computed on this call; a prepared value contract keeps none. A malformed location throws SdkError with code invalid-schema-location. */
  locate(at: SchemaLocation): SourceLocation | null {
    const raw = conformantRaw(this);
    return decode(sdkCall(() => raw.locate(schemaLocationText(at))));
  }
}
// Exact JSON text for a schema location; JSON.stringify escapes an unpaired
// UTF-16 unit, so the engine reads the pointer exactly.
function schemaLocationText(at: SchemaLocation): string {
  return JSON.stringify({ resource: at?.resource, pointer: at?.pointer });
}
type OperationRaw = wasm.WasmOperation | wasm.WasmConformantOperation;
/** Reads shared by plain and conformant operation views. */
abstract class OperationReads extends Managed {
  #metadata?: OperationMetadata;
  #examples?: readonly ExampleMetadata[] | null;
  protected constructor(
    raw: wasm.WasmOperation | wasm.WasmConformantOperation,
  ) {
    super(raw);
  }
  /** Allocate the selected primary key, even when lookup used an alias. */
  get key(): string {
    return handle<OperationRaw>(this).key();
  }
  /** Return a NEW independently disposable ExactJson owner on every call. It survives operation/document disposal and can retain the original arena. */
  value(): ExactJson {
    return new ExactJson(handle<OperationRaw>(this).value());
  }
  /** Cached frozen metadata for this selected operation only; no disposable owner is created. */
  get metadata(): OperationMetadata {
    const raw = handle<OperationRaw>(this);
    return (this.#metadata ??= freezeMetadata(
      decode<OperationMetadata>(sdkCall(() => raw.metadata())),
    ));
  }
  /** Cached frozen example metadata in lexical key order; null means absent, empty means present-empty. Checks every example row but transports no values. */
  get examples(): readonly ExampleMetadata[] | null {
    const raw = handle<OperationRaw>(this);
    if (this.#examples === undefined)
      this.#examples = metadataRows<ExampleMetadata>(
        sdkCall(() => raw.examples()),
      );
    return this.#examples;
  }
  /** Acquire a selected independent example view, or undefined for absence; unrelated row metadata is not interpreted. */
  example(key: string): ExampleView | undefined {
    scalar(key);
    const raw = sdkCall(() => handle<OperationRaw>(this).example(key));
    return raw ? adopt(raw, ExampleView.fromRaw) : undefined;
  }
  /** Allocate a plain array of binding keys in lexical order. No owner disposal, ranking or invocation is involved. */
  get bindings(): readonly string[] {
    return decode(sdkCall(() => handle<OperationRaw>(this).bindings()));
  }
}
/** Retained operation of a Document, selected by primary key or alias. It survives parent disposal; dispose it independently. Exact value inspection is not normative proof or a prepared schema. */
export class OperationView extends OperationReads {
  /** @internal */ constructor(raw: wasm.WasmOperation) {
    super(raw);
  }
  /** Acquire a separate owner sharing this immutable selected operation. */
  retain(): OperationView {
    return new OperationView(handle<wasm.WasmOperation>(this).retain());
  }
  /** Acquire a NEW independently disposable input value; undefined means absent. JSON null, true, false and objects remain present values. */
  input(): ExactJson | undefined {
    const raw = handle<wasm.WasmOperation>(this).input();
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Acquire a NEW independently disposable output value; undefined means absent. */
  output(): ExactJson | undefined {
    const raw = handle<wasm.WasmOperation>(this).output();
    return raw ? new ExactJson(raw) : undefined;
  }
}
/** Retained operation of a ConformantDocument. Its input() and output() hand out value contracts; the plain OperationView of a Document cannot. It survives parent disposal; dispose it independently. */
export class ConformantOperationView extends OperationReads {
  private constructor(raw: wasm.WasmConformantOperation) {
    super(raw);
  }
  /** @internal */ static fromRaw(
    raw: wasm.WasmConformantOperation,
  ): ConformantOperationView {
    return new ConformantOperationView(raw);
  }
  /** Acquire a separate owner of this conformant operation, which hands out value contracts as this one does. */
  retain(): ConformantOperationView {
    return ConformantOperationView.fromRaw(
      conformantOperationRaw(this).retain(),
    );
  }
  /** Acquire a NEW disposable input value contract; undefined means the operation declares no input. Present true, false and object schemas are all contracts. Nothing is prepared here. */
  input(): ValueContract | undefined {
    const raw = conformantOperationRaw(this).input();
    return raw ? adopt(raw, ValueContract.fromRaw) : undefined;
  }
  /** Acquire a NEW disposable output value contract; undefined means the operation declares no output. Nothing is prepared here. */
  output(): ValueContract | undefined {
    const raw = conformantOperationRaw(this).output();
    return raw ? adopt(raw, ValueContract.fromRaw) : undefined;
  }
}
const resourceToken = Symbol("retained resource set");
/** Retained binding declaration view. Metadata is plain; each exact method acquires a separate owner. Neither reading nor retaining establishes conformance. */
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
  value(): ExactJson {
    return new ExactJson(handle<wasm.WasmBinding>(this).value());
  }
  /** Acquire a NEW exact content owner; undefined means absent, while present null remains an exact value. */
  content(): ExactJson | undefined {
    const raw = handle<wasm.WasmBinding>(this).content();
    return raw ? new ExactJson(raw) : undefined;
  }
}
/** Retained source declaration view. Metadata is plain; each exact method acquires a separate owner. Neither reading nor retaining establishes conformance. */
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
  value(): ExactJson {
    return new ExactJson(handle<wasm.WasmSource>(this).value());
  }
  /** Acquire a NEW exact content owner; undefined means absent, while present null remains an exact value. */
  content(): ExactJson | undefined {
    const raw = handle<wasm.WasmSource>(this).content();
    return raw ? new ExactJson(raw) : undefined;
  }
}
/** Retained dependency declaration view. Metadata is plain; each exact method acquires a separate owner. Neither reading nor retaining establishes conformance. */
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
  value(): ExactJson {
    return new ExactJson(handle<wasm.WasmDependency>(this).value());
  }
}
/** Retained example declaration view. Metadata is plain; each exact method acquires a separate owner. Neither reading nor retaining establishes conformance. */
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
  value(): ExactJson {
    return new ExactJson(handle<wasm.WasmExample>(this).value());
  }
  /** Acquire a NEW exact input owner; undefined means absent, while present null remains an exact value. */
  input(): ExactJson | undefined {
    const raw = handle<wasm.WasmExample>(this).input();
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Acquire a NEW exact output owner; undefined means absent, while present null remains an exact value. */
  output(): ExactJson | undefined {
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
    if (entries === undefined) {
      super(new wasm.WasmResources());
      return;
    }
    // Collect every entry, then admit the set once in Rust.
    let pending: wasm.WasmResourceEntries | undefined;
    let raw: wasm.WasmResources;
    try {
      for (const [uri, document] of entries as Iterable<
        readonly [string, ExactJson]
      >) {
        scalar(uri);
        (pending ??= new wasm.WasmResourceEntries()).push(
          uri,
          jsonRaw(document),
        );
      }
      raw = pending
        ? sdkCall(() => pending!.build())
        : new wasm.WasmResources();
    } finally {
      pending?.free();
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
  /** Locate a schema location in one of these resources: the node its pointer names in the resource with its URI, with line and column in that resource's own text. A resource built from a subtree of a larger value is admitted as its own text, so its coordinates count from the subtree's start, where ExactJson.locate on that subtree counts in the whole text it was parsed from. null for a document location, a URI this set does not hold (such as a packaged meta-schema's), or a pointer that names no node. Positions are computed on this call. A malformed location throws SdkError with code invalid-schema-location. */
  locate(at: SchemaLocation): SourceLocation | null {
    const raw = handle<wasm.WasmResources>(this);
    return decode(sdkCall(() => raw.locate(schemaLocationText(at))));
  }
}
/** A conformant operation's input or output value contract, not yet prepared. Obtain it from ConformantOperationView.input() or output(); it retains its proved revision and survives operation and document disposal. Each prepare() compiles anew; retain the PreparedValueContract for repeated validation. */
export class ValueContract extends Managed {
  private constructor(raw: wasm.WasmValueContract) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmValueContract): ValueContract {
    return new ValueContract(raw);
  }
  /** Acquire a NEW independently disposable owner for the exact schema, without preparing it. */
  schema(): ExactJson {
    return new ExactJson(contractRaw(this).schema());
  }
  /** Prepare with the built-in evaluator. Ready transfers a new disposable PreparedValueContract independent of this owner; it does not promise every value is decidable. The default evaluator can prepare qualified missing-resource value contracts, including a bare missing $ref and dual-polarity oneOf bounds. Validation establishes a verdict when independent of missing content, or returns resource-unavailable when dependent. Known errors and unsupported partial contracts still refuse preparation; prepare again with resources to recover, while existing prepared owners retain their original snapshot. Cancelled/transient failures do not poison retries. Pre-aborted work returns no-verdict. */
  prepare(options: PrepareOptions = {}): ValueContractPreparation {
    requireReady();
    const raw = contractRaw(this);
    const { resources, limits, signal } = prepareOptions(options);
    // Omitted resources and limits cross the bridge as absent: the engine uses
    // its shared empty set and default budgets without building either.
    const encoded = limits === undefined ? undefined : encodedLimits(limits);
    const preparation = sdkCall(() =>
      resources === undefined
        ? raw.prepare(encoded, isAborted(signal))
        : raw.prepareWith(resourceRaw(resources), encoded, isAborted(signal)),
    );
    try {
      const result = decode<
        | Exclude<ValueContractPreparation, { status: "ready" }>
        | { status: "ready" }
      >(preparation.result());
      if (result.status !== "ready") {
        if (result.status !== "no-verdict")
          throw new Error("Invalid preparation result.");
        return result;
      }
      const prepared = preparation.takeValueContract();
      if (!prepared)
        throw new Error("Preparation invariant: ready value contract missing.");
      return {
        status: "ready",
        valueContract: adopt(prepared, PreparedValueContract.fromRaw),
      };
    } finally {
      preparation.free();
    }
  }
  /** Return a NEW independently disposable owner of this unprepared value contract. */
  retain(): ValueContract {
    return ValueContract.fromRaw(contractRaw(this).retain());
  }
}
function prepareOptions(options: PrepareOptions): {
  resources?: SchemaResources;
  limits?: Partial<EvaluatorLimits>;
  signal?: AbortSignal;
} {
  if (!options || typeof options !== "object" || Array.isArray(options))
    throw new SdkError(
      "invalid-work-options",
      "Preparation options must be an object.",
    );
  const { resources, limits, ...work } = options;
  if (Object.prototype.hasOwnProperty.call(work, "diagnostics"))
    throw new SdkError(
      "invalid-work-options",
      "The diagnostics option was removed: call explain(value) on the prepared value contract to learn why a value fails.",
    );
  return { resources, limits, signal: workSignal(work) };
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
/** Disposable prepared value contract, independent of its document, resources, operation and contract owners. Repeated validation borrows exact input; ordinary inputs are admitted and temporary owners released within the call. No verdict here proves whole-document conformance. */
export class PreparedValueContract extends Managed {
  private constructor(raw: wasm.WasmPrepared) {
    super(raw);
  }
  /** @internal */ static fromRaw(
    raw: wasm.WasmPrepared,
  ): PreparedValueContract {
    return new PreparedValueContract(raw);
  }
  // Overload order matters: an untyped (any) argument takes the first
  // signature, so it must be one whose result keeps input-error.
  /** Admit ordinary JSON (including nested exact owners), then check it cheaply.
   * A string is a JSON string value, so validate("7") checks the string "7"; use validateJson for JSON text.
   * Expected admission failures return input-error. Caller exceptions and handle misuse throw.
   */
  validate(
    value: Exclude<JsonInput, ExactJson>,
    options?: WorkOptions,
  ): ValueCheck;
  /** The cheap check of an exact root: a failure carries no problems and problemsComplete is false. Cancellation cannot preempt synchronous Wasm. */
  validate(value: ExactJson, options?: WorkOptions): ValueOutcome;
  /** Check a value that may be ordinary or exact: an ordinary one is admitted first, so input-error remains possible. */
  validate(value: JsonInput, options?: WorkOptions): ValueCheck;
  validate(value: JsonInput, options: WorkOptions = {}): ValueCheck {
    return checkValue(this, value, options, false);
  }
  /** Admit exact JSON text or UTF-8 bytes as parseJson does, under the default limits, then check it cheaply: validateJson("7") checks the number 7. Every value outcome is returned, including undefined and no-verdict; input-error is admission failure only. Duplicate members yield no-verdict and numbers are never rounded. Wrong input types and literal unpaired UTF-16 text throw, as for parseJson; a pre-aborted signal returns cancellation first. Parse once with parseJson to check one value repeatedly; cancellation cannot preempt synchronous Wasm. */
  validateJson(input: ExactInput, options: WorkOptions = {}): JsonCheck {
    requireReady();
    const prepared = handle<wasm.WasmPrepared>(this);
    const signal = workSignal(options);
    if (isAborted(signal)) return cancelledValue();
    const data = bytes(input);
    try {
      return decode(prepared.validateJson(data));
    } catch (error) {
      return { outcome: "input-error", error: failure(error).error };
    }
  }
  // As for validate, the first signature is the one an untyped argument takes.
  /** Admit ordinary JSON (including nested exact owners), then explain it.
   * Expected admission failures return input-error. Caller exceptions and handle misuse throw.
   */
  explain(
    value: Exclude<JsonInput, ExactJson>,
    options?: WorkOptions,
  ): ValueCheck;
  /** Explain an exact root: the verdict validate() returns, and for a failure the library's first error with redacted messages and original schema facts. Explanation costs more than the cheap check and is library-bounded: it can allocate in proportion to the schema and value and can overflow the stack on deeply nested values, so keep validate() on paths that check untrusted input. Cancellation after the verdict keeps an established failure with problemsComplete false. */
  explain(value: ExactJson, options?: WorkOptions): ValueOutcome;
  /** Explain a value that may be ordinary or exact: an ordinary one is admitted first, so input-error remains possible. */
  explain(value: JsonInput, options?: WorkOptions): ValueCheck;
  explain(value: JsonInput, options: WorkOptions = {}): ValueCheck {
    return checkValue(this, value, options, true);
  }
  /** Return a NEW independently disposable owner sharing compiled state; it remains valid after the prior prepared owner is disposed. */
  retain(): PreparedValueContract {
    return PreparedValueContract.fromRaw(
      handle<wasm.WasmPrepared>(this).retain(),
    );
  }
}
function checkValue(
  contract: PreparedValueContract,
  value: JsonInput,
  options: WorkOptions,
  explain: boolean,
): ValueCheck {
  requireReady();
  const prepared = handle<wasm.WasmPrepared>(contract);
  const exact = isExact(value) ? jsonRaw(value) : undefined;
  const signal = workSignal(options);
  if (isAborted(signal)) return cancelledValue();
  const run = (raw: wasm.WasmJson, cancelled: boolean): ValueOutcome =>
    decode(
      explain
        ? prepared.explain(raw, cancelled)
        : prepared.validate(raw, cancelled),
    );
  if (exact) return run(exact, isAborted(signal));
  let temporary: ExactJson | undefined;
  try {
    let text: string;
    try {
      text = encodeOrdinary(value, { origins: new WeakMap() });
    } catch (error) {
      if (!(error instanceof PartSerializationError)) throw error;
      return {
        outcome: "input-error",
        error: {
          code: (error.code === "authoring-limit"
            ? "input-limit"
            : error.code) as ValueAdmissionFailure["code"],
          instancePointer: error.partPointer,
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
    return run(jsonRaw(temporary), false);
  } finally {
    temporary?.dispose();
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
/** Construct the fixed discovery URL from an absolute HTTP(S) origin. The origin must be ASCII: a non-ASCII domain name is refused, so pass its IDNA A-label form. Refuses credentials, resource paths, query and fragment; accepts an optional root slash. Does not fetch. The transport owns connection and redirect policy. */
export function discoveryEndpoint(origin: string): string {
  requireReady();
  scalar(origin);
  return sdkCall(() => wasm.discoveryEndpoint(origin));
}
/** A local asynchronous Fetch callback; native function overloads are not required. Called once per hop with that hop's URL; it must scope any credentials it adds by that URL, because the SDK cannot strip headers the callback adds itself. */
export type DiscoveryFetch = (
  url: string,
  init: RequestInit,
) => Promise<Response>;
/** One discovery attempt with an SDK-owned timer and AbortController. Where the host exposes redirect responses, the SDK checks and follows each hop itself; a host that hides them follows redirects under its own policy and the SDK checks only the final URL. */
export interface DiscoveryOptions {
  /** Decoded-body byte limit; omitted selects 1 MiB. Must be a positive safe integer; zero is refused. Bounds accepted/retained document bytes; Fetch may deliver a larger chunk before overflow is detected, so this does not bound host buffering. */
  maxDocumentBytes?: number;
  /** Whole-attempt milliseconds, default 30,000. Must be positive and finite; "unbounded" explicitly omits the deadline. Zero is refused. */
  timeoutMs?: number | "unbounded";
  /** Caller cancellation; aborts I/O and is checked around synchronous assessment. */
  signal?: AbortSignal;
  /** Local asynchronous Fetch-compatible callback; omitted uses host fetch. Honor the supplied normalized URL, redirect mode and cancellation; configure credentials and connection restrictions here. The callback is called once per hop: the SDK requests redirect: "manual" and calls it again for each admitted redirect, including one to another origin, passing only Accept. It must scope any credentials it adds by the URL it is given, as the Rust portable callback must: the SDK cannot strip headers the callback adds itself, so a credential added unconditionally reaches every admitted hop. When this callback answers manual mode with an opaqueredirect, as browser fetch does, the SDK repeats that request with redirect: "follow" and remembers the observation for this callback, so later discoveries through it use follow directly; Fetch then removes Authorization on a cross-origin redirect but keeps other request headers. */
  fetch?: DiscoveryFetch;
}
/** Observed response facts retained even when body reading or document assessment fails. The final URL is metadata only, not an implicit schema base. */
export interface DiscoveryMetadata {
  /** Constructed discovery URL requested from the transport. */
  requestedUrl: string;
  /** URL of the observed response, not a schema base: the URL the SDK last dispatched when it follows hops itself, the host's final URL when the host follows them, otherwise null. */
  finalUrl: string | null;
  /** Observed HTTP status. */
  status: number;
  /** Host Fetch response visibility/type. */
  responseType: ResponseType;
  /** Frozen observed header strings under host Fetch combining/visibility rules. */
  headers: Readonly<Record<string, string>>;
}
/** Validation of a complete bounded discovery body. Found owns proof; other document handles, when present, remain unproven documents. */
type DiscoveryBodyResult =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "found";
      /** Caller-owned document handle for this branch; dispose independently. */
      document: ConformantDocument;
      /** Plain normative report for the exact snapshot. */
      report: ValidationReport;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "non-conformant" | "undetermined";
      /** Caller-owned document handle for this branch; dispose independently. */
      document?: Document;
      /** Plain normative report for the exact snapshot. */
      report: ValidationReport;
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
/** Finite discovery receipt. Only found carries normative proof. Only HTTP 404 means absent; 401/403 are gated; other non-200 statuses are http-status, except that a 301, 302, 303, 307 or 308 with a visible Location is followed or refused: a refused destination, the eleventh redirect, or one a following host left unfollowed is a redirect-policy transport-error. Complete bounded 200 bytes can accompany refused/invalid documents; partial bodies are never returned. Dispose any returned document owner independently. */
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
      reason: "aborted";
    }
  | {
      /** Whole-attempt or positively identified host deadline expired. */
      status: "timeout";
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "transport-error" | "body-error";
      /** Structured expected failure for this branch. */
      error: {
        /** Observable acquisition category. Timeouts have their own status; Fetch exposes no decode diagnosis. */
        kind: "network" | "redirect-policy" | "other";
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
/** Redirects the SDK follows itself, as the Rust checked paths' MAX_REDIRECTS. */
const maxRedirects = 10;
/**
 * Transports observed to hide redirects: one answered redirect: "manual" with an
 * opaqueredirect, as browsers do. Set only by that observation, never by host detection.
 * Keyed by the supplied callback, or by the host fetch function for the default transport,
 * so a callback that exposes hops keeps its checks in a runtime whose own fetch hides them.
 */
const hidesRedirects = new WeakSet<object>();
const redirectStatuses = [301, 302, 303, 307, 308];
/** HTTPS, or HTTP to normalized literal IPv4 127/8 or IPv6 ::1, without credentials. */
function admitUrl(url: URL): boolean {
  const loopback =
    url.hostname === "[::1]" || /^127\.\d+\.\d+\.\d+$/.test(url.hostname);
  return (
    !url.username &&
    !url.password &&
    (url.protocol === "https:" || (url.protocol === "http:" && loopback))
  );
}
/** Destination admission plus refusal of every HTTPS-to-HTTP hop, as in Rust. */
function admitRedirect(previous: URL, next: URL): boolean {
  return (
    !(previous.protocol === "https:" && next.protocol === "http:") &&
    admitUrl(next)
  );
}
/** Resolve a visible Location against the dispatched URL; undefined when refused. */
function redirectTarget(current: string, location: string): string | undefined {
  // As the Rust checked paths: printable ASCII without backslashes, then WHATWG resolution.
  if (!/^[\x21-\x7e]*$/.test(location) || location.includes("\\"))
    return undefined;
  let previous: URL, next: URL;
  try {
    previous = new URL(current);
    next = new URL(location, previous);
  } catch {
    return undefined;
  }
  next.hash = "";
  return admitRedirect(previous, next) ? next.href : undefined;
}
/** Whether a response URL names the dispatched URL; parses only a differing spelling. */
function sameUrl(observed: string, dispatched: string): boolean {
  if (observed === dispatched) return true;
  try {
    return new URL(observed).href === new URL(dispatched).href;
  } catch {
    return false;
  }
}
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
/** Retrieve decoded bytes, then validate them in Rust. Only 404 means absence.
 * Returned document handles belong to the caller and must be disposed.
 * The SDK timer covers headers, followed redirects, decoded reads and validation boundaries.
 * Caller cancellation wins a same-observation tie; the first observed cause is retained.
 * Synchronous Wasm assessment requires a Worker and responsive supervisor for a hard latency bound.
 * Fetch uses redirect: "manual". Where the host exposes the redirect response, as Node does,
 * the SDK checks each Location before dispatching it, as the Rust checked paths do: it refuses
 * every HTTPS-to-HTTP hop, a destination the origin rules refuse and an eleventh redirect, and
 * calls the transport again for each admitted hop. A host that hides redirects answers with an
 * opaqueredirect, as browsers do; the SDK then repeats the request with redirect: "follow" and
 * checks the final URL, and later discoveries through the same transport use follow directly.
 * Per-hop mixed-content and private-network rules then belong to the browser, and an HTTPS
 * final URL proves no hop history.
 */
export async function discover(
  origin: string,
  options: DiscoveryOptions = {},
): Promise<DiscoveryResult> {
  requireReady();
  const requestedUrl = discoveryEndpoint(origin),
    policy = discoveryPolicy();
  const configured = options.maxDocumentBytes ?? policy.defaultMaxDocumentBytes;
  if (!Number.isSafeInteger(configured) || configured <= 0)
    throw new SdkError(
      "invalid-byte-limit",
      "maxDocumentBytes must be a positive safe integer.",
    );
  const timeout = options.timeoutMs ?? 30_000;
  if (timeout !== "unbounded" && (!Number.isFinite(timeout) || timeout <= 0))
    throw new SdkError(
      "invalid-timeout",
      "timeoutMs must be positive and finite, or explicitly unbounded.",
    );
  const host = globalThis.fetch,
    transport: object | undefined = options.fetch ?? host,
    fetcher = options.fetch ?? host?.bind(globalThis);
  if (!fetcher || !transport)
    throw new SdkError(
      "fetch-unavailable",
      "Supply a Fetch callback in this host.",
    );
  const limit = configured,
    controller = new AbortController(),
    signal = controller.signal,
    deadline = timeout === "unbounded" ? Infinity : performance.now() + timeout;
  if (timeout !== "unbounded" && !Number.isFinite(deadline))
    throw new SdkError(
      "invalid-timeout",
      "timeoutMs exceeds the host clock range.",
    );
  let cause: "cancelled" | "timeout" | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cancellation = (): DiscoveryResult =>
    cause === "timeout"
      ? { status: "timeout" }
      : { status: "cancelled", reason: "aborted" };
  const stop = (next: "cancelled" | "timeout") => {
    cause ??= next;
    controller.abort();
  };
  const check = () => {
    if (!cause) {
      if (options.signal?.aborted)
        stop(
          options.signal.reason?.name === "TimeoutError"
            ? "timeout"
            : "cancelled",
        );
      else if (performance.now() >= deadline) stop("timeout");
    }
    return cause !== undefined;
  };
  const cancel = () => {
    check();
  };
  if (check()) return cancellation();
  options.signal?.addEventListener("abort", cancel, { once: true });
  const schedule = () => {
    timer = setTimeout(
      () => {
        if (!check()) schedule();
      },
      Math.min(2_147_483_647, Math.max(0, deadline - performance.now())),
    );
  };
  if (timeout !== "unbounded") schedule();
  let metadata: DiscoveryMetadata | undefined,
    body: Uint8Array | undefined,
    reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  let phase: "transport" | "body" | "assessment" = "transport";
  try {
    let current = requestedUrl,
      // A transport already seen hiding redirects follows them without the repeated request.
      redirect: "manual" | "follow" = hidesRedirects.has(transport)
        ? "follow"
        : "manual",
      followed = 0,
      response: Response;
    while (true) {
      if (check()) return { ...cancellation(), metadata };
      const pending = fetcher(current, {
        method: "GET",
        headers: { Accept: policy.accept },
        signal,
        redirect,
      });
      const observed = await interruptible(
        Promise.resolve(pending).then((response) => {
          if (signal.aborted) void response.body?.cancel().catch(() => {});
          return response;
        }),
        signal,
      );
      response = observed;
      const release = () => void observed.body?.cancel().catch(() => {});
      // The host hides this redirect, so no hop can be checked before dispatch:
      // let the host follow under its own policy and check the final URL.
      if (
        redirect === "manual" &&
        followed === 0 &&
        observed.type === "opaqueredirect"
      ) {
        release();
        hidesRedirects.add(transport);
        redirect = "follow";
        continue;
      }
      const opaque =
        observed.type === "opaque" || observed.type === "opaqueredirect";
      metadata = {
        requestedUrl,
        finalUrl: opaque
          ? null
          : observed.url || (redirect === "manual" ? current : null),
        status: observed.status,
        responseType: observed.type,
        headers: Object.freeze(Object.fromEntries(observed.headers.entries())),
      };
      if (check()) {
        release();
        return { ...cancellation(), metadata };
      }
      const policyRefusal = (message: string): DiscoveryResult => {
        release();
        return {
          status: "transport-error",
          metadata,
          error: { kind: "redirect-policy", message },
        };
      };
      if (opaque)
        return policyRefusal(
          "Discovery policy cannot be established for an opaque response.",
        );
      const location = redirectStatuses.includes(observed.status)
        ? observed.headers.get("location")
        : null;
      if (redirect === "manual") {
        // The SDK dispatched `current` itself; another observed URL means the host followed unchecked hops.
        if (
          observed.redirected ||
          (observed.url && !sameUrl(observed.url, current))
        )
          return policyRefusal(
            "Transport response URL violates the hop contract or destination policy.",
          );
        metadata.finalUrl = current;
        if (location === null) break;
        const next = redirectTarget(current, location);
        if (!next || followed >= maxRedirects)
          return policyRefusal(
            "Redirect destination or redirect limit refused.",
          );
        // No redirect body is an OBI; release it before the next dispatch.
        release();
        current = next;
        followed++;
        continue;
      }
      if (observed.url) {
        let final: URL;
        try {
          final = new URL(observed.url);
        } catch {
          return policyRefusal(
            "Final response URL cannot establish discovery policy.",
          );
        }
        if (!admitRedirect(new URL(requestedUrl), final))
          return policyRefusal(
            "Final response URL violates discovery destination or downgrade policy.",
          );
      }
      // A following host follows these redirects, so one still carrying Location was left unfollowed.
      if (location !== null)
        return policyRefusal(
          "Host did not follow the observed redirect Location.",
        );
      break;
    }
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
        if (check()) return { ...cancellation(), metadata };
        const part = await interruptible(reader.read(), signal);
        // F3: successful EOF completes the body before the following stop check.
        if (part.done) break;
        if (check()) return { ...cancellation(), metadata };
        const chunk = part.value;
        if (!(chunk instanceof Uint8Array))
          throw new SdkError(
            "invalid-body-chunk",
            "Fetch body must yield Uint8Array chunks.",
          );
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
    if (check()) return { ...cancellation(), metadata, body };
    phase = "assessment";
    const assessed = new wasm.WasmDiscovery(body);
    try {
      const result = decode<DiscoveryBodyResult>(assessed.result()),
        raw = assessed.document();
      let document: Document | undefined = raw ? adoptDocument(raw) : undefined;
      if (result.status === "found") {
        if (!document)
          throw new Error("Discovery invariant: conformant document missing.");
        const validation = document.validate();
        document.dispose();
        document = undefined;
        if (validation.status !== "conformant")
          throw new Error("Discovery invariant: conformance changed.");
        if (check()) {
          validation.document.dispose();
          return { ...cancellation(), metadata, body };
        }
        return { ...result, document: validation.document, metadata, body };
      }
      if (check()) {
        document?.dispose();
        return { ...cancellation(), metadata, body };
      }
      return { ...result, ...(document ? { document } : {}), metadata, body };
    } finally {
      assessed.free();
    }
  } catch (error) {
    if (check() || error === aborted)
      return { ...cancellation(), metadata, body };
    if (error instanceof Error && error.name === "TimeoutError")
      return { status: "timeout", metadata, body };
    if (phase === "assessment") throw error;
    return {
      // A failed later hop keeps the earlier redirect metadata but is still acquisition.
      status: phase === "body" ? "body-error" : "transport-error",
      metadata,
      body,
      error: {
        kind: error instanceof SdkError ? "other" : "network",
        message: error instanceof Error ? error.message : "Fetch failed",
      },
    };
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    options.signal?.removeEventListener("abort", cancel);
    if (reader) {
      void reader.cancel().catch(() => {});
      reader.releaseLock();
    }
    controller.abort();
  }
}
/** Publication CORS policy. Exact origins must be canonical ASCII tuple origins. */
export type AllowOrigin =
  | "any"
  | "omit"
  | "null"
  | {
      /** Canonical ASCII serialized tuple origin; sentinel spellings belong to the explicit choices. */
      origin: string;
    };

/** Disposable immutable copy of a proved discovery document. Creates no listener or server; authorization and request routing remain caller-owned. Existing publication remains usable after the original proof is disposed. */
export class DiscoveryPublication extends Managed {
  /** Copy a ConformantDocument into an independently owned publication. allowOrigin defaults to "any"; supports "omit", "null", or an exact ASCII serialized tuple origin. Opaque schemes and noncanonical spellings are refused. Invalid configuration throws; the source proof remains caller-owned. */
  constructor(
    document: ConformantDocument,
    options: {
      /** Optional typed CORS policy; omitted emits *, "omit" omits the header. Authentication, credentials and preflight remain application-owned. */
      allowOrigin?: AllowOrigin;
    } = {},
  ) {
    requireReady();
    const choice = options.allowOrigin ?? "any";
    let allow: string;
    if (choice === "any") allow = "*";
    else if (choice === "omit") allow = "";
    else if (choice === "null") allow = "null";
    else if (
      typeof choice === "object" &&
      choice !== null &&
      typeof choice.origin === "string"
    ) {
      allow = choice.origin;
      scalar(allow);
      if (["", "*", "null"].includes(allow))
        throw new SdkError(
          "invalid-allow-origin",
          "Exact origin must be a canonical ASCII tuple origin.",
        );
    } else
      throw new SdkError(
        "invalid-allow-origin",
        "Expected a typed allow-origin choice.",
      );
    super(
      sdkCall(
        () =>
          new wasm.WasmPublication(
            conformantRaw(document, "publication-conformance"),
            allow,
          ),
      ),
    );
  }
  /** Create a detached Response for the exact route and method, preserving reserved percent escapes. GET returns exact bytes; HEAD returns no body with the original Content-Length; unsupported route is 404 and unsupported method 405. Creates no server and does not perform authentication or preflight handling. */
  respond(request: Request): Response {
    const url = new URL(request.url);
    const path = url.pathname;
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
interface EditEncoding {
  chunks: string[];
  owners: ExactJson[];
  indices: number[];
  seen: Map<ExactJson, number>;
}
/** Iterative, checked serialization. No getters, toJSON hooks, omission, or implicit coercion. */
function encodeOrdinary(
  value: unknown,
  authoring?: AuthorContext,
  exact?: EditEncoding,
): string {
  const childPath = (owner: object, name: string, parent: string) =>
    authoring
      ? (authoring.origins.get(owner)?.get(name) ?? pointerChild(parent, name))
      : "";
  function reject(
    code: AuthoringErrorCode,
    pointer: string,
    message: string,
    limit = false,
  ): never {
    if (authoring) throw new PartSerializationError(code, pointer, message);
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
    const { pointer } = job;
    const value =
      authoring?.editBatch && job.depth === 1
        ? normalizeEdit(job.value, authoring)
        : job.value;
    if (++nodes > 1_000_000 || job.depth > 10_000)
      reject(
        "authoring-limit",
        pointer,
        "Ordinary value exceeds the JSON node or nesting limit.",
        true,
      );
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
      } else add(value.text, pointer);
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

/** Editable normative operation fields. Typed optional undefined means absence; opaque JSON fields preserve explicit null and reject undefined. The edited document still requires normative assessment and separate schema preparation. */
export interface Operation {
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
  examples?: Readonly<Record<string, Example>>;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a part serialization error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Exact input/output example instances, not schemas or proof that an operation accepts them. */
export interface Example {
  /** Optional example explanation. */
  description?: string;
  /** Optional exact input instance, including explicit null. */
  input?: JsonInput;
  /** Optional exact output instance, including explicit null. */
  output?: JsonInput;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a part serialization error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Binding source vocabulary; kind-specific content is exact opaque JSON and is never executed or acquired by core. */
export interface Source {
  /** Source-kind identifier selecting external interpretation. */
  kind: string;
  /** Optional exact kind-specific content; not interpreted by core. */
  content?: JsonInput;
  /** Optional source explanation. */
  description?: string;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a part serialization error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Link from an operation to a source with opaque kind-specific content and annotations. Core does not rank or invoke bindings. */
export interface Binding {
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
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a part serialization error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Dependency declaration with an optional exact source-kind filter. Absence accepts all; a conformant present filter must be nonempty. */
export interface Dependency {
  /** Referenced operation name. */
  operation: string;
  /** Exact permitted source kinds; absence accepts all. An empty array is representable for inspection but fails normative assessment. */
  kinds?: readonly string[];
  /** Optional dependency explanation. */
  description?: string;
  /** Extension/unknown members retained as exact JSON. Normative assessment decides permission; collisions with typed members return a part serialization error. */
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/**
 * Typed part serialization refusal codes for this package. This is a closed TypeScript union; later releases may add codes. Messages explain; codes identify.
 * - field-collision: additionalFields shadows a typed member.
 * - duplicate-field: an input represents the same emitted field more than once.
 * - non-finite-number: NaN or infinity cannot enter JSON.
 * - unsupported-value: undefined, bigint, function or symbol appears in opaque JSON.
 * - sparse-array / array-property: an array has holes or extra named properties.
 * - cyclic-value: an object refers to an active ancestor.
 * - non-plain-object: an object has a custom prototype.
 * - accessor-property / non-enumerable-property / symbol-key: admission would require
 *   executing or silently dropping a property, so the value is refused.
 * - invalid-authoring-object: a typed part container or known field is not representable.
 * - authoring-limit: encoded output exceeds a finite character/byte/node/depth limit.
 * A partPointer addresses the caller's input, including additionalFields, rather
 * than the emitted JSON. Initialization/owner misuse and unexpected exceptions throw.
 */
type AuthoringErrorCode =
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
  | "authoring-limit";
/** An atomic edit was refused. code is stable; partPointer locates a typed part input when available. The document remains unchanged. */
export class EditError extends SdkError {
  /** Logical pointer into a typed part input; null when unavailable. */
  readonly partPointer: string | null;
  /** Construct an edit refusal without inventing source byte coordinates. */
  constructor(
    code: string,
    message: string,
    partPointer: string | null = null,
  ) {
    super(code, message);
    this.name = "EditError";
    this.partPointer = partPointer;
  }
}
class PartSerializationError extends EditError {
  constructor(
    code: AuthoringErrorCode,
    pointer: string | null,
    message: string,
  ) {
    super(code, message, pointer);
  }
}
interface AuthorContext {
  origins: WeakMap<object, Map<string, string>>;
  editBatch?: boolean;
}
function normalizeEdit(value: unknown, context: AuthorContext): unknown {
  const entries = ownEntries(value, "");
  const fields = Object.fromEntries(entries);
  const namespaces: Record<string, AuthorKind> = {
    "set-operations": "operation",
    "set-sources": "source",
    "set-bindings": "binding",
    "set-dependencies": "dependency",
  };
  const kind =
    typeof fields.op === "string" && Object.hasOwn(namespaces, fields.op)
      ? namespaces[fields.op]
      : undefined;
  if (!kind) return value;
  if (entries.some(([key]) => key !== "op" && key !== "value"))
    throw new EditError(
      "invalid-edits",
      "Typed namespace edits accept op and value only.",
    );
  const pointer = "/" + (fields.op as string).slice(4);
  const parts = Object.fromEntries(
    ownEntries(fields.value, pointer).map(([key, part]) => [
      key,
      authorObject(part, kind, pointerChild(pointer, key), context),
    ]),
  );
  return { op: "set", pointer, value: parts };
}
function pointerChild(parent: string, name: string): string {
  return parent + "/" + name.replace(/~/g, "~0").replace(/\//g, "~1");
}
type AuthorKind = "operation" | "example" | "source" | "binding" | "dependency";
const typedFields: Record<AuthorKind, readonly string[]> = {
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
    throw new PartSerializationError(code, at, message);
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
          pointerChild(pointer ?? "", key),
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
    const fieldPath = pointerChild(pointer, key);
    if (key === "additionalFields") {
      if (value === undefined) continue;
      for (const [name, content] of ownEntries(value, fieldPath)) {
        const contentPath = pointerChild(fieldPath, name);
        if (typedFields[kind].includes(name))
          throw new PartSerializationError(
            "field-collision",
            contentPath,
            "Additional field shadows a typed member.",
          );
        if (Object.hasOwn(out, name))
          throw new PartSerializationError(
            "duplicate-field",
            contentPath,
            "The input supplies the same member twice.",
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
      kind === "operation" && key === "examples" ? "example" : undefined;
    checkPartField(kind, key, value, fieldPath);
    if (Object.hasOwn(out, key))
      throw new PartSerializationError(
        "duplicate-field",
        fieldPath,
        "The input supplies the same member twice.",
      );
    origins.set(key, fieldPath);
    out[key] = child
      ? Object.fromEntries(
          ownEntries(value, fieldPath).map(([name, v]) => [
            name,
            authorObject(v, child, pointerChild(fieldPath, name), context),
          ]),
        )
      : (value as JsonInput);
  }
  for (const key of requiredFields[kind]) {
    if (!Object.hasOwn(out, key))
      throw new PartSerializationError(
        "invalid-authoring-object",
        pointerChild(pointer, key),
        "Required typed field is absent.",
      );
  }
  return out;
}
