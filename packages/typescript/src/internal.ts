/** Portable OpenBindings APIs. Call initialize once before constructing values. */
import init, * as wasm from "./wasm/openbindings_wasm.js";
export type WasmInput =
  | BufferSource
  | WebAssembly.Module
  | Response
  | Request
  | URL
  | string;
let initialization: Promise<void> | undefined;
let ready = false;
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
export type ExactInput = string | Uint8Array;
export interface InputFailure {
  status: "input-error";
  error: { code: string; kind?: string; byteOffset?: number; message: string };
}
export type ParseResult<T> = { status: "parsed"; value: T } | InputFailure;
export type Evidence =
  | "satisfied"
  | "violated"
  | "inconclusive"
  | "not-applicable";
export interface SourceLocation {
  pointer: string | null;
  byteOffset: number;
  line: number;
  byteColumn: number;
}
export interface Finding {
  rule: string;
  status: Evidence;
  code: string;
  location: SourceLocation | null;
  message: string;
}
export interface ConformanceReport {
  release: string;
  revision: string;
  policy: string;
  conclusion: "conformant" | "non-conformant" | "undetermined";
  evidence: Readonly<Record<string, Evidence>>;
  findings: readonly Finding[];
  findingsTruncated: boolean;
}
export type Assessment =
  | { status: "assessed"; report: ConformanceReport }
  | {
      status: "version-refused";
      refusal: { declared: string; supported: string };
    };
export type Side = "input" | "output";
export interface SchemaLocation {
  resource: string | null;
  pointer: string;
}
export type NoVerdictReason =
  | "unsupported-capability"
  | "conservative-preparation"
  | "resource-unavailable"
  | "limit-exceeded"
  | "cancelled"
  | "evaluator-failure"
  | "undefined";
export interface NoVerdict {
  reason: NoVerdictReason;
  code: string;
  message: string;
  location: SchemaLocation | null;
}
export interface ValueProblem {
  instancePointer: string;
  schemaLocation: SchemaLocation | null;
  code: string;
  message: string;
}
export type ValueOutcome =
  | { outcome: "satisfies" }
  | {
      outcome: "mismatch";
      problems: readonly ValueProblem[];
      problemsComplete: boolean;
    }
  | { outcome: "no-verdict"; detail: NoVerdict };
export type OperationSelection =
  | { status: "found"; operation: OperationView }
  | { status: "missing" }
  | { status: "ambiguous"; candidates: readonly string[] };
export type ContractPreparation =
  | { status: "ready"; contract: PreparedContract }
  | { status: "no-contract" | "operation-missing" }
  | { status: "operation-ambiguous"; candidates: readonly string[] }
  | { status: "no-verdict"; detail: NoVerdict };
export interface ValueAdmissionFailure {
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
  readonly message: string;
}
export type ValueCheck =
  | ValueOutcome
  | { outcome: "input-error"; error: ValueAdmissionFailure };
