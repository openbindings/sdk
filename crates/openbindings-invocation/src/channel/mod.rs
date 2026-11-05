//! Bounded channels with a single caller-owned output driver. Provider handles
//! carry values and request identities, never evaluator types or callbacks.
mod signal;
#[cfg(test)]
mod tests;
mod validation;
use self::{
    signal::{Signal, Waiter},
    validation::{OfferFailure, RequestId, Slot},
};
use crate::{
    CancellationToken, InvocationError,
    boundary::{BoundaryFailure, BoundaryFailureKind, failure},
    operation::ValidationFailure,
};
use futures_core::Stream;
use openbindings::{JsonLimits, JsonValue};
use std::{
    collections::VecDeque,
    fmt,
    future::{Future, poll_fn},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

type Outcome = Result<(), InvocationError>;
const POLL_CREDITS: usize = 64;

/// Queue capacities, optional value admission policy and error encoding budget.
#[derive(Clone, Copy, Debug)]
pub struct InvocationLimits {
    /// Accepted inputs awaiting delivery, plus at most one pending check.
    pub input_capacity: usize,
    /// Committed outputs awaiting the caller, plus at most one pending emission.
    pub output_capacity: usize,
    /// Optional maximum token bytes for each input or output value; no cap by default.
    /// Values are already parsed when this is checked. This policy does not bound
    /// parsing allocations or retained parent arenas; JSON parsing has its own limits.
    pub max_value_bytes: Option<usize>,
    /// Maximum encoded terminal error or advisory challenge bytes.
    pub max_error_bytes: usize,
}
impl Default for InvocationLimits {
    fn default() -> Self {
        Self {
            input_capacity: 1,
            output_capacity: 4,
            max_value_bytes: None,
            max_error_bytes: 64 * 1024,
        }
    }
}
/// Invalid finite handoff policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitError;
impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("capacities must be positive; terminal limit must be at least 256 bytes")
    }
}
impl std::error::Error for LimitError {}
impl InvocationLimits {
    pub(crate) fn validate(self) -> Result<Self, LimitError> {
        if self.input_capacity == 0 || self.output_capacity == 0 || self.max_error_bytes < 256 {
            Err(LimitError)
        } else {
            Ok(self)
        }
    }
}
/// Local caller submission result. The output terminal remains authoritative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WriteError {
    /// No further input can be accepted.
    Closed,
    /// This value exceeds the configured token-byte allowance.
    ValueTooLarge,
}
impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Closed => "input is closed",
            Self::ValueTooLarge => "value exceeds the handoff byte limit",
        })
    }
}
impl std::error::Error for WriteError {}

