//! Shared lazy invocation lifecycle, independent of binding or operation policy.
use super::Shared;
use crate::{CancellationToken, InvocationError, binding::invocation_relay};
use std::{
    future::{Future, poll_fn},
    pin::pin,
    sync::Arc,
    task::Poll,
};

/// Keep the GAT projection outside the state machine to preserve conditional Send.
pub(crate) async fn drive<F: Future<Output = Result<(), InvocationError>>>(
    start: impl FnOnce() -> Result<F, InvocationError>,
    external: CancellationToken,
    shared: Arc<Shared>,
    finish: fn(Result<(), InvocationError>, &Shared) -> Result<(), InvocationError>,
) -> Result<(), InvocationError> {
    let local = shared.control();
    let _relay = invocation_relay(&external, &shared);
    if local.is_cancelled() {
        return Err(InvocationError::cancelled());
    }
    let mut cancelled = pin!(local.cancelled());
    let mut job = pin!(start()?);
    poll_fn(|cx| {
        if let Some(outcome) = shared.outcome() {
            return Poll::Ready(outcome);
        }
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(InvocationError::cancelled()));
        }
        let result = job.as_mut().poll(cx);
        if let Some(outcome) = shared.outcome() {
            Poll::Ready(outcome)
        } else if local.is_cancelled() {
            Poll::Ready(Err(InvocationError::cancelled()))
        } else {
            result.map(|result| finish(result, &shared))
        }
    })
    .await
}
