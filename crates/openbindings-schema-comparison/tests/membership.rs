use openbindings::JsonValue;
use openbindings_schema_comparison::*;
fn input(text: &str) -> SchemaInput {
    let value = JsonValue::parse(text).unwrap();
    SchemaInput::new(value.clone(), value)
}

#[test]
fn enum_and_required_membership_fit_n_log_n_work_and_keep_first_failure() {
    for key in ["enum", "required"] {
        let n = 4096usize;
        let names = (0..n)
            .map(|i| format!("\"v{i:04}\""))
            .collect::<Vec<_>>()
            .join(",");
        let extras = (0..n)
            .map(|i| format!("\"a{i:04}\""))
            .collect::<Vec<_>>()
            .join(",");
        let schema = |values: &str| input(&format!(r#"{{"type":"object","{key}":[{values}]}}"#));
        let small = schema(&names);
        let large = schema(&format!("{extras},{names}"));
        let failing = schema(&format!("{names},\"z-first\",\"z-second\""));
        let limits = ComparisonLimits {
            max_work: Some(n * (n.ilog2() as usize + 1) * 1024),
            ..Default::default()
        };
        for direction in [Direction::Input, Direction::Output] {
            let swap = (key == "required") == (direction == Direction::Input);
            let (target, candidate) = if swap {
                (&large, &small)
            } else {
                (&small, &large)
            };
            assert_eq!(
                compare(direction, target, candidate, &limits)
                    .unwrap()
                    .verdict(),
                Verdict::Compatible
            );
            let (target, candidate) = if swap {
                (&large, &failing)
            } else {
                (&failing, &large)
            };
            let result = compare(direction, target, candidate, &limits).unwrap();
            assert_eq!(result.verdict(), Verdict::Incompatible);
            assert!(result.reason().unwrap().contains("z-first"));
            assert!(!result.reason().unwrap().contains("z-second"));
        }
    }
}

#[test]
fn indexed_enums_preserve_exact_values_and_const_intersection_reasons() {
    let numbers = "1,1.0,1e0,1e999999999999999999999,9007199254740993";
    let extra = r#"null,false,true,"\ud800","é","e\u0301",[],{}"#;
    let flowing = input(&format!(r#"{{"enum":[{numbers},{extra}]}}"#));
    let choices = input(&format!(
        r#"{{"enum":[{extra},10e999999999999999999998,9007199254740993,1.00]}}"#
    ));
    for direction in [Direction::Input, Direction::Output] {
        assert_eq!(
            compare(direction, &flowing, &choices, &ComparisonLimits::default())
                .unwrap()
                .verdict(),
            Verdict::Compatible
        );
    }
    let rejected = input(&format!(
        r#"{{"enum":[{numbers},{extra},9007199254740992,"later"]}}"#
    ));
    let result = compare(
        Direction::Input,
        &rejected,
        &choices,
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert_eq!(
        result.reason(),
        Some("enum: target value 9007199254740992 not in candidate enum")
    );
    let both = input(r#"{"const":1,"enum":[2,3,4,5,6,7,8,9,10]}"#);
    let ones = input(r#"{"enum":[1,1.0,1e0]}"#);
    let result = compare(Direction::Input, &ones, &both, &ComparisonLimits::default()).unwrap();
    assert_eq!(
        result.reason(),
        Some("enum: target value 1 not in candidate enum")
    );
}