export interface Reference {
  location: SchemaLocation;
  keyword: string;
  spelling: string | null;
  resolution:
    | { outcome: "located"; target: SchemaLocation; dynamicLookup: boolean }
    | { outcome: "unresolved"; detail: NoVerdict };
}
export interface ReferenceReport {
  references: readonly Reference[];
  complete: boolean;
  limitation: NoVerdict | null;
}
export interface WorkOptions {
  signal?: AbortSignal;
}
export interface EvaluatorLimits {
  evaluationSteps: number;
  evaluationDepth: number;
  regexSteps: number;
  maxProblems: number;
  compileJsonDepth: number;
  patternBytes: number;
  patternDepth: number;
}
export interface ContractOptions {
  resources?: SchemaResources;
  limits?: Partial<EvaluatorLimits>;
  /** Most-recently-used preparations retained by the context; default 4, zero disables caching. */
  cacheCapacity?: number;
}
export interface ReferenceOptions extends WorkOptions {
  resources?: SchemaResources;
}
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
export interface OperationMetadata {
  readonly key: string;
  readonly description: string | null;
  readonly aliases: readonly string[] | null;
  readonly hasInput: boolean;
  readonly hasOutput: boolean;
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
/** Structured SDK misuse/configuration/interpretation error. Semantic outcomes use unions. */
export class SdkError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly location?: Readonly<SourceLocation>,
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
  get disposed(): boolean {
    return !handles.has(this);
  }
  dispose(): void {
    const raw = handles.get(this);
    if (raw) {
      handles.delete(this);
      finalizer?.unregister(this);
      raw.free();
    }
  }
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
export type JsonPrimitive = null | boolean | number | string;
export type JsonInput =
  | JsonPrimitive
  | ExactJson
  | readonly JsonInput[]
  | { readonly [name: string]: JsonInput };
export type JsonOutput =
  | JsonPrimitive
  | JsonOutput[]
  | { [name: string]: JsonOutput };
/** Exact immutable JSON; token-preserving text/bytes remain available after conversion is declined. */
export class ExactJson extends Managed {
  /** @internal */ constructor(raw: wasm.WasmJson) {
    super(raw);
    exactOwners.add(this);
  }
  static from(value: JsonInput): ExactJson {
    requireReady();
    return new ExactJson(wasm.WasmJson.parseText(encodeOrdinary(value)));
  }
  retain(): ExactJson {
    return new ExactJson(handle<wasm.WasmJson>(this).retain());
  }
  get text(): string {
    return handle<wasm.WasmJson>(this).text();
  }
  get bytes(): Uint8Array {
    return handle<wasm.WasmJson>(this).bytes();
  }
  get metadata(): {
    kind: string;
    location: SourceLocation;
    duplicateNames: boolean;
  } {
    return decode(handle<wasm.WasmJson>(this).metadata());
  }
  at(pointer: string): ExactJson | undefined {
    scalar(pointer);
    const found = handle<wasm.WasmJson>(this).at(pointer);
    return found ? new ExactJson(found) : undefined;
  }
  get(name: string): ExactJson | undefined {
    scalar(name);
    const found = handle<wasm.WasmJson>(this).get(name);
    return found ? new ExactJson(found) : undefined;
  }
  equals(other: ExactJson): boolean | undefined {
    return handle<wasm.WasmJson>(this).equals(jsonRaw(other));
  }
  toValue():
    | { status: "converted"; value: JsonOutput }
    | { status: "inexact"; message: string } {
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
export function assessDocument(input: ExactInput): Assessment {
  requireReady();
  return decode(wasm.assessBytes(bytes(input)));
}
export function versionPolicy(): {
  authoringVersion: string;
  supportedVersions: string;
  appliedSpecRevision: string;
} {
  requireReady();
  return decode(wasm.versionPolicy());
}
export function checkVersion(
  version: string,
): "supported" | "malformed" | "unsupported" {
  requireReady();
  scalar(version);
  return wasm.checkVersion(version) as ReturnType<typeof checkVersion>;
}
export function liveStorageOwners(): number {
  requireReady();
  return wasm.liveStorageOwners();
}
export class ParsedDocument extends Managed {
  #operations?: readonly OperationMetadata[];
  /** @internal */ constructor(raw: wasm.WasmDocument) {
    super(raw);
  }
  retain(): ParsedDocument {
    return new ParsedDocument(handle<wasm.WasmDocument>(this).retain());
  }
  get originalBytes(): Uint8Array {
    return handle<wasm.WasmDocument>(this).originalBytes();
  }
  get value(): ExactJson {
    return new ExactJson(handle<wasm.WasmDocument>(this).value());
  }
  assess(): Assessment {
    return decode(handle<wasm.WasmDocument>(this).assess());
  }
  validate(): ValidationResult {
    return validateParsed(this);
  }
  get operations(): readonly OperationMetadata[] {
    handle<wasm.WasmDocument>(this);
    return (this.#operations ??= Object.freeze(
      decode<OperationMetadata[]>(
        sdkCall(() => handle<wasm.WasmDocument>(this).operations()),
      ).map((op) =>
        Object.freeze({
          ...op,
          aliases: op.aliases ? Object.freeze(op.aliases) : null,
        }),
      ),
    ));
  }
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

  dependencyAcceptsKind(dependency: string, kind: string): boolean | undefined {
    scalar(dependency);
    scalar(kind);
    return sdkCall(() =>
      handle<wasm.WasmDocument>(this).dependencyAcceptsKind(dependency, kind),
    );
  }
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
  contracts(options: ContractOptions = {}): ValueContracts {
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
          ),
        ),
      );
    } finally {
      empty?.dispose();
    }
  }
}
export type ValidationResult =
  | {
      status: "validated";
      document: ValidatedDocument;
      report: ConformanceReport;
    }
  | Assessment;
const conformanceToken = Symbol("established conformance");
let validateParsed: (document: ParsedDocument) => ValidationResult;
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
  override retain(): ValidatedDocument {
    return new ValidatedDocument(
      handle<wasm.WasmDocument>(this).retain(),
      conformanceToken,
    );
  }
}
export class OperationView extends Managed {
  /** @internal */ constructor(raw: wasm.WasmOperation) {
    super(raw);
  }
  get key(): string {
    return handle<wasm.WasmOperation>(this).key();
  }
  get value(): ExactJson {
    return new ExactJson(handle<wasm.WasmOperation>(this).value());
  }
  get bindings(): readonly string[] {
    return decode(sdkCall(() => handle<wasm.WasmOperation>(this).bindings()));
  }
}
const resourceToken = Symbol("retained resource set");
/** Immutable explicit resources. Batch failure releases partial state and preserves supplied owners.
 * Duplicate normalized URIs are refused, including equal-byte duplicates.
 */
