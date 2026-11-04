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
  /** Original source retrieval URI, or null for the OpenBindings document; distinct from a nested resource’s $id. */
  sourceUri: string | null;
  /** Exact RFC 6901 pointer from that source's root; empty string denotes root. */
  pointer: string;
}

/** Known refusal causes for this facade and its bundled engine. Unsupported capability, conservative preparation, unavailable resources, limits, cancellation and evaluator failure do not establish satisfaction/failure. A potential cycle alone is conservative-preparation. Later package versions may add causes. A value contract that reaches a resource that was not supplied can prepare ready; values whose evaluation reaches the missing content return resource-unavailable during validation. For unavailable resources, Document.references() exposes document-wide reference spellings and original keyword locations, not an exact per-value-contract missing-resource list; apply an application disclosure policy before logging them. */
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

/** Established undefined result under the value-contract rules, separate from an evaluator's inability to answer. */
export interface UndefinedResult {
  /** Stable evaluator-defined detail code. */
  code: string;
  /** Explanatory text; custom evaluators should avoid echoing source or instance data. */
  message: string;
  /** Original schema location when known, otherwise null. */
  location: SchemaOrigin | null;
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
  /** The failing keyword in its original schema source, from that document's or resource's root: ConformantDocument.locate() or ExternalSchemas.locate() gives its coordinates. When the evaluator cannot map a keyword to the original source, the selected schema itself rather than a guessed keyword. */
  schema: SchemaOrigin;
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
      /** Whether selected failure diagnostics, including requested details, completed; false does not weaken the established failure. */
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
}