/// The caller-visible ordered events of one invocation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum OutputEvent {
    /// One successful caller-facing value; it does not establish terminal success.
    Output(JsonValue),
    /// The provider declined further input while output may continue.
    InputClosed,
    /// Normal terminal completion. No event follows it.
    Complete,
    /// Unsuccessful terminal completion. Earlier values are not retracted.
    Error(InvocationError),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Direction {
    Input,
    Output,
}
impl Direction {
    fn index(self) -> usize {
        match self {
            Self::Input => 0,
            Self::Output => 1,
        }
    }
}
struct State {
    input: VecDeque<JsonValue>,
    output: VecDeque<JsonValue>,
    caller_closed: bool,
    provider_closed: bool,
    input_closed_event: bool,
    terminal: Option<Outcome>,
    finding: Option<BoundaryFailure>,
    validation_failure: Option<Arc<ValidationFailure>>,
    emitted: bool,
    slots: [Option<Slot<JsonValue>>; 2],
    credits: usize,
    active_poll: bool,
    yield_notified: bool,
}
pub(crate) struct Shared {
    state: Mutex<State>,
    signal: Arc<Signal>,
    provider_read: Arc<Signal>,
    provider_write: Arc<Signal>,
    local: CancellationToken,
    limits: InvocationLimits,
}
pub(crate) struct Check {
    direction: Direction,
    id: RequestId,
    pub(crate) value: JsonValue,
}
struct PollEpoch(Arc<Shared>);
impl Drop for PollEpoch {
    fn drop(&mut self) {
        self.0.end_poll();
    }
}
impl Shared {
    pub(crate) fn new(limits: InvocationLimits, gated: [bool; 2]) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                input: VecDeque::new(),
                output: VecDeque::new(),
                caller_closed: false,
                provider_closed: false,
                input_closed_event: false,
                terminal: None,
                finding: None,
                validation_failure: None,
                emitted: false,
                slots: gated.map(|gate| gate.then(Slot::default)),
                credits: POLL_CREDITS,
                active_poll: false,
                yield_notified: false,
            }),
            signal: Arc::default(),
            provider_read: Arc::default(),
            provider_write: Arc::default(),
            local: CancellationToken::new(),
            limits,
        })
    }
    pub(crate) fn control(&self) -> CancellationToken {
        self.local.clone()
    }
    pub(crate) fn outcome(&self) -> Option<Outcome> {
        self.state.lock().unwrap().terminal.clone()
    }
    pub(crate) fn record_finding(&self, finding: BoundaryFailure) {
        let mut state = self.state.lock().unwrap();
        if state.terminal.is_none() {
            state.finding.get_or_insert(finding);
        }
    }
    fn notify(&self) {
        self.provider_read.notify();
        self.provider_write.notify();
        self.signal.notify();
    }
    fn normalize(&self, outcome: Outcome) -> (Outcome, Option<BoundaryFailure>) {
        if let Err(error) = &outcome {
            let limits = JsonLimits {
                max_bytes: self.limits.max_error_bytes,
                ..Default::default()
            };
            if error.to_json_with_limits(limits).is_err() {
                return (
                    Err(InvocationError::execution_failed()),
                    Some(failure(
                        BoundaryFailureKind::TerminalResourceLimit,
                        "/error",
                    )),
                );
            }
        }
        (outcome, None)
    }
    fn finish_locked(
        state: &mut State,
        mut outcome: Outcome,
        finding: Option<BoundaryFailure>,
        evidence: Option<Arc<ValidationFailure>>,
    ) -> Vec<JsonValue> {
        if state.emitted
            && outcome
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == "CONTEXT_REQUIRED")
        {
            state
                .finding
                .get_or_insert(failure(BoundaryFailureKind::ChallengeAfterOutput, "/error"));
            outcome = Err(InvocationError::execution_failed());
        }
        if let Some(finding) = finding {
            state.finding.get_or_insert(finding);
        }
        state.terminal = Some(outcome);
        state.validation_failure = evidence;
        state.provider_closed = true;
        state.input_closed_event = false;
        let mut released: Vec<_> = state.input.drain(..).collect();
        released.extend(state.slots.iter_mut().flatten().filter_map(Slot::close));
        released
    }
    pub(crate) fn settle(&self, outcome: Outcome) -> bool {
        let (outcome, finding) = self.normalize(outcome);
        let released = {
            let mut state = self.state.lock().unwrap();
            if state.terminal.is_some() {
                return false;
            }
            Self::finish_locked(&mut state, outcome, finding, None)
        };
        drop(released);
        self.local.cancel();
        self.notify();
        true
    }
    pub(crate) fn pending_check(&self, direction: Direction) -> Option<Check> {
        let mut state = self.state.lock().unwrap();
        if state.terminal.is_some() {
            return None;
        }
        let (id, value) = state.slots[direction.index()].as_mut()?.begin_check()?;
        Some(Check {
            direction,
            id,
            value,
        })
    }
    /// Stale success AND failure are discarded. The direction and Arc-owned
    /// attempt route a slot-local token; cancellation is settled independently.
    pub(crate) fn checked(
        &self,
        check: Check,
        result: Result<(), (InvocationError, Option<Arc<ValidationFailure>>)>,
    ) {
        let Check {
            direction,
            id,
            value,
        } = check;
        let mut value = Some(value);
        let (result, finding) = match result {
            Ok(()) => (Ok(()), None),
            Err((error, evidence)) => {
                let (outcome, finding) = self.normalize(Err(error));
                (Err((outcome.unwrap_err(), evidence)), finding)
            }
        };
        let mut released = Vec::new();
        let settled = {
            let mut state = self.state.lock().unwrap();
            let current = state.terminal.is_none()
                && state.slots[direction.index()]
                    .as_ref()
                    .is_some_and(|slot| slot.checking(id));
            if !current {
                false
            } else {
                match result {
                    Ok(()) => {
                        let slot = state.slots[direction.index()]
                            .as_mut()
                            .expect("current slot");
                        value = slot.accept(id, value.take().expect("checked value")).err();
                        false
                    }
                    Err((error, evidence)) => {
                        released = Self::finish_locked(&mut state, Err(error), finding, evidence);
                        true
                    }
                }
            }
        };
        drop(value);
        drop(released);
        if settled {
            self.local.cancel();
        }
        self.notify();
    }
    fn close_input(&self, provider: bool) {
        let released = {
            let mut state = self.state.lock().unwrap();
            if provider {
                let newly_closed = !state.provider_closed;
                state.provider_closed = true;
                state.input_closed_event |= newly_closed && state.terminal.is_none();
                let mut released: Vec<_> = state.input.drain(..).collect();
                released.extend(state.slots[0].as_mut().and_then(Slot::close));
                released
            } else {
                state.caller_closed = true;
                Vec::new()
            }
        };
        drop(released);
        self.notify();
    }
    fn begin_poll(self: &Arc<Self>) -> PollEpoch {
        // Establish cleanup before any callback can unwind from credit renewal.
        let epoch = PollEpoch(self.clone());
        let wake = {
            let mut state = self.state.lock().unwrap();
            let wake = state.credits == 0;
            state.credits = POLL_CREDITS;
            state.active_poll = true;
            state.yield_notified = false;
            wake
        };
        if wake {
            self.provider_read.notify();
            self.provider_write.notify();
        }
        epoch
    }
    fn end_poll(&self) {
        let wake = {
            let mut state = self.state.lock().unwrap();
            state.active_poll = false;
            state.terminal.is_some()
        };
        if wake {
            self.provider_read.notify();
            self.provider_write.notify();
        }
    }
    // Call only when an I/O operation can make progress or return Ready. An
    // already blocked poll must not consume credits and invent a busy wake loop.
    fn credit(state: &mut State) -> (bool, bool) {
        if state.terminal.is_some() {
            (!state.active_poll, false)
        } else if state.credits > 0 {
            state.credits -= 1;
            (true, false)
        } else {
            (false, !std::mem::replace(&mut state.yield_notified, true))
        }
    }
}

