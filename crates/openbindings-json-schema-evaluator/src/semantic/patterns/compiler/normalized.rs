//! Allocation-free guard for a superset of the Reader's normalized token
//! sequences. This establishes backend syntax exclusions, not ECMA validity.
use openbindings_internal_schema::{Control, Failure, FailureReason};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    pub groups: usize,
    pub maximum_group_depth: usize,
    pub flags: usize,
    pub alternatives: usize,
    pub repetitions: usize,
    pub literals: usize,
    pub assertions: usize,
    pub classes: usize,
    pub class_items: usize,
    pub class_endpoints: usize,
    pub class_ranges: usize,
    pub numeric_digits: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    Grammar { offset: usize },
    Depth { offset: usize },
}

#[derive(Debug)]
pub(super) enum Error {
    Refused(Refusal),
    Attempt(Failure),
}

impl From<Failure> for Error {
    fn from(value: Failure) -> Self {
        Self::Attempt(value)
    }
}

fn overflow(control: &dyn Control) -> Error {
    // Observe a prior sticky cause before requesting an impossible work charge.
    // An Attempt's initial check has already spent a unit, so this also makes
    // work exhaustion sticky in that attempt. Plain controls have no ledger.
    if let Err(failure) = control
        .check()
        .and_then(|()| control.charge_work(usize::MAX))
    {
        return Error::Attempt(failure);
    }
    Error::Attempt(Failure::new(
        FailureReason::LimitExceeded,
        "preparation-work-units",
        "normalized pattern inventory exceeds its work allowance",
    ))
}

fn add(value: &mut usize, increment: usize, control: &dyn Control) -> Result<(), Error> {
    *value = value
        .checked_add(increment)
        .ok_or_else(|| overflow(control))?;
    Ok(())
}

fn reserve_work(bytes: usize, control: &dyn Control) -> Result<(), Error> {
    control.check()?;
    let work = bytes
        .checked_mul(8)
        .and_then(|work| work.checked_add(1))
        .ok_or_else(|| overflow(control))?;
    control.charge_work(work)?;
    Ok(())
}

pub(super) fn inspect(
    form: &str,
    maximum_depth: usize,
    control: &dyn Control,
) -> Result<Summary, Error> {
    reserve_work(form.len(), control)?;
    let mut scan = Scanner {
        bytes: form.as_bytes(),
        at: 0,
        depth: 0,
        maximum_depth,
        summary: Summary::default(),
        control,
    };
    scan.tokens()?;
    control.check()?;
    Ok(scan.summary)
}

struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
    depth: usize,
    maximum_depth: usize,
    summary: Summary,
    control: &'a dyn Control,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn advance(&mut self, count: usize) -> Result<(), Error> {
        for _ in 0..count {
            debug_assert!(self.at < self.bytes.len());
            add(&mut self.at, 1, self.control)?;
            if self.at.is_multiple_of(256) {
                self.control.check()?;
            }
        }
        Ok(())
    }

    fn grammar(&self) -> Error {
        match self.control.check() {
            Err(failure) => Error::Attempt(failure),
            Ok(()) => Error::Refused(Refusal::Grammar { offset: self.at }),
        }
    }

    fn group(&mut self) -> Result<(), Error> {
        if self.depth >= self.maximum_depth {
            self.control.check()?;
            return Err(Error::Refused(Refusal::Depth { offset: self.at }));
        }
        add(&mut self.depth, 1, self.control)?;
        add(&mut self.summary.groups, 1, self.control)?;
        self.summary.maximum_group_depth = self.summary.maximum_group_depth.max(self.depth);
        Ok(())
    }

    fn tokens(&mut self) -> Result<(), Error> {
        while let Some(byte) = self.peek() {
            match byte {
                b'(' => {
                    self.group()?;
                    if self.bytes[self.at..].starts_with(br"(?-u:\b)") {
                        add(&mut self.summary.flags, 2, self.control)?;
                        add(&mut self.summary.assertions, 1, self.control)?;
                        self.advance(8)?;
                        self.depth -= 1;
                    } else if self.bytes[self.at..].starts_with(b"(?:") {
                        self.advance(3)?;
                    } else if self.bytes.get(self.at + 1) == Some(&b'?') {
                        return Err(self.grammar());
                    } else {
                        self.advance(1)?;
                    }
                }
                b')' => {
                    if self.depth == 0 {
                        return Err(self.grammar());
                    }
                    self.depth -= 1;
                    self.advance(1)?;
                }
                b'[' => self.class()?,
                b'\\' => {
                    self.scalar()?;
                    add(&mut self.summary.literals, 1, self.control)?;
                }
                b'^' | b'$' => {
                    add(&mut self.summary.assertions, 1, self.control)?;
                    self.advance(1)?;
                }
                b'|' => {
                    add(&mut self.summary.alternatives, 1, self.control)?;
                    self.advance(1)?;
                }
                b'*' | b'+' | b'?' | b'{' => self.repetition()?,
                _ if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'/') => {
                    add(&mut self.summary.literals, 1, self.control)?;
                    self.advance(1)?;
                }
                _ => return Err(self.grammar()),
            }
        }
        if self.depth != 0 {
            return Err(self.grammar());
        }
        Ok(())
    }

    fn scalar(&mut self) -> Result<u32, Error> {
        if let Some(byte) = self.peek().filter(u8::is_ascii_alphanumeric) {
            self.advance(1)?;
            return Ok(u32::from(byte));
        }
        if !self.bytes[self.at..].starts_with(br"\x{") {
            return Err(self.grammar());
        }
        self.advance(3)?;
        let start = self.at;
        let mut value = 0u32;
        while let Some(byte) = self.peek() {
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'A'..=b'F' => u32::from(byte - b'A' + 10),
                b'}' => break,
                _ => return Err(self.grammar()),
            };
            value = value
                .checked_mul(16)
                .and_then(|value| value.checked_add(digit))
                .ok_or_else(|| self.grammar())?;
            self.advance(1)?;
        }
        if self.at == start || self.peek() != Some(b'}') || char::from_u32(value).is_none() {
            return Err(self.grammar());
        }
        self.advance(1)?;
        Ok(value)
    }

    fn class(&mut self) -> Result<(), Error> {
        add(&mut self.summary.classes, 1, self.control)?;
        self.advance(1)?;
        let mut previous = None;
        while self.peek().is_some_and(|byte| byte != b']') {
            let first = self.scalar()?;
            add(&mut self.summary.class_endpoints, 1, self.control)?;
            let last = if self.peek() == Some(b'-') {
                self.advance(1)?;
                add(&mut self.summary.class_endpoints, 1, self.control)?;
                add(&mut self.summary.class_ranges, 1, self.control)?;
                self.scalar()?
            } else {
                first
            };
            if first > last
                || (first <= 0xDFFF && last >= 0xD800)
                || previous.is_some_and(|previous| first <= previous)
            {
                return Err(self.grammar());
            }
            previous = Some(last);
            add(&mut self.summary.class_items, 1, self.control)?;
        }
        if previous.is_none() || self.peek() != Some(b']') {
            return Err(self.grammar());
        }
        self.advance(1)
    }

    fn decimal(&mut self) -> Result<(), Error> {
        let start = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            add(&mut self.summary.numeric_digits, 1, self.control)?;
            self.advance(1)?;
        }
        if self.at == start {
            return Err(self.grammar());
        }
        Ok(())
    }

    fn repetition(&mut self) -> Result<(), Error> {
        add(&mut self.summary.repetitions, 1, self.control)?;
        let counted = self.peek() == Some(b'{');
        self.advance(1)?;
        if counted {
            self.decimal()?;
            if self.peek() == Some(b',') {
                self.advance(1)?;
                if self.peek() != Some(b'}') {
                    self.decimal()?;
                }
            }
            if self.peek() != Some(b'}') {
                return Err(self.grammar());
            }
            self.advance(1)?;
        }
        if self.peek() == Some(b'?') {
            self.advance(1)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
