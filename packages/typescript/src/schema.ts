/** Original schemas, external resources, preparation and value evaluation. */
import * as wasm from "./wasm/openbindings_wasm.js";
import {
  JsonEncodingError,
  SdkError,
  decode,
  failure,
  sdkCall,
} from "./diagnostics.js";
import type { InputFailure, SourceLocation } from "./diagnostics.js";
import { requireReady } from "./runtime.js";
import {
  ExactJson,
  ORDINARY_LIMITS,
  encodeOrdinary,
  exact,
  inputErrorKind,
  isExact,
  jsonRaw,
  ownEntries,
  scalar,
} from "./json.js";
import type { ExactInput, JsonInput } from "./json.js";
import { Managed, adopt, handle } from "./ownership.js";

/** A keyword or schema in an original schema source, by source URI and exact pointer. sourceUri null denotes the OpenBindings document; supplied URIs identify explicit resources and never initiate acquisition. Private evaluator projection identities are not exposed. Line and column come from the source's holder: ConformantDocument.locate() for the document and ExternalSchemas.locate() for a supplied resource; prepared schemas may retain immutable original sources needed for evaluation. Keep the corresponding document or external-schema owner when explicit coordinate lookup is needed. */
export interface SchemaOrigin {
  /** Original source retrieval URI, the published fixed-schema URI for an OBI-02 finding, or null for the OpenBindings document; distinct from a nested resource’s $id. */
  sourceUri: string | null;
  /** Exact RFC 6901 pointer from that source's root; empty string denotes root. */
  pointer: string;
}

/** Known refusal causes for this facade and its bundled engine. Unsupported capability, conservative preparation, unavailable resources, limits, cancellation and evaluator failure do not establish satisfaction/failure. A potential cycle alone does not establish undefinedness. Later package versions may add causes. Missing content for an ordinary value-evaluation reference does not by itself refuse preparation or decide a value outcome: independent assertions may establish satisfaction or failure. Missing a required meta-schema can instead prevent admission and produce a preparation failure. Document.references() exposes document-wide reference spellings and original keyword locations, not an exact per-schema missing-resource list; apply an application disclosure policy before logging them. */
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
  location: SchemaOrigin | null;
}

/** Established undefined result under the schema evaluation rules, separate from an evaluator's inability to answer. */
export interface UndefinedResult {
  /** Stable evaluator-defined detail code. */
  code: string;
  /** Explanatory text; custom evaluators should avoid echoing source or instance data. */
  message: string;
  /** Original schema location when known, otherwise null. */
  location: SchemaOrigin | null;
}

/** Schema facts included by explain() and OBI-02 findings from the published fixed schema, never copied from rejected instance data. Bounds and enum choices are exact JSON token strings: do not coerce them to Number. Serializing whole problems into a public response sends schema facts; an app with a private contract should forward only instance and code. Render facts as text. Each variant contains complete facts. */
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
  /** Exact RFC 6901 pointer to the failing node, from the value passed to evaluate/explain (also when that value was acquired from a larger text); it can hold an unpaired UTF-16 unit. ExactJson.locate(instance) gives its byte offset, line and column when asked; an ordinary value is located in its exact JSON serialization. */
  instance: string;
  /** The failing keyword in its original schema source, from that document's or resource's root: ConformantDocument.locate() or ExternalSchemas.locate() gives its coordinates. When the evaluator cannot map a keyword to the original source, the selected schema itself rather than a guessed keyword. */
  schema: SchemaOrigin;
  /** Stable evaluator-defined problem code, commonly the failed keyword. */
  code: string;
  /** Explanatory text; present pointers separately and safely. */
  message: string;
  /** Schema facts from the failed keyword. Absent means no matching variant or unavailable facts; facts that cannot fit or be represented exactly leave problemsComplete false. */
  details?: ValueProblemDetails;
}

/** Closed selected-schema result partition: satisfies, fails, undefined, or no-verdict. An established failure may have incomplete diagnostics; problemsComplete does not weaken its verdict. Neither satisfies nor fails proves normative OBI conformance of the surrounding document. */
export type ValueOutcome =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "satisfies";
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "fails";
      /** Value-free explanation of the established failure. */
      message: string;
      /** Retained actual failing instance locations. */
      problems: readonly ValueProblem[];
      /** Whether selected failure diagnostics, including schema facts, completed; false does not weaken the established failure. */
      problemsComplete: boolean;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "undefined";
      /** Established undefined result, with an original schema coordinate when known. */
      detail: UndefinedResult;
    }
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "no-verdict";
      /** Structured no-verdict reason; no satisfaction/failure is claimed. */
      detail: NoVerdict;
    };

