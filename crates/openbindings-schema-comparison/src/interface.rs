use crate::ComparisonFailureKind as FailureKind;
use crate::{
    Comparison, ComparisonContext, ComparisonFailure, ComparisonLimits, Direction, Verdict,
    budget::Attempt, directional,
};
use openbindings::{ConformantDocument, Operation};
use std::fmt;

/// Caller-selected value flow for one required/provided match. A missing match
/// has no role. Both emits realization then consumption for a present match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationRole {
    /// Provided operation accepts required inputs and produces required outputs.
    Realized,
    /// Provided operation supplies inputs accepted by the required operation and
    /// accepts outputs allowed by the required operation.
    Consumed,
    /// Check realization then consumption for a present match. A missing match
    /// still produces one correspondence with no checked role.
    Both,
}
/// One required operation, including skipped slots and any comparison failures.
#[derive(Clone)]
pub struct OperationComparison {
    required: String,
    role: Option<OperationRole>,
    provided: Option<String>,
    output: Option<Result<Comparison, ComparisonFailure>>,
    input: Option<Result<Comparison, ComparisonFailure>>,
}
impl fmt::Debug for OperationComparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationComparison")
            .field("role", &self.role)
            .field("missing", &self.is_missing())
            .field("output", &self.output)
            .field("input", &self.input)
            .finish_non_exhaustive()
    }
}
impl OperationComparison {
    /// Required operation's primary key, even when matched through an alias.
    pub fn required(&self) -> &str {
        &self.required
    }
    /// Provided operation's primary key; absent means missing.
    pub fn provided(&self) -> Option<&str> {
        self.provided.as_deref()
    }
    /// Whether the provided key-and-alias namespace has no match.
    pub fn is_missing(&self) -> bool {
        self.provided.is_none()
    }
    /// Output outcome; absent means either schema was unspecified or the operation is missing.
    pub fn output(&self) -> Option<&Result<Comparison, ComparisonFailure>> {
        self.output.as_ref()
    }
    /// Input outcome; absent means either schema was unspecified or the operation is missing.
    pub fn input(&self) -> Option<&Result<Comparison, ComparisonFailure>> {
        self.input.as_ref()
    }
    /// Checked value flow: Realized or Consumed for a present match, never Both.
    /// Missing operations return None and have exactly one correspondence.
    pub fn role(&self) -> Option<OperationRole> {
        self.role
    }
}
/// An interface issue's kind. Indeterminate outcomes and failures stay distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceIssueKind {
    /// Included required operation is absent from the provided namespace, regardless of selected role.
    Missing,
    /// Realized: provided output does not satisfy required output.
    /// Consumed: provided operation does not accept every output the required operation allows.
    OutputIncompatible,
    /// Realized: provided operation does not accept every required input.
    /// Consumed: provided input is not acceptable to the required operation.
    InputIncompatible,
    /// Output comparison in the selected value-flow direction reached a differing outside-profile position.
    OutputIndeterminate,
    /// Input comparison in the selected value-flow direction reached a differing outside-profile position.
    InputIndeterminate,
    /// Output normalization or comparison in the selected value-flow direction failed without a verdict.
    OutputFailure,
    /// Input normalization or comparison in the selected value-flow direction failed without a verdict.
    InputFailure,
}
/// A borrowed issue in sorted required-key order, output before input.
#[derive(Clone, Copy)]
pub struct InterfaceIssue<'a> {
    operation: &'a OperationComparison,
    kind: InterfaceIssueKind,
    result: Option<&'a Result<Comparison, ComparisonFailure>>,
}
impl fmt::Debug for InterfaceIssue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InterfaceIssue")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
impl<'a> InterfaceIssue<'a> {
    /// The correspondence and checked role that produced this issue.
    pub fn correspondence(&self) -> &'a OperationComparison {
        self.operation
    }
    /// Required operation's primary key.
    pub fn operation(&self) -> &'a str {
        self.operation.required()
    }
    /// The issue category, separate from schema or operation values.
    pub fn kind(&self) -> InterfaceIssueKind {
        self.kind
    }
    /// Report reason, if the directional comparison supplied one.
    pub fn reason(&self) -> Option<&'a str> {
        self.result
            .and_then(|r| r.as_ref().ok())
            .and_then(Comparison::reason)
    }
    /// Comparison failure, if this issue has no verdict.
    pub fn failure(&self) -> Option<ComparisonFailure> {
        self.result.and_then(|r| r.as_ref().err()).cloned()
    }
}
/// All required operations, including successful, unspecified and failed checks.
#[derive(Clone, Debug)]
pub struct InterfaceComparison {
    operations: Vec<OperationComparison>,
}
impl InterfaceComparison {
    /// Sorted required-operation outcomes. Added provided operations need no check.
    pub fn operations(&self) -> &[OperationComparison] {
        &self.operations
    }
    /// Findings in required primary-key order, output before input.
    pub fn issues(&self) -> impl Iterator<Item = InterfaceIssue<'_>> {
        self.operations.iter().flat_map(|operation| {
            let missing = operation.is_missing().then_some(InterfaceIssue {
                operation,
                kind: InterfaceIssueKind::Missing,
                result: None,
            });
            let slot = |direction| {
                let result = match direction {
                    Direction::Output => operation.output(),
                    Direction::Input => operation.input(),
                };
                let kind = match (result, direction) {
                    (Some(Ok(c)), Direction::Output) if c.verdict() == Verdict::Incompatible => {
                        InterfaceIssueKind::OutputIncompatible
                    }
                    (Some(Ok(c)), Direction::Input) if c.verdict() == Verdict::Incompatible => {
                        InterfaceIssueKind::InputIncompatible
                    }
                    (Some(Ok(c)), Direction::Output) if c.verdict() == Verdict::Indeterminate => {
                        InterfaceIssueKind::OutputIndeterminate
                    }
                    (Some(Ok(c)), Direction::Input) if c.verdict() == Verdict::Indeterminate => {
                        InterfaceIssueKind::InputIndeterminate
                    }
                    (Some(Err(_)), Direction::Output) => InterfaceIssueKind::OutputFailure,
                    (Some(Err(_)), Direction::Input) => InterfaceIssueKind::InputFailure,
                    _ => return None,
                };
                Some(InterfaceIssue {
                    operation,
                    kind,
                    result,
                })
            };
            [missing, slot(Direction::Output), slot(Direction::Input)]
                .into_iter()
                .flatten()
        })
    }
}
/// Check every provided operation identified by any required key or alias.
/// Each context must retain the exact document root of its paired conformance
/// proof. A mismatch fails with [`crate::ComparisonFailureKind::ContextMismatch`]
/// before the callback runs. Context bases and supplied resources are used as-is.
/// The callback receives the required operation and each distinct matched
/// provided operation, or None for a missing match. It selects the value flow,
/// or returns None to exclude that match. Both expands a present match into two
/// checks; an included missing match always yields one role-free correspondence.
/// Correspondences sort by required key, provided key and role; each checks
/// output before input. Slot failures remain results for caller summary policy.
pub fn check_interface(
    required: &ConformantDocument,
    provided: &ConformantDocument,
    required_context: &ComparisonContext,
    provided_context: &ComparisonContext,
    role: &dyn Fn(&Operation, Option<&Operation>) -> Option<OperationRole>,
    limits: &ComparisonLimits,
) -> Result<InterfaceComparison, ComparisonFailure> {
    check_interface_with_control(
        required,
        provided,
        required_context,
        provided_context,
        role,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
/// Interface checking with one shared work and memory policy and cooperative control.
/// The callback selects each match's flow as in check_interface. Setup failure
/// returns no report. After setup, exhausted allowances stay in slots and later
/// slots retain the sticky failure. Observed cancellation always returns
/// an error of kind [`crate::ComparisonFailureKind::Cancelled`], including in the final slot.
pub fn check_interface_with_control(
    required: &ConformantDocument,
    provided: &ConformantDocument,
    required_context: &ComparisonContext,
    provided_context: &ComparisonContext,
    role: &dyn Fn(&Operation, Option<&Operation>) -> Option<OperationRole>,
    limits: &ComparisonLimits,
    control: &dyn openbindings::WorkControl,
) -> Result<InterfaceComparison, ComparisonFailure> {
    let mut attempt = Attempt::new(*limits, control);
    attempt.check()?;
    let root = required.document().value();
    let provided_root = provided.document().value();
    for (root, context, side) in [
        (root, required_context, crate::ComparisonSide::Required),
        (
            provided_root,
            provided_context,
            crate::ComparisonSide::Provided,
        ),
    ] {
        if openbindings_internal_json::backend::occurrence(root.view())
            != openbindings_internal_json::backend::occurrence(context.document().view())
        {
            return Err(ComparisonFailure::from(FailureKind::ContextMismatch).with_side(side));
        }
    }
    let members = root
        .get("operations")
        .expect("conformant operations")
        .members()
        .expect("operation map");
    let mut pending = attempt.vector(members.len())?;
    let mut comparisons = attempt.vector(members.len())?;
    let height = provided_root.text().len().saturating_add(1).ilog2() as usize + 1;
    let mut name_bytes = 0usize;
    for member in members {
        let key = member.name.as_str().expect("conformant key");
        attempt.work(key.len().saturating_add(1))?;
        let operation = required.operation(key).expect("required operation");
        let mut matches = std::collections::BTreeMap::new();
        let mut match_bytes = 0usize;
        for name in std::iter::once(key).chain(
            member
                .value
                .get("aliases")
                .into_iter()
                .flat_map(|v| v.elements().expect("aliases"))
                .map(|v| v.as_str().expect("alias")),
        ) {
            attempt.work(
                name.len()
                    .saturating_add(1)
                    .saturating_mul(height)
                    .saturating_mul(12),
            )?;
            if let Some(operation) = provided.resolve_operation(name) {
                let bytes = operation.key().len().saturating_add(512);
                attempt.allocate(bytes)?;
                match_bytes += bytes;
                matches.insert(operation.key().to_owned(), operation.value().to_owned());
            }
        }
        let provided_document = provided;
        let mut add = |provided: Option<String>,
                       value: Option<openbindings::JsonValue>|
         -> Result<(), ComparisonFailure> {
            let matched = provided
                .as_deref()
                .and_then(|key| provided_document.operation(key));
            let Some(role) = role(&operation, matched.as_ref()) else {
                return Ok(());
            };
            let mut push = |selected| -> Result<(), ComparisonFailure> {
                attempt.allocate(
                    key.len()
                        .saturating_add(provided.as_ref().map_or(0, String::len))
                        .saturating_add(2 * std::mem::size_of::<OperationComparison>()),
                )?;
                name_bytes = name_bytes
                    .saturating_add(key.len())
                    .saturating_add(provided.as_ref().map_or(0, String::len));
                pending.push((
                    key.to_owned(),
                    provided.clone(),
                    selected,
                    member.value,
                    value.clone(),
                ));
                Ok(())
            };
            if provided.is_none() {
                push(None)?;
            } else {
                match role {
                    OperationRole::Realized => push(Some(OperationRole::Realized))?,
                    OperationRole::Consumed => push(Some(OperationRole::Consumed))?,
                    OperationRole::Both => {
                        push(Some(OperationRole::Realized))?;
                        push(Some(OperationRole::Consumed))?;
                    }
                }
            }
            Ok(())
        };
        if matches.is_empty() {
            add(None, None)?;
        } else {
            for (key, value) in &matches {
                add(Some(key.clone()), Some(value.clone()))?;
            }
        }
        drop(matches);
        attempt.release(match_bytes);
    }
    attempt.work(
        name_bytes
            .saturating_add(pending.len())
            .saturating_mul(pending.len().saturating_add(1).ilog2() as usize + 1)
            .saturating_mul(2),
    )?;
    pending.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| {
            (a.2 == Some(OperationRole::Consumed)).cmp(&(b.2 == Some(OperationRole::Consumed)))
        })
    });
    for (key, provided_key, role, target, candidate) in pending {
        let mut check = |direction: Direction, name: &str| {
            let (Some(t), Some(c)) = (
                target.get(name),
                candidate.as_ref().and_then(|v| v.get(name)),
            ) else {
                return None;
            };
            let t = required_context.schema(t.to_owned());
            let c = provided_context.schema(c.to_owned());
            Some(attempt.scratch(|attempt| {
                let result = if role == Some(OperationRole::Consumed) {
                    directional::inputs(
                        direction,
                        &c,
                        &t,
                        (
                            crate::ComparisonSide::Provided,
                            crate::ComparisonSide::Required,
                        ),
                        attempt,
                    )
                } else {
                    directional::inputs(
                        direction,
                        &t,
                        &c,
                        (
                            crate::ComparisonSide::Required,
                            crate::ComparisonSide::Provided,
                        ),
                        attempt,
                    )
                };
                // Only the final reason or failure location escapes this slot.
                // Scratch and intermediate diagnostics have already been dropped.
                let retained = match &result {
                    Ok(comparison) => comparison.reason().map_or(0, str::len),
                    Err(failure) => failure.retained_bytes(),
                };
                attempt.retain(retained)?;
                result
            }))
        };
        let output = check(Direction::Output, "output");
        let input = check(Direction::Input, "input");
        comparisons.push(OperationComparison {
            required: key,
            provided: provided_key,
            role,
            output,
            input,
        });
    }
    if control.is_cancelled() {
        return Err(ComparisonFailure::from(FailureKind::Cancelled));
    }
    Ok(InterfaceComparison {
        operations: comparisons,
    })
}
