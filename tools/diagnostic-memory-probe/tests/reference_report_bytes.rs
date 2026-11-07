//! A removed report bound must abort at 256 MiB, not retain gigabytes.
use openbindings::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

struct Ceiling;
static LIVE: AtomicUsize = AtomicUsize::new(0);
const CEILING: usize = 256 * 1024 * 1024;
const POINTER_BYTES: usize = 8 * 1024 * 1024;
fn reserve(bytes: usize) -> bool {
    #[allow(deprecated)] // Keep the declared Rust 1.88 MSRV.
    LIVE.fetch_update(Relaxed, Relaxed, |live| {
        live.checked_add(bytes).filter(|&next| next <= CEILING)
    })
    .is_ok()
}
// SAFETY: Layout, pointer ownership and alignment pass unchanged to System.
// Accounting uses only atomics. Refused allocations return null per GlobalAlloc.
unsafe impl GlobalAlloc for Ceiling {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        let pointer = unsafe { System.alloc(layout) };
        if pointer.is_null() {
            LIVE.fetch_sub(layout.size(), Relaxed);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size(), Relaxed);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // Conservatively fund both blocks while System may move the allocation.
        if !reserve(size) {
            return std::ptr::null_mut();
        }
        let next = unsafe { System.realloc(pointer, layout, size) };
        LIVE.fetch_sub(if next.is_null() { size } else { layout.size() }, Relaxed);
        next
    }
}
#[global_allocator]
static ALLOCATOR: Ceiling = Ceiling;

// Run the existing small, transitive and unresolved reference fixtures unchanged
// in this same bounded-allocation binary as well as in the core test suite.
#[path = "../../../crates/openbindings/tests/reference_inspection_boundary.rs"]
mod existing_fixtures;

fn assert_report(report: ReferenceReport, source: &str, target: &str, count: usize) {
    assert!(!report.complete);
    assert_eq!(report.references.len(), count);
    let limitation = report.limitation.unwrap();
    assert_eq!(limitation.reason, ReferenceRefusalReason::LimitExceeded);
    assert_eq!(limitation.code, "reference-pointer-bytes");
    assert!(limitation.location.is_none());
    let mut bytes = 0;
    for (i, reference) in report.references.iter().enumerate() {
        let expected = format!("{source}/allOf/{i}/$ref");
        assert_eq!(reference.location.pointer, expected);
        assert_eq!(reference.keyword, "$ref");
        assert_eq!(reference.spelling.as_deref(), Some("#target"));
        let ReferenceResolution::Located {
            target: actual,
            dynamic_lookup,
        } = &reference.resolution
        else {
            panic!("located target")
        };
        assert!(!dynamic_lookup);
        assert_eq!(actual.pointer, target);
        bytes += expected.len() + target.len();
    }
    assert_eq!(bytes, POINTER_BYTES, "the last entry fits exactly");
    assert!(bytes + format!("{source}/allOf/{count}/$ref").len() + target.len() > POINTER_BYTES);
}

#[test]
fn long_source_and_shared_long_target_stop_at_the_exact_allowance_on_both_routes() {
    for long_target in [false, true] {
        // Eight one-MiB entries fit exactly. 20,000 unbounded entries would
        // retain about 20 GiB, while the input is below two MiB.
        let fixed_source = "/schemas/refs";
        let fixed_target = "/schemas/target";
        let overhead = if long_target {
            format!("{fixed_source}/allOf/0/$ref").len() + "/schemas/".len()
        } else {
            "/schemas//allOf/0/$ref".len() + fixed_target.len()
        };
        let key = "a".repeat(1024 * 1024 - overhead);
        let (source, target) = if long_target {
            (fixed_source.to_owned(), format!("/schemas/{key}"))
        } else {
            (format!("/schemas/{key}"), fixed_target.to_owned())
        };
        let reference = r##"{"$ref":"#target"}"##;
        let entries = std::iter::repeat_n(reference, 20_000)
            .collect::<Vec<_>>()
            .join(",");
        let (source_key, target_key) = if long_target {
            ("refs", key.as_str())
        } else {
            (key.as_str(), "target")
        };
        let text = format!(
            r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"{source_key}":{{"allOf":[{entries}]}},"{target_key}":{{"$anchor":"target"}}}}}}"#
        );
        let document = Document::parse(text).unwrap();
        let proof = document
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant long-key fixture");
        let control = AtomicBool::new(false);
        assert_report(
            document.references_with_resources(ExternalSchemas::default(), &control),
            &source,
            &target,
            8,
        );
        assert_report(
            proof.references_with_resources(ExternalSchemas::default(), &control),
            &source,
            &target,
            8,
        );
    }
}

