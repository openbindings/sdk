//! Synthetic single-thread allocation/timing probe through public SDK APIs.
//! The only unsafe code forwards allocator contracts unchanged to System.
#![deny(unsafe_op_in_unsafe_fn)]

use openbindings::{ConformantDocument, Document, ExternalSchemas, JsonValue};
use openbindings_invocation::CancellationToken;
use openbindings_invocation::{
    BindingInvoker, BindingInvokerExt, BindingRequest, DefaultOperationInvoker, InvocationError,
    InvocationLimits, InvocationSupport, OperationRequest, OutputEvent, PreflightOutcome,
    ProviderIo, WriteError,
};
use serde::Serialize;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    future::{Future, Ready, pending, poll_fn, ready},
    hint::black_box,
    pin::{Pin, pin},
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::Instant,
};

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED: AtomicU64 = AtomicU64::new(0);
static FREED: AtomicU64 = AtomicU64::new(0);
static LIVE_COUNT: AtomicI64 = AtomicI64::new(0);
static LIVE_BYTES: AtomicI64 = AtomicI64::new(0);
static PEAK_BYTES: AtomicI64 = AtomicI64::new(0);
static MAX_ALLOCATION: AtomicU64 = AtomicU64::new(0);

struct CountingSystem;

fn acquired(bytes: usize, count: bool) {
    if count {
        LIVE_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    ALLOCATED.fetch_add(bytes as u64, Ordering::Relaxed);
    MAX_ALLOCATION.fetch_max(bytes as u64, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(bytes as i64, Ordering::Relaxed) + bytes as i64;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

fn released(bytes: usize, count: bool) {
    if count {
        LIVE_COUNT.fetch_sub(1, Ordering::Relaxed);
    }
    FREED.fetch_add(bytes as u64, Ordering::Relaxed);
    LIVE_BYTES.fetch_sub(bytes as i64, Ordering::Relaxed);
}

// SAFETY: Every operation forwards the original pointer/layout/size unchanged
// to System. Bookkeeping uses only allocation-free atomics and never dereferences
// the allocation. Failed allocation/reallocation leaves ownership unchanged.
unsafe impl GlobalAlloc for CountingSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller's GlobalAlloc contract is passed unchanged to System.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            acquired(layout.size(), true);
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller's GlobalAlloc contract is passed unchanged to System.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            acquired(layout.size(), true);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller guarantees this pointer/layout identify a live allocation.
        unsafe { System.dealloc(pointer, layout) };
        DEALLOCS.fetch_add(1, Ordering::Relaxed);
        released(layout.size(), true);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The original pointer/layout and requested size are forwarded unchanged.
        let result = unsafe { System.realloc(pointer, layout, new_size) };
        if !result.is_null() {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            released(layout.size(), false);
            acquired(new_size, false);
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountingSystem = CountingSystem;

#[derive(Clone, Copy)]
struct Snapshot {
    allocations: u64,
    reallocations: u64,
    deallocations: u64,
    allocated: u64,
    freed: u64,
    live: i64,
    bytes: i64,
}

impl Snapshot {
    fn now() -> Self {
        Self {
            allocations: ALLOCS.load(Ordering::Relaxed),
            reallocations: REALLOCS.load(Ordering::Relaxed),
            deallocations: DEALLOCS.load(Ordering::Relaxed),
            allocated: ALLOCATED.load(Ordering::Relaxed),
            freed: FREED.load(Ordering::Relaxed),
            live: LIVE_COUNT.load(Ordering::Relaxed),
            bytes: LIVE_BYTES.load(Ordering::Relaxed),
        }
    }
}

struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::park();
    }
}

async fn join<A: Future<Output = ()>, B: Future<Output = ()>>(a: A, b: B) {
    let mut a = pin!(a);
    let mut b = pin!(b);
    let mut done_a = false;
    let mut done_b = false;
    poll_fn(|cx| {
        if !done_a {
            done_a = a.as_mut().poll(cx).is_ready();
        }
        if !done_b {
            done_b = b.as_mut().poll(cx).is_ready();
        }
        if done_a && done_b {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

#[derive(Default)]
struct Activity {
    starts: AtomicUsize,
    live: AtomicUsize,
    peak: AtomicUsize,
}
struct Guard<'a>(&'a Activity);
impl Activity {
    fn enter(&self) -> Guard<'_> {
        self.starts.fetch_add(1, Ordering::Relaxed);
        let live = self.live.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak.fetch_max(live, Ordering::Relaxed);
        Guard(self)
    }
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Empty,
    Echo,
    Park,
}

struct Fixture {
    mode: Mode,
    activity: Activity,
}
impl Fixture {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            activity: Activity::default(),
        }
    }
    // Both checked and direct measurements use this same semantic operation.
    fn apply(&self, value: JsonValue) -> JsonValue {
        black_box(value)
    }
    fn direct(&self, value: Option<&JsonValue>, count: usize) {
        let _guard = self.activity.enter();
        if let Some(value) = value {
            for _ in 0..count {
                let output = self.apply(value.clone());
                assert_eq!(output.text(), value.text());
                drop(black_box(output));
            }
        }
    }
}
impl BindingInvoker for Fixture {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;
    fn handle_preflight<'a>(
        &'a self,
        _: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        ready(Ok(PreflightOutcome::NoPrediction))
    }
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        mut io: ProviderIo,
        _control: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            let _guard = self.activity.enter();
            black_box(
                request
                    .interface
                    .binding(&request.binding)
                    .unwrap()
                    .source()
                    .data()
                    .unwrap()
                    .bytes()
                    .len(),
            );
            match self.mode {
                Mode::Empty => {}
                Mode::Echo => {
                    while let Some(value) = io.next().await {
                        io.send(self.apply(value)).await?;
                    }
                }
                Mode::Park => {
                    io.close_input();
                    pending::<()>().await;
                }
            }
            Ok(())
        })
    }
}

