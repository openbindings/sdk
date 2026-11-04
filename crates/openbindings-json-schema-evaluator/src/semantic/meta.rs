//! Consumer-schema admission against the selected original meta-schema.
//! These obligations are preparation state, distinct from recursive evaluation
//! of a meta-schema over progressively smaller parts of its JSON instance.
use super::*;
use openbindings_internal_schema::{Control, Failure, FailureReason};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Checking,
    Valid,
    Invalid,
}
enum Frame {
    Enter(usize),
    Finish { resource: usize, meta: usize },
}
struct Cancellation<'a>(&'a dyn Control);
impl WorkControl for Cancellation<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

pub(super) fn query_failure(cause: QueryFailure) -> PreparationFailure {
    let mut failure = Failure::new(
        FailureReason::UnsupportedCapability,
        "custom-meta-selection",
        "the original meta-schema could not be selected",
    );
    failure.query_failure = Some(Box::new(cause));
    failure.into()
}
fn unsupported(capability: &str) -> PreparationFailure {
    PreparationFailure::new(PreparationFailureKind::UnsupportedCapability {
        capability: capability.into(),
    })
}

pub(super) fn validate(
    root: &Plan,
    construction: &Construction<'_>,
) -> Result<(), PreparationFailure> {
    let attempt = construction.control();
    if root.custom.is_empty() {
        return Ok(());
    }
    // This table belongs to one immutable environment. Resource/target numbers
    // therefore retain original source-record and node identity, profile and
    // scope; no URI-keyed result can escape into another document's cache.
    attempt.charge_allocation(root.custom.len().saturating_mul(64).saturating_add(512))?;
    let mut pending: Vec<_> = root.custom.iter().copied().map(Frame::Enter).collect();
    let mut status = HashMap::new();
    let mut plans = HashMap::<usize, Plan>::new();
    let cancelled = Cancellation(attempt);
    while let Some(frame) = pending.pop() {
        attempt.check()?;
        match frame {
            Frame::Enter(resource) => {
                match status.get(&resource) {
                    Some(Status::Valid) => continue,
                    Some(Status::Invalid) => {
                        return Err(PreparationFailure::new(
                            PreparationFailureKind::InvalidExternalSchema,
                        ));
                    }
                    // Checking never grants validity. A dependency cycle among
                    // admission obligations remains a capability refusal.
                    Some(Status::Checking) => return Err(unsupported("recursive-meta-obligation")),
                    None => {}
                }
                attempt.charge_allocation(512)?;
                status.insert(resource, Status::Checking);
                let meta = root
                    .environment
                    .space()
                    .custom_meta(resource)
                    .ok_or_else(|| unsupported("custom-meta-binding"))?
                    .map_err(query_failure)?;
                if let std::collections::hash_map::Entry::Vacant(entry) = plans.entry(meta) {
                    attempt.charge_allocation(512)?;
                    let mut plan =
                        Plan::at(construction, meta, root.limits.clone(), root.preparation)?;
                    // Every supplied resource used by the meta evaluator has
                    // its own complete syntax obligation, including referenced
                    // helpers whose unused definitions evaluation never enters.
                    for &resource in &plan.resources {
                        let original = root.environment.space().resource(resource);
                        if original.origin != openbindings_internal_schema::ResourceOrigin::Supplied
                        {
                            continue;
                        }
                        if let Some(entry) = original.root {
                            let syntax = preparation::meta_syntax(construction, entry)?;
                            attempt.charge_allocation(syntax.len().saturating_mul(8))?;
                            plan.custom.extend(syntax);
                        }
                    }
                    plan.custom.sort_unstable();
                    plan.custom.dedup();
                    entry.insert(plan);
                }
                let dependencies = &plans[&meta].custom;
                attempt
                    .charge_allocation(dependencies.len().saturating_mul(64).saturating_add(64))?;
                pending.push(Frame::Finish { resource, meta });
                pending.extend(dependencies.iter().copied().map(Frame::Enter));
            }
            Frame::Finish { resource, meta } => {
                let original = root
                    .environment
                    .space()
                    .resource(resource)
                    .root
                    .ok_or_else(|| unsupported("custom-consumer-resource-root"))?;
                let value = &root.environment.space().node(original).value;
                let mut unavailable = None;
                let outcome = plans[&meta].evaluate_tracking_unavailable(
                    value,
                    &cancelled,
                    Some(attempt),
                    &mut unavailable,
                );
                attempt.check()?;
                match outcome {
                    ValueOutcome::Satisfies => {
                        status.insert(resource, Status::Valid);
                    }
                    ValueOutcome::Fails { .. } => {
                        status.insert(resource, Status::Invalid);
                        return Err(PreparationFailure::new(
                            PreparationFailureKind::InvalidExternalSchema,
                        ));
                    }
                    ValueOutcome::Undefined { .. } => {
                        return Err(unsupported("custom-meta-undefined"));
                    }
                    ValueOutcome::NoVerdict { detail } => {
                        let kind = match detail.reason {
                            NoVerdictReason::Cancelled => PreparationFailureKind::Cancelled,
                            NoVerdictReason::LimitExceeded => PreparationFailureKind::Limit {
                                resource: detail.code.into_owned(),
                            },
                            NoVerdictReason::ResourceUnavailable => {
                                PreparationFailureKind::Resolution {
                                    cause: ResolutionFailureKind::ResourceUnavailable {
                                        uri: unavailable.ok_or_else(|| {
                                            unsupported("custom-meta-missing-origin")
                                        })?,
                                    },
                                }
                            }
                            NoVerdictReason::EvaluatorFailure => {
                                PreparationFailureKind::Implementation {
                                    message: detail.message.into_owned(),
                                }
                            }
                            _ => PreparationFailureKind::UnsupportedCapability {
                                capability: detail.code.into_owned(),
                            },
                        };
                        return Err(PreparationFailure::new(kind));
                    }
                }
            }
        }
    }
    attempt.check()?;
    Ok(())
}