/// A lazy invocation. The output half alone drives provider and validation work.
pub struct Invocation<F: Future<Output = Outcome>> {
    input: InvocationInput,
    output: InvocationOutput<F>,
}
impl<F: Future<Output = Outcome>> fmt::Debug for Invocation<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Invocation").finish_non_exhaustive()
    }
}
impl<F: Future<Output = Outcome>> Invocation<F> {
    pub(crate) fn new(shared: Arc<Shared>, future: F) -> Self {
        Self {
            input: InvocationInput {
                shared: shared.clone(),
            },
            output: InvocationOutput {
                waiter: Waiter::new(shared.signal.clone()),
                shared,
                future: Some(Box::pin(future)),
                exhausted: false,
            },
        }
    }
    /// Split the bounded input owner from the provider-driving output owner.
    /// Poll both halves concurrently in the caller's chosen runtime.
    pub fn split(self) -> (InvocationInput, InvocationOutput<F>) {
        (self.input, self.output)
    }
    /// Observe completion or cancel this attempt independently of either half.
    pub fn control(&self) -> InvocationControl {
        self.output.control()
    }
}

/// Cancellation and terminal observation for one attempt; it never drives work.
#[derive(Clone)]
pub struct InvocationControl {
    shared: Arc<Shared>,
}
impl fmt::Debug for InvocationControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvocationControl").finish_non_exhaustive()
    }
}
impl InvocationControl {
    /// Select cancellation if the attempt is still unsettled. No rollback claim.
    pub fn cancel(&self) {
        self.shared.settle(Err(InvocationError::cancelled()));
    }
    /// Owned terminal snapshot; queued output may still precede its event.
    pub fn outcome(&self) -> Option<Outcome> {
        self.shared.outcome()
    }
    /// Local value-free admission or provider boundary evidence.
    pub fn boundary_failure(&self) -> Option<BoundaryFailure> {
        self.shared.state.lock().unwrap().finding
    }
    /// Typed local evidence retained only for a winning validation failure.
    pub fn validation_failure(&self) -> Option<Arc<ValidationFailure>> {
        self.shared.state.lock().unwrap().validation_failure.clone()
    }
}

