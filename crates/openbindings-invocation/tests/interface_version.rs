use openbindings::JsonValue;
use openbindings_invocation::INTERFACE_VERSION;

#[test]
fn interface_version_matches_both_pinned_contracts() {
    for contract in [
        include_str!("fixtures/binding-invoker-0.1.json"),
        include_str!("fixtures/operation-invoker-0.1.json"),
    ] {
        let contract = JsonValue::parse(contract).unwrap();
        assert_eq!(
            contract.get("version").unwrap().as_str(),
            Some(INTERFACE_VERSION)
        );
    }
}
