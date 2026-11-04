use super::{InternalError, Result};
use std::ops::{Add, Mul};

#[derive(Clone, Copy)]
pub(super) struct Amount(pub Option<usize>);
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
impl Mul for Amount {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(self.0.zip(rhs.0).and_then(|(a, b)| a.checked_mul(b)))
    }
}
impl Mul<usize> for Amount {
    type Output = Self;
    fn mul(self, rhs: usize) -> Self {
        self * Self::from(rhs)
    }
}
pub(super) fn checked(value: Amount) -> Result<usize> {
    value.0.ok_or(InternalError::Overflow)
}
pub(super) fn add(value: &mut usize, other: usize) -> Result<()> {
    *value = value.checked_add(other).ok_or(InternalError::Overflow)?;
    Ok(())
}
pub(super) fn growth(width: usize, count: Amount, minimum: usize) -> Amount {
    match count.0 {
        Some(0) => Amount::from(0),
        Some(n) => Amount::from(n.max(minimum)) * width * 4,
        None => Amount(None),
    }
}
pub(super) fn sort(count: usize) -> Amount {
    if count < 2 {
        Amount::from(0)
    } else {
        Amount::from(count) * count * 32 + Amount::from(count) * 512
    }
}
