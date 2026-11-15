/** Documents, conformance proofs, canonical entries and atomic edits. */
import * as wasm from "./wasm/openbindings_wasm.js";
import {
  EditError,
  SdkError,
  decode,
  failure,
  sdkCall,
  requireUsable,
  wasmCall,
} from "./diagnostics.js";
import type { ParseResult, SourceLocation } from "./diagnostics.js";
import {
  Managed,
  adopt,
  finalizer,
  freeRaw,
  handle,
  handles,
  registerHandle,
} from "./ownership.js";
import {
  ExactJson,
  encodeOrdinary,
  exact,
  isExact,
  jsonRaw,
  limitsOption,
  ownEntries,
  scalar,
  wellFormed,
} from "./json.js";
import type {
  EditEncoding,
  ExactInput,
  JsonInput,
  JsonLimits,
} from "./json.js";
import { requireReady } from "./runtime.js";
import { reportLimits } from "./report-limits.js";
import {
  ExternalSchemas,
  Schema,
  referenceOptions,
  resourceRaw,
  schemaLocationText,
} from "./schema.js";
import type {
  ReferenceOptions,
  ReferenceReport,
  SchemaOrigin,
  ValueProblemDetails,
} from "./schema.js";

/** Closed per-rule evidence partition: met establishes the rule; violated establishes a counterexample; inconclusive lacks proof; not-applicable records a failed prerequisite. Findings explain evidence but do not replace it. */
export type Evidence = "met" | "violated" | "inconclusive" | "not-applicable";

/** One explanatory normative-rule finding. Use rule/code/status for logic and render message/pointer as text. Retained coordinates refer to original source; bounded findings do not replace the report evidence map. */
export interface Finding {
  /** Normative rule identifier such as OBI-01. */
  rule: string;
  /** Evidence expressed by this finding; consult the report map for aggregate rule evidence. */
  status: Evidence;
  /** Stable detailed diagnostic identifier. Admission limits use byte-limit, depth-limit or node-limit. */
  code: string;
  /** Original source coordinate, null only when no position is available. Admission limits carry the parser offset with a null pointer. */
  location: SourceLocation | null;
  /** Human-readable explanation; render as text rather than HTML. */
  message: string;
  /** Failed OBI-02 keyword from the published fixed schema; failed unions name the applicator. */
  keyword?: string;
  /** Keyword location in the published fixed schema, identified by retrievalUri. */
  schema?: SchemaOrigin;
  /** Complete published-schema facts; never rejected document values. One required fact per missing member. Disallowed members use location alone; anyOf/oneOf, pattern and uniqueItems have no details. The pinned 0.2 schema's largest keyword/schema/details tuple is 201 serialized JSON bytes (202 in Rust JSON), guarded by an exhaustive schema test without a runtime fact-size cap. */
  details?: ValueProblemDetails;
}

/** Retention allowances for explanatory findings, independent of rule evidence and JSON admission. Each value is a nonnegative 32-bit integer. */
export interface ValidationLimits {
  /** Maximum retained findings; default 4096. Zero retains none. */
  maxFindings: number;
  /** Aggregate generated source-pointer bytes; default 8 MiB. UTF-8, or two bytes per UTF-16 unit through an unpaired name. This is not a total heap allowance. */
  maxPointerBytes: number;
}

/** Options for assessing every document rule, with independent finding retention. */
export interface ValidationOptions {
  /** Override report retention; omitted limits use the cached default assessment. Unspecified entries use validationLimits(). */
  limits?: Partial<ValidationLimits>;
}

// These budgets are fixed by the initialized module. Public defaults still
// return fresh frozen copies; custom assessments reuse only this metadata.
let defaultValidationLimits: Readonly<ValidationLimits> | undefined;

function validationOptions(
  options: ValidationOptions,
): ValidationLimits | undefined {
  if (!options || typeof options !== "object" || Array.isArray(options))
    throw new SdkError(
      "invalid-validation-options",
      "Validation options must be an object.",
    );
  let limits: unknown;
  for (const [key, value] of ownEntries(options)) {
    if (key !== "limits")
      throw new SdkError(
        "invalid-validation-options",
        "Unknown validation option.",
      );
    limits = value;
  }
  return limits === undefined
    ? undefined
    : reportLimits(
        (defaultValidationLimits ??= validationLimits()),
        limits,
        "invalid-validation-limits",
      );
}

