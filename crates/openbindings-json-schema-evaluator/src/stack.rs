//! The bounds the evaluator sizes from the stack the host gives it
//! (`Limits::stack_bytes`), and the refusals that name it.
//!
//! Three costs recurse with the value contract or the value: evaluation, which the
//! library runs by recursion through the schemas it enters; the library's
//! `unevaluatedProperties` and `unevaluatedItems` validators, which recurse in
//! ways the static depth bound does not model; and the library's compilation
//! of an unevaluated keyword's annotation walk. Each bound is the stack scaled
//! by a cost measured in release builds on Linux x86_64, Windows x86_64 and
//! macOS arm64 (`tests/evaluation_stack.rs` and `tests/walk_stack.rs` qualify
//! them at several stack sizes, and `docs/performance-boundaries.md` gives each
//! host's margins from 128 KiB to 2 MiB).
use openbindings::{NoVerdict, NoVerdictReason};

/// The stack each schema evaluation the static depth bound counts may take.
const EVALUATION_BYTES: usize = 256;
/// The share of the stack a guarded pass may use: half, which leaves room for
/// the frames one level of the value adds before the next check.
const GUARD_SHARE: usize = 2;
/// The share of the stack left to the caller's frames before preparation: a
/// fifth.
const CALLER_SHARE: usize = 5;
/// The stack the rest of preparation takes around an annotation walk: the
/// `$ref` targets the library is compiling when it reaches the holder (up to
/// 48 KiB), its and the SDK's own frames, and what a Windows thread keeps for
/// itself.
const WALK_RESERVE: usize = 99 * 1024;
/// The stack one unit of an annotation walk's weight takes at most.
const WALK_UNIT: usize = 1024;

/// The bounds a stack of `stack_bytes` gives.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stack {
    pub(crate) bytes: usize,
}
impl Stack {
    /// The most schema evaluations a value's evaluation may nest.
    pub(crate) fn evaluation_depth(self) -> u64 {
        (self.bytes / EVALUATION_BYTES) as u64
    }
    /// The stack a guarded pass may use.
    pub(crate) fn guard(self) -> usize {
        self.bytes / GUARD_SHARE
    }
    /// The stack preparation keeps for an annotation walk.
    fn walk_bytes(self) -> usize {
        (self.bytes - self.bytes / CALLER_SHARE).saturating_sub(WALK_RESERVE)
    }
    /// The heaviest annotation walk an unevaluated keyword may follow.
    pub(crate) fn walk(self) -> u32 {
        u32::try_from(self.walk_bytes() / WALK_UNIT).unwrap_or(u32::MAX)
    }

    /// A value nested deeper than `deepest` levels, or any value when
    /// `deepest` is `None`, could nest more schema evaluations than this
    /// stack admits.
    pub(crate) fn too_deep(self, deepest: Option<usize>) -> NoVerdict {
        let admitted = format!(
            "the {} schema evaluations its {} stack_bytes holds at {EVALUATION_BYTES} bytes each",
            self.evaluation_depth(),
            crate::bytes(self.bytes),
        );
        let message = match deepest {
            Some(levels) => format!(
                "this value nests more than {levels} levels, the deepest this value contract \
                 evaluates within {admitted}; {RAISE}"
            ),
            None => format!(
                "evaluating any value against this value contract could nest more than \
                 {admitted}; {RAISE}"
            ),
        };
        NoVerdict::new(
            NoVerdictReason::LimitExceeded,
            "evaluation-depth-limit",
            message,
        )
    }
    /// A guarded pass used more stack than it may.
    pub(crate) fn guard_exceeded(self) -> NoVerdict {
        NoVerdict::new(
            NoVerdictReason::LimitExceeded,
            "evaluation-depth-limit",
            format!(
                "this evaluation used more than {} of stack, the half of its {} stack_bytes a \
                 contract holding unevaluatedProperties or unevaluatedItems may use as it \
                 evaluates; {RAISE}",
                crate::bytes(self.guard()),
                crate::bytes(self.bytes),
            ),
        )
    }
    /// An unevaluated keyword's annotation walk weighs `weight`, more than
    /// this stack admits.
    pub(crate) fn walk_too_heavy(self, weight: u32) -> NoVerdict {
        NoVerdict::new(
            NoVerdictReason::LimitExceeded,
            "unevaluated-walk-limit",
            format!(
                "this keyword's annotation walk weighs {weight}, more than the {} its {} \
                 stack_bytes admits: a fifth of the stack is left to the caller, {} to the rest \
                 of preparation, and each unit takes {}; {RAISE}",
                self.walk(),
                crate::bytes(self.bytes),
                crate::bytes(WALK_RESERVE),
                crate::bytes(WALK_UNIT),
            ),
        )
    }
}
const RAISE: &str = "raise stack_bytes (stackBytes in TypeScript)";
