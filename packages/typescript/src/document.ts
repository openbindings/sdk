/** Documents, conformance proofs, canonical entries and atomic edits. */
import * as wasm from "./wasm/openbindings_wasm.js";
import {
  EditError,
  SdkError,
  decode,
  failure,
  sdkCall,
} from "./diagnostics.js";
import type { ParseResult, SourceLocation } from "./diagnostics.js";
import {
  Managed,
  adopt,
  adoptMany,
  canonicalRaw,
  finalizer,
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
import {
  ExternalSchemas,
  Schema,
  resourceRaw,
  schemaLocationText,
} from "./schema.js";
import type {
  ReferenceOptions,
  ReferenceReport,
  SchemaOrigin,
} from "./schema.js";

/** Closed per-rule evidence partition: met establishes the rule; violated establishes a counterexample; inconclusive lacks proof; not-applicable records a failed prerequisite. Findings explain evidence but do not replace it. */
export type Evidence = "met" | "violated" | "inconclusive" | "not-applicable";

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

// Private scalar transfers for canonical owners. Exact data never passes through them.
interface OperationMetadata {
  key: string;
  description: string | null;
  aliases: readonly string[] | null;
  tags: readonly string[] | null;
  deprecated: boolean | null;
}

interface BindingMetadata {
  key: string;
  operation: string;
  source: string;
  description: string | null;
  preference: number | null;
  idempotent: boolean | null;
  deprecated: boolean | null;
}

interface SourceMetadata {
  key: string;
  kind: string;
  description: string | null;
}

interface DependencyMetadata {
  key: string;
  operation: string;
  description: string | null;
  kinds: readonly string[] | null;
}

interface ExampleMetadata {
  key: string;
  description: string | null;
}

function freezeMetadata<T extends object>(row: T): T {
  for (const value of Object.values(row))
    if (Array.isArray(value)) Object.freeze(value);
  return Object.freeze(row);
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

// A JavaScript receiver's class is never authority for a proof-requiring call.
function operationRaw(owner: Operation): wasm.WasmOperation {
  const raw = handle<wasm.WasmOperation>(owner);
  if (!(raw instanceof wasm.WasmOperation))
    throw new SdkError(
      "document-conformance",
      "A conformant operation is required.",
    );
  return raw;
}

/** Assess all normative document rules directly from exact source, including invalid JSON. Unsupported declared versions return version-refused; parse/limit failures become truthful evidence. Without `limits`, admits 32 MiB UTF-8 and 1,000,000 nodes at any nesting, as Rust's `validate_document` does; with them, as `validate_document_with_limits` does, unspecified entries keeping the JsonLimits defaults, including the 10,000-level nesting cap. A document past a limit leaves every rule inconclusive with input-limit findings naming the limit. Returns plain data with no handle to dispose. Malformed limits or options throw SdkError invalid-json-limits; initialization, wrong input types and literal unpaired UTF-16 text throw; represent unpaired units with JSON escapes or exact UTF-8 bytes. */
export function validateDocument(
  input: ExactInput,
  options: {
    /** Admission limits; unspecified entries keep the JsonLimits defaults. */
    limits?: Partial<JsonLimits>;
  } = {},
): DocumentReport {
  requireReady();
  const limits = limitsOption(options);
  const data = exact(input);
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

/** Exact snapshot operations shared by draft and conformant owners. */
abstract class DocumentSnapshot extends Managed {
  /** @internal */
  protected constructor(raw: wasm.WasmDocument | wasm.WasmConformantDocument) {
    super(raw);
  }
  /** Copy the original UTF-8 bytes, including formatting. */
  toBytes(): Uint8Array {
    return documentRaw(this).toBytes();
  }
  /** Retain an independent exact JSON owner. Dispose it separately. */
  value(): ExactJson {
    return new ExactJson(documentRaw(this).value());
  }
  /** The admission limits retained by this snapshot and applied to its edits. */
  get limits(): Readonly<JsonLimits> {
    const [maxBytes, maxNodes, maxDepth] = documentRaw(this).limits();
    return Object.freeze({
      maxBytes: maxBytes!,
      maxNodes: maxNodes!,
      maxDepth: maxDepth!,
    });
  }
  /** Inspect original schema references without fetching resources or claiming conformance. */
  references(options: ReferenceOptions = {}): ReferenceReport {
    const resources = options.resources,
      empty = resources ? undefined : new ExternalSchemas();
    try {
      return decode(
        sdkCall(() =>
          documentRaw(this).references(
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
  private constructor(raw: wasm.WasmDocument, token: symbol) {
    if (token !== documentToken)
      throw new TypeError("Use Document.parse() or Document.create().");
    super(raw);
  }
  static {
    adoptDocument = (raw) => new Document(raw, documentToken);
  }
  /** Admit exact JSON, including broken documents, without conformance proof, under admission limits: by default 32 MiB UTF-8, 1,000,000 nodes and 10,000 container levels, and `limits` overrides them, unspecified entries keeping those defaults, as Rust's `Document::parse_with_limits` does. The document keeps its limits (`limits`): its edits apply them to what they write, as Rust's do. Schema preparation keeps its own fixed bounds. Expected input refusal uses input-error; initialization and API misuse throw. */
  static parse(
    input: ExactInput,
    options: {
      /** Admission limits the document keeps; unspecified entries keep their defaults. */
      limits?: Partial<JsonLimits>;
    } = {},
  ): ParseResult<Document> {
    requireReady();
    const limits = limitsOption(options);
    const data = exact(input);
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
    return adoptDocument(wasm.WasmDocument.create());
  }
  /** Retain a separate owner sharing this immutable revision. */
  retain(): Document {
    return adoptDocument(handle<wasm.WasmDocument>(this).retain());
  }
  /** Assess all rules; the conformant branch owns a new ConformantDocument. The original owner keeps its revision. Other branches retain evidence or a version refusal. */
  validate(): DocumentValidation {
    return assessDocument(this);
  }
  #commit(call: (raw: wasm.WasmDocument) => void): Document {
    editableRaw(this);
    const next = this.retain();
    try {
      sdkCall(() => call(editableRaw(next)));
    } catch (error) {
      next.dispose();
      if (error instanceof SdkError && error.code !== "disposed-handle")
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
        limits: handle<wasm.WasmDocument>(this).limits(),
        document: true,
      }),
    );
    const values = exact.owners.map(jsonRaw);
    const envelope = new Uint32Array([
      Math.min(exact.envelopeBytes, 0xffffffff),
      Math.min(exact.envelopeNodes, 0xffffffff),
    ]);
    return this.#commit((raw) => {
      const result = raw.editEncoded(
        exact.chunks,
        new Uint32Array(exact.indices),
        values,
        envelope,
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
    // Only a pointer holding an unpaired unit needs the batch route, whose JSON
    // escapes keep it exact across the bridge.
    if (isExact(value) && wellFormed(pointer))
      return this.#commit((raw) => raw.set(pointer, jsonRaw(value)));
    else return this.edit([{ op: "set", pointer, value }]);
  }
  /** Remove an existing member/array element at an RFC 6901 pointer, which may hold an unpaired UTF-16 unit as a finding's can. Throws EditError on refusal; removing the root is not supported. */
  remove(pointer: string): Document {
    if (wellFormed(pointer)) return this.#commit((raw) => raw.remove(pointer));
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

let adoptConformant: (raw: wasm.WasmConformantDocument) => ConformantDocument;

let assessDocument: (document: DocumentSnapshot) => DocumentValidation;

/** Retained proof for one immutable snapshot whose normative OBI rules were established. Obtain via Document.validate(); preparation remains evaluator-dependent. It shares read-only inspection and deterministic disposal, without inheriting Document or its setters. */
export class ConformantDocument extends DocumentSnapshot {
  declare private readonly conformantDocumentBrand: void;
  private constructor(raw: wasm.WasmConformantDocument, token: symbol) {
    if (token !== conformanceToken)
      throw new TypeError("Use Document.validate() to establish conformance.");
    super(raw);
  }
  static {
    adoptConformant = (raw) => new ConformantDocument(raw, conformanceToken);
    assessDocument = (document: DocumentSnapshot): DocumentValidation => {
      const transfer = documentRaw(document).validate();
      try {
        const result = decode<DocumentReport>(transfer.report());
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
  /** Retain all operations in lexical key order. Dispose each independently. */
  operations(): readonly Operation[] {
    return adoptMany(
      sdkCall(() => conformantRaw(this).operations()),
      Operation.fromRaw,
    );
  }
  /** Select by exact primary key; aliases do not match here. */
  operation(key: string): Operation | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).operation(key));
    return raw ? adopt(raw, Operation.fromRaw) : undefined;
  }
  /** Select by primary key or alias in the proved shared namespace. */
  resolveOperation(name: string): Operation | undefined {
    scalar(name);
    const raw = sdkCall(() => conformantRaw(this).resolveOperation(name));
    return raw ? adopt(raw, Operation.fromRaw) : undefined;
  }
  /** Retain each declared binding; undefined means absent, an empty array means present-empty. */
  bindings(): readonly Binding[] | undefined {
    const rows = sdkCall(() => conformantRaw(this).bindings());
    return rows ? adoptMany(rows, Binding.fromRaw) : undefined;
  }
  /** Retain a binding selected by exact key; undefined when absent. */
  binding(key: string): Binding | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).binding(key));
    return raw ? adopt(raw, Binding.fromRaw) : undefined;
  }
  /** Retain each declared source; undefined means absent, an empty array means present-empty. */
  sources(): readonly Source[] | undefined {
    const rows = sdkCall(() => conformantRaw(this).sources());
    return rows ? adoptMany(rows, Source.fromRaw) : undefined;
  }
  /** Retain a source selected by exact key; undefined when absent. */
  source(key: string): Source | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).source(key));
    return raw ? adopt(raw, Source.fromRaw) : undefined;
  }
  /** Retain each declared dependency; undefined means absent, an empty array means present-empty. */
  dependencies(): readonly Dependency[] | undefined {
    const rows = sdkCall(() => conformantRaw(this).dependencies());
    return rows ? adoptMany(rows, Dependency.fromRaw) : undefined;
  }
  /** Retain a dependency selected by exact key; undefined when absent. */
  dependency(key: string): Dependency | undefined {
    scalar(key);
    const raw = sdkCall(() => conformantRaw(this).dependency(key));
    return raw ? adopt(raw, Dependency.fromRaw) : undefined;
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
  /** Locate a schema location in this document: the node its pointer names, with line and column in the document's text. null for a location in a supplied resource, or a pointer that names no node in this revision. Line and column are computed on this call from the original text. A malformed location throws SdkError with code invalid-schema-location. */
  locate(at: SchemaOrigin): SourceLocation | null {
    const raw = conformantRaw(this);
    return decode(sdkCall(() => raw.locate(schemaLocationText(at))));
  }
}

/** A canonical operation in a conformant immutable document. Child owners survive disposal of their parents. */
export class Operation extends Managed {
  #metadata?: OperationMetadata;
  private constructor(raw: wasm.WasmOperation) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmOperation): Operation {
    return new Operation(raw);
  }
  /** Retain an independent owner of this same immutable declaration. */
  retain(): Operation {
    return adopt(
      canonicalRaw(this, wasm.WasmOperation).retain(),
      Operation.fromRaw,
    );
  }
  /** Retain the same document proof as a separately disposable owner. */
  document(): ConformantDocument {
    return adopt(
      canonicalRaw(this, wasm.WasmOperation).document(),
      adoptConformant,
    );
  }
  /** Retain the original declaration, including extension fields and exact tokens. */
  value(): ExactJson {
    return new ExactJson(canonicalRaw(this, wasm.WasmOperation).value());
  }
  get #fields(): OperationMetadata {
    const raw = canonicalRaw(this, wasm.WasmOperation);
    return (this.#metadata ??= freezeMetadata(
      decode<OperationMetadata>(raw.metadata()),
    ));
  }
  /** Exact primary key in this declaration’s namespace. */
  get key(): string {
    return this.#fields.key;
  }
  /** Optional description; undefined preserves absence. */
  get description(): string | undefined {
    return this.#fields.description ?? undefined;
  }
  /** Optional declared deprecation flag; absence remains undefined, distinct from false. */
  get deprecated(): boolean | undefined {
    return this.#fields.deprecated ?? undefined;
  }
  /** Declared aliases in declaration order; frozen, with undefined for absence. */
  get aliases(): readonly string[] | undefined {
    return this.#fields.aliases ?? undefined;
  }
  /** Declared tags in declaration order; frozen, with undefined for absence. */
  get tags(): readonly string[] | undefined {
    return this.#fields.tags ?? undefined;
  }
  /** Retain the input schema in its original context; undefined when absent. Does not prepare or evaluate it. */
  input(): Schema | undefined {
    const raw = operationRaw(this).input();
    return raw ? adopt(raw, Schema.fromRaw) : undefined;
  }
  /** Retain the output schema in its original context; undefined when absent. Does not prepare or evaluate it. */
  output(): Schema | undefined {
    const raw = operationRaw(this).output();
    return raw ? adopt(raw, Schema.fromRaw) : undefined;
  }
  /** Retain bindings in lexical order, without ranking or selection. */
  bindings(): readonly Binding[] {
    return adoptMany(operationRaw(this).bindings(), Binding.fromRaw);
  }
  /** Retain examples in lexical key order; undefined means absent and an empty array means present-empty. Dispose each owner. */
  examples(): readonly Example[] | undefined {
    const rows = operationRaw(this).examples();
    return rows ? adoptMany(rows, Example.fromRaw) : undefined;
  }
  /** Retain an example by its operation-local key; undefined when absent. */
  example(key: string): Example | undefined {
    scalar(key);
    const raw = operationRaw(this).example(key);
    return raw ? adopt(raw, Example.fromRaw) : undefined;
  }
}