/** Return a fresh frozen copy of default validation-report retention allowances. Requires initialization. */
export function validationLimits(): Readonly<ValidationLimits> {
  requireReady();
  const [maxFindings, maxPointerBytes] = wasmCall(() =>
    wasm.validationLimits(),
  );
  return Object.freeze({
    maxFindings: maxFindings!,
    maxPointerBytes: maxPointerBytes!,
  });
}

/** Normative OBI assessment at an exact specification revision. All rules retain independent evidence even if findings are omitted. By default presentation retains at most 4096 findings and 8 MiB aggregate generated-pointer bytes (UTF-8, or two per UTF-16 unit for a pointer through a name with an unpaired surrogate); the pinned 0.2 schema's largest keyword/schema/details tuple is 201 serialized JSON bytes (202 in Rust JSON), guarded by an exhaustive schema test without a runtime fact-size cap. Document.validate({ limits }) selects different retention allowances. findingsTruncated records omissions without changing the conclusion. */
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
  /** True when the selected finding-count or aggregate generated-pointer byte allowance omitted findings. Omission never changes evidence or substitutes a shortened pointer. */
  findingsTruncated: boolean;
}

/** Normative assessment or a distinct unsupported-version refusal, as plain data: the result of validateDocument(). An assessed report can be conformant, non-conformant or undetermined. It owns no handles; Document.validate() returns a DocumentValidation, which adds the retained proof. */
export type DocumentReport =
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

/** Detached, runtime-frozen metadata for a canonical operation. Contains only declared scalar fields and string arrays, no methods or document reference. Survives proof disposal and needs no disposal. A key used with another proof selects that proof's revision. */
export interface Operation {
  /** Exact primary key in this declaration's namespace. */
  readonly key: string;
  /** Optional description; undefined preserves absence. */
  readonly description?: string;
  /** Optional declared deprecation flag; absence remains undefined, distinct from false. */
  readonly deprecated?: boolean;
  /** Declared aliases in declaration order; frozen, with undefined for absence. */
  readonly aliases?: readonly string[];
  /** Declared tags in declaration order; frozen, with undefined for absence. */
  readonly tags?: readonly string[];
}

/** Detached, runtime-frozen metadata for a canonical binding. Contains only declared scalar fields and string arrays, no methods or document reference. Survives proof disposal and needs no disposal. A key used with another proof selects that proof's revision. */
export interface Binding {
  /** Exact primary key in this declaration's namespace. */
  readonly key: string;
  /** Optional description; undefined preserves absence. */
  readonly description?: string;
  /** Declared primary operation key; never an alias. */
  readonly operationKey: string;
  /** Declared primary source key in the same document. */
  readonly sourceKey: string;
  /** Optional interoperable integer preference; reading performs no ranking or binding selection. */
  readonly preference?: number;
  /** Optional declared idempotence flag; undefined preserves absence, distinct from false. */
  readonly idempotent?: boolean;
  /** Optional declared deprecation flag; undefined preserves absence, distinct from false. */
  readonly deprecated?: boolean;
}

/** Detached, runtime-frozen metadata for a canonical source. Contains only declared scalar fields and string arrays, no methods or document reference. Survives proof disposal and needs no disposal. A key used with another proof selects that proof's revision. */
export interface Source {
  /** Exact primary key in this declaration's namespace. */
  readonly key: string;
  /** Optional description; undefined preserves absence. */
  readonly description?: string;
  /** Exact declared kind name, without URI normalization or kind interpretation. */
  readonly kind: string;
}

