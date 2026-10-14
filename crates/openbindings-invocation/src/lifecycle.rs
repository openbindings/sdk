use crate::CancellationToken;
use crate::{
    BoundaryError, InvocationError,
    operation::{Gate, ValidationFailure},
};
use futures_core::Stream;
use openbindings::{JsonLimits, JsonValue};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    future::{Future, poll_fn},
    pin::Pin,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

type Outcome = Result<(), InvocationError>;

/// Finite handoff policy. Logical bytes do not bound retained JSON parent arenas.
#[derive(Clone, Copy, Debug)]
pub struct InvocationLimits {
    /// Maximum queued accepted inputs; one additional pending write may retain its value.
    pub input_capacity: usize,
    /// Maximum queued accepted outputs; one additional pending emission may retain its value.
    pub output_capacity: usize,
    /// Maximum exact token bytes for each input/output, excluding its retained parent arena.
    pub max_value_bytes: usize,
    /// Maximum encoded portable error (or advisory challenge) bytes.
    pub max_error_bytes: usize,
}
impl Default for InvocationLimits {
    fn default() -> Self {
        Self {
            input_capacity: 1,
            output_capacity: 4,
            max_value_bytes: 1024 * 1024,
            max_error_bytes: 64 * 1024,
        }
    }
}
/// Invalid finite resource policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitError;
impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("capacities and value limit must be positive; terminal limit must be at least 256 bytes")
    }
}
impl std::error::Error for LimitError {}
impl InvocationLimits {
    pub(crate) fn validate(self) -> Result<Self, LimitError> {
        if self.input_capacity == 0
            || self.output_capacity == 0
            || self.max_value_bytes == 0
            || self.max_error_bytes < 256
        {
            Err(LimitError)
        } else {
            Ok(self)
        }
    }
}
/// A local write result, never the authoritative invocation outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError {
    /// Input stopped accepting values. Read the output terminal for its outcome.
    Closed,
    /// This submission exceeded the configured token-byte limit and was not accepted.
    ValueTooLarge,
}
impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Closed => "input is closed; read outputs through terminal",
            Self::ValueTooLarge => "value exceeds the handoff byte limit",
        })
    }
}
impl std::error::Error for WriteError {}