/** A canonical binding in a conformant immutable document. Child owners survive disposal of their parents. */
export class Binding extends Managed {
  #metadata?: BindingMetadata;
  private constructor(raw: wasm.WasmBinding) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmBinding): Binding {
    return new Binding(raw);
  }
  /** Retain an independent owner of this same immutable declaration. */
  retain(): Binding {
    return adopt(
      canonicalRaw(this, wasm.WasmBinding).retain(),
      Binding.fromRaw,
    );
  }
  /** Retain the same document proof as a separately disposable owner. */
  document(): ConformantDocument {
    return adopt(
      canonicalRaw(this, wasm.WasmBinding).document(),
      adoptConformant,
    );
  }
  /** Retain the original declaration, including extension fields and exact tokens. */
  value(): ExactJson {
    return new ExactJson(canonicalRaw(this, wasm.WasmBinding).value());
  }
  get #fields(): BindingMetadata {
    const raw = canonicalRaw(this, wasm.WasmBinding);
    return (this.#metadata ??= freezeMetadata(
      decode<BindingMetadata>(raw.metadata()),
    ));
  }
  /** Exact primary key in this declaration’s namespace. */
  get key(): string {
    return this.#fields.key;
  }
  /** Optional description; undefined preserves absence. */
  get description(): string | undefined {
    return this.#fields.description ?? undefined;
  }
  /** Retain exact opaque data. Undefined means absent; explicit null remains a value. */
  data(): ExactJson | undefined {
    const raw = canonicalRaw(this, wasm.WasmBinding).data();
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Declared primary operation key; never an alias. */
  get operationKey(): string {
    return this.#fields.operation;
  }
  /** Declared primary source key in the same document. */
  get sourceKey(): string {
    return this.#fields.source;
  }
  /** Optional interoperable integer preference. Reading it performs no ranking or binding selection. */
  get preference(): number | undefined {
    return this.#fields.preference ?? undefined;
  }
  /** Optional declared idempotence flag; undefined preserves absence, distinct from false. */
  get idempotent(): boolean | undefined {
    return this.#fields.idempotent ?? undefined;
  }
  /** Optional declared deprecation flag; absence remains undefined, distinct from false. */
  get deprecated(): boolean | undefined {
    return this.#fields.deprecated ?? undefined;
  }
  /** Retain the referenced operation from this same conformant snapshot. */
  operation(): Operation {
    return adopt(
      canonicalRaw(this, wasm.WasmBinding).operation(),
      Operation.fromRaw,
    );
  }
  /** Retain the referenced source from this same conformant snapshot. */
  source(): Source {
    return adopt(canonicalRaw(this, wasm.WasmBinding).source(), Source.fromRaw);
  }
}