/** Original-context lookup failure. Undefined describes a local reference defect, not an instance verdict. */
export type ResolutionFailureKind =
  | {
      /** A reference defect whose meaning is undefined by the schema rules. */
      kind: "undefined";
      /** The original reference defect; this judges no instance. */
      details:
        | "malformed-reference"
        | "missing-target"
        | "non-schema-target"
        | "missing-anchor"
        | "ambiguous-anchor"
        | "invalid-identifier";
    }
  | {
      /** A resource or dialect the current environment cannot supply. */
      kind: "resource-unavailable" | "unsupported-dialect";
      /** Identity of the unavailable resource or dialect. */
      details: {
        /** Original requested resource URI or dialect identifier; resolution performs no implicit resource acquisition. */
        uri: string;
      };
    }
  | {
      /** The implementation cannot perform a required schema capability. */
      kind: "unsupported-capability";
      /** Capability that prevented resolution. */
      details: {
        /** Stable unsupported capability identifier. */ capability: string;
      };
    }
  | {
      /** Resolution exceeded an implementation work or storage budget. */
      kind: "limit";
      /** Budget that prevented resolution. */
      details: {
        /** Stable exhausted resource identifier. */ resource: string;
      };
    }
  | {
      /** Other closed resolution refusals; none establishes a value verdict. */
      kind:
        | "invalid-external-schema"
        | "cancelled"
        | "wrong-environment"
        | "not-schema-location"
        | "invalid-anchor-name";
    };

/** A failed preparation attempt, distinct from every per-value outcome. */
export type PreparationFailure = {
  /** Original schema coordinate when known; no generated coordinate is substituted. */
  origin: SchemaOrigin | null;
} & (
  | {
      /** Original-context schema resolution failed. */ kind: "resolution";
      /** Retained structured resolution cause. */ details: {
        /** Resolution refusal in the original schema environment. */ cause: ResolutionFailureKind;
      };
    }
  | {
      /** A required evaluator capability is unavailable. */ kind: "unsupported-capability";
      /** Capability that prevented preparation. */ details: {
        /** Stable unsupported capability identifier. */ capability: string;
      };
    }
  | {
      /** Preparation exhausted a work or storage budget. */ kind: "limit";
      /** Budget that prevented preparation. */ details: {
        /** Stable exhausted resource identifier. */ resource: string;
      };
    }
  | {
      /** Cancellation or invalid supplied schema prevented preparation. */ kind:
        | "cancelled"
        | "invalid-external-schema";
    }
  | {
      /** The evaluator encountered an implementation failure. */ kind: "implementation";
      /** Explanatory implementation information. */ details: {
        /** Human-readable explanation; not a value verdict. */ message: string;
      };
    }
);

/** Preparation transfers an owned PreparedSchema or a refusal that judges no instance. Ready means prepared, not decidable: evaluation may still return no-verdict. */
export type SchemaPreparation =
  | {
      /** Preparation succeeded; dispose the transferred schema. */ status: "ready";
      /** Independent prepared schema owner. */ schema: PreparedSchema;
    }
  | {
      /** Preparation failed before evaluating any instance. */ status: "refused";
      /** Structured preparation refusal with its original origin when known. */ failure: PreparationFailure;
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
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "input-error";
      /** Structured expected failure for this branch. */
      error: ValueAdmissionFailure;
    };

/** Exact JSON text validation result: input-error when the text is not admitted as JSON, with the same error parseJson reports; after admission, the same selected-schema ValueOutcome as for an ExactJson root. */
export type JsonCheck =
  | ValueOutcome
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "input-error";
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
  location: SchemaOrigin | null;
  /** Resolved identity of the unavailable resource, when that is the cause. */
  resourceUri: string | null;
}

