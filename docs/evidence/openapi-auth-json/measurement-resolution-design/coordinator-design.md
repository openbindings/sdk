# Independent coordinator design — identity controls before qualification

Date: 2026-10-13. Written before reading Claude's new independent proposal.
Status: proposal only; no implementation, timing or qualification authorized by this file.
Runtime remains3315307; latest evidence checkpoint5ce1c04. The historical failures
and the previous inconclusive diagnostic remain unchanged.

## Decision to make

Can a single prospectively chosen instrument measure the existing complete-call
workloads with sufficient resolution, and which object-identity confounds survive?
Do not optimize the product until there is evidence of redundant product work.
Separate these questions:

1. Same object, different measurement labels: temporal/schedule/instrument variation.
2. Distinct provider/client objects, shared immutable fixture data: instance variation
   after removing avoidable differences in payload allocation.
3. Distinct objects and independently allocated equivalent fixture data: the original
   construction regime, used to diagnose the consequences of sharing fixture data.

None identifies a CPU mechanism conclusively. In particular, source/provider state
and allocator history remain possible causes when equivalent independent objects
perform differently.

## One candidate method, selected before results

Use a common fixture factory, indexed condition objects and separate label-to-object
mapping. Resolve the function index outside the timing region. Same-object nulls
invoke the exact same object through both labels; do not build two equivalent wrappers
and call that the same object. Distinct-object nulls keep both objects alive while
labels are crossed; do not rebuild between label permutations.

The proposed final instrument shares **preexisting immutable input and response
storage** between compared conditions. These are JsonValue input handles and the
finite response buffer supplied by the caller/programmable host. Source documents,
prepared-source caches and authenticated versus anonymous condition state remain
separate. Context values retain the correct mechanism/selector/grant distinctions.

Per-call work remains inside each timed invocation: input/body conversion and parsing,
credential ownership/copying, context selection, preparation, client origin/header
checks, host response creation, result conversion, output destruction and completion.
Sharing caller-owned immutable fixture storage is not caching converted requests,
parsed per-call output, prepared invocation results or owned attempt credentials.
A four-condition preflight checks wire equality, exact expected auth headers and one
host effect, and verifies warm source reuse. No provider alternates between auth and
anonymous source documents; its one-source cache must remain warm.

### Sampling

Keep seven condition samples, and the final product statistic as a difference of
condition medians. Use blocks of exactly64 complete calls. The old64KiB total of
roughly350µs suggests roughly22ms blocks, reducing the share of switch-affected first
calls from about1/6 to1/64. This is a reason to test, not a performance prediction.

Use the cyclic order `0010203112132233`. Its16 cyclic transitions contain each ordered
pair (including self transitions) exactly once. Every condition occupies four slots.
Before the first timed cycle, execute one declared untimed64-call block of the cyclic
predecessor; thereafter complete entire cycles. Retain chronology across boundaries.
Each sample accumulates complete cycles until every condition has >=200ms of measured
work. Use sum(elapsed_ns)/sum(call_count), never an unweighted mean of block means.
Reserve raw-log capacity before timing; stop with an explicit capacity error rather
than reallocating within a measured group. Do not delete individual slow blocks.

Rotate the cyclic starting position by a frozen (process,row,sample,configuration)
formula, and alternate reversal. Rotation and reversal preserve cyclic edge balance.
Use the same schedule for direct/adapter and candidate/closed, with four anonymous
conditions in the closed process. Record every index, object ID, label, predecessor,
construction rank, elapsed time and count. The current50ms and2ms samplers remain
historical; neither is another selectable candidate in this study.

### Identity/control matrix

Four workloads: AND POST64KiB,3.0.4 and3.1.2, binding and operation.
Four fresh diagnostic processes (not product qualification pairs). Four rather than
three allows each row equal representation of both construction orientations.

For each row/process, construct two pairs of direct instances and two pairs of
adapter instances: the common-fixture pair and the independent-fixture pair. Keep
all objects alive for all configurations in that row. Alternate whole construction
orientation by (process+row)%2. Within each pair A/B names denote retained physical
objects, not semantic auth state.

Nine configurations per row:
- one alias-null configuration: D0/Da map to the same common direct A; C0/Ca map to
  the same common adapter A;
- four common-fixture distinct-instance null configurations: independently cross the
  D label mapping and C label mapping through all four bit pairs00,01,10,11;
- four independent-fixture null configurations with those same four mappings.

Use a fixed cyclic configuration rotation across row/process; publish the complete
schedule before running. Seven samples in every configuration. This is144control
configurations, not a search over144 candidate designs. Independent-fixture results
are diagnostic; alias/common-fixture results assess the one proposed final method.
The diagnostic contains no real-auth timing, so method selection cannot follow a
favorable auth result. It uses the real complete anonymous call pipeline.

This matrix is deliberately broad for causal discrimination. Its weakness is cost
and the chance that many strict per-cell controls produce an inconclusive result.
A comparison should consider whether fewer redundant configurations preserve the
needed contrasts without hiding them in a pooled mean.

