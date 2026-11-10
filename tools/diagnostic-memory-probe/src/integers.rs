//! The selected exact-integer branch performs no heap allocation.
use super::*;
use openbindings_internal_json::{
    decimal::Decimal,
    numeric::{Bound, KEYWORDS},
};
use std::hint::black_box;

pub(super) fn run() {
    let bounds: Vec<_> = KEYWORDS
        .iter()
        .map(|key| Bound::parse(key, "7").unwrap())
        .collect();
    let tokens = [
        "-9223372036854775808",
        "-1",
        "-0",
        "0",
        "1",
        "9223372036854775807",
    ];
    let before = ALLOCATIONS.load(Relaxed);
    for _ in 0..10_000 {
        for token in tokens {
            let integer = black_box(token).parse::<i64>().unwrap();
            assert!(black_box(Decimal::parse(black_box(token))).is_integer());
            for bound in &bounds {
                assert!(black_box(bound.admits_integer(black_box(integer))).is_some());
            }
        }
    }
    let allocations = ALLOCATIONS.load(Relaxed) - before;
    assert_eq!(allocations, 0);
    println!(
        "{}",
        serde_json::json!({"phase":"integer-fast-path", "decimal_parses":60000, "bound_checks":300000, "allocations":allocations})
    );
}
