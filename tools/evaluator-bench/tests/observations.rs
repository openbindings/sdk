//! Exercise the executable's fixed JSON protocol without naming SDK types.
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn run(request: Value) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_openbindings-evaluator-bench"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn document_validation_acquires_proof_only_for_conformance() {
    for (document, status, count) in [
        (
            r#"{"openbindings":"0.2.0","operations":{}}"#,
            "conformant",
            0,
        ),
        (
            r#"{"openbindings":"0.2.0","operations":{"bad name":{}}}"#,
            "non-conformant",
            2,
        ),
    ] {
        let output = run(json!({"mode":"document", "document":document,
            "expected":{"status":status,"finding_count":count}, "iterations":2}));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let observed: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(observed["outcomes"][status], 2);
        assert_eq!(observed["observation"]["report"]["conclusion"], status);
    }
}
#[test]
fn conformant_reads_visit_all_text_and_distinguish_absent_contracts() {
    let output = run(
        json!({"mode":"read", "document":r#"{"openbindings":"0.2.0","operations":{"a":{"aliases":["b"],"description":"\u00e9","tags":["x","y"],"input":false},"z":{"output":true}}}"#,
        "expected":{"status":"conformant", "reads":{"operations":2,"aliases":1,
            "descriptions":1,"tags":2,"inputs":1,"outputs":1,"text_bytes":7}}, "iterations":2}),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(observed["outcomes"]["read"], 2);
}
#[test]
fn explain_checks_reported_count_and_original_problem_locations() {
    for count in [1, 64] {
        let output = run(
            json!({"mode":"explain", "schema":r#"{"type":"array","items":{"type":"integer"}}"#,
            "value":serde_json::to_string(&vec!["bad"; count]).unwrap(), "iterations":2,
            "expected":{"status":"fails", "problem_count":1, "first_problem":{
                "code":"type", "instance_pointer":"/0", "schema_location":{
                    "pointer":"/items/type", "resource":"https://benchmark.invalid/root"},
                "message":"expected JSON type: integer"}}}),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let observed: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(observed["outcomes"]["fails"], 2);
    }
}
#[test]
fn incorrect_expected_outcomes_abort_instead_of_producing_timings() {
    let output = run(
        json!({"mode":"document", "document":r#"{"openbindings":"0.2.0","operations":{}}"#,
        "expected":{"status":"non-conformant"}}),
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected status"));
}
#[test]
fn each_mode_times_its_own_operation_and_counts_every_iteration() {
    let modes = [
        ("cold", 1, "prepared"),
        ("baseline", 3, "baseline"),
        ("parse", 3, "parsed"),
        ("compile", 3, "prepared"),
        ("context", 3, "prepared"),
        ("eval", 3, "satisfies"),
        ("full", 3, "satisfies"),
    ];
    for engine in ["sdk", "jsonschema-rs"] {
        for (mode, iterations, outcome) in modes {
            if engine == "jsonschema-rs" && mode == "context" {
                continue;
            }
            let output = run(
                json!({"engine":engine, "mode":mode, "iterations":iterations,
                "schema":r#"{"type":"integer","minimum":0}"#, "value":"42",
                "expected":{"status":"satisfies"}}),
            );
            assert!(
                output.status.success(),
                "{engine} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let observed: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                observed["outcomes"],
                json!({outcome: iterations}),
                "{engine} {mode}"
            );
        }
        // A failing value is counted as a failure, not as the mode's name.
        for mode in ["eval", "full"] {
            let output = run(json!({"engine":engine, "mode":mode, "iterations":3,
                "schema":r#"{"type":"integer","minimum":0}"#, "value":"-1",
                "expected":{"status":"fails"}}));
            let observed: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(observed["outcomes"], json!({"fails": 3}), "{engine} {mode}");
        }
    }
}
#[test]
fn unsupported_modes_abort_without_measuring() {
    for (engine, mode, message) in [
        ("sdk", "evaluate", "unknown mode"),
        ("jsonschema-rs", "context", "unsupported peer mode"),
    ] {
        let output = run(
            json!({"engine":engine, "mode":mode, "schema":"true", "value":"1",
            "expected":{"status":"satisfies"}}),
        );
        assert!(!output.status.success(), "{engine} {mode}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
    }
}
#[test]
fn provenance_reports_the_embedded_source_digests() {
    let output = Command::new(env!("CARGO_BIN_EXE_openbindings-evaluator-bench"))
        .arg("--provenance")
        .output()
        .unwrap();
    assert!(output.status.success());
    let reported: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["instrument", "sdk"] {
        let digest = reported[key].as_str().unwrap();
        assert_eq!(digest.len(), 16, "{key}: {digest}");
        assert!(
            digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "{key}: {digest}"
        );
    }
}
