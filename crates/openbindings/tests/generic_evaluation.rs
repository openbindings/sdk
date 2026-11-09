use openbindings::*;
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

fn selected() -> Schema {
    Document::parse(r#"{"openbindings":"0.2.0","operations":{"run":{"input":true}}}"#)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
        .resolve_operation("run")
        .unwrap()
        .input()
        .unwrap()
}
struct LocalEvaluator(Rc<Cell<usize>>);
struct LocalValidator {
    root: SchemaNode,
    evaluations: Rc<Cell<usize>>,
}
impl SchemaEvaluator for LocalEvaluator {
    type Validator = LocalValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Ok(LocalValidator {
            root: request.environment(limits, control)?.root(),
            evaluations: self.0.clone(),
        })
    }
}
impl SchemaValidator for LocalValidator {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        assert_eq!(self.root.value().view().as_bool(), Some(true));
        self.evaluations.set(self.evaluations.get() + 1);
        ValueOutcome::Satisfies
    }
}
#[test]
fn local_nonclone_validator_owns_only_what_it_retains() {
    let calls = Rc::new(Cell::new(0));
    let prepared = {
        let evaluator = LocalEvaluator(calls.clone());
        let schema = selected();
        let external = ExternalSchemas::default();
        evaluator
            .prepare(
                EvaluationRequest {
                    schema: &schema,
                    external_schemas: &external,
                },
                &PreparationLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap()
    };
    assert!(matches!(
        prepared.evaluate(&JsonValue::null(), Detail::Outcome, &AtomicBool::new(false)),
        ValueOutcome::Satisfies
    ));
    assert_eq!(calls.get(), 1);
    let validator: LocalValidator = prepared;
    assert_eq!(
        validator.root.pointer().as_str(),
        Some("/operations/run/input")
    );
}
struct SendEvaluator;
struct SendValidator(Cell<usize>);
impl SchemaEvaluator for SendEvaluator {
    type Validator = SendValidator;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Ok(SendValidator(Cell::new(0)))
    }
}
impl SchemaValidator for SendValidator {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        self.0.set(self.0.get() + 1);
        ValueOutcome::Satisfies
    }
}
#[test]
fn owned_send_validator_does_not_require_sync_or_arc() {
    fn assert_send<T: Send>(_: &T) {}
    let prepared = selected()
        .prepare(&SendEvaluator, &ExternalSchemas::default())
        .unwrap();
    assert_send(&prepared);
    std::thread::spawn(move || {
        assert!(matches!(
            prepared.evaluate(&JsonValue::null()),
            ValueOutcome::Satisfies
        ))
    })
    .join()
    .unwrap();
}
struct CancelDuring<'a> {
    signal: &'a AtomicBool,
    calls: &'a Cell<usize>,
    drops: Rc<Cell<usize>>,
}
struct Dropped(Rc<Cell<usize>>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
impl SchemaValidator for Dropped {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
impl SchemaEvaluator for CancelDuring<'_> {
    type Validator = Dropped;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Dropped, PreparationFailure> {
        self.calls.set(self.calls.get() + 1);
        self.signal.store(true, Ordering::Release);
        Ok(Dropped(self.drops.clone()))
    }
}
#[test]
fn preparation_cancellation_never_returns_or_leaks_a_ready_validator() {
    let calls = Cell::new(0);
    let drops = Rc::new(Cell::new(0));
    let signal = AtomicBool::new(true);
    let evaluator = CancelDuring {
        signal: &signal,
        calls: &calls,
        drops: drops.clone(),
    };
    let schema = selected();
    let external = ExternalSchemas::default();
    let limits = PreparationLimits::default();
    assert_eq!(
        schema
            .prepare_with_control(&evaluator, &external, &limits, &signal)
            .unwrap_err()
            .kind,
        PreparationFailureKind::Cancelled
    );
    assert_eq!(calls.get(), 0);
    let signal = AtomicBool::new(false);
    let evaluator = CancelDuring {
        signal: &signal,
        calls: &calls,
        drops: drops.clone(),
    };
    assert_eq!(
        schema
            .prepare_with_control(&evaluator, &external, &limits, &signal)
            .unwrap_err()
            .kind,
        PreparationFailureKind::Cancelled
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(drops.get(), 1);
    assert!(
        schema
            .prepare_with_control(
                &SendEvaluator,
                &external,
                &limits,
                &std::sync::atomic::AtomicBool::new(false)
            )
            .is_ok()
    );
}
struct ResolveOnly;
impl SchemaEvaluator for ResolveOnly {
    type Validator = SendValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<SendValidator, PreparationFailure> {
        let environment = request.environment(limits, control)?;
        let external = environment
            .resolve(&environment.root(), "https://example.test/external")?
            .schema;
        environment.resolve(&external, "#missing")?;
        Ok(SendValidator(Cell::new(0)))
    }
}
#[test]
fn local_reference_defect_is_a_preparation_cause_not_an_instance_verdict() {
    let external = ExternalSchemas::new([openbindings::ExternalSchema {
        uri: ("https://example.test/external").into(),
        document: JsonValue::parse("{}").unwrap(),
    }])
    .unwrap();
    let error = selected().prepare(&ResolveOnly, &external).unwrap_err();
    assert_eq!(
        error.kind,
        PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::Undefined(ReferenceDefect::MissingAnchor)
        }
    );
    assert_eq!(
        error.origin.unwrap().source_uri.as_deref(),
        Some("https://example.test/external")
    );
}