/** Detached, runtime-frozen metadata for a canonical dependency. Contains only declared scalar fields and string arrays, no methods or document reference. Survives proof disposal and needs no disposal. A key used with another proof selects that proof's revision. */
export interface Dependency {
  /** Exact primary key in this declaration's namespace. */
  readonly key: string;
  /** Optional description; undefined preserves absence. */
  readonly description?: string;
  /** Declared primary operation key; never an alias. */
  readonly operationKey: string;
  /** Optional frozen kind-name filter; absent accepts every kind. Present filters are nonempty. */
  readonly kinds?: readonly string[];
}

/** Detached, runtime-frozen metadata for a canonical example. Contains only declared scalar fields and string arrays, no methods or document reference. Survives proof disposal and needs no disposal. A key used with another proof selects that proof's revision. */
export interface Example {
  /** Exact example key, local to its declaring operation. */
  readonly key: string;
  /** Optional description; undefined preserves absence. */
  readonly description?: string;
}

/** Compare a detached dependency's kind filter exactly. An absent filter accepts every string, including unpaired UTF-16; no live proof or initialization is required. */
export function acceptsKind(dependency: Dependency, kind: string): boolean {
  if (typeof kind !== "string")
    throw new TypeError("Expected a kind name string.");
  return dependency.kinds === undefined || dependency.kinds.includes(kind);
}

function freezeMetadata<T extends object>(row: T): T {
  for (const value of Object.values(row))
    if (Array.isArray(value)) Object.freeze(value);
  return Object.freeze(row);
}

function metadata<T extends object>(text: string): T | undefined {
  const row = decode<T | null>(text);
  return row === null ? undefined : freezeMetadata(row);
}

function metadataList<T extends object>(
  text: string,
): readonly T[] | undefined {
  const rows = decode<T[] | null>(text);
  if (rows === null) return undefined;
  for (const row of rows) freezeMetadata(row);
  return Object.freeze(rows);
}

const documentRaw = (owner: DocumentSnapshot) =>
  handle<wasm.WasmDocument | wasm.WasmConformantDocument>(owner);

function editableRaw(owner: DocumentSnapshot): wasm.WasmDocument {
  const raw = handle<wasm.WasmDocument>(owner);
  if (!(raw instanceof wasm.WasmDocument))
    throw new SdkError(
      "immutable-document",
      "A conformant document cannot be edited.",
    );
  return raw;
}

/** @internal */
export function conformantRaw(
  owner: ConformantDocument,
  code = "document-conformance",
): wasm.WasmConformantDocument {
  const raw = handle<wasm.WasmConformantDocument>(owner);
  if (!(raw instanceof wasm.WasmConformantDocument))
    throw new SdkError(code, "A conformant document is required.");
  return raw;
}

/** Assess all normative document rules directly from exact source, including invalid JSON. Unsupported declared versions return version-refused; parse/limit failures become truthful evidence. By default admits 32 MiB UTF-8, 1,000,000 nodes and 10,000 container levels, as Rust's `validate_document` does. Explicit limits override these as `validate_document_with_limits` does; unspecified entries keep the JsonLimits defaults. A document past a limit leaves every rule inconclusive with byte-limit, depth-limit or node-limit findings at the parser offset. To admit more, pass limits.maxBytes, limits.maxDepth or limits.maxNodes respectively; messages contain no API advice. Returns plain data with no handle to dispose. Malformed limits or options throw SdkError invalid-json-limits; initialization, wrong input types and literal unpaired UTF-16 text throw; represent unpaired units with JSON escapes or exact UTF-8 bytes. */
export function validateDocument(
  input: ExactInput,
  options: {
    /** Admission limits; unspecified entries keep the JsonLimits defaults. */
    limits?: Partial<JsonLimits>;
  } = {},
): DocumentReport {
  requireReady();
  const limits = limitsOption(options);
  const data = exact(input, limits);
  return decode(
    sdkCall(() =>
      typeof data === "string"
        ? wasm.validateText(data, limits)
        : wasm.validateBytes(data, limits),
    ),
  );
}

/** Return authoring default, supported specification line and exact applied specification revision. These identities are independent of the npm package version. */
export function versionPolicy(): {
  /** Default specification version emitted by Document.create(). */
  authoringVersion: string;
  /** Supported stable specification line. */
  supportedVersions: string;
  /** Exact applied specification Git revision. */
  appliedSpecRevision: string;
} {
  requireReady();
  return decode(wasmCall(() => wasm.versionPolicy()));
}

