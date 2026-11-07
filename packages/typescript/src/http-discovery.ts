/** Optional HTTP discovery and publication over the document core. */
import * as wasm from "./wasm/openbindings_wasm.js";
import { requireReady } from "./runtime.js";
import { SdkError, decode, sdkCall } from "./diagnostics.js";
import { scalar } from "./json.js";
import {
  ConformantDocument,
  Document,
  adoptDocument,
  conformantRaw,
} from "./document.js";
import type { ValidationReport } from "./document.js";
import { Managed, handle } from "./ownership.js";

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

/** One discovery attempt with an SDK-owned AbortController, and an SDK-owned timer when `timeoutMs` sets a deadline. Where the host exposes redirect responses, the SDK checks and follows each hop itself; a host that hides them follows redirects under its own policy and the SDK checks only the final URL. */
export interface DiscoveryOptions {
  /** Decoded-body byte limit; omitted selects 1 MiB. Must be a positive safe integer; zero is refused. Bounds accepted/retained document bytes; Fetch may deliver a larger chunk before overflow is detected, so this does not bound host buffering. */
  maxDocumentBytes?: number;
  /** Whole-attempt deadline in milliseconds. Omitted, the attempt has no deadline and runs until it completes or `signal` aborts it: a deadline only bounds time, so it is the application's choice. When set it must be positive and finite; zero is refused. Headers, followed redirects, decoded reads and validation boundaries share it. */
  timeoutMs?: number;
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
  // As portable Rust discovery: printable ASCII without backslashes, then WHATWG resolution.
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
 * Without `timeoutMs` the attempt has no deadline; when set, its timer covers headers, followed
 * redirects, decoded reads and validation boundaries.
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
  // null reads as omitted, as it does for maxDocumentBytes.
  const timeout = options.timeoutMs ?? undefined;
  if (timeout !== undefined && (!Number.isFinite(timeout) || timeout <= 0))
    throw new SdkError(
      "invalid-timeout",
      "timeoutMs must be positive and finite; omit it for no deadline.",
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
    deadline = timeout === undefined ? Infinity : performance.now() + timeout;
  if (timeout !== undefined && !Number.isFinite(deadline))
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
  if (timeout !== undefined) schedule();
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
    // The body is validated under limits that admit the bound, as in Rust; a
    // body within it cannot pass 4294967295 bytes of Wasm memory.
    const assessed = new wasm.WasmDiscovery(body, Math.min(limit, 0xffffffff));
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
  /** Create a detached Response for the route and method. A percent-encoded unreserved character in the path matches that character (RFC 3986 section 2.3), so /.well-known/%6Fpenbindings is the route; a reserved escape such as %2F is preserved and never matches /. GET returns exact bytes; HEAD returns no body with the original Content-Length; unsupported route is 404 and unsupported method 405. Creates no server and does not perform authentication or preflight handling. */
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

export { SdkError } from "./diagnostics.js";
