//! Qualification-only host task keeps borrowed SDK halves inside their owners.
//! One pending command per direction; no provider queue or detached invocation.
use openbindings::{JsonValue, WorkControl};
use openbindings_invocation::{
    BindingInvocationInput, BindingRequest, InvocationError, ProviderIo,
    binding::{BindingInvoker, Invoker},
};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::{Pin, pin},
    rc::Rc,
    task::{Poll, Waker},
    thread::LocalKey,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise, spawn_local};

thread_local! {
    static DRIVERS: Cell<u32> = const { Cell::new(0) };
    static PROVIDERS: Cell<u32> = const { Cell::new(0) };
    static COMMANDS: Cell<u32> = const { Cell::new(0) };
    static TOKENS: Cell<u32> = const { Cell::new(0) };
    static STARTS: Cell<u32> = const { Cell::new(0) };
}
struct Count(&'static LocalKey<Cell<u32>>);
impl Count {
    fn new(key: &'static LocalKey<Cell<u32>>) -> Self {
        key.with(|n| n.set(n.get() + 1));
        Self(key)
    }
}
impl Drop for Count {
    fn drop(&mut self) {
        self.0.with(|n| n.set(n.get() - 1));
    }
}

struct Reply<T>(Rc<RefCell<(Option<T>, Option<Waker>)>>);
impl<T> Clone for Reply<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Reply<T> {
    fn new() -> Self {
        Self(Rc::new(RefCell::new((None, None))))
    }
    fn resolve(&self, value: T) {
        let wake = {
            let mut slot = self.0.borrow_mut();
            slot.0 = Some(value);
            slot.1.take()
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
    async fn wait(&self) -> T {
        poll_fn(|cx| {
            let mut slot = self.0.borrow_mut();
            if let Some(value) = slot.0.take() {
                Poll::Ready(value)
            } else {
                slot.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}
type WriteReply = Reply<Result<(), &'static str>>;
type ReadReply = Reply<Option<Result<JsonValue, InvocationError>>>;
#[derive(Default)]
struct State {
    write: Option<(JsonValue, WriteReply)>,
    read: Option<ReadReply>,
    write_busy: bool,
    read_busy: bool,
    close: bool,
    input_closed: bool,
    cancel: bool,
    disposed: bool,
    finished: bool,
    write_wake: Option<Waker>,
    read_wake: Option<Waker>,
    cancel_wake: Option<Waker>,
}
type Shared = Rc<RefCell<State>>;
fn wake(shared: &Shared) {
    let wakes = {
        let mut s = shared.borrow_mut();
        [
            s.write_wake.take(),
            s.read_wake.take(),
            s.cancel_wake.take(),
        ]
    };
    for wake in wakes.into_iter().flatten() {
        wake.wake();
    }
}
async fn join<A: Future<Output = ()>, B: Future<Output = ()>>(a: A, b: B) {
    let (mut a, mut b) = (pin!(a), pin!(b));
    let (mut a_done, mut b_done) = (false, false);
    poll_fn(|cx| {
        if !a_done {
            a_done = a.as_mut().poll(cx).is_ready();
        }
        if !b_done {
            b_done = b.as_mut().poll(cx).is_ready();
        }
        if a_done && b_done {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

struct Local {
    token: Rc<Count>,
    gate: js_sys::Promise,
    mode: String,
}
impl BindingInvoker for Local {
    type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == "example.wasm-local"
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        mut io: ProviderIo,
        _: WorkControl,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            let _active = Count::new(&PROVIDERS);
            STARTS.with(|n| n.set(n.get() + 1));
            let local = self.token.clone();
            JsFuture::from(self.gate.clone())
                .await
                .map_err(|_| InvocationError::execution_failed())?;
            assert!(Rc::strong_count(&local) >= 2); // Rc survives an actual host Promise await.
            if self.mode == "no-output" {
                io.close_input();
                return Ok(());
            }
            if self.mode == "burst" {
                io.close_input();
                for n in 0..9 {
                    io.emit(JsonValue::parse(n.to_string()).unwrap()).await?;
                }
                return Ok(());
            }
            while let Some(value) = io.next().await {
                if self.mode == "unary" {
                    io.close_input();
                }
                io.emit(value).await?;
                match self.mode.as_str() {
                    "error-null" => {
                        return Err(InvocationError::new(
                            "example.failure",
                            Some(JsonValue::parse("null").unwrap()),
                        )
                        .unwrap());
                    }
                    "error-absent" => {
                        return Err(InvocationError::new("example.failure", None).unwrap());
                    }
                    "unary" => break,
                    _ => {}
                }
            }
            Ok(())
        })
    }
}

/// Owns host command slots; its one scoped task owns request/provider and SDK halves.
#[wasm_bindgen]
pub struct InvocationBridge {
    shared: Shared,
}

#[wasm_bindgen]
impl InvocationBridge {
    #[wasm_bindgen(constructor)]
    pub fn new(
        request: &str,
        mode: String,
        gate: js_sys::Promise,
    ) -> Result<InvocationBridge, JsValue> {
        let payload =
            JsonValue::parse(request).map_err(|_| JsValue::from_str("INVALID_REQUEST"))?;
        let request = BindingInvocationInput::from_json(&payload)
            .map_err(|_| JsValue::from_str("INVALID_REQUEST"))?;
        let shared = Shared::default();
        let state = shared.clone();
        let driver_count = Count::new(&DRIVERS);
        spawn_local(async move {
            let _driver_count = driver_count;
            let provider = Local {
                token: Rc::new(Count::new(&TOKENS)),
                gate,
                mode,
            };
            let facade = Invoker::new(Rc::new(provider));
            let call = facade.invoke(request.request());
            drive(call, state).await;
        });
        Ok(Self { shared })
    }

    /// Qualification-only operation entry point using the same command bridge.
    #[wasm_bindgen(js_name = forOperation)]
    pub fn for_operation(
        document: &str,
        name: String,
        mode: String,
        gate: js_sys::Promise,
        validate: bool,
    ) -> Result<InvocationBridge, JsValue> {
        let document = openbindings::ParsedDocument::parse(document)
            .map_err(|_| JsValue::from_str("INVALID_DOCUMENT"))?;
        let provider = Local {
            token: Rc::new(Count::new(&TOKENS)),
            gate,
            mode,
        };
        let mut facade =
            openbindings_invocation::operation::Invoker::new(document, Rc::new(provider));
        if validate {
            facade =
                facade
                    .with_evaluator(
                        std::sync::Arc::new(
                            openbindings_json_schema_evaluator::DefaultEvaluator::new(),
                        ),
                        openbindings::ResourceSet::default(),
                    )
                    .map_err(|_| JsValue::from_str("INVALID_DOCUMENT"))?;
        }
        let shared = Shared::default();
        let state = shared.clone();
        let driver_count = Count::new(&DRIVERS);
        spawn_local(async move {
            let _driver_count = driver_count;
            let call = facade.invoke(openbindings_invocation::operation::Request::operation(
                &name,
            ));
            drive(call, state).await;
        });
        Ok(Self { shared })
    }

    /// A Promise borrows only the command slot, never the exported wasm class.
    pub fn write(&self, text: String) -> js_sys::Promise {
        let shared = self.shared.clone();
        future_to_promise(async move {
            let value = JsonValue::parse(text).map_err(|_| JsValue::from_str("INVALID_VALUE"))?;
            let reply = WriteReply::new();
            {
                let mut s = shared.borrow_mut();
                if s.write_busy {
                    return Err(JsValue::from_str("WRITE_BUSY"));
                }
                if s.input_closed || s.finished || s.disposed {
                    return Err(JsValue::from_str("INPUT_CLOSED"));
                }
                s.write_busy = true;
                s.write = Some((value, reply.clone()));
            }
            let _pending = Count::new(&COMMANDS);
            wake(&shared);
            let result = reply.wait().await;
            shared.borrow_mut().write_busy = false;
            result
                .map(|()| JsValue::UNDEFINED)
                .map_err(JsValue::from_str)
        })
    }

    pub fn next(&self) -> js_sys::Promise {
        let shared = self.shared.clone();
        future_to_promise(async move {
            let reply = ReadReply::new();
            {
                let mut s = shared.borrow_mut();
                if s.read_busy {
                    return Err(JsValue::from_str("READ_BUSY"));
                }
                if s.finished {
                    return Ok(JsValue::UNDEFINED);
                }
                if s.disposed {
                    return Err(JsValue::from_str("{\"code\":\"ERR_CANCELLED\"}"));
                }
                s.read_busy = true;
                s.read = Some(reply.clone());
            }
            let _pending = Count::new(&COMMANDS);
            wake(&shared);
            let result = reply.wait().await;
            shared.borrow_mut().read_busy = false;
            match result {
                None => Ok(JsValue::UNDEFINED),
                Some(Ok(value)) => Ok(JsValue::from_str(value.text())),
                Some(Err(error)) => Err(JsValue::from_str(error.to_json().unwrap().text())),
            }
        })
    }
    pub fn close(&self) {
        {
            let mut s = self.shared.borrow_mut();
            s.input_closed = true;
            s.close = true;
        }
        wake(&self.shared);
    }
    pub fn cancel(&self) {
        self.shared.borrow_mut().cancel = true;
        wake(&self.shared);
    }
    pub fn dispose(&self) {
        self.shared.borrow_mut().disposed = true;
        wake(&self.shared);
    }
}
impl Drop for InvocationBridge {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[wasm_bindgen(js_name = bridgeStats)]
pub fn stats() -> String {
    format!(
        "{{\"drivers\":{},\"providers\":{},\"commands\":{},\"tokens\":{},\"starts\":{}}}",
        DRIVERS.get(),
        PROVIDERS.get(),
        COMMANDS.get(),
        TOKENS.get(),
        STARTS.get()
    )
}

async fn drive<F: Future<Output = Result<(), InvocationError>>>(
    call: openbindings_invocation::Invocation<F>,
    state: Shared,
) {
    let control = call.control();
    if state.borrow().cancel || state.borrow().disposed {
        control.cancel();
    }
    let (mut input, mut output) = call.split();
    let writes = async {
        loop {
            enum Command {
                Write(JsonValue, WriteReply),
                Close,
                Stop,
            }
            let command = poll_fn(|cx| {
                let mut s = state.borrow_mut();
                if s.disposed || s.finished {
                    return Poll::Ready(Command::Stop);
                }
                if s.close {
                    s.close = false;
                    return Poll::Ready(Command::Close);
                }
                if let Some((value, reply)) = s.write.take() {
                    return Poll::Ready(Command::Write(value, reply));
                }
                s.write_wake = Some(cx.waker().clone());
                Poll::Pending
            })
            .await;
            match command {
                Command::Stop => break,
                Command::Close => input.close(),
                Command::Write(value, reply) => {
                    let result = input.write(value).await.map_err(|_| "INPUT_CLOSED");
                    state.borrow_mut().input_closed = input.is_closed();
                    reply.resolve(result);
                }
            }
        }
    };
    let reads = async {
        loop {
            let reply = poll_fn(|cx| {
                let mut s = state.borrow_mut();
                if s.disposed {
                    return Poll::Ready(None);
                }
                if let Some(reply) = s.read.take() {
                    return Poll::Ready(Some(reply));
                }
                s.read_wake = Some(cx.waker().clone());
                Poll::Pending
            })
            .await;
            let Some(reply) = reply else {
                break;
            };
            let value = output.next().await;
            let terminal = value.as_ref().is_none_or(Result::is_err);
            reply.resolve(value);
            if terminal {
                state.borrow_mut().finished = true;
                wake(&state);
                break;
            }
        }
    };
    let cancellation = async {
        let cancel = poll_fn(|cx| {
            let mut s = state.borrow_mut();
            if s.cancel || s.disposed {
                Poll::Ready(true)
            } else if s.finished {
                Poll::Ready(false)
            } else {
                s.cancel_wake = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await;
        if cancel {
            control.cancel();
        }
    };
    join(join(writes, reads), cancellation).await;
    let (write, read) = {
        let mut s = state.borrow_mut();
        s.finished = true;
        (s.write.take(), s.read.take())
    };
    if let Some((_, reply)) = write {
        reply.resolve(Err("INPUT_CLOSED"));
    }
    if let Some(reply) = read {
        reply.resolve(Some(Err(InvocationError::cancelled())));
    }
    wake(&state);
}
