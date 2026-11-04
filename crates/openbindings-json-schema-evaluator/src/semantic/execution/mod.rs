//! Explicit schema applications. A frame is unfinished evaluation, not schema data.
mod array;
mod logical;
mod object;
#[cfg(test)]
mod tests;
use super::*;

pub(super) enum Attempt {
    Complete(State),
    Abort,
}
pub(super) enum Reference {
    Ready(State),
    Target(usize),
}
struct Request<'v> {
    node: usize,
    value: JsonRef<'v>,
    context: Context,
}
struct Frame<'v> {
    visit: Visit,
    value: JsonRef<'v>,
    fields: readings::Fields,
    state: State,
    phase: Phase,
}
enum Phase {
    Start,
    References {
        next: usize,
        waiting: bool,
        only: bool,
    },
    Logical(logical::Cursor),
    Object(object::Cursor),
    Array(array::Cursor),
    Done,
}
enum Action<'v> {
    Continue,
    Child(Request<'v>),
    Complete,
}
enum Entry<'v> {
    Ready(State),
    Frame(Frame<'v>),
    Abort,
}
enum Beginning {
    Ready(State),
    Body(State),
    ReferenceObject,
}
impl<'v> Frame<'v> {
    fn request(&self, node: usize, value: JsonRef<'v>) -> Action<'v> {
        Action::Child(Request {
            node,
            value,
            context: self.visit.context.clone(),
        })
    }
    fn child(
        &self,
        run: &Run<'_>,
        child: JsonRef<'_>,
        value: JsonRef<'v>,
    ) -> Result<Action<'v>, NoVerdict> {
        Ok(self.request(run.child(self.visit.schema, child)?, value))
    }
    fn schema<'p>(&self, plan: &'p Plan) -> Interpreted<'p> {
        let original = plan.environment.space().node(self.visit.schema);
        Interpreted {
            value: original.value.view(),
            profile: original.profile.expect("entered known schema"),
            fields: self.fields,
        }
    }
    fn advance(
        &mut self,
        run: &mut Run<'_>,
        mut returned: Option<State>,
    ) -> Result<Action<'v>, NoVerdict> {
        loop {
            run.graph.tick()?;
            let phase = std::mem::replace(&mut self.phase, Phase::Done);
            let action = match phase {
                Phase::Start => {
                    match run.begin(
                        self.visit.schema,
                        self.fields,
                        self.value,
                        &self.visit.context,
                    )? {
                        Beginning::Ready(state) => {
                            self.state = state;
                            Action::Complete
                        }
                        Beginning::Body(state) => {
                            self.state = state;
                            self.phase = Phase::References {
                                next: 0,
                                waiting: false,
                                only: false,
                            };
                            Action::Continue
                        }
                        Beginning::ReferenceObject => {
                            self.phase = Phase::References {
                                next: 0,
                                waiting: false,
                                only: true,
                            };
                            Action::Continue
                        }
                    }
                }
                Phase::References {
                    mut next,
                    waiting,
                    only,
                } => {
                    if waiting {
                        let child = returned.take().expect("reference child result");
                        if only {
                            self.state = child;
                            return Ok(Action::Complete);
                        }
                        run.conjunction(&mut self.state, &child)?;
                        next += 1;
                    }
                    let schema = self.schema(run.plan);
                    let mut requested = None;
                    for (index, keyword) in ["$ref", "$dynamicRef", "$recursiveRef"]
                        .iter()
                        .enumerate()
                        .skip(next)
                    {
                        if let Some(reference) = schema.get(keyword) {
                            match run.reference(
                                self.visit.schema,
                                keyword,
                                reference,
                                self.value,
                                &self.visit.context,
                            )? {
                                Reference::Ready(state) => {
                                    if only {
                                        self.state = state;
                                        return Ok(Action::Complete);
                                    }
                                    run.conjunction(&mut self.state, &state)?;
                                }
                                Reference::Target(node) => {
                                    requested = Some((index, node));
                                    break;
                                }
                            }
                        }
                    }
                    if let Some((next, node)) = requested {
                        self.phase = Phase::References {
                            next,
                            waiting: true,
                            only,
                        };
                        self.request(node, self.value)
                    } else {
                        self.phase = Phase::Logical(logical::Cursor::start());
                        Action::Continue
                    }
                }
                Phase::Logical(cursor) => {
                    if self.fields.logical() {
                        logical::advance(self, run, cursor, returned.take())?
                    } else {
                        debug_assert!(returned.is_none());
                        self.phase = Phase::Object(object::Cursor::start());
                        Action::Continue
                    }
                }
                Phase::Object(cursor) => {
                    if self.fields.object() {
                        object::advance(self, run, cursor, returned.take())?
                    } else {
                        debug_assert!(returned.is_none());
                        self.phase = Phase::Array(array::Cursor::start());
                        Action::Continue
                    }
                }
                Phase::Array(cursor) => {
                    if self.fields.array() {
                        array::advance(self, run, cursor, returned.take())?
                    } else {
                        debug_assert!(returned.is_none());
                        Action::Complete
                    }
                }
                Phase::Done => Action::Complete,
            };
            if !matches!(action, Action::Continue) {
                return Ok(action);
            }
        }
    }
}
impl Run<'_> {
    pub(super) fn execute(
        &mut self,
        node: usize,
        value: JsonRef<'_>,
    ) -> Result<Attempt, NoVerdict> {
        let result = self.execute_frames(node, value);
        // Errors and provisional aborts do not complete suspended parents.
        // Reusing a later attempt must never observe their active membership.
        self.active.clear();
        result
    }
    fn execute_frames(&mut self, node: usize, value: JsonRef<'_>) -> Result<Attempt, NoVerdict> {
        let mut frames = Vec::<Frame<'_>>::new();
        let mut request = Some(Request {
            node,
            value,
            context: self.empty_context.clone(),
        });
        let mut returned = None;
        loop {
            if let Some(next) = request.take() {
                let entered = self.enter_application(next)?;
                self.graph.tick()?;
                match entered {
                    Entry::Ready(state) => returned = Some(state),
                    Entry::Frame(frame) => {
                        let needed = frames
                            .len()
                            .checked_add(1)
                            .ok_or_else(|| decision::limit("semantic-analysis-memory-limit"))?;
                        storage::reserve(&mut frames, needed, &mut self.graph)?;
                        frames.push(frame);
                    }
                    Entry::Abort => return Ok(Attempt::Abort),
                }
            }
            let Some(frame) = frames.last_mut() else {
                return Ok(Attempt::Complete(returned.expect("root result")));
            };
            match frame.advance(self, returned.take())? {
                Action::Child(next) => request = Some(next),
                Action::Complete => {
                    let frame = frames.pop().expect("completed frame");
                    returned = Some(self.complete_application(frame)?);
                }
                Action::Continue => unreachable!("advance finishes dispatch"),
            }
        }
    }
    fn enter_application<'v>(&mut self, request: Request<'v>) -> Result<Entry<'v>, NoVerdict> {
        let Request {
            node,
            value,
            context,
        } = request;
        self.graph.tick()?;
        let fields = self.plan.fields(node);
        if fields.is_leaf() {
            let context = self.enter(node, &context)?;
            return match self.begin(node, fields, value, &context)? {
                Beginning::Ready(state) => Ok(Entry::Ready(state)),
                _ => unreachable!("certified leaf has no child application"),
            };
        }
        let (node, context, pure_cycle) = self.alias(node, &context)?;
        if pure_cycle {
            return self
                .effect(
                    Class::Undefined,
                    node,
                    "$alias-cycle",
                    value,
                    &context,
                    None,
                )
                .map(Entry::Ready);
        }
        let fields = self.plan.fields(node);
        if fields.is_leaf() {
            return match self.begin(node, fields, value, &context)? {
                Beginning::Ready(state) => Ok(Entry::Ready(state)),
                _ => unreachable!("alias reached a certified leaf"),
            };
        }
        self.context_copy(&context)?;
        let key = Visit {
            schema: node,
            instance: backend::ref_id(value),
            context: context.clone(),
        };
        if let Some(known) = self.memo.get(&key) {
            return known.copied(&mut self.graph).map(Entry::Ready);
        }
        if self.opaque_cycles.contains(&key) {
            return self
                .unknown_schema(
                    Class::Inability,
                    node,
                    "$cycle-summary",
                    value,
                    &context,
                    None,
                )
                .map(Entry::Ready);
        }
        if let Some(start) = self
            .active
            .find(&key, context_work(&context), &mut self.graph)?
        {
            for (visit, ()) in self.active.suffix(start) {
                reserve_context(&mut self.graph, &visit.context)?;
                self.graph.charge(256)?;
                self.pending_cycles.insert(visit.clone());
            }
            let first = self.atoms.len();
            let result = self.recursive_cut(&key, value)?;
            for id in first..self.atoms.len() {
                self.graph.charge(32)?;
                self.cuts.insert(id as u32);
            }
            return Ok(Entry::Ready(result));
        }
        if self.provisional.contains(&key) {
            return Ok(Entry::Abort);
        }
        self.active
            .push(key.clone(), (), context_work(&context), &mut self.graph)?;
        Ok(Entry::Frame(Frame {
            visit: key,
            value,
            fields,
            state: State::truth(TRUE),
            phase: Phase::Start,
        }))
    }
    fn complete_application(&mut self, frame: Frame<'_>) -> Result<State, NoVerdict> {
        let Frame {
            visit: key,
            value,
            state: mut result,
            ..
        } = frame;
        let (active, ()) = self.active.pop().expect("completing active application");
        assert!(active == key, "ordered application completion");
        for annotation in result
            .properties
            .values_mut()
            .chain(result.items.values_mut())
        {
            *annotation = self.graph.and(result.valid, *annotation)?;
        }
        let complete = result.independent(&mut self.graph, &self.cuts)?;
        if !complete || self.facts.contains_key(&key) {
            self.certify_coordinates(&key, value, &result)?;
        }
        self.context_copy(&key.context)?;
        if complete && self.plan.queries.is_empty() {
            // Original child positions form a finite tree without references.
            // No cyclic refinement or repeated same-instance application can
            // reuse this frame. Keep its result with the parent, not once per
            // instance in a call-wide memo. Effect identities remain separate.
            return Ok(result);
        }
        if complete {
            let copied = result.copied(&mut self.graph)?;
            self.graph.charge(256)?;
            self.memo.insert(key, copied);
        } else {
            self.graph.charge(256)?;
            self.provisional.insert(key);
        }
        Ok(result)
    }
}