/// Sole caller input owner. This type carries no provider or evaluator future.
pub struct InvocationInput {
    shared: Arc<Shared>,
}
impl fmt::Debug for InvocationInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvocationInput").finish_non_exhaustive()
    }
}
impl InvocationInput {
    /// Accept one exact value into the bounded queue; this is not dispatch acknowledgement.
    /// Dropping a pending send accepts no value. Poll output concurrently to drive work.
    ///
    /// # Errors
    /// Returns Closed after either input half closes, or ValueTooLarge for an
    /// oversized submission that was not accepted.
    pub async fn send(&mut self, value: JsonValue) -> Result<(), WriteError> {
        if self
            .shared
            .limits
            .max_value_bytes
            .is_some_and(|maximum| value.bytes().len() > maximum)
        {
            return Err(WriteError::ValueTooLarge);
        }
        let mut value = Some(value);
        let mut waiter = Waiter::new(self.shared.signal.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            let result = {
                let mut state = self.shared.state.lock().unwrap();
                if state.caller_closed || state.provider_closed {
                    Poll::Ready(Err(WriteError::Closed))
                } else if state.input.len() < self.shared.limits.input_capacity {
                    state
                        .input
                        .push_back(value.take().expect("uncompleted send"));
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            };
            if result.is_ready() {
                self.shared.provider_read.notify();
                self.shared.signal.notify();
            }
            result
        })
        .await
    }
    /// Idempotently stop new submissions; already accepted input still drains.
    pub fn close(&mut self) {
        self.shared.close_input(false);
    }
    /// Whether caller or provider has stopped accepting further input.
    pub fn is_closed(&self) -> bool {
        let state = self.shared.state.lock().unwrap();
        state.caller_closed || state.provider_closed
    }
    /// Wait for either input owner to close. This does not drive provider work.
    pub async fn closed(&self) {
        let mut waiter = Waiter::new(self.shared.signal.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            if self.is_closed() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await
    }
}
impl Drop for InvocationInput {
    fn drop(&mut self) {
        self.shared.close_input(false);
    }
}

/// Sole output owner and driver. No SDK task is spawned. Dropping it cancels
/// unsettled local work and releases the provider future outside channel locks.
pub struct InvocationOutput<F: Future<Output = Outcome>> {
    shared: Arc<Shared>,
    future: Option<Pin<Box<F>>>,
    waiter: Waiter,
    exhausted: bool,
}
impl<F: Future<Output = Outcome>> fmt::Debug for InvocationOutput<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvocationOutput").finish_non_exhaustive()
    }
}
impl<F: Future<Output = Outcome>> InvocationOutput<F> {
    /// Observe or cancel this attempt without retaining its provider future.
    pub fn control(&self) -> InvocationControl {
        InvocationControl {
            shared: self.shared.clone(),
        }
    }
    /// Read one ordered event, driving provider work cooperatively. After a
    /// Complete/Error event all subsequent reads return None.
    #[allow(clippy::should_implement_trait)]
    pub async fn next(&mut self) -> Option<OutputEvent> {
        // The scoped registration releases this task's waker if the read is dropped.
        self.waiter.clear();
        let mut waiter = Waiter::new(self.shared.signal.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            self.poll_event(cx)
        })
        .await
    }
    fn poll_event(&mut self, _cx: &mut Context<'_>) -> Poll<Option<OutputEvent>> {
        if self.exhausted {
            return Poll::Ready(None);
        }
        if self.shared.outcome().is_none()
            && let Some(future) = &mut self.future
        {
            let epoch = self.shared.begin_poll();
            // A retained provider future may keep this proxy, but never the
            // scoped output read's caller waker. Waiter owns that registration.
            let waker = std::task::Waker::from(self.shared.signal.clone());
            let outcome = future.as_mut().poll(&mut Context::from_waker(&waker));
            if let Poll::Ready(outcome) = outcome {
                self.shared.settle(outcome);
            }
            drop(epoch);
        }
        if self.shared.outcome().is_some() {
            drop(self.future.take());
        }
        let result = {
            let mut state = self.shared.state.lock().unwrap();
            if let Some(value) = state.output.pop_front() {
                Poll::Ready(Some(OutputEvent::Output(value)))
            } else if let Some(outcome) = &state.terminal {
                self.exhausted = true;
                Poll::Ready(Some(match outcome {
                    Ok(()) => OutputEvent::Complete,
                    Err(error) => OutputEvent::Error(error.clone()),
                }))
            } else if std::mem::take(&mut state.input_closed_event) {
                Poll::Ready(Some(OutputEvent::InputClosed))
            } else {
                Poll::Pending
            }
        };
        if result.is_ready() {
            self.shared.provider_write.notify();
        }
        result
    }
}
impl<F: Future<Output = Outcome>> Stream for InvocationOutput<F> {
    type Item = OutputEvent;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<OutputEvent>> {
        let this = self.get_mut();
        this.waiter.register(cx.waker());
        let result = this.poll_event(cx);
        if result.is_ready() {
            this.waiter.clear();
        }
        result
    }
}
impl<F: Future<Output = Outcome>> Drop for InvocationOutput<F> {
    fn drop(&mut self) {
        self.waiter.clear();
        self.shared.settle(Err(InvocationError::cancelled()));
        let released = {
            let mut state = self.shared.state.lock().unwrap();
            std::mem::take(&mut state.output)
        };
        drop(released);
        drop(self.future.take());
    }
}

