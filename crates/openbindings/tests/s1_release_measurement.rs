//! Manual CR-1/CR-2 measurement, never a timing assertion.
use openbindings::{Conformance, assess_document};
use std::{fmt::Write, hint::black_box, time::Instant};

fn fixture(n: usize, duplicates: bool) -> String {
    let mut text = String::from("{\"openbindings\":\"0.2.0\",\"operations\":{},");
    if duplicates {
        text.push_str("\"x-e\":{");
        for i in 0..2 * n {
            if i > 0 {
                text.push(',');
            }
            write!(text, "\"k{}\":0", i % n).unwrap();
        }
    } else {
        text.push_str("\"schemas\":{");
        for i in 0..n {
            if i > 0 {
                text.push(',');
            }
            write!(text, "\"s{i}\":{{\"$ref\":\"#/schemas/s{}\"}}", n - 1).unwrap();
        }
    }
    text.push_str("}}");
    text
}

#[test]
#[ignore = "manual release measurement; run with --release --ignored --nocapture"]
fn cr1_cr2_release_medians() {
    println!("debug_assertions={}", cfg!(debug_assertions));
    for (duplicates, sizes) in [
        (true, [10_000, 20_000, 46_000]),
        (false, [10_000, 25_000, 40_000]),
    ] {
        for n in sizes {
            let text = fixture(n, duplicates);
            let expected = if duplicates {
                Conformance::NonConformant
            } else {
                Conformance::Conformant
            };
            for _ in 0..2 {
                assert_eq!(
                    assess_document(&text).unwrap().report().conclusion,
                    expected
                );
            }
            let mut elapsed = Vec::new();
            for _ in 0..5 {
                let start = Instant::now();
                let assessment = black_box(assess_document(black_box(&text)).unwrap());
                elapsed.push(start.elapsed());
                assert_eq!(assessment.report().conclusion, expected);
            }
            elapsed.sort_unstable();
            println!(
                "CR-{} N={n} bytes={} median_ms={:.3}",
                if duplicates { 1 } else { 2 },
                text.len(),
                elapsed[2].as_secs_f64() * 1000.0
            );
        }
    }
}
