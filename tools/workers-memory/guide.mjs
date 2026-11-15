// Keep these options identical to docs/workers.md's recipe. The recipe test
// compares the documented objects with these values before running the stages.
const MiB = 1048576;
export const jsonLimits = { maxBytes: 4 * MiB, maxNodes: 250_000, maxDepth: 10_000 };
export const preparationLimits = { maxSdkReservationBytes: 16 * MiB, maxSdkWorkUnits: 100_000_000 };
export const prepareOptions = { preparationLimits,
  limits: { evaluationSdkWorkUnits: 100_000_000, evaluationSdkReservationBytes: 4 * MiB } };