impl Run<'_> {
    fn begin(
        &mut self,
        node: usize,
        fields: readings::Fields,
        value: JsonRef<'_>,
        context: &Context,
    ) -> Result<Beginning, NoVerdict> {
        // Plan owns the original source for this entire run. Borrowing through
        // that immutable owner keeps later mutable Run calls independent.
        let plan = self.plan;
        let schema = &plan.environment.space().node(node).value;
        if let Some(boolean) = schema.view().as_bool() {
            let mut state = State::truth(if boolean { TRUE } else { FALSE });
            if !boolean {
                self.record_failure(&mut state, node, value, None)?;
            }
            return Ok(Beginning::Ready(state));
        }
        if !self.space().eligible(self.space().node(node).resource) {
            return self
                .unknown_schema(Class::Inability, node, "$identity", value, context, None)
                .map(Beginning::Ready);
        }
        if self
            .space()
            .query_inventory(self.space().node(node).resource)
            .is_err()
        {
            return self
                .unknown_schema(Class::Inability, node, "$dialect", value, context, None)
                .map(Beginning::Ready);
        }
        let Some(profile) = self.space().node(node).profile else {
            return self
                .unknown_schema(Class::Inability, node, "$dialect", value, context, None)
                .map(Beginning::Ready);
        };
        let schema = Interpreted {
            value: schema.view(),
            profile,
            fields,
        };
        if profile.reference_object(schema.value) {
            return Ok(Beginning::ReferenceObject);
        }
        let mut state = State::truth(TRUE);
        // Identifier declarations establish resource/base context, not an
        // assertion about this instance. A malformed identifier matters when
        // a reference actually depends on that context; reference() retains
        // the original query failure instead of inventing a validity atom.
        match self.primitives(node, &schema, value) {
            Ok(None) => {}
            Ok(Some(keyword)) => {
                state.valid = FALSE;
                self.record_failure(&mut state, node, value, Some(keyword))?;
            }
            Err(detail)
                if matches!(
                    detail.reason,
                    NoVerdictReason::LimitExceeded | NoVerdictReason::Cancelled
                ) =>
            {
                return Err(detail);
            }
            Err(detail) => {
                let unknown =
                    self.effect(Class::Inability, node, "$predicate", value, context, None)?;
                self.cause(unknown.valid, detail)?;
                self.conjunction(&mut state, &unknown)?;
            }
        }
        if let Some(format) = self.plan.formats.get(&node) {
            match format.evaluate(value, &mut self.graph) {
                Ok(true) => {}
                Ok(false) => {
                    state.valid = FALSE;
                    self.record_failure(&mut state, node, value, Some("format"))?;
                }
                Err(detail)
                    if matches!(
                        detail.reason,
                        NoVerdictReason::LimitExceeded | NoVerdictReason::Cancelled
                    ) =>
                {
                    return Err(detail);
                }
                Err(detail) => {
                    let unknown =
                        self.effect(Class::Inability, node, "format", value, context, None)?;
                    self.cause(unknown.valid, detail)?;
                    self.conjunction(&mut state, &unknown)?;
                }
            }
        }
        if let Some(pattern) = schema.get("pattern")
            && value.kind() == JsonKind::String
        {
            let matched = self.pattern(node, "pattern", pattern, value, context)?;
            state.valid = self.graph.and(state.valid, matched)?;
            if matched == FALSE {
                self.record_failure(&mut state, node, value, Some("pattern"))?;
            }
        }
        if fields.is_leaf() {
            return Ok(Beginning::Ready(state));
        }
        Ok(Beginning::Body(state))
    }
}
