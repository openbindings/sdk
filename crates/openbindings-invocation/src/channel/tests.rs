use super::*;
use std::{
    cell::Cell,
    future::pending,
    pin::pin,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Wake, Waker},
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn only_output_drives_and_committed_values_precede_the_terminal() {
    let shared = Shared::new(InvocationLimits::default(), [false, false]);
    let mut io = ProviderIo::new(shared.clone());
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let session = Invocation::from_parts(shared, async move {
        called.set(called.get() + 1);
        let value = io.next().await.expect("one input");
        io.send(value).await?;
        Ok(())
    });
    let (mut input, mut output) = session.split();
    assert!(poll(pin!(input.send(json("9007199254740993")))).is_ready());
    assert_eq!(calls.get(), 0);
    let Poll::Ready(Some(OutputEvent::Output(value))) = poll(pin!(output.next())) else {
        panic!("output")
    };
    assert_eq!(value.text(), "9007199254740993");
    assert_eq!(calls.get(), 1);
    assert!(matches!(
        poll(pin!(output.next())),
        Poll::Ready(Some(OutputEvent::Complete))
    ));
    assert!(matches!(poll(pin!(output.next())), Poll::Ready(None)));
}

#[test]
fn caller_close_drains_but_provider_close_discards_and_notifies_once() {
    let shared = Shared::new(InvocationLimits::default(), [false, false]);
    let (mut reader, _writer) = ProviderIo::new(shared.clone()).split();
    let (mut input, mut output) =
        Invocation::from_parts(shared.clone(), pending::<Outcome>()).split();
    assert!(poll(pin!(input.send(json("1")))).is_ready());
    input.close();
    assert!(matches!(poll(pin!(reader.next())), Poll::Ready(Some(_))));
    assert!(matches!(poll(pin!(reader.next())), Poll::Ready(None)));
    reader.close();
    reader.close();
    assert!(matches!(
        poll(pin!(output.next())),
        Poll::Ready(Some(OutputEvent::InputClosed))
    ));
    assert!(poll(pin!(output.next())).is_pending());
    shared.settle(Ok(()));
    assert!(matches!(
        poll(pin!(output.next())),
        Poll::Ready(Some(OutputEvent::Complete))
    ));
}

#[test]
fn terminal_suppresses_an_undelivered_input_closed_notification() {
    let shared = Shared::new(InvocationLimits::default(), [false, false]);
    let (mut reader, _writer) = ProviderIo::new(shared.clone()).split();
    let (_input, mut output) = Invocation::from_parts(shared.clone(), pending::<Outcome>()).split();
    reader.close();
    shared.settle(Ok(()));
    assert!(matches!(
        poll(pin!(output.next())),
        Poll::Ready(Some(OutputEvent::Complete))
    ));
    assert!(matches!(poll(pin!(output.next())), Poll::Ready(None)));
}

#[test]
fn dropped_checked_output_and_stale_failure_cannot_affect_a_new_send() {
    let shared = Shared::new(InvocationLimits::default(), [false, true]);
    let (_reader, mut writer) = ProviderIo::new(shared.clone()).split();
    let old_check = {
        let mut old = pin!(writer.send(json("1")));
        assert!(poll(old.as_mut()).is_pending());
        shared.pending_check(Direction::Output).unwrap()
    }; // withdrawal while the driver owns the checked value
    let mut next = pin!(writer.send(json("2")));
    assert!(poll(next.as_mut()).is_pending());
    shared.checked(old_check, Err((InvocationError::execution_failed(), None)));
    assert!(shared.outcome().is_none());
    let check = shared.pending_check(Direction::Output).unwrap();
    assert_eq!(check.value.text(), "2");
    shared.checked(check, Ok(()));
    assert!(shared.state.lock().unwrap().output.is_empty());
    assert!(matches!(poll(next.as_mut()), Poll::Ready(Ok(()))));
    assert_eq!(
        shared
            .state
            .lock()
            .unwrap()
            .output
            .pop_front()
            .unwrap()
            .text(),
        "2"
    );
}

#[test]
fn dropped_acknowledged_output_is_never_committed() {
    let shared = Shared::new(InvocationLimits::default(), [false, true]);
    let (_reader, mut writer) = ProviderIo::new(shared.clone()).split();
    {
        let mut send = pin!(writer.send(json("1")));
        assert!(poll(send.as_mut()).is_pending());
        let check = shared.pending_check(Direction::Output).unwrap();
        shared.checked(check, Ok(()));
    }
    assert!(shared.state.lock().unwrap().output.is_empty());
    assert!(shared.pending_check(Direction::Output).is_none());
}