/** Classify strict SemVer: stable 0.2.x is supported, prereleases/other well-formed lines unsupported, invalid syntax malformed. Build metadata does not affect support and numeric components have no machine-integer ceiling. */
export function checkVersion(
  version: string,
): "supported" | "malformed" | "unsupported" {
  requireReady();
  scalar(version);
  return wasmCall(() => wasm.checkVersion(version)) as ReturnType<
    typeof checkVersion
  >;
}

/** Exact snapshot operations shared by draft and conformant owners. */
abstract class DocumentSnapshot extends Managed {
  /** @internal */
  protected constructor(raw: wasm.WasmDocument | wasm.WasmConformantDocument) {
    super(raw);
  }
  /** Copy the original UTF-8 bytes, including formatting. */
  toBytes(): Uint8Array {
    return wasmCall(() => documentRaw(this).toBytes());
  }
  /** Retain an independent exact JSON owner. Dispose it separately. */
  value(): ExactJson {
    return new ExactJson(wasmCall(() => documentRaw(this).value()));
  }
  /** The admission limits retained by this snapshot and applied to its edits. */
  get limits(): Readonly<JsonLimits> {
    const [maxBytes, maxNodes, maxDepth] = wasmCall(() =>
      documentRaw(this).limits(),
    );
    return Object.freeze({
      maxBytes: maxBytes!,
      maxNodes: maxNodes!,
      maxDepth: maxDepth!,
    });
  }
  /** Inspect original schema references without fetching resources or claiming conformance. Missing or malformed openbindings declarations use 0.2's rules without adding a limitation; well-formed unsupported versions and duplicate member names are refused. */
  references(options: ReferenceOptions = {}): ReferenceReport {
    requireUsable();
    const { resources, aborted, maxReportBytes } = referenceOptions(options),
      empty = resources ? undefined : new ExternalSchemas();
    try {
      return decode(
        sdkCall(() =>
          documentRaw(this).references(
            resourceRaw(resources ?? empty!),
            aborted,
            maxReportBytes,
          ),
        ),
      );
    } finally {
      empty?.dispose();
    }
  }
}

/** One exact JSON edit. Paths address the snapshot before the entire atomic batch. */
export type Edit =
  | {
      /** Replace or insert one value in the original revision. */ readonly op: "set";
      /** RFC 6901 pointer in the revision before this batch. */ readonly pointer: string;
      /** Borrowed exact value or checked ordinary JSON input. */ readonly value: JsonInput;
    }
  | {
      /** Remove one existing value from the original revision. */ readonly op: "remove";
      /** RFC 6901 pointer in the revision before this batch. */ readonly pointer: string;
    };

/** @internal */
export let adoptDocument: (raw: wasm.WasmDocument) => Document;

const documentToken = Symbol("document owner");

