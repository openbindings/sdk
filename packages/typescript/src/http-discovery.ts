/** Optional HTTP discovery companion. Fetch follows redirects under browser per-hop policy; the SDK checks the final URL and owns the attempt timer. */
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