/** One reference keyword in a supported schema position. Missing or malformed openbindings declarations use 0.2's schema positions and resolution rules. Opaque content is not interpreted as schema; located is static resolution, and dynamicLookup still requires evaluation-time scope handling. */
export interface Reference {
  /** Original reference-keyword location. */
  location: SchemaOrigin;
  /** Original location of the schema beginning the site's resource, not the resolved base. Null exactly for the document resource. A supplied source's top resource uses its sourceUri and an empty pointer even without $id. Conformant Rust callers can use SchemaNode::resource_base for the base. */
  resourceRoot: SchemaOrigin | null;
  /** Reference keyword spelling, such as $ref or $dynamicRef. */
  keyword: string;
  /** Original scalar string, or null when not representable. */
  spelling: string | null;
  /** Static resolution or structured refusal; does not acquire resources. */
  resolution:
    | {
        /** Located establishes a static target; unresolved preserves the refusal. */
        status: "located";
        /** Original source target coordinate. */
        target: SchemaOrigin;
        /** Whether runtime dynamic-anchor scope must still be applied. */
        dynamicLookup: boolean;
      }
    | {
        /** Located establishes a static target; unresolved preserves the refusal. */
        status: "unresolved";
        /** Structured unresolved-reference cause. */
        detail: ReferenceRefusal;
      };
}

/** Original-context reference inspection, separate from normative proof and preparation. complete describes traversal completion; a complete report can still contain unresolved reference results.
 * Missing or malformed openbindings declarations use 0.2's rules, matching validation of a text declaring no version. This adds no limitation; a complete traversal is complete. Well-formed unsupported declarations and duplicate member names still prevent inspection.
 * Retains at most 8 MiB of generated and source text in reference entries: all location, resource-root, target and refusal pointers, every origin's sourceUri, keywords, spellings, refusal codes/messages and unavailable-resource URIs. Text uses UTF-8 bytes; pointers through an unpaired name use two bytes per UTF-16 unit. This is a text allowance, not total heap usage. Each whole entry is charged before materializing or cloning its text. The first entry that does not fit stops traversal, sets complete to false and reports a limit-exceeded limitation with code `reference-report-bytes` and no omitted location.
 */
export interface ReferenceReport {
  /** Retained reference observations in traversal order. */
  references: readonly Reference[];
  /** Whether traversal completed; unresolved entries can occur even when true. */
  complete: boolean;
  /** Traversal-level limitation when incomplete, otherwise null. Inspection has no separate schema nesting bound beyond document admission. */
  limitation: ReferenceRefusal | null;
}

/** Cooperative work control. A pre-aborted signal returns cancellation. Synchronous Wasm does not yield to same-thread JavaScript, so use a worker plus host scheduling to interrupt long synchronous work externally. This is not a wall-clock deadline. */
export interface WorkOptions {
  /** Optional caller-owned cooperative cancellation signal; synchronous Wasm only observes same-thread cancellation at call boundaries. */
  signal?: AbortSignal;
}

/** Built-in evaluator budgets. Each numeric value is a nonnegative 32-bit integer; evaluationSdkWorkUnits and schemaResources also accept null. Preparation exhaustion returns a preparation failure; evaluation exhaustion returns no-verdict. Exhausted diagnostics preserve an established failure with problemsComplete false. These budgets are not elapsed-time deadlines or total process-memory bounds. */
export interface EvaluatorLimits {
  /** Cooperative SDK evaluation work units; null (default) sets no caller-selected cap. Internal capability bounds still apply. These are SDK-defined debits, not CPU operations or a count of schema steps. */
  evaluationSdkWorkUnits: number | null;
  /** Maximum UTF-8 bytes in one schema pattern and its expanded form; default 1,048,576. */
  patternBytes: number;
  /** Caller-selected authored pattern group ceiling; default 32. The supported maximum remains 32 even when a larger value is requested. */
  patternDepth: number;
  /** Aggregate retained pattern-form bytes; default 4,194,304. Equivalent normalized forms share one immutable program across value patterns and property names. Expanded forms can be larger than their authored patterns. */
  totalPatternBytes: number;
  /** Immutable pattern-program footprint; default and supported maximum 67,108,864 bytes. Per-call search caches use evaluationSdkReservationBytes; this is not a process-memory measurement. */
  totalPatternMemory: number;
  /** Cumulative SDK evaluation storage reservations; default 50,331,648 bytes. Includes proof, memo, scratch, search-cache and diagnostic storage. These are SDK estimates, not allocator requests, peak live memory, immutable pattern-program size or a process-memory bound. */
  evaluationSdkReservationBytes: number;
  /** Reached schema-resource bound; null (default) sets no additional resource-count cap. Original-environment preparation still has its own footprint, work and allocation bounds. */
  schemaResources: number | null;
}

