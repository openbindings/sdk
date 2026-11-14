//! Two transport seams for the guide. The sample transport completes a local
//! exchange; a protocol implementation would read/write through its host handles.
use kind_adapter::Memory;
use openbindings_invocation::{
    BindingInvoker, BindingInvokerExt, BindingRequest, CancellationToken, ContextChallenge,
    InvocationError, ProviderIo,
};
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
};

type Outcome = Result<(), InvocationError>;

trait Transport {
    type Exchange<'a>: Future<Output = Outcome>
    where
        Self: 'a;
    fn exchange<'a>(&'a self, io: ProviderIo, control: CancellationToken) -> Self::Exchange<'a>;
}

// This future erases no auto traits. Pin<Box<F>> can also hold a !Unpin F.
struct Exchange<F> {
    admission: Option<InvocationError>,
    work: Pin<Box<F>>,
}
impl<F: Future<Output = Outcome>> Future for Exchange<F> {
    type Output = Outcome;
    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(error) = this.admission.take() {
            return std::task::Poll::Ready(Err(error));
        }
        this.work.as_mut().poll(cx)
    }
}

struct Adapter<T>(T);
impl<T: Transport> BindingInvoker for Adapter<T> {
    type Invoke<'a>
        = Exchange<T::Exchange<'a>>
    where
        Self: 'a;
    type Preflight<'a>
        = Ready<Result<Option<ContextChallenge>, InvocationError>>
    where
        Self: 'a;

    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invoke<'a> {
        Exchange {
            admission: admit(&request).err(),
            // Constructing this future must not start transport work.
            work: Box::pin(self.0.exchange(io, control)),
        }
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Self::Preflight<'a> {
        Memory.handle_preflight(request, control)
    }
}

fn admit(request: &BindingRequest) -> Outcome {
    let binding = request
        .interface
        .binding(&request.binding)
        .ok_or_else(InvocationError::binding_not_found)?;
    let expected = openbindings::JsonValue::parse(r#"{"mode":"echo"}"#).unwrap();
    let source = openbindings::JsonValue::parse(r#"{"channel":"local"}"#).unwrap();
    if binding.source().kind() != kind_adapter::KIND
        || !binding
            .data()
            .is_some_and(|data| data.semantic_eq(&expected) == Some(true))
        || !binding
            .source()
            .data()
            .is_some_and(|data| data.semantic_eq(&source) == Some(true))
    {
        return Err(InvocationError::new("example.memory.invalid-data", None).unwrap());
    }
    Ok(())
}

struct Callback<S>(S);
impl<S, Fut> BindingInvoker for Callback<S>
where
    S: Fn(ProviderIo, CancellationToken) -> Fut + Sync,
    // Fut is separate from Self, so Self: 'a does not imply Fut: 'a.
    Fut: Future<Output = Outcome> + Send + 'static,
{
    type Invoke<'a>
        = Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>
    where
        Self: 'a;
    type Preflight<'a>
        = Ready<Result<Option<ContextChallenge>, InvocationError>>
    where
        Self: 'a;
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            admit(&request)?;
            (self.0)(io, control).await
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Self::Preflight<'a> {
        Memory.handle_preflight(request, control)
    }
}

struct Native;
impl Transport for Native {
    type Exchange<'a> = Ready<Outcome>;
    fn exchange<'a>(&'a self, _: ProviderIo, _: CancellationToken) -> Self::Exchange<'a> {
        ready(Ok(()))
    }
}

// A genuinely borrowed local future, without a 'static requirement on T.
struct Local<'h>(&'h std::rc::Rc<()>);
impl Transport for Local<'_> {
    type Exchange<'a>
        = Pin<Box<dyn Future<Output = Outcome> + 'a>>
    where
        Self: 'a;
    fn exchange<'a>(&'a self, _: ProviderIo, _: CancellationToken) -> Self::Exchange<'a> {
        Box::pin(async move {
            let _handle = self.0.clone();
            Ok(())
        })
    }
}

fn request() -> BindingRequest {
    let interface = openbindings::Document::parse(
        r#"{
        "openbindings":"0.2.0","operations":{"echo":{}},
        "sources":{"s":{"kind":"example.memory@1","data":{"channel":"local"}}},
        "bindings":{"b":{"source":"s","operation":"echo","data":{"mode":"echo"}}}
    }"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    BindingRequest::new(interface, "b")
}

#[tokio::test]
async fn native_is_send_and_borrowed_local_transport_needs_no_static() {
    fn require_send<T: Send>(value: T) -> T {
        value
    }
    let native = Adapter(Native);
    let call = require_send(native.invoke(request(), CancellationToken::new()).unwrap());
    let (_, mut output) = call.split();
    while output.next().await.is_some() {}

    let callback = Callback(|_: ProviderIo, _: CancellationToken| ready(Ok(())));
    let call = require_send(
        callback
            .invoke(request(), CancellationToken::new())
            .unwrap(),
    );
    let (_, mut output) = call.split();
    while output.next().await.is_some() {}

    let handle = std::rc::Rc::new(());
    let local = Adapter(Local(&handle));
    let (_, mut output) = local
        .invoke(request(), CancellationToken::new())
        .unwrap()
        .split();
    while output.next().await.is_some() {}
}
