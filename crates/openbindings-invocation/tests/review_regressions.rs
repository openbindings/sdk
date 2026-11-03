//! Independent review witnesses for driver cancellation and value ownership.
#[cfg(test)]
mod tests {
    use openbindings::{
        ConformantDocument, Detail, Document, EvaluationRequest, ExternalSchemas, JsonValue,
        PreparationFailure, PreparationFailureKind, PreparationLimits, SchemaEvaluator,
        SchemaValidator, ValueOutcome, WorkControl,
    };
    use openbindings_invocation::{
        BindingInvoker, BindingRequest, CancellationToken, InvocationError, InvocationSupport,
        OperationClient, OperationRequest, OutputEvent, PreflightOutcome, ProviderInput,
        ProviderIo, ProviderOutput,
    };
    use std::{
        cell::Cell,
        future::{Future, pending},
        pin::{Pin, pin},
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll, Waker},
    };
    fn j(t: &str) -> JsonValue {
        JsonValue::parse(t).unwrap()
    }
    fn doc(body: &str) -> ConformantDocument {
        Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{body}}},"sources":{{"s":{{"kind":"review"}}}},"bindings":{{"b":{{"operation":"op","source":"s"}}}}}}"#)).unwrap().validate().unwrap().conformant().unwrap()
    }
    fn poll<F: Future>(f: Pin<&mut F>) -> Poll<F::Output> {
        f.poll(&mut Context::from_waker(Waker::noop()))
    }
    #[derive(Clone, Copy)]
    enum Mode {
        Echo,
        PublishOutput,
        PublishInput,
        Park,
    }
    struct P {
        mode: Mode,
        writer: Arc<Mutex<Option<ProviderOutput>>>,
        reader: Arc<Mutex<Option<ProviderInput>>>,
        reads: Arc<AtomicUsize>,
        calls: Arc<AtomicUsize>,
        advisory_control: Arc<Mutex<Option<CancellationToken>>>,
    }
    impl P {
        fn new(mode: Mode) -> Self {
            Self {
                mode,
                writer: Arc::default(),
                reader: Arc::default(),
                reads: Arc::default(),
                calls: Arc::default(),
                advisory_control: Arc::default(),
            }
        }
    }
    impl InvocationSupport for P {
        fn can_invoke(&self, _: &str) -> bool {
            true
        }
    }
    impl BindingInvoker for P {
        type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
        type Preflight<'a> =
            Pin<Box<dyn Future<Output = Result<PreflightOutcome, InvocationError>> + Send + 'a>>;
        fn invoke<'a>(
            &'a self,
            _: BindingRequest,
            mut io: ProviderIo,
            _: CancellationToken,
        ) -> Self::Invoke<'a> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move {
                match self.mode {
                    Mode::Echo => {
                        if let Some(value) = io.next().await {
                            self.reads.fetch_add(1, Ordering::Relaxed);
                            io.send(value).await?;
                        }
                        Ok(())
                    }
                    Mode::PublishOutput => {
                        let (reader, writer) = io.split();
                        *self.writer.lock().unwrap() = Some(writer);
                        let _hold = reader;
                        pending().await
                    }
                    Mode::PublishInput => {
                        let (reader, writer) = io.split();
                        *self.reader.lock().unwrap() = Some(reader);
                        let _hold = writer;
                        pending().await
                    }
                    Mode::Park => pending().await,
                }
            })
        }
        fn preflight<'a>(
            &'a self,
            _: BindingRequest,
            control: CancellationToken,
        ) -> Self::Preflight<'a> {
            *self.advisory_control.lock().unwrap() = Some(control);
            Box::pin(pending())
        }
    }
    #[derive(Clone, Copy)]
    enum EvalMode {
        Satisfy,
        CancelPrepare,
        CancelEvaluate,
        CloseThenRefuse,
        CloseThenCancel,
    }
    struct E {
        mode: EvalMode,
        external: CancellationToken,
        reader: Arc<Mutex<Option<ProviderInput>>>,
        preps: Arc<AtomicUsize>,
        evals: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
    }
    struct V {
        mode: EvalMode,
        external: CancellationToken,
        evals: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
        local: Cell<usize>,
    }
    impl Drop for V {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }
    impl SchemaEvaluator for E {
        type Validator = V;
        fn prepare(
            &self,
            _: EvaluationRequest<'_>,
            _: &PreparationLimits,
            _: &dyn WorkControl,
        ) -> Result<V, PreparationFailure> {
            self.preps.fetch_add(1, Ordering::Relaxed);
            match self.mode {
                EvalMode::CancelPrepare => self.external.cancel(),
                EvalMode::CloseThenRefuse | EvalMode::CloseThenCancel => {
                    self.reader.lock().unwrap().as_mut().unwrap().close();
                    return Err(PreparationFailure::new(
                        if matches!(self.mode, EvalMode::CloseThenCancel) {
                            PreparationFailureKind::Cancelled
                        } else {
                            PreparationFailureKind::UnsupportedCapability {
                                capability: "review".into(),
                            }
                        },
                    ));
                }
                _ => (),
            }
            Ok(V {
                mode: self.mode,
                external: self.external.clone(),
                evals: self.evals.clone(),
                drops: self.drops.clone(),
                local: Cell::new(0),
            })
        }
    }
    impl SchemaValidator for V {
        fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
            self.evals.fetch_add(1, Ordering::Relaxed);
            self.local.set(self.local.get() + 1);
            if matches!(self.mode, EvalMode::CancelEvaluate) {
                self.external.cancel();
            }
            ValueOutcome::Satisfies
        }
    }
    fn evaluator(
        mode: EvalMode,
        external: &CancellationToken,
        reader: Arc<Mutex<Option<ProviderInput>>>,
    ) -> E {
        E {
            mode,
            external: external.clone(),
            reader,
            preps: Arc::default(),
            evals: Arc::default(),
            drops: Arc::default(),
        }
    }
    fn error(p: Poll<Option<OutputEvent>>) -> String {
        let Poll::Ready(Some(OutputEvent::Error(e))) = p else {
            panic!("expected error, got {p:?}")
        };
        e.code().into()
    }
    #[test]
    fn external_cancellation_withholds_independent_acknowledged_output_before_commit() {
        let p = P::new(Mode::PublishOutput);
        let external = CancellationToken::new();
        let e = evaluator(EvalMode::Satisfy, &external, p.reader.clone());
        let client = OperationClient::new(&p, e, ExternalSchemas::default());
        let (_input, mut output) = client
            .invoke(
                OperationRequest::binding(doc(r#"{"output":true}"#), "b"),
                external.clone(),
            )
            .unwrap()
            .split();
        assert!(poll(pin!(output.next())).is_pending());
        let mut writer = p.writer.lock().unwrap().take().unwrap();
        let mut send = pin!(writer.send(j("42")));
        assert!(poll(send.as_mut()).is_pending());
        assert!(poll(pin!(output.next())).is_pending());
        external.cancel();
        let completion = poll(send.as_mut());
        assert!(
            !matches!(completion, Poll::Ready(Ok(()))),
            "output committed after external cancellation returned: {completion:?}"
        );
        assert_eq!(error(poll(pin!(output.next()))), "ERR_CANCELLED");
    }
    #[test]
    fn external_cancellation_withholds_independent_acknowledged_input_before_delivery() {
        let p = P::new(Mode::PublishInput);
        let external = CancellationToken::new();
        let e = evaluator(EvalMode::Satisfy, &external, p.reader.clone());
        let client = OperationClient::new(&p, e, ExternalSchemas::default());
        let (mut input, mut output) = client
            .invoke(
                OperationRequest::binding(doc(r#"{"input":true}"#), "b"),
                external.clone(),
            )
            .unwrap()
            .split();
        assert!(poll(pin!(output.next())).is_pending());
        assert!(poll(pin!(input.send(j("42")))).is_ready());
        {
            let mut guard = p.reader.lock().unwrap();
            assert!(poll(pin!(guard.as_mut().unwrap().next())).is_pending());
        }
        assert!(poll(pin!(output.next())).is_pending());
        external.cancel();
        {
            let mut guard = p.reader.lock().unwrap();
            let completion = poll(pin!(guard.as_mut().unwrap().next()));
            assert!(
                completion.is_pending(),
                "cancelled input released or treated as EOF: {completion:?}"
            );
        }
        assert_eq!(error(poll(pin!(output.next()))), "ERR_CANCELLED");
    }
    #[test]
    fn cancellation_inside_prepare_and_evaluate_withholds_input_and_releases_validator() {
        for mode in [EvalMode::CancelPrepare, EvalMode::CancelEvaluate] {
            let p = P::new(Mode::Echo);
            let external = CancellationToken::new();
            let e = evaluator(mode, &external, p.reader.clone());
            let drops = e.drops.clone();
            let client = OperationClient::new(&p, e, ExternalSchemas::default());
            let (mut input, mut output) = client
                .invoke(
                    OperationRequest::binding(doc(r#"{"input":true}"#), "b"),
                    external,
                )
                .unwrap()
                .split();
            assert!(poll(pin!(input.send(j("1")))).is_ready());
            assert_eq!(error(poll(pin!(output.next()))), "ERR_CANCELLED");
            assert_eq!(p.reads.load(Ordering::Relaxed), 0);
            assert_eq!(drops.load(Ordering::Relaxed), 1);
        }
    }
    #[test]
    fn stale_refusal_is_discarded_but_stale_observed_cancellation_is_global() {
        for mode in [EvalMode::CloseThenRefuse, EvalMode::CloseThenCancel] {
            let p = P::new(Mode::PublishInput);
            let external = CancellationToken::new();
            let e = evaluator(mode, &external, p.reader.clone());
            let client = OperationClient::new(&p, e, ExternalSchemas::default());
            let (mut input, mut output) = client
                .invoke(
                    OperationRequest::binding(doc(r#"{"input":true}"#), "b"),
                    external,
                )
                .unwrap()
                .split();
            assert!(poll(pin!(output.next())).is_pending());
            assert!(poll(pin!(input.send(j("1")))).is_ready());
            {
                let mut g = p.reader.lock().unwrap();
                assert!(poll(pin!(g.as_mut().unwrap().next())).is_pending());
            }
            let result = poll(pin!(output.next()));
            if matches!(mode, EvalMode::CloseThenCancel) {
                assert_eq!(error(result), "ERR_CANCELLED");
            } else {
                assert!(matches!(
                    result,
                    Poll::Ready(Some(OutputEvent::InputClosed))
                ));
                assert!(output.control().outcome().is_none());
                assert!(output.control().validation_failure().is_none());
            }
        }
    }
    #[test]
    fn cell_validator_is_send_without_sync_and_advisory_drop_cancels_provider() {
        fn send<T: Send>(_: &T) {}
        let p = P::new(Mode::Park);
        let external = CancellationToken::new();
        let e = evaluator(EvalMode::Satisfy, &external, p.reader.clone());
        let client = OperationClient::new(&p, e, ExternalSchemas::default());
        let call = client
            .invoke(
                OperationRequest::binding(doc(r#"{"input":true}"#), "b"),
                external.clone(),
            )
            .unwrap();
        send(&call);
        drop(call);
        {
            let advisory = client
                .preflight(
                    OperationRequest::binding(doc(r#"{"input":false}"#), "b"),
                    external,
                )
                .unwrap();
            send(&advisory);
            let mut advisory = pin!(advisory);
            assert!(poll(advisory.as_mut()).is_pending());
        }
        assert!(
            p.advisory_control
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .is_cancelled()
        );
    }
}