#[test]
fn cancelled_input_read_retains_value_but_owner_close_invalidates_check() {
    let shared = Shared::new(InvocationLimits::default(), [true, false]);
    let (mut reader, _writer) = ProviderIo::new(shared.clone()).split();
    let mut input = InvocationInput {
        shared: shared.clone(),
    };
    assert!(poll(pin!(input.send(json("1")))).is_ready());
    assert!(poll(pin!(reader.next())).is_pending());
    let check = shared.pending_check(Direction::Input).unwrap();
    shared.checked(check, Ok(()));
    assert!(matches!(poll(pin!(reader.next())), Poll::Ready(Some(_))));
    assert!(poll(pin!(input.send(json("2")))).is_ready());
    assert!(poll(pin!(reader.next())).is_pending());
    let check = shared.pending_check(Direction::Input).unwrap();
    reader.close();
    shared.checked(check, Err((InvocationError::execution_failed(), None)));
    assert!(shared.outcome().is_none());
    assert!(matches!(poll(pin!(reader.next())), Poll::Ready(None)));
}

#[test]
fn rejected_input_is_never_a_clean_eof_in_either_poll_context() {
    for active in [false, true] {
        let shared = Shared::new(InvocationLimits::default(), [true, false]);
        let (mut reader, _writer) = ProviderIo::new(shared.clone()).split();
        let mut input = InvocationInput {
            shared: shared.clone(),
        };
        assert!(poll(pin!(input.send(json("1")))).is_ready());
        assert!(poll(pin!(reader.next())).is_pending());
        let check = shared.pending_check(Direction::Input).unwrap();
        shared.checked(check, Err((InvocationError::execution_failed(), None)));
        let _epoch = active.then(|| shared.begin_poll());
        assert!(poll(pin!(reader.next())).is_pending());
        assert!(shared.control().is_cancelled());
    }
}

#[derive(Default)]
struct Count(AtomicUsize);
impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn blocked_polls_do_not_spend_credits_or_create_a_busy_wake_loop() {
    let shared = Shared::new(InvocationLimits::default(), [false, false]);
    let (mut reader, _writer) = ProviderIo::new(shared.clone()).split();
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    let mut waiter = Waiter::new(shared.signal.clone());
    waiter.register(&waker);
    let _epoch = shared.begin_poll();
    for _ in 0..1000 {
        assert!(
            Pin::new(&mut reader)
                .poll_next(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    assert_eq!(shared.state.lock().unwrap().credits, POLL_CREDITS);
    assert_eq!(count.0.load(Ordering::Relaxed), 0);
}

#[test]
fn ready_bypass_and_graceful_eof_yield_but_post_terminal_output_cleanup_resolves() {
    let shared = Shared::new(InvocationLimits::default(), [false, false]);
    let (mut reader, mut writer) = ProviderIo::new(shared.clone()).split();
    shared.close_input(false);
    let epoch = shared.begin_poll();
    for _ in 0..POLL_CREDITS {
        assert!(matches!(poll(pin!(reader.next())), Poll::Ready(None)));
    }
    assert!(poll(pin!(reader.next())).is_pending());
    shared.settle(Err(InvocationError::cancelled()));
    assert!(poll(pin!(writer.send(json("1")))).is_pending());
    drop(epoch);
    assert!(matches!(
        poll(pin!(writer.send(json("1")))),
        Poll::Ready(Err(_))
    ));
    assert!(poll(pin!(reader.next())).is_pending());
}

#[test]
fn epoch_guard_clears_active_state_on_unwind() {
    let shared = Shared::new(InvocationLimits::default(), [false, false]);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _epoch = shared.begin_poll();
        panic!("provider panic");
    }));
    assert!(outcome.is_err());
    assert!(!shared.state.lock().unwrap().active_poll);
}

// Independent review regressions; original failing probes retained with receipts.
#[test]
fn scoped_output_read_releases_callers_waker_while_provider_stays_pending() {
    let shared = Shared::new(InvocationLimits::default(), [false, false]);
    let mut io = ProviderIo::new(shared.clone());
    let (_input, mut output) = Invocation::from_parts(shared, async move {
        let _ = io.next().await;
        Ok(())
    })
    .split();
    let owner = Arc::new(Count::default());
    let weak = Arc::downgrade(&owner);
    let waker = Waker::from(owner);
    {
        let mut read = pin!(output.next());
        assert!(
            read.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    drop(waker);
    assert!(
        weak.upgrade().is_none(),
        "retained provider holds caller waker after scoped output read is gone"
    );
    assert!(output.shared.outcome().is_none());
}
struct PanicWake;
impl Wake for PanicWake {
    fn wake(self: Arc<Self>) {
        panic!("wake at epoch entry");
    }
    fn wake_by_ref(self: &Arc<Self>) {
        panic!("wake at epoch entry");
    }
}
#[test]
fn epoch_entry_waker_panic_clears_active_flag() {
    let shared = Shared::new(InvocationLimits::default(), [false, false]);
    shared.state.lock().unwrap().credits = 0;
    let mut waiter = Waiter::new(shared.provider_read.clone());
    waiter.register(&Waker::from(Arc::new(PanicWake)));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _epoch = shared.begin_poll();
        }))
        .is_err()
    );
    waiter.clear();
    assert!(
        !shared.state.lock().unwrap().active_poll,
        "epoch never constructed before external wake callback"
    );
}