/// The provider's independently usable input and output handles. Their types
/// and shared storage do not depend on an operation evaluator or its validator.
pub struct ProviderIo {
    input: ProviderInput,
    output: ProviderOutput,
}
impl fmt::Debug for ProviderIo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderIo").finish_non_exhaustive()
    }
}
impl ProviderIo {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        Self {
            input: ProviderInput {
                waiter: Waiter::new(shared.provider_read.clone()),
                shared: shared.clone(),
                request: None,
            },
            output: ProviderOutput { shared },
        }
    }
    /// Take the sole reader and emitter for independently polled provider work.
    pub fn split(self) -> (ProviderInput, ProviderOutput) {
        (self.input, self.output)
    }
    /// Receive one value or graceful end-of-input. Rejection is never clean EOF.
    pub async fn next(&mut self) -> Option<JsonValue> {
        self.input.next().await
    }
    /// Commit one exact output under backpressure and any operation validation.
    ///
    /// # Errors
    /// Returns the winning terminal failure if this emission cannot be committed.
    pub async fn send(&mut self, value: JsonValue) -> Result<(), InvocationError> {
        self.output.send(value).await
    }
    /// Decline further input while allowing output to continue.
    pub fn close_input(&mut self) {
        self.input.close();
    }
}

/// Sole provider reader. A cancelled read keeps accepted input for its owner.
/// An unsuccessful terminal suspends reads instead of presenting invalid input
/// as graceful EOF; use the provider cancellation token for independent cleanup.
pub struct ProviderInput {
    shared: Arc<Shared>,
    request: Option<RequestId>,
    waiter: Waiter,
}
impl fmt::Debug for ProviderInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderInput").finish_non_exhaustive()
    }
}
impl ProviderInput {
    /// Receive one validated input, or None after genuine graceful closure.
    #[allow(clippy::should_implement_trait)]
    pub async fn next(&mut self) -> Option<JsonValue> {
        self.waiter.clear();
        let mut waiter = Waiter::new(self.shared.provider_read.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            self.receive()
        })
        .await
    }
    fn receive(&mut self) -> Poll<Option<JsonValue>> {
        let mut notify = false;
        let mut refused = None;
        let result = {
            let mut state = self.shared.state.lock().unwrap();
            if matches!(state.terminal, Some(Err(_))) {
                Poll::Pending
            } else if state.provider_closed {
                let (credit, wake) = Shared::credit(&mut state);
                notify |= wake;
                if credit {
                    Poll::Ready(None)
                } else {
                    Poll::Pending
                }
            } else if let Some(id) = self.request {
                if state.slots[0]
                    .as_ref()
                    .is_some_and(|slot| slot.accepted(id))
                {
                    let (credit, wake) = Shared::credit(&mut state);
                    notify |= wake;
                    if credit {
                        let value = state.slots[0]
                            .as_mut()
                            .and_then(|slot| slot.take_accepted(id));
                        self.request = None;
                        Poll::Ready(value)
                    } else {
                        Poll::Pending
                    }
                } else {
                    Poll::Pending
                }
            } else if !state.input.is_empty() {
                let (credit, wake) = Shared::credit(&mut state);
                notify |= wake;
                if !credit {
                    Poll::Pending
                } else {
                    let value = state.input.pop_front().expect("nonempty input");
                    if let Some(slot) = &mut state.slots[0] {
                        match slot.offer(value) {
                            Ok(id) => {
                                self.request = Some(id);
                                notify = true;
                            }
                            Err(
                                OfferFailure::Busy(value)
                                | OfferFailure::Closed(value)
                                | OfferFailure::Exhausted(value),
                            ) => {
                                refused = Some(value);
                            }
                        }
                        Poll::Pending
                    } else {
                        Poll::Ready(Some(value))
                    }
                }
            } else if state.caller_closed {
                let (credit, wake) = Shared::credit(&mut state);
                notify |= wake;
                if credit {
                    Poll::Ready(None)
                } else {
                    Poll::Pending
                }
            } else {
                Poll::Pending
            }
        };
        if refused.is_some() {
            drop(refused);
            self.shared
                .record_finding(failure(BoundaryFailureKind::RequestIdentityLimit, "/input"));
            self.shared.settle(Err(InvocationError::execution_failed()));
        }
        if notify || result.is_ready() {
            self.shared.signal.notify();
        }
        result
    }
    /// Decline further input, release unread values, and schedule InputClosed.
    pub fn close(&mut self) {
        self.waiter.clear();
        self.request = None;
        self.shared.close_input(true);
    }
}
impl Stream for ProviderInput {
    type Item = JsonValue;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<JsonValue>> {
        let this = self.get_mut();
        this.waiter.register(cx.waker());
        let result = this.receive();
        if result.is_ready() {
            this.waiter.clear();
        }
        result
    }
}
impl Drop for ProviderInput {
    fn drop(&mut self) {
        self.close();
    }
}

