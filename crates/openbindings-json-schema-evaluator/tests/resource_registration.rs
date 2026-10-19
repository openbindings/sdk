//! Preparation registers every projected resource with the library's `Registry`
//! one at a time, before any compilation work is counted, so registering must
//! cost time linear in the number of resources. A builder that rebuilds its
//! pending resources on every `add` (and so on every pair `extend` adds) costs
//! O(n^2) instead.
//!
//! Each test times registration of `N` and `4 * N` resources, alternating, and
//! keeps the fastest run of each size. Linear growth gives a ratio near 4 and
//! quadratic growth near 16, so a bound of 8 leaves a factor of two on either
//! side. Other work on the host can only slow a run, which raises a minimum and
//! never lowers it, and the test passes as soon as the fastest large run is
//! within the bound.
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const N: usize = 1_000;
const RUNS: usize = 5;
const BOUND: u32 = 8;

fn resources(count: usize) -> Vec<(String, Value)> {
    (0..count)
        .map(|i| {
            (
                format!("https://example.test/resource/{i}"),
                json!({"type": "integer"}),
            )
        })
        .collect()
}

fn add_each(resources: &[(String, Value)]) -> Duration {
    let start = Instant::now();
    let mut builder = jsonschema::Registry::new();
    for (uri, schema) in resources {
        builder = builder.add(uri, schema).expect("valid URI");
    }
    let elapsed = start.elapsed();
    drop(builder);
    elapsed
}

fn extend_all(resources: &[(String, Value)]) -> Duration {
    let start = Instant::now();
    let builder = jsonschema::Registry::new()
        .extend(resources.iter().map(|(uri, schema)| (uri, schema)))
        .expect("valid URIs");
    let elapsed = start.elapsed();
    drop(builder);
    elapsed
}

fn assert_linear(register: fn(&[(String, Value)]) -> Duration) {
    let small = resources(N);
    let large = resources(4 * N);
    let (mut small_best, mut large_best) = (Duration::MAX, Duration::MAX);
    for _ in 0..RUNS {
        small_best = small_best.min(register(&small));
        large_best = large_best.min(register(&large));
        if large_best < small_best * BOUND {
            return;
        }
    }
    panic!(
        "registering {} resources took {large_best:?}, more than {BOUND} times the \
         {small_best:?} for {N}: registration grows faster than linearly",
        4 * N
    );
}

// One test runs both, one after the other, so neither times its runs while the
// other allocates on another thread.
#[test]
fn registering_many_resources_is_linear() {
    assert_linear(add_each);
    assert_linear(extend_all);
}