/** One exact, immutable OBI snapshot. Edits return a new snapshot; owners, views and proofs keep their original revision. */
export class Document extends DocumentSnapshot {
  /** Estimate heap bytes held by this snapshot's shared storage now: source text, parsed nodes and buffers, and built indexes, conformance reports and the schema resolution cached for preparation without supplied resources (a resolution built with supplied resources is held by that resource set and not counted). Clones and proofs share storage, so charge a snapshot once; a proof reaches it through asDocument(). Reads existing capacities without parsing, validating or building caches. Later namespace lookups can add entries and aliases, schema queries can add their inventory and identities, and location lookups can add two machine words per 4096 source bytes with geometrically reserved capacity capped at floor(sourceBytes / 4096) checkpoints (8 bytes each on Wasm). Call again to include growth. Tree/hash storage is estimated. Excludes separately owned external resources, validators, process-wide packaged schemas, allocator overhead and Wasm memory growth. */
  retainedBytes(): number {
    return wasmCall(() => handle<wasm.WasmDocument>(this).retainedBytes());
  }
  private constructor(raw: wasm.WasmDocument, token: symbol) {
    if (token !== documentToken)
      throw new TypeError("Use Document.parse() or Document.create().");
    super(raw);
  }
  static {
    adoptDocument = (raw) => new Document(raw, documentToken);
  }
  /** Admit exact JSON, including broken documents, without conformance proof, under admission limits: by default 32 MiB UTF-8, 1,000,000 nodes and 10,000 container levels, and `limits` overrides them, unspecified entries keeping those defaults, as Rust's `Document::parse_with_limits` does. The document keeps its limits (`limits`): its edits apply them to what they write, as Rust's do. Schema preparation keeps its own memory bounds and has no work cap by default. Expected input refusal uses input-error; initialization and API misuse throw. */
  static parse(
    input: ExactInput,
    options: {
      /** Admission limits the document keeps; unspecified entries keep their defaults. */
      limits?: Partial<JsonLimits>;
    } = {},
  ): ParseResult<Document> {
    requireReady();
    const limits = limitsOption(options);
    const data = exact(input, limits);
    try {
      return {
        status: "parsed",
        value: adoptDocument(
          typeof data === "string"
            ? wasm.WasmDocument.parseText(data, limits)
            : wasm.WasmDocument.parseBytes(data, limits),
        ),
      };
    } catch (error) {
      return failure(error);
    }
  }
  /** Create an empty OBI declaring the supported authoring version. Validate separately to obtain proof. */
  static create(): Document {
    requireReady();
    return adoptDocument(wasmCall(() => wasm.WasmDocument.create()));
  }
  /** Retain a separate owner sharing this immutable revision. */
  retain(): Document {
    return adoptDocument(
      wasmCall(() => handle<wasm.WasmDocument>(this).retain()),
    );
  }
  /** Assess all rules; the conformant branch owns a new ConformantDocument. The original owner keeps its revision. Other branches retain evidence or a version refusal. Omitted limits use the cached default assessment; selected retention never changes rule evidence or the conclusion. Custom reports do not replace the default cache. */
  validate(options?: ValidationOptions): DocumentValidation {
    requireUsable();
    return assessDocument(
      this,
      options === undefined ? undefined : validationOptions(options),
    );
  }
  #commit(call: (raw: wasm.WasmDocument) => void): Document {
    editableRaw(this);
    const next = this.retain();
    try {
      sdkCall(() => call(editableRaw(next)));
    } catch (error) {
      next.dispose();
      if (
        error instanceof SdkError &&
        error.code !== "disposed-handle" &&
        error.code !== "wasm-trap"
      )
        throw new EditError(error.code, error.message);
      throw error;
    }
    return next;
  }
  #encoded(value: readonly Edit[]): Document {
    handle(this);
    const exact: EditEncoding = {
      chunks: [],
      owners: [],
      indices: [],
      seen: new Map(),
      envelopeBytes: 0,
      envelopeNodes: 0,
    };
    // What the edit writes is bounded by the limits this document keeps.
    exact.chunks.push(
      encodeOrdinary(value, { editBatch: true }, exact, {
        limits: wasmCall(() => handle<wasm.WasmDocument>(this).limits()),
        document: true,
      }),
    );
    const values = exact.owners.map(jsonRaw);
    const envelope = new Uint32Array([
      Math.min(exact.envelopeBytes, 0xffffffff),
      Math.min(exact.envelopeNodes, 0xffffffff),
    ]);
    return this.#commit((raw) => {
      const result = wasmCall(() =>
        raw.editEncoded(
          exact.chunks,
          new Uint32Array(exact.indices),
          values,
          envelope,
        ),
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
  /** Atomically apply exact JSON edits in request order at shared insertion boundaries, with one bridge call including exact inputs. Untouched bytes stay exact. Invalid paths, duplicates, overlaps and admission limits throw EditError and leave the document unchanged. */
  edit(edits: readonly Edit[]): Document {
    handle(this);
    editableRaw(this);
    return this.#encoded(edits);
  }
  /** Replace or insert a value at an RFC 6901 pointer. A segment may hold an unpaired UTF-16 unit, so a finding's pointer addresses the member it reports. Exact owners are borrowed; ordinary values are checked without invoking getters/toJSON. */
  set(pointer: string, value: JsonInput): Document {
    requireUsable();
    // Only a pointer holding an unpaired unit needs the batch route, whose JSON
    // escapes keep it exact across the bridge.
    if (isExact(value) && wellFormed(pointer))
      return this.#commit((raw) =>
        wasmCall(() => raw.set(pointer, jsonRaw(value))),
      );
    else return this.edit([{ op: "set", pointer, value }]);
  }
  /** Remove an existing member/array element at an RFC 6901 pointer, which may hold an unpaired UTF-16 unit as a finding's can. Throws EditError on refusal; removing the root is not supported. */
  remove(pointer: string): Document {
    requireUsable();
    if (wellFormed(pointer))
      return this.#commit((raw) => wasmCall(() => raw.remove(pointer)));
    else return this.edit([{ op: "remove", pointer }]);
  }
}

