//! Fresh guarded AST ownership with prepaid construction and destruction.
use super::normalized::{self, Summary};
use openbindings_internal_schema::{Control, Failure, FailureReason};
use regex_syntax::ast::{self, Ast};
use std::mem::size_of;
use std::ops::{Add, Mul};

// Fixed-size layout checks and both checked site sums. No input traversal or
// allocation occurs while computing these sums; their arithmetic has a fixed
// number of operations independent of the counter values.
const SCHEDULE_WORK: usize = 1024;

#[derive(Debug)]
pub(super) struct Parsed {
    pub tree: Ast,
    pub inventory: Summary,
}

#[derive(Debug)]
pub(super) enum Refusal {
    Normalized(normalized::Refusal),
    Representation,
    TargetLayout,
}

#[derive(Debug)]
pub(super) enum Error {
    Refused(Refusal),
    Attempt(Failure),
}

impl From<Failure> for Error {
    fn from(failure: Failure) -> Self {
        Self::Attempt(failure)
    }
}

pub(super) fn prepare(
    form: &str,
    maximum_group_depth: usize,
    maximum_ast_depth: u32,
    control: &dyn Control,
) -> Result<Parsed, Error> {
    control.check()?;
    let inventory =
        normalized::inspect(form, maximum_group_depth, control).map_err(|error| match error {
            normalized::Error::Refused(refusal) => Error::Refused(Refusal::Normalized(refusal)),
            normalized::Error::Attempt(failure) => Error::Attempt(failure),
        })?;
    control.charge_work(SCHEDULE_WORK)?;
    let layout = Layout::current().ok_or(Error::Refused(Refusal::TargetLayout))?;
    let cost = reservation(form.len(), &inventory, layout, control)?;
    control.charge_work(cost.work)?;
    control.charge_allocation(cost.allocation)?;
    let result = {
        #[cfg(test)]
        tests::entered_parser();
        let mut parser = ast::parse::ParserBuilder::new()
            .nest_limit(maximum_ast_depth)
            .build();
        parser.parse(form)
    };
    // Parser ownership is already gone. Both successful AST and error cleanup
    // were prepaid, including when this completion check refuses publication.
    control.check()?;
    match result {
        Ok(tree) => Ok(Parsed { tree, inventory }),
        Err(error) => {
            drop(error);
            Err(Error::Refused(Refusal::Representation))
        }
    }
}

#[derive(Clone, Copy)]
struct Amount(Option<usize>);

impl From<usize> for Amount {
    fn from(value: usize) -> Self {
        Self(Some(value))
    }
}

impl Add for Amount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.zip(rhs.0).and_then(|(a, b)| a.checked_add(b)))
    }
}

impl Add<usize> for Amount {
    type Output = Self;
    fn add(self, rhs: usize) -> Self {
        self + Self::from(rhs)
    }
}

impl Mul<usize> for Amount {
    type Output = Self;
    fn mul(self, rhs: usize) -> Self {
        Self(self.0.and_then(|value| value.checked_mul(rhs)))
    }
}

fn sum<const N: usize>(terms: [Amount; N]) -> Amount {
    terms.into_iter().fold(Amount::from(0), |a, b| a + b)
}

fn growth(count: Amount, width: usize, minimum: usize) -> Amount {
    match count.0 {
        Some(0) => Amount::from(0),
        Some(count) => Amount::from(count.max(minimum)) * width * 4,
        None => Amount(None),
    }
}

fn work_overflow(control: &dyn Control) -> Failure {
    // Preserve cancellation or a previous exhausted ledger. The initial stage
    // check has spent work, so this impossible charge poisons a shared Attempt.
    if let Err(failure) = control
        .check()
        .and_then(|()| control.charge_work(usize::MAX))
    {
        return failure;
    }
    Failure::new(
        FailureReason::LimitExceeded,
        "preparation-work-units",
        "pattern AST work reservation exceeds its allowance",
    )
}

fn allocation_overflow(control: &dyn Control) -> Failure {
    if let Err(failure) = control.check() {
        return failure;
    }
    control.allocation_limit_reached();
    Failure::new(
        FailureReason::LimitExceeded,
        "preparation-allocation-bytes",
        "pattern AST allocation reservation exceeds its allowance",
    )
}

#[derive(Debug)]
struct Reservation {
    work: usize,
    allocation: usize,
}

