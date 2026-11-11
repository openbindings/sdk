use crate::{
    Comparison, ComparisonFailure, ComparisonLimits, Direction, SchemaInput, Verdict,
    budget::Attempt, directional,
};
use openbindings::ConformantDocument;
use std::fmt;

/// One required operation, including skipped slots and any comparison failures.
#[derive(Clone)]
pub struct OperationComparison {
    required: String,
    provided: Option<String>,
    output: Option<Result<Comparison, ComparisonFailure>>,
    input: Option<Result<Comparison, ComparisonFailure>>,
}
impl fmt::Debug for OperationComparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationComparison")
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
    /// Collapse this operation. A failure is never converted to a verdict.
    pub fn verdict(&self) -> Result<Verdict, ComparisonFailure> {
        if self.is_missing() {
            return Ok(Verdict::Incompatible);
        }
        let mut verdict = Verdict::Compatible;
        for result in [self.output(), self.input()].into_iter().flatten() {
            verdict = verdict.max(result.as_ref().map_err(|e| *e)?.verdict());
        }
        Ok(verdict)
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
    /// Collapse all operations using the profile dominance. Returns the first
    /// comparison failure in report order when any check failed, never a verdict.
    pub fn verdict(&self) -> Result<Verdict, ComparisonFailure> {
        let mut verdict = Verdict::Compatible;
        for operation in &self.operations {
            verdict = verdict.max(operation.verdict()?);
        }
        Ok(verdict)
    }
}
/// Check provided operations as realizing the required document's contract.
/// Swap the explicit roles to check the opposite consumption relationship.
/// Matching tries the required key, then its authored aliases, in the provided
/// flat key-and-alias namespace. Unspecified schema pairs are skipped entirely.
/// Setup failure returns no report; slot failures stay in the completed report.
pub fn check_interface(
    required: &ConformantDocument,
    provided: &ConformantDocument,
    limits: &ComparisonLimits,
) -> Result<InterfaceComparison, ComparisonFailure> {
    check_interface_with_control(
        required,
        provided,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
/// Interface checking with one shared monotone budget and cooperative control.
/// After a resource refusal, subsequent present pairs retain that same failure;
/// no failed allowance is reset for a later operation.
pub fn check_interface_with_control(
    required: &ConformantDocument,
    provided: &ConformantDocument,
    limits: &ComparisonLimits,
    control: &dyn openbindings::WorkControl,
) -> Result<InterfaceComparison, ComparisonFailure> {
    let mut attempt = Attempt::new(*limits, control);
    attempt.check()?;
    let root = required.document().value();
    let members = root
        .get("operations")
        .expect("conformant operations")
        .members()
        .expect("operation map");
    let mut pending = attempt.vector(members.len())?;
    let mut comparisons = attempt.vector(members.len())?;
    let provided_root = provided.document().value();
    let height = provided_root.text().len().saturating_add(1).ilog2() as usize + 1;
    let mut name_bytes = 0usize;
    for member in members {
        let key = member.name.as_str().expect("conformant key");
        name_bytes = name_bytes.saturating_add(key.len());
        attempt.work(key.len().saturating_add(1))?;
        let mut matched = None;
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
                matched = Some(operation);
                break;
            }
        }
        attempt.allocate(
            key.len()
                .saturating_add(matched.as_ref().map_or(0, |o| o.key().len())),
        )?;
        let provided_key = matched.as_ref().map(|o| o.key().to_owned());
        let provided_value = matched
            .as_ref()
            .map(|operation| operation.value().to_owned());
        // Retain views now, before a slot can exhaust the monotone allowance.
        pending.push((key.to_owned(), provided_key, member.value, provided_value));
    }
    attempt.work(
        name_bytes
            .saturating_add(pending.len())
            .saturating_mul(pending.len().saturating_add(1).ilog2() as usize + 1)
            .saturating_mul(2),
    )?;
    pending.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (key, provided_key, target, candidate) in pending {
        let mut check = |direction: Direction, name: &str| {
            let (Some(t), Some(c)) = (
                target.get(name),
                candidate.as_ref().and_then(|v| v.get(name)),
            ) else {
                return None;
            };
            let t = SchemaInput::new(t.to_owned(), root.clone());
            let c = SchemaInput::new(c.to_owned(), provided_root.clone());
            Some(directional::inputs(direction, &t, &c, &mut attempt))
        };
        let output = check(Direction::Output, "output");
        let input = check(Direction::Input, "input");
        comparisons.push(OperationComparison {
            required: key,
            provided: provided_key,
            output,
            input,
        });
    }
    Ok(InterfaceComparison {
        operations: comparisons,
    })
}