/** A canonical source in a conformant immutable document. Child owners survive disposal of their parents. */
export class Source extends Managed {
  #metadata?: SourceMetadata;
  private constructor(raw: wasm.WasmSource) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmSource): Source {
    return new Source(raw);
  }
  /** Retain an independent owner of this same immutable declaration. */
  retain(): Source {
    return adopt(canonicalRaw(this, wasm.WasmSource).retain(), Source.fromRaw);
  }
  /** Retain the same document proof as a separately disposable owner. */
  document(): ConformantDocument {
    return adopt(
      canonicalRaw(this, wasm.WasmSource).document(),
      adoptConformant,
    );
  }
  /** Retain the original declaration, including extension fields and exact tokens. */
  value(): ExactJson {
    return new ExactJson(canonicalRaw(this, wasm.WasmSource).value());
  }
  get #fields(): SourceMetadata {
    const raw = canonicalRaw(this, wasm.WasmSource);
    return (this.#metadata ??= freezeMetadata(
      decode<SourceMetadata>(raw.metadata()),
    ));
  }
  /** Exact primary key in this declaration’s namespace. */
  get key(): string {
    return this.#fields.key;
  }
  /** Optional description; undefined preserves absence. */
  get description(): string | undefined {
    return this.#fields.description ?? undefined;
  }
  /** Retain exact opaque data. Undefined means absent; explicit null remains a value. */
  data(): ExactJson | undefined {
    const raw = canonicalRaw(this, wasm.WasmSource).data();
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Exact declared kind name, without URI normalization or kind interpretation. */
  get kind(): string {
    return this.#fields.kind;
  }
}

