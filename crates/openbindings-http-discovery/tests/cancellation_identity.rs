use openbindings::WorkControl;
use openbindings_http_discovery::{Cancellation, CancellationToken};

#[test]
fn companions_reexport_one_token_and_future_identity() {
    let discovery = CancellationToken::new();
    let invocation: openbindings_invocation::CancellationToken = discovery.clone();
    let inspection: openbindings_source_inspection::CancellationToken = invocation.clone();
    let synthesis: openbindings_interface_synthesis::CancellationToken = inspection.clone();
    let waits: [Cancellation; 4] = [
        discovery.cancelled(),
        {
            let wait: openbindings_invocation::Cancellation = invocation.cancelled();
            wait
        },
        {
            let wait: openbindings_source_inspection::Cancellation = inspection.cancelled();
            wait
        },
        {
            let wait: openbindings_interface_synthesis::Cancellation = synthesis.cancelled();
            wait
        },
    ];
    let synchronous: &dyn WorkControl = &synthesis;
    assert!(synchronous.check().is_ok());
    discovery.cancel();
    assert_eq!(synchronous.check(), Err(openbindings::Cancelled));
    assert!(invocation.is_cancelled() && inspection.is_cancelled());
    drop(waits);
}