impl InvocationSupport for Fixture {
    fn can_invoke(&self, kind: &str) -> bool {
        kind == "resource.fixture"
    }
}

// A current DefaultOperationInvoker requires an explicit evaluator. Absent contracts
// bypass it; the diagnostic lane deliberately asks an incapable evaluator.
struct RefusingEvaluator;
struct NeverPrepared;
impl openbindings::SchemaEvaluator for RefusingEvaluator {
    type Validator = NeverPrepared;
    fn prepare(
        &self,
        _: openbindings::EvaluationRequest<'_>,
        _: &openbindings::PreparationLimits,
        _: &dyn openbindings::WorkControl,
    ) -> Result<Self::Validator, openbindings::PreparationFailure> {
        Err(openbindings::PreparationFailure::new(
            openbindings::PreparationFailureKind::UnsupportedCapability {
                capability: "measurement.fixture".into(),
            },
        ))
    }
}
impl openbindings::SchemaValidator for NeverPrepared {
    fn evaluate(
        &self,
        _: &JsonValue,
        _: openbindings::Detail,
        _: &dyn openbindings::WorkControl,
    ) -> openbindings::ValueOutcome {
        unreachable!("preparation always refuses")
    }
}

fn document(data: &JsonValue) -> ConformantDocument {
    Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"echo":{{"aliases":["say"]}}}},"bindings":{{"b":{{"operation":"echo","source":"s"}}}},"sources":{{"s":{{"kind":"resource.fixture","data":{}}}}}}}"#, data.text()))
        .unwrap().validate().unwrap().conformant().unwrap()
}

fn zero(invoker: &Fixture, request: BindingRequest) {
    let call = invoker.invoke(request, CancellationToken::new()).unwrap();
    let (mut input, mut output) = call.split();
    input.close();
    block_on(async {
        let mut completed = false;
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Complete => completed = true,
                OutputEvent::InputClosed => {}
                _ => panic!("unexpected zero-value event: {event:?}"),
            }
        }
        assert!(completed);
    });
}

fn echo(invoker: &Fixture, request: BindingRequest, value: &JsonValue, count: usize) {
    echo_call(
        invoker.invoke(request, CancellationToken::new()).unwrap(),
        value,
        count,
    );
}
fn echo_call<F: Future<Output = Result<(), InvocationError>>>(
    call: openbindings_invocation::Invocation<F>,
    value: &JsonValue,
    count: usize,
) {
    let (mut input, mut outputs) = call.split();
    block_on(join(
        async {
            for _ in 0..count {
                input.send(value.clone()).await.unwrap();
            }
            input.close();
        },
        async {
            let mut received = 0;
            let mut completed = false;
            while let Some(event) = outputs.next().await {
                match event {
                    OutputEvent::Output(output) => {
                        assert_eq!(output.text(), value.text());
                        received += 1;
                        drop(black_box(output));
                    }
                    OutputEvent::Complete => completed = true,
                    OutputEvent::InputClosed => {}
                    _ => panic!("unexpected echo event: {event:?}"),
                }
            }
            assert_eq!(received, count);
            assert!(completed);
        },
    ));
}