/** One reference keyword in a supported schema position. Opaque content is not interpreted as schema; located is static resolution, and dynamicLookup still requires evaluation-time scope handling. */
export interface Reference {
  /** Original reference-keyword location. */
  location: SchemaOrigin;
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

/** Original-context reference inspection, separate from normative proof and preparation. complete describes traversal completion; a complete report can still contain unresolved reference results. */
export interface ReferenceReport {
  /** Retained reference observations in traversal order. */
  references: readonly Reference[];
  /** Whether traversal completed; unresolved entries can occur even when true. */
  complete: boolean;
  /** Traversal-level limitation when incomplete, otherwise null. Inspection skips the references of a schema nested more than 256 schemas below the schema position that holds it, with a limit-exceeded limitation (`schema-depth`). */
  limitation: ReferenceRefusal | null;
}

/** Cooperative work control. A pre-aborted signal returns cancellation. Synchronous Wasm does not yield to same-thread JavaScript, so use a worker plus host scheduling to interrupt long synchronous work externally. This is not a wall-clock deadline. */
export interface WorkOptions {
  /** Optional caller-owned cooperative cancellation signal; synchronous Wasm only observes same-thread cancellation at call boundaries. */
  signal?: AbortSignal;
}

/** Default-evaluator limits, each a nonnegative 32-bit integer; `evaluationSteps` and `schemaResources` may also be `null`, their defaults. A limit that protects the process (its stack or memory) has a default; one that would only bound time is off unless set. Exceeding a limit produces no-verdict, except that exhausted post-verdict diagnostics leave an established failure without problems. */
export interface EvaluatorLimits {
  /** Work units per verdict/diagnostic pass. `null`, the default, sets no cap, so a pass takes time that grows with the schema (exponentially for some, such as an `anyOf` naming the next level twice) and the value; an application evaluating untrusted value contracts or values within a deadline sets one. A number charges a unit for each schema a reference enters and each element, member and literal comparison step, and yields `evaluation-work-limit` once a pass spends it. A cap places a work marker before every reference, so each reference a value's evaluation follows nests one more schema evaluation: within the same `stackBytes` a recursive value contract decides fewer levels, at worst about half as many (an array nested through one subschema reference per level decides 203 levels at the default stack instead of 340). */
  evaluationSteps: number | null;
  /** Bytes of stack preparation and evaluation may use; default 262,144 (256 KiB). The evaluator sizes three bounds from it: a value is evaluated only if the schema evaluations it could nest fit 256 bytes each (1,024 at the default; with `evaluationSteps` set, each reference followed nests one more), otherwise it yields the `evaluation-depth-limit` no-verdict without being evaluated, and an explanation is built only within half that; a value contract holding `unevaluatedProperties` or `unevaluatedItems` is also guarded as it evaluates, and a pass that uses more than half the stack (128 KiB at the default) stops with the same no-verdict, a bound on the stack's bytes, so the depth it admits varies with the build and the host; and preparation refuses an unevaluated keyword's annotation walk weighing more than the stack admits with `unevaluated-walk-limit` (a fifth left to the caller, 99 KiB to the rest of preparation, 1 KiB per unit: 105 at the default). Each refusal states its arithmetic. In Wasm the guard measures the module's own 1 MiB stack, while the JavaScript engine's call stack bounds how deep the module's calls go: Node 22 checked and explained the deepest admitted values of the shapes probed up to 524,288 (512 KiB), and at 1 MiB a guarded value contract's deepest admitted value overflowed the engine's stack. */
  stackBytes: number;
  /** Projected JSON nesting admitted to compilation; default 512. */
  compileJsonDepth: number;
  /** Maximum UTF-8 bytes per schema regex, and in the form the linear-time engine compiles it from; default 1,048,576 (1 MiB). A longer form yields the `pattern-size-limit` no-verdict. */
  patternBytes: number;
  /** Maximum regex group nesting; default 32. */
  patternDepth: number;
  /** Maximum UTF-8 bytes in the forms the evaluator keeps for one value contract's schema regexes together: each distinct form of its `pattern` values and the form of each `patternProperties` name; default 4,194,304 (4 MiB). A form spells every set out, so it can be far longer than its regex (`\S` takes 191 bytes). Preparation counts each form as it is written and yields the `pattern-total-byte-limit` no-verdict at the first that passes the limit. */
  totalPatternBytes: number;
  /** Bytes of memory one value contract's schema regexes may use together, in the SDK's run matchers and the linear-time engine; default 67,108,864 (64 MiB). A distinct `pattern` form that is one anchored sequence of single-character runs, every run but the last with a fixed count, is matched by a run matcher, which holds 104 bytes, 24 for each step (adjacent literal characters share one, as adjacent runs of one set do), 16 for each character of a short fixed part of ASCII held as a table instead, 8 for each range of a run's members past ASCII and a byte for each byte of literal text. Each other distinct `pattern` form and each `patternProperties` name is compiled through the regex crate's two size settings. A quarter of the memory, at most 2 MiB each, is their search caches in equal parts, which bound only how fast a search runs. Each form first compiles within 48 KiB, and the run matchers and these first allowances share the other three quarters: at the default those are 48 MiB, which hold 1,024 first allowances, and each 48 KiB or part of it that the run matchers hold takes the place of one. Preparation reads the patterns in order and yields the `pattern-total-memory-limit` no-verdict at the first pattern past what the three quarters hold, before any is compiled, so run matchers read early can bring the refusal to an earlier compiled pattern; when the compiled patterns read by then pass the count on their own, the message states the count, and otherwise the run matchers' bytes too. The forms that need more share what is left equally, those that need less than their part keep only what they need and leave the rest to the others, at most 32 MiB each; so small patterns elsewhere change a large one's outcome only through what they hold, 48 KiB for each compiled pattern and its bytes for each run matcher, and no outcome depends on the order strings reach the patterns. A `patternProperties` name that does not fit what it is given refuses preparation, and a `pattern` value, compiled the first time a string reaches it, gives that string no verdict: `pattern-total-memory-limit`, whose message states the arithmetic, or `pattern-size-limit` when the form needs more than 32 MiB. */
  totalPatternMemory: number;
  /** Bytes the SDK's program for one value contract may grow by beyond its projected schemas; default 50,331,648 (48 MiB). Dynamic specialization copies a schema reached through `$dynamicRef` once for each dynamic context it is reached in; preparation estimates the memory each copy and each context takes as the SDK holds it, and refuses a value contract whose copies and contexts would pass the projected schemas' own estimate by more than this with the `program-specialization-limit` no-verdict, whose message states the arithmetic. It counts the SDK's own program, not the library's compiled program. */
  programBytes: number;
  /** Schema resources one value contract may reach: the document resource holding its schema and each embedded `$id`, supplied and packaged meta-schema resource it reaches. Ordinary value contracts reach one or a few. `null`, the default, sets no cap. A number refuses a value contract reaching more with the `schema-resource-limit` no-verdict before compiling it, so an application preparing untrusted documents or resources can bound preparation at the size it chooses. The refusal comes after the value contract is projected and before the library compiles it, so it bounds the time and memory of that compilation. */
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
        "Evaluator limits must be known budgets, each a nonnegative integer no greater than 4294967295; evaluationSteps and schemaResources may also be null for their defaults.",
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
  /** Return a NEW independently disposable owner of this unprepared value contract. */
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
  if (Object.prototype.hasOwnProperty.call(work, "diagnostics"))
    throw new SdkError(
      "invalid-work-options",
      "The diagnostics option was removed: call explain(value) on the prepared value contract to learn why a value fails.",
    );
  return { resources, limits, signal: workSignal(work) };
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

/** Disposable prepared value contract, independent of its document, resources, operation and contract owners. Repeated validation borrows exact input; ordinary inputs are admitted and temporary owners released within the call. No verdict here proves whole-document conformance. */
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
  /** Admit exact JSON text or UTF-8 bytes as parseJson does, under the default limits, then check it cheaply: evaluateJson("7") checks the number 7. Every value outcome is returned, including undefined and no-verdict; input-error is admission failure only. Duplicate members yield no-verdict and numbers are never rounded. Wrong input types and literal unpaired UTF-16 text throw, as for parseJson; exact admission precedes cancellation observation. Parse once with parseJson to check one value repeatedly; cancellation cannot preempt synchronous Wasm. */
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
  /** Explain an exact root: the verdict evaluate() returns, and for a failure the library's first error with redacted messages and original schema facts. Explanation costs more than the cheap check: it allocates in proportion to the schema and value, so keep evaluate() on paths that check untrusted input. It is built only for values within half the nesting `stackBytes` admits, and a deeper failure is reported without its problem; a `stackBytes` larger than the JavaScript engine's call stack holds can still overflow that stack (see `stackBytes`). Cancellation after the verdict keeps an established failure with problemsComplete false. */
  explain(value: ExactJson, options?: WorkOptions): ValueOutcome;
  /** Explain a value that may be ordinary or exact: an ordinary one is admitted first, so input-error remains possible. */
  explain(value: JsonInput, options?: WorkOptions): ValueCheck;
  explain(value: JsonInput, options: WorkOptions = {}): ValueCheck {
    return checkValue(this, value, options, true);
  }
  /** Return a NEW independently disposable owner sharing compiled state; it remains valid after the prior prepared owner is disposed. */
  retain(): PreparedSchema {
    return PreparedSchema.fromRaw(handle<wasm.WasmPrepared>(this).retain());
  }
}

function checkValue(
  contract: PreparedSchema,
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