#[derive(Default)]
struct Signal {
    next: AtomicUsize,
    generation: AtomicUsize,
    waiters: Mutex<HashMap<usize, Arc<Waker>>>,
}
impl Signal {
    fn notify(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let waiters: Vec<_> = self.waiters.lock().unwrap().values().cloned().collect();
        for w in waiters {
            w.wake_by_ref();
        }
    }
}
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.notify();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}
struct Waiter {
    signal: Arc<Signal>,
    id: Option<usize>,
}
impl Waiter {
    fn new(signal: Arc<Signal>) -> Self {
        Self { signal, id: None }
    }
    fn clear(&mut self) {
        if let Some(id) = self.id.take() {
            let removed = self.signal.waiters.lock().unwrap().remove(&id);
            drop(removed); // A Waker destructor can reenter just like wake().
        }
    }
    fn register(&mut self, waker: &Waker) {
        let id = {
            let waiters = self.signal.waiters.lock().unwrap();
            let id = *self.id.get_or_insert_with(|| {
                loop {
                    let id = self.signal.next.fetch_add(1, Ordering::Relaxed);
                    if !waiters.contains_key(&id) {
                        break id;
                    }
                }
            });
            if waiters.get(&id).is_some_and(|old| old.will_wake(waker)) {
                return;
            }
            id
        };
        let new = Arc::new(waker.clone()); // RawWaker clone may also reenter.
        let old = self.signal.waiters.lock().unwrap().insert(id, new);
        drop(old);
    }
}
impl Drop for Waiter {
    fn drop(&mut self) {
        self.clear();
    }
}
struct State {
    input: VecDeque<JsonValue>,
    output: VecDeque<JsonValue>,
    caller_closed: bool,
    provider_closed: bool,
    terminal: Option<Outcome>,
    finding: Option<BoundaryError>,
    validation_failure: Option<Arc<ValidationFailure>>,
    emitted: bool,
    abandoned: bool,
}
pub(crate) struct Shared {
    state: Mutex<State>,
    signal: Arc<Signal>,
    provider_read: Arc<Signal>,
    provider_write: Arc<Signal>,
    local: CancellationToken,
    limits: InvocationLimits,
}
impl Shared {
    pub(crate) fn new(limits: InvocationLimits) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                input: VecDeque::new(),
                output: VecDeque::new(),
                caller_closed: false,
                provider_closed: false,
                terminal: None,
                finding: None,
                validation_failure: None,
                emitted: false,
                abandoned: false,
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
    pub(crate) fn record_finding(&self, finding: BoundaryError) {
        let mut state = self.state.lock().unwrap();
        if state.terminal.is_none() {
            state.finding.get_or_insert(finding);
        }
    }
    fn boundary_error(&self) -> Option<BoundaryError> {
        self.state.lock().unwrap().finding
    }
    fn outcome(&self) -> Option<Outcome> {
        self.state.lock().unwrap().terminal.clone()
    }
    fn validation_failure(&self) -> Option<Arc<ValidationFailure>> {
        self.state.lock().unwrap().validation_failure.clone()
    }
    fn fail_validation(&self, error: InvocationError, failure: Option<Arc<ValidationFailure>>) {
        if self.settle_with_validation(Err(error), failure) {
            self.local.cancel();
            self.provider_read.notify();
            self.provider_write.notify();
            self.signal.notify();
        }
    }
    fn settle(&self, outcome: Outcome) -> bool {
        self.settle_with_validation(outcome, None)
    }
    fn settle_with_validation(
        &self,
        mut outcome: Outcome,
        failure: Option<Arc<ValidationFailure>>,
    ) -> bool {
        if self.state.lock().unwrap().terminal.is_some() {
            return false;
        }
        let mut finding = None;
        if let Err(error) = &outcome {
            let mut limits = JsonLimits::default();
            limits.max_bytes = self.limits.max_error_bytes;
            if error.to_json_with_limits(limits).is_err() {
                finding = Some(BoundaryError::new("terminal-resource-limit", "/error"));
                outcome = Err(InvocationError::execution_failed());
            }
        }
        {
            let mut s = self.state.lock().unwrap();
            if s.terminal.is_some() {
                return false;
            }
            if let Some(finding) = finding {
                s.finding.get_or_insert(finding);
            }
            if s.emitted
                && outcome
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.code() == "CONTEXT_REQUIRED")
            {
                s.finding
                    .get_or_insert(BoundaryError::new("challenge-after-output", "/error"));
                outcome = Err(InvocationError::execution_failed());
            }
            s.validation_failure = failure;
            s.terminal = Some(outcome);
            s.provider_closed = true;
            s.input.clear();
            true
        }
    }
    fn close_input(&self, provider: bool) {
        {
            let mut s = self.state.lock().unwrap();
            if provider {
                s.provider_closed = true;
                s.input.clear();
            } else {
                s.caller_closed = true;
            }
        }
        self.provider_read.notify();
        self.signal.notify();
    }
}
struct Slot<F> {
    future: Option<Pin<Box<F>>>,
    polling: bool,
    stopped: bool,
}
struct Engine<F> {
    shared: Arc<Shared>,
    slot: Mutex<Slot<F>>,
}
impl<F: Future<Output = Outcome>> Engine<F> {
    fn drive(&self) {
        let mut future = {
            let mut slot = self.slot.lock().unwrap();
            if slot.stopped || slot.polling {
                return;
            }
            let Some(future) = slot.future.take() else {
                return;
            };
            slot.polling = true;
            future
        };
        let generation = self.shared.signal.generation.load(Ordering::Acquire);
        let waker = Waker::from(self.shared.signal.clone());
        // No locks are held while polling provider code or invoking its wakers.
        let result = if self.shared.outcome().is_some() {
            Poll::Pending
        } else {
            future.as_mut().poll(&mut Context::from_waker(&waker))
        };
        if let Poll::Ready(outcome) = result {
            self.shared.settle(outcome);
        }
        let stopped = self.shared.outcome().is_some();
        {
            let mut slot = self.slot.lock().unwrap();
            slot.polling = false;
            if stopped {
                slot.stopped = true;
            }
            if !slot.stopped {
                slot.future = Some(future);
            }
            // Otherwise drop outside the lock below.
            else {
                drop(slot);
                drop(future);
            }
        }
        if stopped || self.shared.signal.generation.load(Ordering::Acquire) != generation {
            self.shared.signal.notify();
        }
    }
    fn cancel(&self, abandon: bool) {
        let changed = self.shared.settle(Err(InvocationError::cancelled()));
        if abandon {
            let mut s = self.shared.state.lock().unwrap();
            s.abandoned = true;
            s.output.clear();
        }
        if changed || abandon {
            let future = {
                let mut slot = self.slot.lock().unwrap();
                slot.stopped = true;
                slot.future.take()
            };
            self.shared.local.cancel();
            drop(future);
            self.shared.provider_read.notify();
            self.shared.provider_write.notify();
            self.shared.signal.notify();
        }
    }
}