/** A canonical dependency in a conformant immutable document. Child owners survive disposal of their parents. */
export class Dependency extends Managed {
  #metadata?: DependencyMetadata;
  private constructor(raw: wasm.WasmDependency) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmDependency): Dependency {
    return new Dependency(raw);
  }
  /** Retain an independent owner of this same immutable declaration. */
  retain(): Dependency {
    return adopt(
      canonicalRaw(this, wasm.WasmDependency).retain(),
      Dependency.fromRaw,
    );
  }
  /** Retain the same document proof as a separately disposable owner. */
  document(): ConformantDocument {
    return adopt(
      canonicalRaw(this, wasm.WasmDependency).document(),
      adoptConformant,
    );
  }
  /** Retain the original declaration, including extension fields and exact tokens. */
  value(): ExactJson {
    return new ExactJson(canonicalRaw(this, wasm.WasmDependency).value());
  }
  get #fields(): DependencyMetadata {
    const raw = canonicalRaw(this, wasm.WasmDependency);
    return (this.#metadata ??= freezeMetadata(
      decode<DependencyMetadata>(raw.metadata()),
    ));
  }
  /** Exact primary key in this declaration’s namespace. */
  get key(): string {
    return this.#fields.key;
  }
  /** Optional description; undefined preserves absence. */
  get description(): string | undefined {
    return this.#fields.description ?? undefined;
  }
  /** Declared primary operation key; never an alias. */
  get operationKey(): string {
    return this.#fields.operation;
  }
  /** Optional frozen kind-name filter; absent accepts every kind. Present filters are nonempty. */
  get kinds(): readonly string[] | undefined {
    return this.#fields.kinds ?? undefined;
  }
  /** Retain the referenced operation from this same conformant snapshot. */
  operation(): Operation {
    return adopt(
      canonicalRaw(this, wasm.WasmDependency).operation(),
      Operation.fromRaw,
    );
  }
  /** Exact kind-name comparison. An absent filter accepts all names. */
  acceptsKind(kind: string): boolean {
    if (typeof kind !== "string")
      throw new TypeError("Expected a kind name string.");
    return sdkCall(() =>
      canonicalRaw(this, wasm.WasmDependency).acceptsKind(JSON.stringify(kind)),
    );
  }
}