export class SchemaResources extends Managed {
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
  /** Return a new immutable context; the original remains usable. */
  with(uri: string, document: ExactJson): SchemaResources {
    scalar(uri);
    return SchemaResources.fromRaw(
      sdkCall(() =>
        handle<wasm.WasmResources>(this).add(uri, jsonRaw(document)),
      ),
    );
  }
  retain(): SchemaResources {
    return SchemaResources.fromRaw(handle<wasm.WasmResources>(this).retain());
  }
}
export class ValueContracts extends Managed {
  /** @internal */ constructor(raw: wasm.WasmContracts) {
    super(raw);
  }
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
        if (info?.kind !== "limit") throw error;
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

  retain(): PreparedContract {
    return new PreparedContract(handle<wasm.WasmPrepared>(this).retain());
  }
}
export interface DiscoveryPolicy {
  version: string;
  revision: string;
  wellKnownPath: string;
  mediaType: string;
  accept: string;
  defaultMaxDocumentBytes: number;
}
export function discoveryPolicy(): DiscoveryPolicy {
  requireReady();
  return decode(wasm.discoveryPolicy());
}
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
export interface DiscoveryOptions {
  maxDocumentBytes?: number;
  signal?: AbortSignal;
  fetch?: DiscoveryFetch;
}
export interface DiscoveryMetadata {
  requestedUrl: string;
  finalUrl: string | null;
  status: number;
  responseType: ResponseType;
  headers: Readonly<Record<string, string>>;
}
type DiscoveryBodyResult =
  | { status: "found"; document: ValidatedDocument; report: ConformanceReport }
  | {
      status: "non-conformant" | "undetermined";
      document?: ParsedDocument;
      report: ConformanceReport;
    }
  | {
      status: "version-refused";
      refusal: { declared: string; supported: string };
    };
export type DiscoveryResult = (
  | DiscoveryBodyResult
  | { status: "absent" | "gated" | "http-status" }
  | { status: "body-limit"; limit: number }
  | { status: "cancelled"; reason: "aborted" | "timeout" }
  | {
      status: "transport-error" | "body-error";
      error: { kind: "network" | "timeout" | "other"; message: string };
    }
) & { metadata?: DiscoveryMetadata; body?: Uint8Array };
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
/** A copied immutable publication snapshot; no listener or server is created. */
export class DiscoveryPublication extends Managed {
  constructor(
    document: ValidatedDocument,
    options: { allowOrigin?: string } = {},
  ) {
    requireReady();
    const allow = options.allowOrigin ?? "";
    scalar(allow);
    super(
      sdkCall(() => new wasm.WasmPublication(documentRaw(document), allow)),
    );
  }
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

export interface OperationDraft {
  description?: string;
  deprecated?: boolean;
  tags?: readonly string[];
  aliases?: readonly string[];
  input?: JsonInput;
  output?: JsonInput;
  examples?: Readonly<Record<string, ExampleDraft>>;
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
export interface ExampleDraft {
  description?: string;
  input?: JsonInput;
  output?: JsonInput;
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
export interface SourceDraft {
  kind: string;
  content?: JsonInput;
  description?: string;
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
export interface BindingDraft {
  operation: string;
  source: string;
  content?: JsonInput;
  preference?: number;
  idempotent?: boolean;
  deprecated?: boolean;
  description?: string;
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
export interface DependencyDraft {
  operation: string;
  kinds?: readonly string[];
  description?: string;
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
export interface DocumentDraft {
  openbindings?: string;
  operations: Readonly<Record<string, OperationDraft>>;
  name?: string;
  version?: string;
  description?: string;
  schemas?: Readonly<Record<string, JsonInput>>;
  sources?: Readonly<Record<string, SourceDraft>>;
  bindings?: Readonly<Record<string, BindingDraft>>;
  dependencies?: Readonly<Record<string, DependencyDraft>>;
  additionalFields?: Readonly<Record<string, JsonInput>>;
}
/** Stable expected-invalid-draft categories; messages are explanatory, not identifiers. */
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
  | "invalid-draft";
export interface AuthoringFailure {
  code: AuthoringErrorCode;
  /** JSON Pointer into the caller's draft; empty means root, null means unavailable. Never a byte offset. */
  draftPointer: string | null;
  message: string;
}
export type AuthoringResult =
  | { status: "authored"; document: ParsedDocument }
  | { status: "authoring-error"; error: AuthoringFailure };
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
    if (info?.kind === "limit")
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
        if (typedFields[kind].includes(name) || name === "additionalFields")
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
        ? (
            {
              operations: "operation",
              sources: "source",
              bindings: "binding",
              dependencies: "dependency",
            } as const
          )[key as "operations"]
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