fn cancel_call<F: Future<Output = Result<(), InvocationError>>>(
    call: openbindings_invocation::Invocation<F>,
    value: &JsonValue,
) {
    let control = call.control();
    let (mut input, mut output) = call.split();
    block_on(async {
        // Drive the provider until it explicitly declines input, then cancel
        // its pending work. A queued send alone does not drive the provider.
        assert!(matches!(
            output.next().await,
            Some(OutputEvent::InputClosed)
        ));
        assert_eq!(input.send(value.clone()).await, Err(WriteError::Closed));
        control.cancel();
        let Some(OutputEvent::Error(error)) = output.next().await else {
            panic!("missing cancellation")
        };
        assert_eq!(error.code(), "ERR_CANCELLED");
        assert!(output.next().await.is_none());
    });
}
fn cancelled(invoker: &Fixture, request: BindingRequest, value: &JsonValue) {
    cancel_call(
        invoker.invoke(request, CancellationToken::new()).unwrap(),
        value,
    );
}

#[derive(Serialize)]
struct Measurement {
    name: &'static str,
    iterations: usize,
    values: usize,
    elapsed_ns: u128,
    allocations: u64,
    reallocations: u64,
    deallocations: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
    live_allocation_delta: i64,
    live_byte_delta: i64,
    peak_additional_live_bytes: i64,
    max_single_allocation: u64,
}