/** A canonical example in a conformant immutable document. Child owners survive disposal of their parents. */
export class Example extends Managed {
  #metadata?: ExampleMetadata;
  private constructor(raw: wasm.WasmExample) {
    super(raw);
  }
  /** @internal */ static fromRaw(raw: wasm.WasmExample): Example {
    return new Example(raw);
  }
  /** Retain an independent owner of this same immutable declaration. */
  retain(): Example {
    return adopt(
      canonicalRaw(this, wasm.WasmExample).retain(),
      Example.fromRaw,
    );
  }
  /** Retain the same document proof as a separately disposable owner. */
  document(): ConformantDocument {
    return adopt(
      canonicalRaw(this, wasm.WasmExample).document(),
      adoptConformant,
    );
  }
  /** Retain the original declaration, including extension fields and exact tokens. */
  value(): ExactJson {
    return new ExactJson(canonicalRaw(this, wasm.WasmExample).value());
  }
  get #fields(): ExampleMetadata {
    const raw = canonicalRaw(this, wasm.WasmExample);
    return (this.#metadata ??= freezeMetadata(
      decode<ExampleMetadata>(raw.metadata()),
    ));
  }
  /** Exact example key, local to its declaring operation. */
  get key(): string {
    return this.#fields.key;
  }
  /** Optional description; undefined preserves absence. */
  get description(): string | undefined {
    return this.#fields.description ?? undefined;
  }
  /** Retain the exact example input, without validating it against the operation schema. Undefined means absent; null remains a value. */
  input(): ExactJson | undefined {
    const raw = canonicalRaw(this, wasm.WasmExample).input();
    return raw ? new ExactJson(raw) : undefined;
  }
  /** Retain the exact example output, without validating it against the operation schema. Undefined means absent; null remains a value. */
  output(): ExactJson | undefined {
    const raw = canonicalRaw(this, wasm.WasmExample).output();
    return raw ? new ExactJson(raw) : undefined;
  }
}