/// One lazy invocation; polling either half drives its provider job.
/// Merely retaining a handle does not schedule work. Dropping outputs abandons
/// active work; dropping input only half-closes input.
pub struct Invocation<F: Future<Output = Outcome>> {
    input: Input<F>,
    outputs: Outputs<F>,
}
impl<F: Future<Output = Outcome>> Invocation<F> {
    pub(crate) fn new(shared: Arc<Shared>, future: F) -> Self {
        let engine = Arc::new(Engine {
            shared,
            slot: Mutex::new(Slot {
                future: Some(Box::pin(future)),
                polling: false,
                stopped: false,
            }),
        });
        Self {
            input: Input {
                engine: engine.clone(),
            },
            outputs: Outputs {
                waiter: Waiter::new(engine.shared.signal.clone()),
                engine,
                exhausted: false,
            },
        }
    }
    /// Consume the handle into the sole input and output owners.
    pub fn split(self) -> (Input<F>, Outputs<F>) {
        (self.input, self.outputs)
    }
    /// Obtain independent cancellation/outcome observation for this borrowed lifetime.
    pub fn control(&self) -> Control<F> {
        self.outputs.control()
    }
    /// Accept one value into the bounded SDK handoff. This is not dispatch acknowledgement.
    pub async fn write(&mut self, value: JsonValue) -> Result<(), WriteError> {
        self.input.write(value).await
    }
    /// Gracefully stop new writes; accepted inputs remain available to the provider.
    pub fn close(&mut self) {
        self.input.close();
    }
    /// Read values through one terminal failure or normal exhaustion.
    pub async fn next(&mut self) -> Option<Result<JsonValue, InvocationError>> {
        self.outputs.next().await
    }
    /// Owned terminal snapshot, independent of whether queued output was read.
    /// `None` means unsettled; output receipt by itself is not terminal success.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outputs.outcome()
    }
    /// Value-free local finding when checked admission or a provider result failed.
    /// Bounded value-free local finding; never a portable native-cause field.
    pub fn boundary_error(&self) -> Option<BoundaryError> {
        self.outputs.boundary_error()
    }
    /// Local operation-validation evidence for the winning terminal failure.
    /// Binding-only invocation returns None. Evidence is never portable error data.
    pub fn validation_failure(&self) -> Option<Arc<ValidationFailure>> {
        self.outputs.validation_failure()
    }
}
/// Independent cancellation/outcome observation. Its weak driver reference does
/// not keep provider work alive after the output owner is released.
/// Its Rust lifetime still follows the borrowed invocation. Retain the owned
/// [`Self::outcome`] snapshot to use the result after those borrows end.
pub struct Control<F: Future<Output = Outcome>> {
    engine: Weak<Engine<F>>,
    shared: Arc<Shared>,
}
impl<F: Future<Output = Outcome>> Clone for Control<F> {
    fn clone(&self) -> Self {
        Self {
            engine: self.engine.clone(),
            shared: self.shared.clone(),
        }
    }
}
impl<F: Future<Output = Outcome>> Control<F> {
    /// Settle cancellation promptly; previously accepted outputs remain ordered before it.
    /// This does not undo external effects or prove detached work has stopped.
    pub fn cancel(&self) {
        if let Some(engine) = self.engine.upgrade() {
            engine.cancel(false);
        }
    }
    /// Owned terminal snapshot, independent of whether queued output was read.
    /// `None` means unsettled; output receipt by itself is not terminal success.
    pub fn outcome(&self) -> Option<Outcome> {
        self.shared.outcome()
    }
    /// Bounded value-free local finding; never a portable native-cause field.
    pub fn boundary_error(&self) -> Option<BoundaryError> {
        self.shared.boundary_error()
    }
    /// Local operation-validation evidence for the winning terminal failure.
    /// Binding-only invocation returns None. Evidence is never portable error data.
    pub fn validation_failure(&self) -> Option<Arc<ValidationFailure>> {
        self.shared.validation_failure()
    }
    /// Whether either direction has ended input acceptance. This is nonterminal.
    pub fn input_is_closed(&self) -> bool {
        let s = self.shared.state.lock().unwrap();
        s.caller_closed || s.provider_closed
    }
}
/// Single input owner. Drop is a graceful half-close, not cancellation.
pub struct Input<F: Future<Output = Outcome>> {
    engine: Arc<Engine<F>>,
}
impl<F: Future<Output = Outcome>> Input<F> {
    /// Accept a value into the SDK handoff when capacity permits. Dropping a
    /// pending write does not admit it. Success does not acknowledge dispatch.
    pub async fn write(&mut self, value: JsonValue) -> Result<(), WriteError> {
        if value.bytes().len() > self.engine.shared.limits.max_value_bytes {
            return Err(WriteError::ValueTooLarge);
        }
        let mut value = Some(value);
        let mut waiter = Waiter::new(self.engine.shared.signal.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            self.engine.drive();
            let result = {
                let mut s = self.engine.shared.state.lock().unwrap();
                if s.caller_closed || s.provider_closed {
                    Poll::Ready(Err(WriteError::Closed))
                } else if s.input.len() < self.engine.shared.limits.input_capacity {
                    s.input
                        .push_back(value.take().expect("polled completed write"));
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            };
            if result.is_ready() {
                self.engine.shared.provider_read.notify();
                self.engine.shared.signal.notify();
            }
            result
        })
        .await
    }
    /// Graceful, idempotent half-close; accepted inputs may still drain.
    pub fn close(&mut self) {
        self.engine.shared.close_input(false);
    }
    /// Observe caller/provider input closure without driving work.
    pub fn is_closed(&self) -> bool {
        let s = self.engine.shared.state.lock().unwrap();
        s.caller_closed || s.provider_closed
    }
    /// Resolve when either caller or provider closes input. Polling drives the job.
    pub async fn closed(&mut self) {
        let mut waiter = Waiter::new(self.engine.shared.signal.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            self.engine.drive();
            if self.is_closed() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await
    }
}
impl<F: Future<Output = Outcome>> Drop for Input<F> {
    fn drop(&mut self) {
        self.engine.shared.close_input(false);
    }
}
/// Single output owner. Values accepted before settlement precede its terminal.
/// Drop before settlement cancels and clears the queue. Stream is fused after terminal.
pub struct Outputs<F: Future<Output = Outcome>> {
    engine: Arc<Engine<F>>,
    waiter: Waiter,
    exhausted: bool,
}
impl<F: Future<Output = Outcome>> Outputs<F> {
    /// Obtain independent cancellation/outcome observation for this borrowed lifetime.
    pub fn control(&self) -> Control<F> {
        Control {
            engine: Arc::downgrade(&self.engine),
            shared: self.engine.shared.clone(),
        }
    }
    /// Owned terminal snapshot, independent of whether queued output was read.
    /// `None` means unsettled; output receipt by itself is not terminal success.
    pub fn outcome(&self) -> Option<Outcome> {
        self.engine.shared.outcome()
    }
    /// Bounded value-free local finding; never a portable native-cause field.
    pub fn boundary_error(&self) -> Option<BoundaryError> {
        self.engine.shared.boundary_error()
    }
    /// Local operation-validation evidence for the winning terminal failure.
    /// Binding-only invocation returns None. Evidence is never portable error data.
    pub fn validation_failure(&self) -> Option<Arc<ValidationFailure>> {
        self.engine.shared.validation_failure()
    }
    /// Await one value or terminal failure. Subsequent reads are exhausted.
    /// Dropping a pending read consumes nothing and releases its task waker.
    #[allow(clippy::should_implement_trait)] // Async companion to Stream::poll_next.
    pub fn next(
        &mut self,
    ) -> impl Future<Output = Option<Result<JsonValue, InvocationError>>> + '_ {
        struct Read<'a, F: Future<Output = Outcome>>(&'a mut Outputs<F>);
        impl<F: Future<Output = Outcome>> Future for Read<'_, F> {
            type Output = Option<Result<JsonValue, InvocationError>>;
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                Pin::new(&mut *self.get_mut().0).poll_next(cx)
            }
        }
        impl<F: Future<Output = Outcome>> Drop for Read<'_, F> {
            fn drop(&mut self) {
                self.0.waiter.clear();
            }
        }
        Read(self)
    }
}
impl<F: Future<Output = Outcome>> Stream for Outputs<F> {
    type Item = Result<JsonValue, InvocationError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.exhausted {
            return Poll::Ready(None);
        }
        this.waiter.register(cx.waker());
        this.engine.drive();
        let result = {
            let mut s = this.engine.shared.state.lock().unwrap();
            if let Some(value) = s.output.pop_front() {
                Poll::Ready(Some(Ok(value)))
            } else if let Some(outcome) = &s.terminal {
                this.exhausted = true;
                Poll::Ready(outcome.as_ref().err().cloned().map(Err))
            } else {
                Poll::Pending
            }
        };
        if result.is_ready() {
            this.waiter.clear();
            this.engine.shared.provider_write.notify();
        }
        result
    }
}
impl<F: Future<Output = Outcome>> futures_core::stream::FusedStream for Outputs<F> {
    fn is_terminated(&self) -> bool {
        self.exhausted
    }
}
impl<F: Future<Output = Outcome>> Drop for Outputs<F> {
    fn drop(&mut self) {
        self.engine.cancel(true);
    }
}