/** Inputs for one bounded preparation with the built-in evaluator. Resources are explicit immutable inputs and are never fetched. The evaluator interprets the selected original schema profile, including supported external dialects and custom vocabularies. Missing content, local undefined effects and unsupported capabilities remain distinct: evaluation may establish a result through an independent branch, prove undefined dependence, or return a typed no-verdict. Evaluated annotations and bounded recursive reasoning participate in that decision. Pre-aborted work returns a cancelled preparation failure. */
export interface PrepareOptions extends WorkOptions {
  /** Explicit resources borrowed for this preparation; omitted means none. No URI is fetched and the passed owner remains caller-owned. */
  resources?: ExternalSchemas;
  /** Overrides for built-in evaluator budgets; unspecified entries use evaluatorLimits(). */
  limits?: Partial<EvaluatorLimits>;
}

/** Explicit resources and cooperative cancellation for reference inspection; no retrieval or global resource registry is used. */
export interface ReferenceOptions extends WorkOptions {
  /** Explicit resource set borrowed for inspection; no owner is transferred and no URI is fetched. */
  resources?: ExternalSchemas;
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
        "Evaluator limits must be known budgets, each a nonnegative integer no greater than 4294967295; evaluationSdkWorkUnits and schemaResources may also be null for their defaults.",
      );
    encoded[key.replace(/[A-Z]/g, (c) => "_" + c.toLowerCase())] = value;
  }
  return JSON.stringify(encoded);
}

/** @internal */
export const resourceRaw = (owner: ExternalSchemas) =>
  handle<wasm.WasmResources>(owner);

function schemaRaw(owner: Schema): wasm.WasmSchema {
  const raw = handle<wasm.WasmSchema>(owner);
  if (!(raw instanceof wasm.WasmSchema))
    throw new SdkError(
      "document-conformance",
      "A schema retained from a conformant document is required.",
    );
  return raw;
}

// Exact JSON text for a schema location; JSON.stringify escapes an unpaired
// UTF-16 unit, so the engine reads the pointer exactly.
/** @internal */
export function schemaLocationText(at: SchemaOrigin): string {
  return JSON.stringify({ source_uri: at?.sourceUri, pointer: at?.pointer });
}

const resourceToken = Symbol("retained resource set");

/** Immutable explicit resource set. URIs must be absolute without a nonempty fragment; normalized duplicate identities are refused even for equal bytes. Resources are retained, never fetched. Batch failure releases partial state and leaves supplied owners usable. Distinct contexts may associate the same URI with different snapshots. */
export class ExternalSchemas extends Managed {
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

  private static fromRaw(raw: wasm.WasmResources): ExternalSchemas {
    return new ExternalSchemas(raw, resourceToken);
  }
  /** Return a NEW independently disposable resource-set owner with one added resource; the original and supplied document stay usable. Invalid/duplicate identity throws; no acquisition. */
  with(uri: string, document: ExactJson): ExternalSchemas {
    scalar(uri);
    return ExternalSchemas.fromRaw(
      sdkCall(() =>
        handle<wasm.WasmResources>(this).add(uri, jsonRaw(document)),
      ),
    );
  }
  /** Return a NEW independently disposable owner of this immutable set. */
  retain(): ExternalSchemas {
    return ExternalSchemas.fromRaw(handle<wasm.WasmResources>(this).retain());
  }
  /** Locate a schema location in one of these resources: the node its pointer names in the resource with its URI, with line and column in that resource's own text. A resource built from a subtree of a larger value is admitted as its own text, so its coordinates count from the subtree's start, where ExactJson.locate on that subtree counts in the whole text it was parsed from. null for a document location, a URI this set does not hold (such as a packaged meta-schema's), or a pointer that names no node. Positions are computed on this call. A malformed location throws SdkError with code invalid-schema-location. */
  locate(at: SchemaOrigin): SourceLocation | null {
    const raw = handle<wasm.WasmResources>(this);
    return decode(sdkCall(() => raw.locate(schemaLocationText(at))));
  }
}

