use crate::{
    Comparison, ComparisonContext, ComparisonFailure, ComparisonLimits, Direction, Verdict,
    budget::Attempt, directional,
};
use openbindings::{ConformantDocument, Operation};
use std::fmt;

/// Caller-selected use of a required operation. Both emits a separate result
/// for each role, with realization before consumption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationRole {
    /// Provided operation realizes the required contract.
    Realized,
    /// Provided operation consumes the required contract.
    Consumed,
    /// Check both realization and consumption.
    Both,
}
/// One required operation, including skipped slots and any comparison failures.
#[derive(Clone)]
pub struct OperationComparison {
    required: String,
    role: OperationRole,
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
    /// Role checked for this correspondence. Both produces two correspondences.
    pub fn role(&self) -> OperationRole {
        self.role
    }
}
/// An interface issue's kind. Indeterminate outcomes and failures stay distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceIssueKind {
    /// Required operation is absent from the provided namespace.
    Missing,
    /// Provided output does not satisfy required output.
    OutputIncompatible,
    /// Required input is not acceptable to the provided input.
    InputIncompatible,
    /// Output comparison reached a differing outside-profile position.
    OutputIndeterminate,
    /// Input comparison reached a differing outside-profile position.
    InputIndeterminate,
    /// Output normalization or comparison failed without a verdict.
    OutputFailure,
    /// Input normalization or comparison failed without a verdict.
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
        self.result.and_then(|r| r.as_ref().err()).copied()
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
/// The callback chooses each required operation's role, or None to exclude it.
/// Correspondences sort by required key, provided key and role; each checks
/// output before input. Slot failures remain results for caller summary policy.
pub fn check_interface(
    required: &ConformantDocument,
    provided: &ConformantDocument,
    role: &dyn Fn(&Operation) -> Option<OperationRole>,
    limits: &ComparisonLimits,
) -> Result<InterfaceComparison, ComparisonFailure> {
    check_interface_with_control(
        required,
        provided,
        role,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
/// Interface checking with one monotone allowance and cooperative control.
/// Setup failure returns no report; subsequent slot failures stay in the report.
pub fn check_interface_with_control(
    required: &ConformantDocument,
    provided: &ConformantDocument,
    role: &dyn Fn(&Operation) -> Option<OperationRole>,
    limits: &ComparisonLimits,
    control: &dyn openbindings::WorkControl,
) -> Result<InterfaceComparison, ComparisonFailure> {
    let mut attempt = Attempt::new(*limits, control);
    attempt.check()?;
    let root = required.document().value();
    let provided_root = provided.document().value();
    let required_context = ComparisonContext::new(root.clone());
    let provided_context = ComparisonContext::new(provided_root.clone());
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
        let Some(role) = role(&operation) else {
            continue;
        };
        let mut matches = std::collections::BTreeMap::new();
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
                attempt.allocate(operation.key().len().saturating_add(512))?;
                matches.insert(operation.key().to_owned(), operation.value().to_owned());
            }
        }
        let mut add = |provided: Option<String>,
                       value: Option<openbindings::JsonValue>|
         -> Result<(), ComparisonFailure> {
            for selected in [OperationRole::Realized, OperationRole::Consumed] {
                if role != OperationRole::Both && selected != role {
                    continue;
                }
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
    }
    attempt.work(
        name_bytes
            .saturating_add(pending.len())
            .saturating_mul(pending.len().saturating_add(1).ilog2() as usize + 1)
            .saturating_mul(2),
    )?;
    pending.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
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
            Some(if role == OperationRole::Consumed {
                directional::inputs(direction, &c, &t, &mut attempt)
            } else {
                directional::inputs(direction, &t, &c, &mut attempt)
            })
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
    Ok(InterfaceComparison {
        operations: comparisons,
    })
}