## Control decision and arithmetic

For every alias/common-fixture configuration in every row/process, use the existing
1,000ns engineering guard on each of:
`abs(Da-D0)`, `abs(Ca-C0)`, `abs((Ca-C0)-(Da-D0))`.
No pooling across mappings/processes to rescue a failure, no null subtraction and
no averaging signed physical-object offsets into an apparent zero. Independent-
fixture diagnostic cells retain failures but do not qualify the proposed shared-data
method. Any eligible-cell failure stops as inconclusive; finish the fixed matrix
unless there is corruption, host-effect failure, user interruption or a hard cap.

Report per-object means/medians and label-, mapping- and construction-oriented
contrasts descriptively. If alias passes but common-fixture distinct objects fail,
object identity remains an unresolved measurement confound. If common passes and
independent fails, that supports sensitivity to fixture construction but does not
identify a particular allocator/cache cause. Alias failure leaves a timing/schedule
problem even before independent-instance comparisons. No p-value claim is needed.

Define an observed null envelope per workload as the maximum
`abs(direct null delta)+abs(adapter null delta)` over all eligible configurations and
processes. Keep it separate from the1µs guards; it is a conservative empirical
screen for marginal numerical passes, not a confidence interval or error bound.
Do not choose a shorter/slower schedule after seeing it.

## Conditional qualification

Only after independent review accepts all controls, permit one full qualification
using exactly the chosen method and identical source on candidate/closed SDKs.
Keep the original24unsecured and16auth rows, three fresh alternating process pairs,
seven >=200ms condition samples, exact context S, every-pair rule and >15% baseline
spread check. The formula and constants do not change. Do not replace64KiB by1KiB,
change the statistic to paired-difference medians, or compare prepared requests only.

Repeat the same-object and common-fixture identity controls for each large workload
inside each candidate qualification process, before and after its real-auth timing,
with both simultaneous D/C mappings. These controls detect loss of resolution during
qualification. A failed control makes that process inconclusive. Keep condition
sample/aggregate rules identical. The exact expanded schedule must be committed with
the diagnostic instrument, not invented after an adequate diagnostic result.

For each auth gate, report numerical slack and the observed null envelope from
qualification and diagnostic controls. For the64KiB rows, a numerical pass whose
slack is no greater than this envelope is inconclusive. This additional conservative
screen cannot increase a budget or prove a statistical guarantee. Smaller rows keep
the original budgets; no unmeasured extrapolation from AND1KiB is permitted.

The larger envelope screen and extra qualification controls increase conservative
stops; this is preferable to implying that a1µs null guard resolves a442ns margin.
A full numerical failure remains failed. A numerical pass with inadequate controls
remains inconclusive. Historical outcomes never change.

## Bounds and handoff

Design pass now only. On later execution authorization: one measurement-only branch
patch, one instrument correctness review/repair before timing, one fixed diagnostic,
then at most one final qualification if its gate allows it. No runtime optimization,
source/media/auth expansion, new APIs, bindingspec work or sampling-parameter search.

Working ceilings:30minutes diagnostic timing and35minutes final timing, plus a fixed
10second settling period once after sequential release builds. These are abort caps,
not targets; partial runs cannot qualify. Expected control cost is roughly14–18minutes
at prior totals, but estimates are not promises. No changes to sample duration/call
count to fit a deadline. Fixed environment probes before/after each process; missing
probes are recorded, not converted into a disturbance claim or conditional rerun.
No builds/tests run alongside measurements.

Before timing, commit protocol, exact fixtures/manifest, mapping/construction schedule,
source/runner hashes, compiler/target settings, package provenance, preflight oracles,
raw validator and stop/classifier code. Fresh extracted qualification may use an
explicitly reviewed harness-only receipt extension if every product and functional
consumer input remains identical; otherwise rerun the affected qualification.
Any extension must leave the old receipt intact and prove the new harness was built
against those exact qualified packages. No forged "full retest" claim.

Store compressed raw blocks, all process outputs and failures, environment results,
recipe and executable hashes, and a retained local executable. Result reviewers must
recompute raw aggregates/medians, verify identity mappings and timed work, check the
stop rule, and distinguish investigation closure from slice completion. No repeated
unchanged run, cleanup that discards failures, or threshold relaxation. If controls
fail again, close this proposal's execution as inconclusive and bring back a precise
measurement limitation; do not launch a third instrument redesign automatically.

## Alternatives not selected

- Repeating the current sampler or only increasing duration: does not address
  identity/label confounding and would be an unbounded search for a favorable sample.
- Null-bias subtraction: depends on transferring an unproven error model into auth.
- Pinning CPU cores/changing system policy: adds a deployment/environment assumption
  and does not distinguish object construction; first establish the contrast locally.
- Product changes before localization: risks spending a repair on noise.
- Replacing large-body rows with small-body rows: violates the preserved gate scope.
- Qualification on averaged favorable mappings: can hide the very instance bias the
  controls are meant to expose.