/** An original contextual schema, not yet prepared. Obtain it from Operation.input(), output(), or ConformantDocument.schema(); it retains its proved revision and survives operation and document disposal. Each prepare() returns an independent prepared owner; retain the PreparedSchema for repeated evaluation. */
export class Schema extends Managed {
  private constructor(raw: wasm.WasmSchema) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmSchema): Schema {
    return new Schema(raw);
  }
  /** Acquire a NEW independently disposable owner for the exact schema, without preparing it. */
  json(): ExactJson {
    return new ExactJson(schemaRaw(this).json());
  }
  /** Prepare with the built-in evaluator. Ready transfers a disposable PreparedSchema independent of this owner and preserves its original reference environment; it does not promise every value is decidable. Missing external content or a locally undefined predicate is not a blanket refusal: each evaluation accounts for applicator dependence and evaluated annotations. Unsupported reasoning, exhausted limits and cancellation remain explicit failures or no-verdict outcomes. Supplying a different resource snapshot requires a new preparation; existing prepared owners keep their original meaning. Cancelled/transient failures do not poison retries. Pre-aborted work returns a cancelled preparation failure. */
  prepare(options: PrepareOptions = {}): SchemaPreparation {
    requireReady();
    const raw = schemaRaw(this);
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
        Exclude<SchemaPreparation, { status: "ready" }> | { status: "ready" }
      >(preparation.result());
      if (result.status !== "ready") {
        if (result.status !== "refused")
          throw new Error("Invalid preparation result.");
        return result;
      }
      const prepared = preparation.takeSchema();
      if (!prepared)
        throw new Error("Preparation invariant: ready schema missing.");
      return {
        status: "ready",
        schema: adopt(prepared, PreparedSchema.fromRaw),
      };
    } finally {
      preparation.free();
    }
  }
  /** Return a NEW independently disposable owner of this original schema. */
  retain(): Schema {
    return Schema.fromRaw(schemaRaw(this).retain());
  }
}

function prepareOptions(options: PrepareOptions): {
  resources?: ExternalSchemas;
  limits?: Partial<EvaluatorLimits>;
  signal?: AbortSignal;
} {
  if (!options || typeof options !== "object" || Array.isArray(options))
    throw new SdkError(
      "invalid-work-options",
      "Preparation options must be an object.",
    );
  const { resources, limits, ...work } = options;
  return { resources, limits, signal: workSignal(work) };
}

/** @internal */
export function referenceOptions(options: ReferenceOptions): {
  resources?: ExternalSchemas;
  aborted: boolean;
} {
  if (!options || typeof options !== "object" || Array.isArray(options))
    throw new SdkError(
      "invalid-work-options",
      "Reference options must be an object.",
    );
  const { resources, ...work } = options;
  const signal = workSignal(work);
  if (resources !== undefined && !(resources instanceof ExternalSchemas))
    throw new SdkError(
      "invalid-work-options",
      "resources must be an ExternalSchemas owner.",
    );
  return { resources, aborted: isAborted(signal) };
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
    status: "no-verdict",
    detail: {
      reason: "cancelled",
      code: "caller-cancelled",
      message: "the caller cancelled this operation",
      location: null,
    },
  };
}

