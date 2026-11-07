use openbindings::WorkControl;
use openbindings_http_discovery::{Cancellation, CancellationToken};

#[test]
fn companions_reexport_one_token_and_future_identity() {
    let discovery = CancellationToken::new();
    let invocation: openbindings_invocation::CancellationToken = discovery.clone();
    let description: openbindings_kind_description::CancellationToken = invocation.clone();
    let pulling: openbindings_source_pull::CancellationToken = description.clone();
    let waits: [Cancellation; 4] = [
        discovery.cancelled(),
        {
            let wait: openbindings_invocation::Cancellation = invocation.cancelled();
            wait
        },
        {
            let wait: openbindings_kind_description::Cancellation = description.cancelled();
            wait
        },
        {
            let wait: openbindings_source_pull::Cancellation = pulling.cancelled();
            wait
        },
    ];
    let synchronous: &dyn WorkControl = &pulling;
    assert!(synchronous.check().is_ok());
    discovery.cancel();
    assert_eq!(synchronous.check(), Err(openbindings::Cancelled));
    assert!(invocation.is_cancelled() && description.is_cancelled());
    drop(waits);
}
