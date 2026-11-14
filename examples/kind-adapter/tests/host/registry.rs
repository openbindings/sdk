//! Host policy: one registered slot per exact kind, validated before insertion.
use super::erased::{DynChecker, DynDescriptor, DynInvoker, DynPuller, Work};
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, ContextChallenge, InvocationError,
    InvocationSupport, ProviderIo,
};
use openbindings_kind_description::{DescribeKindRequest, DescriptionLimits};
use std::{collections::BTreeMap, error::Error, future::ready, rc::Rc};

pub struct Slot {
    pub descriptor: Rc<dyn DynDescriptor>,
    pub checker: Option<Rc<dyn DynChecker>>,
    pub puller: Option<Rc<dyn DynPuller>>,
    pub invoker: Option<Rc<dyn DynInvoker>>,
}

#[derive(Default)]
pub struct Registry(BTreeMap<String, Slot>);
impl Registry {
    pub async fn register(&mut self, kind: &str, slot: Slot) -> Result<(), Box<dyn Error>> {
        if self.0.contains_key(kind) {
            return Err("duplicate kind registration".into());
        }
        let limits = DescriptionLimits::default();
        let listed = slot
            .descriptor
            .list(CancellationToken::new(), limits)
            .await?;
        let summary = listed
            .iter()
            .find(|s| s.kind().as_str() == Some(kind))
            .ok_or("kind missing from listing")?;
        let description = slot
            .descriptor
            .describe(
                DescribeKindRequest::new(kind)?,
                CancellationToken::new(),
                limits,
            )
            .await?;
        let listed: std::collections::BTreeSet<_> = summary
            .supports()
            .map(|key| key.as_str().ok_or("support key is not scalar text"))
            .collect::<Result<_, _>>()?;
        let described: std::collections::BTreeSet<_> = description
            .supports()
            .map(|key| key.as_str().ok_or("support key is not scalar text"))
            .collect::<Result<_, _>>()?;
        if listed != described {
            return Err("published support disagreement".into());
        }
        // Presence of a disclosure is checked against supports by carrier admission.
        let invokes = slot.invoker.as_ref().is_some_and(|i| i.can_invoke(kind));
        if description.invoke().is_some() != invokes
            || description.check().is_some() != slot.checker.is_some()
            || description.pull().is_some() != slot.puller.is_some()
        {
            return Err("registered role disagreement".into());
        }
        self.0.insert(kind.to_owned(), slot);
        Ok(())
    }
    pub fn get(&self, kind: &str) -> Option<&Slot> {
        self.0.get(kind)
    }

    fn route(&self, request: &BindingRequest) -> Result<&dyn DynInvoker, InvocationError> {
        let binding = request
            .interface
            .binding(&request.binding)
            .ok_or_else(InvocationError::binding_not_found)?;
        self.get(binding.source().kind())
            .and_then(|slot| slot.invoker.as_deref())
            .ok_or_else(InvocationError::refused)
    }
}
impl InvocationSupport for Registry {
    fn can_invoke(&self, kind: &str) -> bool {
        self.get(kind)
            .and_then(|slot| slot.invoker.as_ref())
            .is_some_and(|invoker| invoker.can_invoke(kind))
    }
}
impl BindingInvoker for Registry {
    type Invoke<'a> = Work<'a, Result<(), InvocationError>>;
    type Preflight<'a> = Work<'a, Result<Option<ContextChallenge>, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invoke<'a> {
        match self.route(&request) {
            Ok(invoker) => invoker.forward_invoke(request, io, control),
            Err(error) => Box::pin(ready(Err(error))),
        }
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Self::Preflight<'a> {
        match self.route(&request) {
            Ok(invoker) => invoker.forward_preflight(request, control),
            Err(error) => Box::pin(ready(Err(error))),
        }
    }
}