/** Document validation, as Rust's DocumentValidation is: the conformant branch transfers an independent ConformantDocument owner, the proof, with its report. Otherwise a DocumentReport branch explains refusal, nonconformance or uncertainty; no proof owner is created. */
export type DocumentValidation =
  | {
      /** Result discriminant; narrow this before reading branch-specific fields. */
      status: "conformant";
      /** Caller-owned document handle for this branch; dispose independently. */
      document: ConformantDocument;
      /** Plain normative report for the exact snapshot. */
      report: ValidationReport;
    }
  | DocumentReport;

const conformanceToken = Symbol("established conformance");

let assessDocument: (
  document: DocumentSnapshot,
  limits?: ValidationLimits,
) => DocumentValidation;

/** Retained proof for one immutable snapshot whose normative OBI rules were established. Obtain via Document.validate(); preparation remains evaluator-dependent. It shares read-only inspection and deterministic disposal, without inheriting Document or its setters. */
export class ConformantDocument extends DocumentSnapshot {
  declare private readonly conformantDocumentBrand: void;
  private constructor(raw: wasm.WasmConformantDocument, token: symbol) {
    if (token !== conformanceToken)
      throw new TypeError("Use Document.validate() to establish conformance.");
    super(raw);
  }
  static {
    assessDocument = (
      document: DocumentSnapshot,
      limits?: ValidationLimits,
    ): DocumentValidation => {
      const raw = documentRaw(document);
      const transfer = limits
        ? wasmCall(() =>
            (raw as wasm.WasmDocument).validateWithLimits(
              limits.maxFindings,
              limits.maxPointerBytes,
            ),
          )
        : wasmCall(() => raw.validate());
      try {
        const result = decode<DocumentReport>(
          wasmCall(() => transfer.report()),
        );
        const proof = wasmCall(() => transfer.takeConformant());
        if (!proof) return result;
        if (result.status !== "report") {
          freeRaw(proof);
          throw new Error("Validation transfer invariant.");
        }
        return {
          status: "conformant",
          document: new ConformantDocument(proof, conformanceToken),
          report: result.report,
        };
      } finally {
        freeRaw(transfer);
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
    return adoptDocument(wasmCall(() => conformantRaw(this).asDocument()));
  }
  /** Consume this proof and return an independent editable owner. Every alias of this proof becomes disposed; independently retained owners survive. */
  intoDocument(): Document {
    const raw = sdkCall(() => conformantRaw(this).intoInner());
    handles.delete(this);
    finalizer?.unregister(this);
    return adoptDocument(raw);
  }
  /** The supported specification version, distinct from the interface version label. */
  get openbindings(): string {
    return decode(sdkCall(() => conformantRaw(this).openbindings()));
  }
  /** Optional document name; undefined preserves absence. */
  get name(): string | undefined {
    return (
      decode<string | null>(sdkCall(() => conformantRaw(this).name())) ??
      undefined
    );
  }
  /** Optional interface version label, independent of the specification version. */
  get version(): string | undefined {
    return (
      decode<string | null>(sdkCall(() => conformantRaw(this).version())) ??
      undefined
    );
  }
  /** Optional document description; undefined preserves absence. */
  get description(): string | undefined {
    return (
      decode<string | null>(sdkCall(() => conformantRaw(this).description())) ??
      undefined
    );
  }
  /** Read detached frozen operations in lexical primary-key order. */
  operations(): readonly Operation[] {
    return metadataList<Operation>(
      sdkCall(() => conformantRaw(this).operations()),
    )!;
  }
  /** Read detached frozen metadata by exact primary key; undefined when absent. Aliases do not match. */
  operation(key: string): Operation | undefined {
    scalar(key);
    return sdkCall(() => conformantRaw(this).operation(key, key));
  }
  /** Retain the exact operation declaration, including extension fields and exact tokens. Undefined for a missing primary key. Dispose the returned owner independently. */
  operationValue(key: string): ExactJson | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).operationValue(key));
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Read detached frozen bindings in lexical primary-key order. Undefined means absent; an empty array means present-empty. */
  bindings(): readonly Binding[] | undefined {
    return metadataList<Binding>(sdkCall(() => conformantRaw(this).bindings()));
  }
  /** Read detached frozen metadata by exact primary key; undefined when absent. Aliases do not match. */
  binding(key: string): Binding | undefined {
    scalar(key);
    return metadata<Binding>(sdkCall(() => conformantRaw(this).binding(key)));
  }
  /** Retain the exact binding declaration, including extension fields and exact tokens. Undefined for a missing primary key. Dispose the returned owner independently. */
  bindingValue(key: string): ExactJson | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).bindingValue(key));
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Read detached frozen sources in lexical primary-key order. Undefined means absent; an empty array means present-empty. */
  sources(): readonly Source[] | undefined {
    return metadataList<Source>(sdkCall(() => conformantRaw(this).sources()));
  }
  /** Read detached frozen metadata by exact primary key; undefined when absent. Aliases do not match. */
  source(key: string): Source | undefined {
    scalar(key);
    return metadata<Source>(sdkCall(() => conformantRaw(this).source(key)));
  }
  /** Retain the exact source declaration, including extension fields and exact tokens. Undefined for a missing primary key. Dispose the returned owner independently. */
  sourceValue(key: string): ExactJson | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).sourceValue(key));
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Read detached frozen dependencies in lexical primary-key order. Undefined means absent; an empty array means present-empty. */
  dependencies(): readonly Dependency[] | undefined {
    return metadataList<Dependency>(
      sdkCall(() => conformantRaw(this).dependencies()),
    );
  }
  /** Read detached frozen metadata by exact primary key; undefined when absent. Aliases do not match. */
  dependency(key: string): Dependency | undefined {
    scalar(key);
    return metadata<Dependency>(
      sdkCall(() => conformantRaw(this).dependency(key)),
    );
  }
  /** Retain the exact dependency declaration, including extension fields and exact tokens. Undefined for a missing primary key. Dispose the returned owner independently. */
  dependencyValue(key: string): ExactJson | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).dependencyValue(key));
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Read detached frozen operation metadata by primary key or alias in the proved shared namespace. */
  resolveOperation(name: string): Operation | undefined {
    scalar(name);
    return metadata<Operation>(
      sdkCall(() => conformantRaw(this).resolveOperation(name)),
    );
  }
  /** Read detached frozen bindings for a primary operation key, in lexical order without ranking. Undefined for a missing operation; an empty array when it has no bindings. */
  operationBindings(operationKey: string): readonly Binding[] | undefined {
    scalar(operationKey);
    return metadataList<Binding>(
      sdkCall(() => conformantRaw(this).operationBindings(operationKey)),
    );
  }
  /** Read detached frozen examples in lexical operation-local key order, without validating instances. Undefined for a missing operation or absent examples; an empty array means present-empty. */
  examples(operationKey: string): readonly Example[] | undefined {
    scalar(operationKey);
    return metadataList<Example>(
      sdkCall(() => conformantRaw(this).examples(operationKey)),
    );
  }
  /** Read detached frozen metadata by primary operation key and operation-local example key. Undefined when either is absent. */
  example(operationKey: string, exampleKey: string): Example | undefined {
    scalar(operationKey);
    scalar(exampleKey);
    return metadata<Example>(
      sdkCall(() => conformantRaw(this).example(operationKey, exampleKey)),
    );
  }
  /** Retain the exact example declaration by primary operation key and local example key, including extensions. Undefined when either entry is missing. Dispose the owner independently. */
  exampleValue(
    operationKey: string,
    exampleKey: string,
  ): ExactJson | undefined {
    scalar(operationKey);
    scalar(exampleKey);
    const raw = sdkCall(() =>
      conformantRaw(this).exampleValue(operationKey, exampleKey),
    );
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Retain the input schema in its original context by primary operation key, without preparation or evaluation. Undefined for a missing operation or absent input; false remains a schema. Dispose the owner independently. */
  inputSchema(operationKey: string): Schema | undefined {
    scalar(operationKey);
    const raw = sdkCall(() => conformantRaw(this).inputSchema(operationKey));
    return raw ? adopt(raw, Schema.fromRaw) : undefined;
  }
  /** Retain exact example input by primary operation key and local example key, without instance validation. Undefined for missing entries or absent input; JSON null remains an owner. Dispose it independently. */
  exampleInput(
    operationKey: string,
    exampleKey: string,
  ): ExactJson | undefined {
    scalar(operationKey);
    scalar(exampleKey);
    const raw = sdkCall(() =>
      conformantRaw(this).exampleInput(operationKey, exampleKey),
    );
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Retain the output schema in its original context by primary operation key, without preparation or evaluation. Undefined for a missing operation or absent output; false remains a schema. Dispose the owner independently. */
  outputSchema(operationKey: string): Schema | undefined {
    scalar(operationKey);
    const raw = sdkCall(() => conformantRaw(this).outputSchema(operationKey));
    return raw ? adopt(raw, Schema.fromRaw) : undefined;
  }
  /** Retain exact example output by primary operation key and local example key, without instance validation. Undefined for missing entries or absent output; JSON null remains an owner. Dispose it independently. */
  exampleOutput(
    operationKey: string,
    exampleKey: string,
  ): ExactJson | undefined {
    scalar(operationKey);
    scalar(exampleKey);
    const raw = sdkCall(() =>
      conformantRaw(this).exampleOutput(operationKey, exampleKey),
    );
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Retain exact opaque source data by primary key. Undefined for a missing entry or absent data; explicit JSON null remains an owner. Dispose it independently. */
  sourceData(key: string): ExactJson | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).sourceData(key));
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Retain exact opaque binding data by primary key. Undefined for a missing entry or absent data; explicit JSON null remains an owner. Dispose it independently. */
  bindingData(key: string): ExactJson | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).bindingData(key));
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Retain a named schema in its original context, without preparation or evaluation. */
  schema(key: string): Schema | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).schema(key));
    return raw ? adopt(raw, Schema.fromRaw) : undefined;
  }
  /** Retain named schemas in lexical order. Dispose each value independently. */
  schemas(): ReadonlyMap<string, Schema> | undefined {
    const names = decode<string[] | null>(
      sdkCall(() => conformantRaw(this).schemaNames()),
    );
    if (names === null) return undefined;
    const result = new Map<string, Schema>();
    try {
      for (const name of names) result.set(name, this.schema(name)!);
      return result;
    } catch (error) {
      for (const value of result.values()) value.dispose();
      throw error;
    }
  }
  /** Locate a schema location in this document: the node its pointer names, with line and column in the document's text. undefined for a location in a supplied resource, or a pointer that names no node in this revision. Line and column are computed on this call from the original text. A malformed location throws SdkError with code invalid-schema-location. */
  locate(at: SchemaOrigin): SourceLocation | undefined {
    conformantRaw(this);
    // Reading the location can run caller code, so the handle is taken after it.
    const location = schemaLocationText(at);
    return (
      decode<SourceLocation | null>(
        sdkCall(() => conformantRaw(this).locate(location)),
      ) ?? undefined
    );
  }
}
