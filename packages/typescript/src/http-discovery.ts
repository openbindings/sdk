/** Optional HTTP discovery companion. Where the host exposes redirect responses, as Node does, the SDK checks every hop before dispatching it, as the Rust checked paths do; a browser hides them, so it follows redirects under its own per-hop policy and the SDK checks the final URL. The SDK owns the attempt timer. */
export {
  discover,
  discoveryEndpoint,
  discoveryPolicy,
  DiscoveryPublication,
  SdkError,
} from "./internal.js";
export type {
  AllowOrigin,
  DiscoveryFetch,
  DiscoveryOptions,
  DiscoveryMetadata,
  DiscoveryResult,
  DiscoveryPolicy,
} from "./internal.js";