/// Sole provider emitter. Acknowledged validation is still only an offer until
/// this live send future commits it and returns Ready in the same poll.
pub struct ProviderOutput {
    shared: Arc<Shared>,
}
impl fmt::Debug for ProviderOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderOutput").finish_non_exhaustive()
    }
}
struct OutputOffer {
    shared: Arc<Shared>,
    id: Option<RequestId>,
}
impl Drop for OutputOffer {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let released = {
                let mut state = self.shared.state.lock().unwrap();
                state.slots[1].as_mut().and_then(|slot| slot.withdraw(id))
            };
            drop(released);
            // Withdrawing Checking returns no value but still changes state.
            self.shared.notify();
        }
    }
}
impl ProviderOutput {
    /// Commit one output. Dropping the future before Ready withdraws its offer;
    /// no value from that send can later be committed by the validation driver.
    ///
    /// # Errors
    /// Returns the invocation's terminal failure on rejection, cancellation,
    /// oversized output or prior failure; after success it returns ERR_CANCELLED.
    pub async fn send(&mut self, value: JsonValue) -> Result<(), InvocationError> {
        let oversized = self
            .shared
            .limits
            .max_value_bytes
            .is_some_and(|maximum| value.bytes().len() > maximum);
        let mut value = Some(value);
        let mut offer = OutputOffer {
            shared: self.shared.clone(),
            id: None,
        };
        let mut waiter = Waiter::new(self.shared.provider_write.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            let mut notify = false;
            let mut invalid = false;
            let mut refused = None;
            let result = {
                let mut state = self.shared.state.lock().unwrap();
                if state.terminal.is_some() {
                    let (credit, wake) = Shared::credit(&mut state);
                    notify |= wake;
                    if credit {
                        Poll::Ready(Err(state
                            .terminal
                            .as_ref()
                            .unwrap()
                            .clone()
                            .err()
                            .unwrap_or_else(InvocationError::cancelled)))
                    } else {
                        Poll::Pending
                    }
                } else if oversized {
                    let (credit, wake) = Shared::credit(&mut state);
                    notify |= wake;
                    invalid = credit;
                    Poll::Pending
                } else if let Some(id) = offer.id {
                    if state.output.len() < self.shared.limits.output_capacity
                        && state.slots[1]
                            .as_ref()
                            .is_some_and(|slot| slot.accepted(id))
                    {
                        let (credit, wake) = Shared::credit(&mut state);
                        notify |= wake;
                        if credit {
                            let accepted = state.slots[1]
                                .as_mut()
                                .and_then(|slot| slot.take_accepted(id))
                                .expect("accepted offer");
                            state.output.push_back(accepted);
                            state.emitted = true;
                            offer.id = None;
                            Poll::Ready(Ok(()))
                        } else {
                            Poll::Pending
                        }
                    } else {
                        Poll::Pending
                    }
                } else if state.slots[1].is_some()
                    || state.output.len() < self.shared.limits.output_capacity
                {
                    let (credit, wake) = Shared::credit(&mut state);
                    notify |= wake;
                    if !credit {
                        Poll::Pending
                    } else if let Some(slot) = &mut state.slots[1] {
                        match slot.offer(value.take().expect("uncommitted value")) {
                            Ok(id) => {
                                offer.id = Some(id);
                                notify = true;
                            }
                            Err(
                                OfferFailure::Busy(value)
                                | OfferFailure::Closed(value)
                                | OfferFailure::Exhausted(value),
                            ) => {
                                refused = Some(value);
                            }
                        }
                        Poll::Pending
                    } else {
                        state
                            .output
                            .push_back(value.take().expect("uncommitted value"));
                        state.emitted = true;
                        Poll::Ready(Ok(()))
                    }
                } else {
                    Poll::Pending
                }
            };
            if invalid || refused.is_some() {
                drop(refused);
                self.shared.record_finding(failure(
                    if invalid {
                        BoundaryFailureKind::OutputResourceLimit
                    } else {
                        BoundaryFailureKind::RequestIdentityLimit
                    },
                    "/output",
                ));
                self.shared.settle(Err(InvocationError::execution_failed()));
            }
            if notify || result.is_ready() {
                self.shared.signal.notify();
            }
            result
        })
        .await
    }
}
