//! An invoker needs no puller, descriptor, evaluator or async runtime dependency.
use openbindings_invocation::*;
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
};

pub struct Echo;
impl BindingInvoker for Echo {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        _: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            while let Some(value) = io.next().await {
                io.send(value).await?;
            }
            Ok(())
        })
    }
    fn handle_preflight<'a>(&'a self, _: BindingRequest, _: CancellationToken) -> Self::Preflight<'a> {
        ready(Ok(PreflightOutcome::NoPrediction))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings::{Document, JsonValue};
    use std::task::{Context, Poll, Waker};
    fn ready_value<F: Future>(future: F) -> F::Output {
        match Box::pin(future)
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("local ready boundary"),
        }
    }
    #[test]
    fn explicit_binding_invocation_with_no_other_interface_or_runtime() {
        let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"echo":{}},"sources":{"s":{"kind":"example.echo"}},"bindings":{"b":{"source":"s","operation":"echo"}}}"#).unwrap().validate().unwrap().conformant().unwrap();
        let client = Echo;
        let (mut input, mut output) = client
            .invoke(BindingRequest::new(document, "b"), CancellationToken::new())
            .unwrap()
            .split();
        ready_value(input.send(JsonValue::parse("9007199254740993").unwrap())).unwrap();
        input.close();
        let Some(OutputEvent::Output(value)) = ready_value(output.next()) else {
            panic!("output")
        };
        assert_eq!(value.text(), "9007199254740993");
        loop {
            match ready_value(output.next()) {
                Some(OutputEvent::Complete) => break,
                Some(OutputEvent::InputClosed) => {}
                other => panic!("unexpected terminal {other:?}"),
            }
        }
    }
}