fn reservation(
    bytes: usize,
    inventory: &Summary,
    z: Layout,
    control: &dyn Control,
) -> Result<Reservation, Failure> {
    let t = Amount::from(inventory.literals) + inventory.assertions;
    let g = Amount::from(inventory.groups);
    let b = Amount::from(inventory.alternatives);
    let q = Amount::from(inventory.repetitions);
    let k = Amount::from(inventory.classes);
    let r = Amount::from(inventory.class_items);
    let f = Amount::from(inventory.flags);
    let bytes = Amount::from(bytes);
    let c = g + b + 1;
    let n = t + k + g * 3 + q + b * 3 + 2;
    let d = n * 2 + g * 2 + q + c;
    let w = |width: usize| width.div_ceil(z.word);

    // Named sites follow the independently reviewed AST work schedule. Public
    // payload widths are checked below; private widths are pinned per target.
    let work = sum([
        bytes * 32 + g * 32 + k * 16 + q * 16 + 64,
        (bytes + g + k + q + 1) * 128,
        bytes * 5, // numeric scratch insertion/relocation
        bytes,     // numeric radix scan
        bytes,     // at most one error-owned source copy
        (t * w(z.literal) + k * w(z.class)) * 6,
        (g * w(z.group) + q * w(z.repetition)) * 6,
        (c * w(z.concat) + b * w(z.alternation)) * 6,
        (c + g) * w(z.span) * 6,
        f * w(z.flag) * 6 + f * 4,
        (g * 2 + b + q) * w(z.concat) * 2,
        (t + k + g + q + b * 2) * w(z.ast) * 6,
        r * w(z.item) * 10,
        k * w(z.set) * 6,
        (g + b) * w(z.group_state) * 6,
        k * w(z.class_state) * 6,
        (n + r + k) * 32,
        (n * w(z.ast_visit) + (r + k) * w(z.class_visit)) * 6,
        (d + n) * 8,
        n * w(z.ast) * 6 + (n + g + q + c + b) * w(z.span) * 6,
        (r * 2 + k * 5) * 8,
        (r + k) * w(z.set) * 6,
    ]);
    let allocation = sum([
        t * z.literal + k * z.class,
        g * z.group + q * z.repetition + (g * 2 + q) * z.ast,
        c * z.concat + b * z.alternation + (c + g) * z.span,
        (t + k + g + q + b * 2 + (c + b) * 4) * z.ast * 4,
        (r + k * 4) * z.item * 4,
        growth(g + b, z.group_state, 4),
        growth(k, z.class_state, 4),
        g * z.flag * 4,
        (bytes + 8) * 4,
        growth(n, z.ast_visit, 4) + (r + k * 4) * z.class_visit * 4,
        (n + (g + q + c + b) * 4) * z.ast * 4 + (n + g + q + c + b) * z.span,
        (r + k * 4) * z.set * 4,
        bytes,
    ]);
    Ok(Reservation {
        work: work.0.ok_or_else(|| work_overflow(control))?,
        allocation: allocation.0.ok_or_else(|| allocation_overflow(control))?,
    })
}

#[derive(Clone, Copy)]
struct Layout {
    word: usize,
    ast: usize,
    span: usize,
    literal: usize,
    group: usize,
    repetition: usize,
    concat: usize,
    alternation: usize,
    class: usize,
    set: usize,
    item: usize,
    flag: usize,
    group_state: usize,
    class_state: usize,
    ast_visit: usize,
    class_visit: usize,
}

impl Layout {
    fn pinned(word: usize) -> Option<Self> {
        match word {
            8 => Some(Self {
                word,
                ast: 16,
                span: 48,
                literal: 56,
                group: 144,
                repetition: 128,
                concat: 72,
                alternation: 72,
                class: 216,
                set: 160,
                item: 160,
                flag: 56,
                group_state: 224,
                class_state: 288,
                ast_visit: 40,
                class_visit: 48,
            }),
            4 => Some(Self {
                word,
                ast: 8,
                span: 24,
                literal: 32,
                group: 72,
                repetition: 68,
                concat: 36,
                alternation: 36,
                class: 116,
                set: 88,
                item: 88,
                flag: 28,
                group_state: 112,
                class_state: 152,
                ast_visit: 20,
                class_visit: 24,
            }),
            _ => None,
        }
    }

    fn current() -> Option<Self> {
        let layout = Self::pinned(size_of::<usize>())?;
        let public = [
            (layout.ast, size_of::<Ast>()),
            (layout.span, size_of::<ast::Span>()),
            (layout.literal, size_of::<ast::Literal>()),
            (layout.group, size_of::<ast::Group>()),
            (layout.repetition, size_of::<ast::Repetition>()),
            (layout.concat, size_of::<ast::Concat>()),
            (layout.alternation, size_of::<ast::Alternation>()),
            (layout.class, size_of::<ast::ClassBracketed>()),
            (layout.set, size_of::<ast::ClassSet>()),
            (layout.item, size_of::<ast::ClassSetItem>()),
            (layout.flag, size_of::<ast::FlagsItem>()),
        ];
        (public.into_iter().all(|(pinned, actual)| pinned == actual)
            && size_of::<ast::Assertion>() <= layout.literal)
            .then_some(layout)
    }
}

#[cfg(test)]
mod tests;
