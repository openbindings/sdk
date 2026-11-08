//! Test-only accounting at resolver text-processing boundaries. Spelling is
//! counted by default; only an unavailable-resource entry retained by the report credits output.
#[cfg(test)]
thread_local! {
    static BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[inline]
pub(crate) fn text(bytes: usize) {
    #[cfg(test)]
    BYTES.with(|count| count.set(count.get().saturating_add(bytes)));
    #[cfg(not(test))]
    let _ = bytes;
}
#[cfg(test)]
pub(crate) fn retained(bytes: usize) {
    BYTES.with(|count| count.set(count.get() - bytes));
}
#[cfg(test)]
pub(crate) fn take() -> usize {
    BYTES.with(|count| count.replace(0))
}