#[test]
fn unpaired_names_charge_two_bytes_per_unit_including_the_keyword() {
    let target = "/operations/op/input";
    let prefix = "/operations/op/input/$defs/";
    let suffix = "/allOf/0/$dynamicRef";
    let key_len = (1024 * 1024 - target.len()) / 2 - prefix.len() - suffix.len() - 1;
    let key = "a".repeat(key_len);
    let entries = std::iter::repeat_n(r##"{"$dynamicRef":"#target"}"##, 100)
        .collect::<Vec<_>>()
        .join(",");
    let text = format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"$dynamicAnchor":"target","$defs":{{"{key}\ud800":{{"allOf":[{entries}]}}}}}}}}}}}}"#
    );
    let document = Document::parse(text).unwrap();
    // Unpaired JSON strings are admitted drafts but violate document OBI-01.
    {
        let report = document.references();
        assert!(!report.complete);
        assert_eq!(report.limitation.unwrap().code, "reference-pointer-bytes");
        assert_eq!(report.references.len(), 8);
        let mut bytes = 0;
        for (i, reference) in report.references.iter().enumerate() {
            let expected: Vec<u16> = format!("{prefix}{key}")
                .encode_utf16()
                .chain([0xd800])
                .chain(format!("/allOf/{i}/$dynamicRef").encode_utf16())
                .collect();
            assert!(reference.location.pointer.as_str().is_none());
            assert!(reference.location.pointer.utf16_units().eq(expected));
            let ReferenceResolution::Located {
                target: actual,
                dynamic_lookup,
            } = &reference.resolution
            else {
                panic!("located")
            };
            assert!(*dynamic_lookup);
            assert_eq!(actual.pointer, target);
            bytes += 2 * reference.location.pointer.utf16_units().count() + target.len();
        }
        assert_eq!(bytes, POINTER_BYTES);
    }
}

#[test]
fn unresolved_refusal_pointers_share_the_allowance() {
    let prefix = "/operations/op/input/$defs/";
    // The keyword location and the failure's schema location each own a pointer.
    let key_len = (POINTER_BYTES - 5) / 2 - prefix.len() - "/allOf/0".len();
    let key = "a".repeat(key_len);
    let text = format!(
        r##"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"$defs":{{"{key}":{{"allOf":[{{"$ref":"https://missing.test/schema"}},{{"$ref":"https://missing.test/schema"}}]}}}}}}}}}}}}"##
    );
    let document = Document::parse(text).unwrap();
    let validation = document.validate().unwrap();
    assert!(
        validation.conformant().is_some(),
        "{:?}",
        validation.report().evidence
    );
    let proof = validation.conformant().unwrap();
    for report in [document.references(), proof.references()] {
        assert_eq!(report.references.len(), 1);
        assert_eq!(report.limitation.unwrap().code, "reference-pointer-bytes");
        let reference = &report.references[0];
        let ReferenceResolution::Unresolved { detail } = &reference.resolution else {
            panic!("unresolved")
        };
        let source = reference.location.pointer.as_str().unwrap();
        let failure = detail.location.as_ref().unwrap().pointer.as_str().unwrap();
        assert_eq!(source.len() + failure.len(), POINTER_BYTES - 1);
        assert_eq!(source, format!("{failure}/$ref"));
    }
}