/** Disposable prepared schema that survives disposal of its document, external-schema, operation and original-schema handles. Its evaluator may retain immutable original sources. Repeated evaluation borrows exact input; ordinary inputs are admitted and temporary owners released within the call. No verdict here proves whole-document conformance. */
export class PreparedSchema extends Managed {
  private constructor(raw: wasm.WasmPrepared) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmPrepared): PreparedSchema {
    return new PreparedSchema(raw);
  }
  // Overload order matters: an untyped (any) argument takes the first
  // signature, so it must be one whose result keeps input-error.
  /** Admit ordinary JSON (including nested exact owners), then check it cheaply.
   * A string is a JSON string value, so evaluate("7") checks the string "7"; use evaluateJson for JSON text.
   * Expected admission failures return input-error. Caller exceptions and handle misuse throw.
   */
  evaluate(
    value: Exclude<JsonInput, ExactJson>,
    options?: WorkOptions,
  ): ValueCheck;
  /** The cheap check of an exact root: a failure carries no problems and problemsComplete is false. Cancellation cannot preempt synchronous Wasm. */
  evaluate(value: ExactJson, options?: WorkOptions): ValueOutcome;
  /** Check a value that may be ordinary or exact: an ordinary one is admitted first, so input-error remains possible. */
  evaluate(value: JsonInput, options?: WorkOptions): ValueCheck;
  evaluate(value: JsonInput, options: WorkOptions = {}): ValueCheck {
    return checkValue(this, value, options, false);
  }
  /** Admit exact JSON text or UTF-8 bytes as parseJson does, under the default limits (32 MiB, 10,000 container levels and 1,000,000 nodes), then check it cheaply: evaluateJson("7") checks the number 7. Bytes past the byte limit, or text with more UTF-16 units than it, cross into Wasm only as the prefix that establishes the refusal. Every value outcome is returned, including undefined and no-verdict; input-error is admission failure only. Duplicate members yield no-verdict and numbers are never rounded. Wrong input types and literal unpaired UTF-16 text throw, as for parseJson; exact admission precedes cancellation observation. Parse once with parseJson to check one value repeatedly; cancellation cannot preempt synchronous Wasm. */
  evaluateJson(input: ExactInput, options: WorkOptions = {}): JsonCheck {
    requireReady();
    const prepared = handle<wasm.WasmPrepared>(this);
    const signal = workSignal(options);
    const data = exact(input);
    try {
      return decode(
        typeof data === "string"
          ? prepared.evaluateJsonText(data, isAborted(signal))
          : prepared.evaluateJson(data, isAborted(signal)),
      );
    } catch (error) {
      return { status: "input-error", error: failure(error).error };
    }
  }
  // As for evaluate, the first signature is the one an untyped argument takes.
  /** Admit ordinary JSON (including nested exact owners), then explain it.
   * Expected admission failures return input-error. Caller exceptions and handle misuse throw.
   */
  explain(
    value: Exclude<JsonInput, ExactJson>,
    options?: WorkOptions,
  ): ValueCheck;
  /** Explain an exact root with the same verdict as evaluate() and available bounded problems and schema facts at original locations. Explanation can allocate more than the cheap check. Missing witnesses, exhausted diagnostic limits or cancellation after an established failure leave problemsComplete false without changing the verdict. */
  explain(value: ExactJson, options?: WorkOptions): ValueOutcome;
  /** Explain a value that may be ordinary or exact: an ordinary one is admitted first, so input-error remains possible. */
  explain(value: JsonInput, options?: WorkOptions): ValueCheck;
  explain(value: JsonInput, options: WorkOptions = {}): ValueCheck {
    return checkValue(this, value, options, true);
  }
  /** Return a NEW independently disposable owner sharing prepared state; it remains valid after the prior prepared owner is disposed. */
  retain(): PreparedSchema {
    return PreparedSchema.fromRaw(handle<wasm.WasmPrepared>(this).retain());
  }
}

function checkValue(
  schema: PreparedSchema,
  value: JsonInput,
  options: WorkOptions,
  explain: boolean,
): ValueCheck {
  requireReady();
  const prepared = handle<wasm.WasmPrepared>(schema);
  const exact = isExact(value) ? jsonRaw(value) : undefined;
  const signal = workSignal(options);
  if (isAborted(signal)) return cancelledValue();
  const run = (raw: wasm.WasmJson, cancelled: boolean): ValueOutcome =>
    decode(
      explain
        ? prepared.explain(raw, cancelled)
        : prepared.evaluate(raw, cancelled),
    );
  if (exact) return run(exact, isAborted(signal));
  let temporary: ExactJson | undefined;
  try {
    let text: string;
    try {
      text = encodeOrdinary(value, {});
    } catch (error) {
      if (!(error instanceof JsonEncodingError)) throw error;
      return {
        status: "input-error",
        error: {
          code: (error.code === "encoding-limit"
            ? "input-limit"
            : error.code) as ValueAdmissionFailure["code"],
          instancePointer: error.inputPointer,
          message: error.message,
        },
      };
    }
    try {
      temporary = new ExactJson(wasm.WasmJson.parseText(text));
    } catch (error) {
      // Only known JSON admission limits can refuse text produced by the checked encoder.
      if (inputErrorKind(error) !== "Limit") throw error;
      return {
        status: "input-error",
        error: {
          code: "input-limit",
          instancePointer: null,
          message: ORDINARY_LIMITS,
        },
      };
    }
    if (isAborted(signal)) return cancelledValue();
    return run(jsonRaw(temporary), false);
  } finally {
    temporary?.dispose();
  }
}