/// Provider endpoints. Split for concurrent bidi work without cloning producers.
pub struct ProviderIo {
    input: ProviderInput,
    output: ProviderOutput,
}
impl ProviderIo {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        Self::with_gates(shared, None, None)
    }
    pub(crate) fn with_gates(
        shared: Arc<Shared>,
        input: Option<Gate>,
        output: Option<Gate>,
    ) -> Self {
        Self {
            input: ProviderInput {
                shared: shared.clone(),
                gate: input,
                failed: false,
            },
            output: ProviderOutput {
                shared,
                gate: output,
            },
        }
    }
    /// Take the single reader and emitter for independently polled bidi work.
    pub fn split(self) -> (ProviderInput, ProviderOutput) {
        (self.input, self.output)
    }
    /// Receive one accepted input, or None after input closes and drains.
    pub async fn next(&mut self) -> Option<JsonValue> {
        self.input.next().await
    }
    /// Commit one output with backpressure. Earlier committed outputs precede
    /// the terminal; an oversized value ends unsuccessfully without emission.
    pub async fn emit(&mut self, value: JsonValue) -> Result<(), InvocationError> {
        self.output.emit(value).await
    }
    /// End consumption, discarding unread input; output remains available.
    pub fn close_input(&mut self) {
        self.input.close();
    }
}
/// Provider input owner. Closing or dropping discards unread input and stops writes.
pub struct ProviderInput {
    shared: Arc<Shared>,
    gate: Option<Gate>,
    failed: bool,
}
impl ProviderInput {
    /// Receive one accepted input, or None after input closes and drains.
    pub async fn next(&mut self) -> Option<JsonValue> {
        let mut waiter = Waiter::new(self.shared.provider_read.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            if self.failed {
                return Poll::Pending;
            }
            let result = {
                let mut s = self.shared.state.lock().unwrap();
                if s.provider_closed {
                    Poll::Ready(None)
                } else if let Some(value) = s.input.pop_front() {
                    Poll::Ready(Some(value))
                } else if s.caller_closed {
                    Poll::Ready(None)
                } else {
                    Poll::Pending
                }
            };
            // Evaluator callbacks run without any lifecycle lock. A rejected
            // value is never a clean EOF: suspend this job so the driver can
            // drop it on its now-settled terminal before provider code resumes.
            if let Poll::Ready(Some(value)) = &result {
                if let Some(gate) = &mut self.gate
                    && let Err((error, failure)) = gate.check(value, &self.shared.local)
                {
                    self.failed = true;
                    self.shared.fail_validation(error, failure);
                    return Poll::Pending;
                }
                if self.shared.outcome().is_some() {
                    return Poll::Pending;
                }
            }
            if result.is_ready() {
                self.shared.signal.notify();
            }
            result
        })
        .await
    }
    /// Stop input delivery and discard unread queued inputs.
    pub fn close(&mut self) {
        self.shared.close_input(true);
    }
}
impl Drop for ProviderInput {
    fn drop(&mut self) {
        self.shared.close_input(true);
    }
}
/// Provider output owner. Each awaited emit admits at most one exact value.
pub struct ProviderOutput {
    shared: Arc<Shared>,
    gate: Option<Gate>,
}
impl ProviderOutput {
    /// Commit one output with backpressure. Earlier committed outputs precede
    /// the terminal; an oversized value ends unsuccessfully without emission.
    pub async fn emit(&mut self, value: JsonValue) -> Result<(), InvocationError> {
        if value.bytes().len() > self.shared.limits.max_value_bytes {
            self.shared
                .record_finding(BoundaryError::new("output-resource-limit", "/output"));
            self.shared.settle(Err(InvocationError::execution_failed()));
            self.shared.signal.notify();
            return Err(InvocationError::execution_failed());
        }
        if self.shared.outcome().is_some() {
            return Err(InvocationError::cancelled());
        }
        if let Some(gate) = &mut self.gate
            && let Err((error, failure)) = gate.check(&value, &self.shared.local)
        {
            self.shared.fail_validation(error.clone(), failure);
            return Err(error);
        }
        let mut value = Some(value);
        let mut waiter = Waiter::new(self.shared.provider_write.clone());
        poll_fn(|cx| {
            waiter.register(cx.waker());
            let result = {
                let mut s = self.shared.state.lock().unwrap();
                if s.terminal.is_some() || s.abandoned {
                    Poll::Ready(Err(InvocationError::cancelled()))
                } else if s.output.len() < self.shared.limits.output_capacity {
                    s.emitted = true;
                    s.output
                        .push_back(value.take().expect("polled completed emission"));
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            };
            if result.is_ready() {
                self.shared.signal.notify();
            }
            result
        })
        .await
    }
}

#[cfg(test)]
mod driver_tests {
    use super::*;
    use std::{future::pending, pin::pin, sync::Barrier};
    fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(Waker::noop()))
    }
    fn json(s: &str) -> JsonValue {
        JsonValue::parse(s).unwrap()
    }

    struct DropReentrant {
        signal: Weak<Signal>,
        drops: Arc<AtomicUsize>,
    }
    #[allow(clippy::manual_noop_waker)] // Its destructor is the tested callback.
    impl Wake for DropReentrant {
        fn wake(self: Arc<Self>) {}
    }
    impl Drop for DropReentrant {
        fn drop(&mut self) {
            if let Some(signal) = self.signal.upgrade() {
                assert!(
                    signal.waiters.try_lock().is_ok(),
                    "Waker dropped under signal lock"
                );
                signal.notify();
            }
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn last_waker_drop_and_replacement_can_reenter_signal() {
        for operation in ["clear", "drop", "replace"] {
            let signal = Arc::new(Signal::default());
            let drops = Arc::new(AtomicUsize::new(0));
            let waker = Waker::from(Arc::new(DropReentrant {
                signal: Arc::downgrade(&signal),
                drops: drops.clone(),
            }));
            let mut waiter = Waiter::new(signal);
            waiter.register(&waker);
            drop(waker);
            match operation {
                "clear" => waiter.clear(),
                "drop" => drop(waiter),
                "replace" => waiter.register(Waker::noop()),
                _ => unreachable!(),
            }
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }
    #[test]
    fn losing_terminal_cannot_change_the_settled_diagnostic() {
        let shared = Shared::new(InvocationLimits::default());
        assert!(shared.settle(Err(InvocationError::cancelled())));
        assert!(!shared.settle(Err(
            InvocationError::new("x".repeat(100_000), None).unwrap()
        )));
        shared.record_finding(BoundaryError::new("late-finding", ""));
        assert!(shared.boundary_error().is_none());
        assert_eq!(
            shared.outcome().unwrap().unwrap_err().code(),
            "ERR_CANCELLED"
        );
    }
    #[test]
    fn finite_slots_and_scoped_pending_waiters() {
        let shared = Shared::new(InvocationLimits::default());
        let mut io = ProviderIo::new(shared.clone());
        let future = async move {
            for _ in 0..5 {
                io.emit(json("0")).await?;
            }
            pending::<()>().await;
            Ok(())
        };
        let (mut input, mut output) = Invocation::new(shared.clone(), future).split();
        {
            let mut write = pin!(input.write(json("1")));
            assert!(matches!(poll(write.as_mut()), Poll::Ready(Ok(()))));
        }
        for _ in 0..2000 {
            let mut write = pin!(input.write(json("2")));
            assert!(poll(write.as_mut()).is_pending());
        }
        assert!(shared.signal.waiters.lock().unwrap().is_empty());
        assert_eq!(shared.provider_write.waiters.lock().unwrap().len(), 1);
        {
            let s = shared.state.lock().unwrap();
            assert_eq!(s.input.len(), 1);
            assert_eq!(s.output.len(), 4);
        }
        // One pending provider value, one possible pending caller value; mutable
        // endpoint borrowing prevents concurrent submissions on either owner.
        output.control().cancel();
        assert!(shared.provider_write.waiters.lock().unwrap().is_empty());
        {
            let s = shared.state.lock().unwrap();
            assert!(s.input.is_empty());
            assert_eq!(s.output.len(), 4);
        }
        drop(input);
        for _ in 0..4 {
            let mut next = pin!(output.next());
            assert!(matches!(poll(next.as_mut()), Poll::Ready(Some(Ok(_)))));
        }
        {
            let mut next = pin!(output.next());
            assert!(matches!(poll(next.as_mut()), Poll::Ready(Some(Err(_)))));
        }
        assert!(shared.signal.waiters.lock().unwrap().is_empty());
        drop(output);
        assert!(shared.state.lock().unwrap().output.is_empty());
    }
    #[test]
    fn dropping_pending_reads_releases_only_their_waiters() {
        let shared = Shared::new(InvocationLimits::default());
        let io = ProviderIo::new(shared.clone());
        let future = async move {
            let _io = io;
            pending::<()>().await;
            Ok(())
        };
        let (_, mut output) = Invocation::new(shared.clone(), future).split();
        for _ in 0..2000 {
            let mut next = pin!(output.next());
            assert!(poll(next.as_mut()).is_pending());
        }
        assert!(shared.signal.waiters.lock().unwrap().is_empty());
    }
    struct WakeCount(AtomicUsize);
    impl Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn provider_endpoints_notify_their_own_task_wakers() {
        let shared = Shared::new(InvocationLimits::default());
        let (mut reader, mut writer) = ProviderIo::new(shared.clone()).split();
        let job = async {
            pending::<()>().await;
            Ok(())
        };
        let (mut input, mut output) = Invocation::new(shared, job).split();
        let reads = Arc::new(WakeCount(AtomicUsize::new(0)));
        let read_waker = Waker::from(reads.clone());
        {
            let mut next = pin!(reader.next());
            assert!(
                next.as_mut()
                    .poll(&mut Context::from_waker(&read_waker))
                    .is_pending()
            );
            let mut write = pin!(input.write(json("7")));
            assert!(matches!(poll(write.as_mut()), Poll::Ready(Ok(()))));
            assert!(reads.0.load(Ordering::SeqCst) > 0);
            assert!(matches!(poll(next.as_mut()), Poll::Ready(Some(_))));
        }
        for _ in 0..4 {
            let mut emit = pin!(writer.emit(json("8")));
            assert!(matches!(poll(emit.as_mut()), Poll::Ready(Ok(()))));
        }
        let writes = Arc::new(WakeCount(AtomicUsize::new(0)));
        let write_waker = Waker::from(writes.clone());
        let mut emit = pin!(writer.emit(json("9")));
        assert!(
            emit.as_mut()
                .poll(&mut Context::from_waker(&write_waker))
                .is_pending()
        );
        let mut next = pin!(output.next());
        assert!(matches!(poll(next.as_mut()), Poll::Ready(Some(Ok(_)))));
        assert!(writes.0.load(Ordering::SeqCst) > 0);
        assert!(matches!(poll(emit.as_mut()), Poll::Ready(Ok(()))));
    }
    struct Reenter<F: Future<Output = Outcome> + Send + 'static> {
        engine: Weak<Engine<F>>,
        calls: AtomicUsize,
    }
    impl<F: Future<Output = Outcome> + Send + 'static> Wake for Reenter<F> {
        fn wake(self: Arc<Self>) {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(n < 1000, "recursive wake loop");
            if let Some(engine) = self.engine.upgrade() {
                engine.drive();
            }
        }
    }
    #[test]
    fn synchronous_reentrant_wakers_do_not_poll_twice_or_deadlock() {
        let shared = Shared::new(InvocationLimits::default());
        let mut io = ProviderIo::new(shared.clone());
        let job = async move {
            for _ in 0..8 {
                io.emit(json("1")).await?;
            }
            Ok(())
        };
        let invocation = Invocation::new(shared.clone(), job);
        let engine = invocation.input.engine.clone();
        let reenter = Arc::new(Reenter {
            engine: Arc::downgrade(&engine),
            calls: AtomicUsize::new(0),
        });
        let mut waiter = Waiter::new(shared.signal.clone());
        waiter.register(&Waker::from(reenter.clone()));
        engine.drive();
        assert_eq!(shared.state.lock().unwrap().output.len(), 4);
        for _ in 0..4 {
            shared.state.lock().unwrap().output.pop_front();
            shared.provider_write.notify();
        }
        assert!(shared.outcome().unwrap().is_ok());
        assert!(reenter.calls.load(Ordering::SeqCst) > 0);
        drop(waiter);
        drop(invocation);
    }
    #[test]
    fn cancel_during_a_concurrent_provider_poll_wins_and_prevents_late_completion() {
        let shared = Shared::new(InvocationLimits::default());
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let job = {
            let entered = entered.clone();
            let release = release.clone();
            async move {
                entered.wait();
                release.wait();
                Err(InvocationError::new("late.error", None).unwrap())
            }
        };
        let invocation = Invocation::new(shared.clone(), job);
        let engine = invocation.input.engine.clone();
        let other = engine.clone();
        std::thread::scope(|scope| {
            let task = scope.spawn(move || other.drive());
            entered.wait();
            engine.cancel(false);
            assert_eq!(
                shared.outcome().unwrap().unwrap_err().code(),
                "ERR_CANCELLED"
            );
            release.wait();
            task.join().unwrap();
        });
        assert_eq!(
            shared.outcome().unwrap().unwrap_err().code(),
            "ERR_CANCELLED"
        );
        assert!(engine.slot.lock().unwrap().future.is_none());
    }
}