fn measure(
    name: &'static str,
    iterations: usize,
    values_per_iteration: usize,
    mut job: impl FnMut(),
) -> Measurement {
    for _ in 0..128 {
        job();
    }
    let before = Snapshot::now();
    PEAK_BYTES.store(before.bytes, Ordering::Relaxed);
    MAX_ALLOCATION.store(0, Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..iterations {
        job();
    }
    let elapsed_ns = start.elapsed().as_nanos();
    let after = Snapshot::now();
    Measurement {
        name,
        iterations,
        values: iterations * values_per_iteration,
        elapsed_ns,
        allocations: after.allocations - before.allocations,
        reallocations: after.reallocations - before.reallocations,
        deallocations: after.deallocations - before.deallocations,
        allocated_bytes: after.allocated - before.allocated,
        freed_bytes: after.freed - before.freed,
        live_allocation_delta: after.live - before.live,
        live_byte_delta: after.bytes - before.bytes,
        peak_additional_live_bytes: PEAK_BYTES.load(Ordering::Relaxed) - before.bytes,
        max_single_allocation: MAX_ALLOCATION.load(Ordering::Relaxed),
    }
}

fn backing_value(bytes: usize) -> JsonValue {
    let prefix = r#"{"selected":0,"padding":""#;
    let suffix = "\"}";
    JsonValue::parse(format!(
        "{prefix}{}{suffix}",
        "x".repeat(bytes - prefix.len() - suffix.len())
    ))
    .unwrap()
}

#[derive(Serialize)]
struct Retention {
    completed: usize,
    cancelled: usize,
    completed_live_allocations_delta: i64,
    completed_live_bytes_delta: i64,
    cancelled_live_allocations_delta: i64,
    cancelled_live_bytes_delta: i64,
    all_backing_owners_released_allocations_delta: i64,
    all_backing_owners_released_bytes_delta: i64,
    live_provider_guards: usize,
}

fn retention() -> Retention {
    const ITERATIONS: usize = 10_000;
    // Warm every runtime path before establishing the lifetime accounting baseline.
    let before_owners = Snapshot::now();
    let (completed_delta, cancelled_delta, live_guards) = {
        let parent = backing_value(1024 * 1024);
        let subtree = parent.get("selected").unwrap().to_owned();
        let request = BindingRequest::new(document(&parent), "b");
        let complete = Fixture::new(Mode::Echo);
        let cancel = Fixture::new(Mode::Park);
        let complete_invoker = &complete;
        let cancel_invoker = &cancel;
        for _ in 0..128 {
            echo(&complete_invoker, request.clone(), &subtree, 1);
            cancelled(&cancel_invoker, request.clone(), &subtree);
        }
        let before_complete = Snapshot::now();
        for _ in 0..ITERATIONS {
            echo(&complete_invoker, request.clone(), &subtree, 1);
        }
        let after_complete = Snapshot::now();
        for _ in 0..ITERATIONS {
            cancelled(&cancel_invoker, request.clone(), &subtree);
        }
        let after_cancel = Snapshot::now();
        assert_eq!(
            complete.activity.starts.load(Ordering::Relaxed),
            ITERATIONS + 128
        );
        assert_eq!(
            cancel.activity.starts.load(Ordering::Relaxed),
            ITERATIONS + 128
        );
        (
            (
                after_complete.live - before_complete.live,
                after_complete.bytes - before_complete.bytes,
            ),
            (
                after_cancel.live - after_complete.live,
                after_cancel.bytes - after_complete.bytes,
            ),
            complete.activity.live.load(Ordering::Relaxed)
                + cancel.activity.live.load(Ordering::Relaxed),
        )
    };
    let after_owners = Snapshot::now();
    Retention {
        completed: ITERATIONS,
        cancelled: ITERATIONS,
        completed_live_allocations_delta: completed_delta.0,
        completed_live_bytes_delta: completed_delta.1,
        cancelled_live_allocations_delta: cancelled_delta.0,
        cancelled_live_bytes_delta: cancelled_delta.1,
        all_backing_owners_released_allocations_delta: after_owners.live - before_owners.live,
        all_backing_owners_released_bytes_delta: after_owners.bytes - before_owners.bytes,
        live_provider_guards: live_guards,
    }
}

#[derive(Serialize)]
struct Receipt {
    format: &'static str,
    scope: &'static str,
    counting: &'static str,
    input_capacity: usize,
    output_capacity: usize,
    source_logical_bytes: usize,
    subtree_logical_bytes: usize,
    subtree_backing_bytes: usize,
    stream_values_per_call: usize,
    measurements: Vec<Measurement>,
    retention: Retention,
    provider_live_guards: usize,
    provider_peak_guards: usize,
}

fn main() {
    let iterations = 10_000;
    let parent = backing_value(1024 * 1024);
    let subtree = parent.get("selected").unwrap().to_owned();
    let small = JsonValue::parse("0").unwrap();
    let large = JsonValue::parse(format!("\"{}\"", "x".repeat(64 * 1024 - 2))).unwrap();
    assert_eq!(parent.bytes().len(), 1024 * 1024);
    assert_eq!(large.bytes().len(), 64 * 1024);
    let proof = document(&parent);
    let request = BindingRequest::new(proof.clone(), "b");
    let empty = Fixture::new(Mode::Empty);
    let fixture = Fixture::new(Mode::Echo);
    let parked = Fixture::new(Mode::Park);
    let empty_invoker = &empty;
    let invoker = &fixture;
    let parked_invoker = &parked;
    let mut measurements = Vec::with_capacity(14);
    measurements.push(measure("direct_zero", iterations, 0, || {
        empty.direct(None, 0)
    }));
    measurements.push(measure("handle_creation_release", iterations, 0, || {
        drop(black_box(
            invoker
                .invoke(request.clone(), CancellationToken::new())
                .unwrap(),
        ))
    }));
    measurements.push(measure("checked_zero", iterations, 0, || {
        zero(&empty_invoker, request.clone())
    }));
    for (name, value) in [
        ("1_byte", &small),
        ("64_kib", &large),
        ("1_byte_subtree_1_mib_owner", &subtree),
    ] {
        let (direct_name, checked_name) = match name {
            "1_byte" => ("direct_unary_1_byte", "checked_unary_1_byte"),
            "64_kib" => ("direct_unary_64_kib", "checked_unary_64_kib"),
            _ => ("direct_unary_subtree", "checked_unary_subtree"),
        };
        measurements.push(measure(direct_name, iterations, 1, || {
            fixture.direct(Some(value), 1)
        }));
        measurements.push(measure(checked_name, iterations, 1, || {
            echo(&invoker, request.clone(), value, 1)
        }));
    }
    for (direct_name, checked_name, value) in [
        ("direct_stream_1_byte", "checked_stream_1_byte", &small),
        ("direct_stream_64_kib", "checked_stream_64_kib", &large),
    ] {
        measurements.push(measure(direct_name, 1_000, 64, || {
            fixture.direct(Some(value), 64)
        }));
        measurements.push(measure(checked_name, 1_000, 64, || {
            echo(&invoker, request.clone(), value, 64)
        }));
    }
    measurements.push(measure("checked_cancel_release", iterations, 0, || {
        cancelled(&parked_invoker, request.clone(), &subtree)
    }));
    let operation =
        DefaultOperationInvoker::new(&fixture, RefusingEvaluator, ExternalSchemas::default());
    let operation_park =
        DefaultOperationInvoker::new(&parked, RefusingEvaluator, ExternalSchemas::default());
    // Resource acquisition/creation is setup, as document construction is above.
    // Measure retaining already supplied dependencies, not a resource-set factory.
    let external = ExternalSchemas::default();
    // Client construction only retains its explicit dependencies.
    measurements.push(measure(
        "operation_invoker_creation_release",
        iterations,
        0,
        || {
            drop(black_box(DefaultOperationInvoker::new(
                &fixture,
                RefusingEvaluator,
                external.clone(),
            )))
        },
    ));
    measurements.push(measure("operation_unary_1_byte", iterations, 1, || {
        echo_call(
            operation
                .invoke(
                    OperationRequest::operation(proof.clone(), "say"),
                    CancellationToken::new(),
                )
                .unwrap(),
            &small,
            1,
        )
    }));
    measurements.push(measure("operation_stream_1_byte", 1_000, 64, || {
        echo_call(
            operation
                .invoke(
                    OperationRequest::operation(proof.clone(), "say"),
                    CancellationToken::new(),
                )
                .unwrap(),
            &small,
            64,
        )
    }));
    measurements.push(measure("operation_unary_subtree", iterations, 1, || {
        echo_call(
            operation
                .invoke(
                    OperationRequest::binding(proof.clone(), "b"),
                    CancellationToken::new(),
                )
                .unwrap(),
            &subtree,
            1,
        )
    }));
    measurements.push(measure("operation_cancel_release", iterations, 0, || {
        cancel_call(
            operation_park
                .invoke(
                    OperationRequest::operation(proof.clone(), "say"),
                    CancellationToken::new(),
                )
                .unwrap(),
            &subtree,
        );
    }));
    // Explicit incapable evaluation exercises diagnostic ownership. The old
    // implicit missing-evaluator API is not a compatibility target.
    let declared = Document::parse(r#"{"openbindings":"0.2.0","operations":{"echo":{"input":true}},"bindings":{"b":{"operation":"echo","source":"s"}},"sources":{"s":{"kind":"resource.fixture","data":null}}}"#)
        .unwrap().validate().unwrap().conformant().unwrap();
    let unresolved =
        DefaultOperationInvoker::new(&fixture, RefusingEvaluator, ExternalSchemas::default());
    measurements.push(measure(
        "operation_validation_failure_release",
        iterations,
        0,
        || {
            let call = unresolved
                .invoke(
                    OperationRequest::operation(declared.clone(), "echo"),
                    CancellationToken::new(),
                )
                .unwrap();
            let control = call.control();
            let (mut input, mut output) = call.split();
            block_on(async {
                input.send(small.clone()).await.unwrap();
                input.close();
                let mut rejected = false;
                while let Some(event) = output.next().await {
                    match event {
                        OutputEvent::Error(error) => {
                            assert_eq!(error.code(), "ERR_SCHEMA_UNRESOLVED");
                            rejected = true;
                        }
                        OutputEvent::InputClosed => {}
                        _ => panic!("unexpected validation event: {event:?}"),
                    }
                }
                assert!(rejected);
                assert!(control.validation_failure().is_some());
            });
        },
    ));
    // Dropping requests and clients releases document proofs and backing owners.
    let owners_before = Snapshot::now();
    {
        let proof = document(&JsonValue::parse("null").unwrap());
        let invoker =
            DefaultOperationInvoker::new(&fixture, RefusingEvaluator, ExternalSchemas::default());
        echo_call(
            invoker
                .invoke(
                    OperationRequest::operation(proof, "echo"),
                    CancellationToken::new(),
                )
                .unwrap(),
            &small,
            1,
        );
    }
    let owners_after = Snapshot::now();
    assert_eq!(owners_before.live, owners_after.live);
    assert_eq!(owners_before.bytes, owners_after.bytes);
    let retention = retention();
    let defaults = InvocationLimits::default();
    let receipt = Receipt {
        format: "openbindings-invocation-resource-probe-v2",
        scope: "single-thread synthetic public-API measurement, release build, no network or RSS claim",
        counting: "successful System allocation/reallocation calls; realloc counts full new reservation and frees old reservation; allocator atomics and fixture guard atomics are included in timings; setup/receipt encoding excluded",
        input_capacity: defaults.input_capacity,
        output_capacity: defaults.output_capacity,
        source_logical_bytes: parent.bytes().len(),
        subtree_logical_bytes: subtree.bytes().len(),
        subtree_backing_bytes: subtree.original_source().len(),
        stream_values_per_call: 64,
        measurements,
        retention,
        provider_live_guards: empty.activity.live.load(Ordering::Relaxed)
            + fixture.activity.live.load(Ordering::Relaxed)
            + parked.activity.live.load(Ordering::Relaxed),
        provider_peak_guards: empty
            .activity
            .peak
            .load(Ordering::Relaxed)
            .max(fixture.activity.peak.load(Ordering::Relaxed))
            .max(parked.activity.peak.load(Ordering::Relaxed)),
    };
    println!("{}", serde_json::to_string(&receipt).unwrap());
}
