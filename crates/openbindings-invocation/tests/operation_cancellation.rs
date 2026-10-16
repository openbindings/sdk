// Cancellation and lifecycle regressions for the operation invoker.
#[cfg(test)]
mod tests {
    use openbindings::{
        Document, JsonValue, NoVerdict, PreparedSchema, ResourceSet, SchemaEvaluator,
        SchemaRequest, ValueOutcome, ValueProblem,
    };
    use openbindings_invocation::CancellationToken;
    use openbindings_invocation::{
        BindingRequest, InvocationError, ProviderIo,
        binding::BindingInvoker,
        operation::{Invoker, Request},
    };
    use std::{
        future::{Future, poll_fn},
        pin::Pin,
        sync::{
            Arc, Barrier,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };

    fn doc() -> Document {
        Document::parse(r#"{"openbindings":"0.2.0","operations":{"run":{"input":true}},"bindings":{"b":{"operation":"run","source":"s"}},"sources":{"s":{"kind":"test"}}}"#).unwrap()
    }
    struct Evaluator {
        barrier: Option<Arc<Barrier>>,
        saw_cancel: Arc<AtomicBool>,
    }
    struct Prepared {
        barrier: Option<Arc<Barrier>>,
        saw_cancel: Arc<AtomicBool>,
    }
    impl SchemaEvaluator for Evaluator {
        fn prepare(
            &self,
            _: &SchemaRequest,
            _: &dyn openbindings::WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            Ok(Arc::new(Prepared {
                barrier: self.barrier.clone(),
                saw_cancel: self.saw_cancel.clone(),
            }))
        }
    }
    impl PreparedSchema for Prepared {
        fn validate(&self, _: &JsonValue, c: &dyn openbindings::WorkControl) -> ValueOutcome {
            if let Some(barrier) = &self.barrier {
                barrier.wait();
                let until = Instant::now() + Duration::from_secs(2);
                while Instant::now() < until {
                    if c.is_cancelled() {
                        self.saw_cancel.store(true, Ordering::SeqCst);
                        break;
                    }
                    std::thread::yield_now();
                }
            }
            ValueOutcome::Fails {
                message: "value does not satisfy the contract".into(),
                problems: vec![ValueProblem {
                    details: None,
                    instance_pointer: String::new(),
                    schema_location: None,
                    code: "test.false".into(),
                    message: "reject".into(),
                }],
                problems_complete: true,
            }
        }
    }
    struct Provider {
        repeat_poll: bool,
        eof: Arc<AtomicBool>,
    }
    impl BindingInvoker for Provider {
        type Invocation<'a> =
            Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
        fn supports_kind(&self, k: &str) -> bool {
            k == "test"
        }
        fn invoke<'a>(
            &'a self,
            _: BindingRequest<'a>,
            mut io: ProviderIo,
            _: CancellationToken,
        ) -> Self::Invocation<'a> {
            Box::pin(async move {
                let value = if self.repeat_poll {
                    let mut next = std::pin::pin!(io.next());
                    poll_fn(|cx| match next.as_mut().poll(cx) {
                        Poll::Ready(value) => Poll::Ready(value),
                        Poll::Pending => next.as_mut().poll(cx),
                    })
                    .await
                } else {
                    io.next().await
                };
                if value.is_none() {
                    self.eof.store(true, Ordering::SeqCst);
                }
                Ok(())
            })
        }
    }
    #[test]
    fn external_cancel_reaches_cooperative_evaluator() {
        let barrier = Arc::new(Barrier::new(2));
        let saw_cancel = Arc::new(AtomicBool::new(false));
        let invoker = Invoker::new(
            doc().validate().unwrap().conformant().unwrap(),
            Provider {
                repeat_poll: false,
                eof: Arc::default(),
            },
        )
        .with_evaluator(
            Arc::new(Evaluator {
                barrier: Some(barrier.clone()),
                saw_cancel: saw_cancel.clone(),
            }),
            ResourceSet::default(),
        );
        let control = CancellationToken::new();
        let mut call = invoker.invoke_with_control(Request::binding("b"), &control);
        assert!(
            std::pin::pin!(call.write(JsonValue::null()))
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        );
        let cancelling = std::thread::spawn(move || {
            barrier.wait();
            control.cancel();
        });
        let result = std::pin::pin!(call.next()).poll(&mut Context::from_waker(Waker::noop()));
        cancelling.join().unwrap();
        let Poll::Ready(result) = result else {
            panic!("terminal must be ready")
        };
        assert_eq!(result.unwrap().unwrap_err().code(), "ERR_CANCELLED");
        assert!(
            saw_cancel.load(Ordering::SeqCst),
            "external cancellation should be observable by cooperative evaluator"
        );
        assert!(
            call.validation_failure().is_none(),
            "the losing evaluator failure must not attach evidence to cancellation"
        );
        assert!(call.control().validation_failure().is_none());
    }
    #[test]
    fn rejected_input_cannot_turn_into_eof_on_second_poll() {
        let eof = Arc::new(AtomicBool::new(false));
        let invoker = Invoker::new(
            doc().validate().unwrap().conformant().unwrap(),
            Provider {
                repeat_poll: true,
                eof: eof.clone(),
            },
        )
        .with_evaluator(
            Arc::new(Evaluator {
                barrier: None,
                saw_cancel: Arc::default(),
            }),
            ResourceSet::default(),
        );
        let mut call = invoker.invoke(Request::binding("b"));
        assert!(
            std::pin::pin!(call.write(JsonValue::null()))
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        );
        let result = std::pin::pin!(call.next()).poll(&mut Context::from_waker(Waker::noop()));
        let Poll::Ready(result) = result else {
            panic!("terminal must be ready")
        };
        assert_eq!(
            result.unwrap().unwrap_err().code(),
            "ERR_OPERATION_VALIDATION_FAILED"
        );
        assert!(
            !eof.load(Ordering::SeqCst),
            "rejected input was observed by provider as clean EOF"
        );
    }
    struct CancelThenComplete(CancellationToken);
    impl BindingInvoker for CancelThenComplete {
        type Invocation<'a> =
            Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
        fn supports_kind(&self, k: &str) -> bool {
            k == "test"
        }
        fn invoke<'a>(
            &'a self,
            _: BindingRequest<'a>,
            _: ProviderIo,
            local: CancellationToken,
        ) -> Self::Invocation<'a> {
            Box::pin(async move {
                self.0.cancel();
                assert!(local.is_cancelled());
                Ok(())
            })
        }
    }
    impl openbindings_invocation::binding::BindingPreflight for CancelThenComplete {
        type Preflight<'a>
            = std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            Option<openbindings_invocation::ContextChallenge>,
                            InvocationError,
                        >,
                    > + Send
                    + 'a,
            >,
        >
        where
            Self: 'a;
        fn preflight<'a>(
            &'a self,
            _: BindingRequest<'a>,
            local: CancellationToken,
        ) -> Self::Preflight<'a> {
            Box::pin(async move {
                self.0.cancel();
                assert!(local.is_cancelled());
                Ok(None)
            })
        }
    }

    #[test]
    fn cancellation_during_poll_overrides_ready_provider_success() {
        let external = CancellationToken::new();
        let invoker = Invoker::new(
            doc().validate().unwrap().conformant().unwrap(),
            CancelThenComplete(external.clone()),
        );
        let mut call = invoker.invoke_with_control(Request::binding("b"), &external);
        let result = std::pin::pin!(call.next()).poll(&mut Context::from_waker(Waker::noop()));
        let Poll::Ready(result) = result else {
            panic!("terminal must be ready")
        };
        assert_eq!(result.unwrap().unwrap_err().code(), "ERR_CANCELLED");
        assert_eq!(call.outcome().unwrap().unwrap_err().code(), "ERR_CANCELLED");
        assert!(call.validation_failure().is_none());
        assert!(external.is_cancelled());
    }

    #[test]
    fn cancellation_during_poll_overrides_ready_preflight_success() {
        let external = CancellationToken::new();
        let invoker = Invoker::new(
            doc().validate().unwrap().conformant().unwrap(),
            CancelThenComplete(external.clone()),
        );
        let result = std::pin::pin!(
            invoker.preflight_with_provider_and_control(Request::binding("b"), &external)
        )
        .poll(&mut Context::from_waker(Waker::noop()));
        let Poll::Ready(result) = result else {
            panic!("preflight must be ready")
        };
        assert_eq!(result.unwrap_err().code(), "ERR_CANCELLED");
        assert!(external.is_cancelled());
    }
    struct RejectOutputThenCancel(CancellationToken);
    impl BindingInvoker for RejectOutputThenCancel {
        type Invocation<'a> =
            Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
        fn supports_kind(&self, k: &str) -> bool {
            k == "test"
        }
        fn invoke<'a>(
            &'a self,
            _: BindingRequest<'a>,
            mut io: ProviderIo,
            _: CancellationToken,
        ) -> Self::Invocation<'a> {
            Box::pin(async move {
                assert_eq!(
                    io.emit(JsonValue::null()).await.unwrap_err().code(),
                    "ERR_OPERATION_VALIDATION_FAILED"
                );
                self.0.cancel();
                Ok(())
            })
        }
    }
    #[test]
    fn already_settled_validation_keeps_its_evidence_when_external_cancel_follows() {
        let external = CancellationToken::new();
        let document=Document::parse(r#"{"openbindings":"0.2.0","operations":{"run":{"output":true}},"bindings":{"b":{"operation":"run","source":"s"}},"sources":{"s":{"kind":"test"}}}"#).unwrap();
        let invoker = Invoker::new(
            document.validate().unwrap().conformant().unwrap(),
            RejectOutputThenCancel(external.clone()),
        )
        .with_evaluator(
            Arc::new(Evaluator {
                barrier: None,
                saw_cancel: Arc::default(),
            }),
            ResourceSet::default(),
        );
        let mut call = invoker.invoke_with_control(Request::binding("b"), &external);
        let result = std::pin::pin!(call.next()).poll(&mut Context::from_waker(Waker::noop()));
        let Poll::Ready(result) = result else {
            panic!("terminal must be ready")
        };
        assert_eq!(
            result.unwrap().unwrap_err().code(),
            "ERR_OPERATION_VALIDATION_FAILED"
        );
        assert!(external.is_cancelled());
        let evidence = call.validation_failure().unwrap();
        assert_eq!(evidence.direction(), openbindings::Direction::Output);
        assert!(matches!(evidence.outcome(), ValueOutcome::Fails { .. }));
        let control = call.control();
        control.cancel();
        assert_eq!(
            control.outcome().unwrap().unwrap_err().code(),
            "ERR_OPERATION_VALIDATION_FAILED"
        );
        assert!(Arc::ptr_eq(
            &evidence,
            &control.validation_failure().unwrap()
        ));
        assert!(matches!(
            std::pin::pin!(call.next()).poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(None)
        ));
    }

    struct Preparing(Arc<Barrier>);
    impl SchemaEvaluator for Preparing {
        fn prepare(
            &self,
            _: &SchemaRequest,
            control: &dyn openbindings::WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            self.0.wait();
            let until = Instant::now() + Duration::from_secs(5);
            while !control.is_cancelled() && Instant::now() < until {
                std::thread::yield_now();
            }
            assert!(
                control.is_cancelled(),
                "relay must reach synchronous preparation"
            );
            control.check()?;
            unreachable!()
        }
    }
    #[test]
    fn external_cancel_reaches_preparation_in_current_poll() {
        let barrier = Arc::new(Barrier::new(2));
        let invoker = Invoker::new(
            doc().validate().unwrap().conformant().unwrap(),
            Provider {
                repeat_poll: false,
                eof: Arc::default(),
            },
        )
        .with_evaluator(Arc::new(Preparing(barrier.clone())), ResourceSet::default());
        let external = CancellationToken::new();
        let mut call = invoker.invoke_with_control(Request::binding("b"), &external);
        assert!(
            std::pin::pin!(call.write(JsonValue::null()))
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        );
        let thread = std::thread::spawn(move || {
            barrier.wait();
            external.cancel();
        });
        let result = std::pin::pin!(call.next()).poll(&mut Context::from_waker(Waker::noop()));
        thread.join().unwrap();
        assert!(matches!(result, Poll::Ready(Some(Err(e))) if e.code() == "ERR_CANCELLED"));
        assert!(call.validation_failure().is_none());
    }

    struct CancelDuringStart(CancellationToken);
    impl BindingInvoker for CancelDuringStart {
        type Invocation<'a> = std::future::Ready<Result<(), InvocationError>>;
        fn supports_kind(&self, _: &str) -> bool {
            true
        }
        fn invoke<'a>(
            &'a self,
            _: BindingRequest<'a>,
            _: ProviderIo,
            local: CancellationToken,
        ) -> Self::Invocation<'a> {
            self.0.cancel();
            assert!(local.is_cancelled(), "relay must be installed before start");
            std::future::ready(Ok(()))
        }
    }
    impl openbindings_invocation::binding::BindingPreflight for CancelDuringStart {
        type Preflight<'a> = std::future::Ready<
            Result<Option<openbindings_invocation::ContextChallenge>, InvocationError>,
        >;
        fn preflight<'a>(
            &'a self,
            _: BindingRequest<'a>,
            local: CancellationToken,
        ) -> Self::Preflight<'a> {
            self.0.cancel();
            assert!(
                local.is_cancelled(),
                "relay must be installed before preflight start"
            );
            std::future::ready(Ok(None))
        }
    }
    #[test]
    fn synchronous_provider_and_preflight_start_observe_relay() {
        let external = CancellationToken::new();
        let invoker = Invoker::new(
            doc().validate().unwrap().conformant().unwrap(),
            CancelDuringStart(external.clone()),
        );
        let mut call = invoker.invoke_with_control(Request::binding("b"), &external);
        assert!(
            matches!(std::pin::pin!(call.next()).poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Some(Err(e))) if e.code() == "ERR_CANCELLED")
        );
        let external = CancellationToken::new();
        let invoker = Invoker::new(
            doc().validate().unwrap().conformant().unwrap(),
            CancelDuringStart(external.clone()),
        );
        assert!(
            matches!(std::pin::pin!(invoker.preflight_with_provider_and_control(Request::binding("b"), &external))
            .poll(&mut Context::from_waker(Waker::noop())), Poll::Ready(Err(e)) if e.code() == "ERR_CANCELLED")
        );
    }
}
