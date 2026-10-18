//! Diagnostic probe, not for merge: the deepest pattern nesting each phase
//! completes on a small thread stack. Each trial runs in a child process
//! because a stack overflow aborts the process.
//!
//! cargo run -p openbindings-json-schema-evaluator --example pattern_stack_probe [--release]
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::process::Command;

const SHAPES: [(&str, &str, &str); 7] = [
    ("group", "(", ")"),
    ("noncap-star", "(?:", ")*"),
    ("alt", "(?:a|", ")"),
    ("class-plus", "(?:[a-z]", ")+"),
    ("lookahead", "(?=", ")"),
    ("neg-lookahead", "(?!", ")"),
    ("lookbehind", "(?<=", ")"),
];
// regex-new: regress parse and compile only (jsonschema::ob_ecma::Regex::new).
// regex-match: regress matching "a" only; compiled on a large stack.
// doc: document parse, validation and operation resolution only.
// prepare-*: the pattern_stack test's whole `prepared` path.
// check-*: value validation only; prepared on a large stack.
const PHASES: [&str; 7] = [
    "regex-new",
    "regex-match",
    "doc",
    "prepare-pattern",
    "prepare-pp",
    "check-pattern",
    "check-pp",
];
const MAX_DEPTH: usize = 300;

fn pattern(shape: &str, depth: usize) -> String {
    let (_, open, close) = SHAPES.iter().find(|(name, _, _)| *name == shape).unwrap();
    format!("{}a{}", open.repeat(depth), close.repeat(depth))
}

fn schema(keyword: &str, pattern: &str) -> String {
    let quoted = serde_json::to_string(pattern).unwrap();
    match keyword {
        "pattern" => format!(r#"{{"pattern":{quoted}}}"#),
        _ => format!(r#"{{"patternProperties":{{{quoted}:true}}}}"#),
    }
}

fn document(schema: &str) -> String {
    format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#)
}

fn prepare(schema: &str) -> Result<PreparedContract, NoVerdict> {
    let proof = Document::parse(document(schema))
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    let limits = Limits {
        pattern_depth: usize::MAX,
        ..Limits::default()
    };
    operation
        .input()
        .unwrap()
        .prepare(&DefaultEvaluator::with_limits(limits))
}

fn on_stack<T: Send + 'static>(bytes: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

const LARGE: usize = 256 * 1024 * 1024;

fn code(completed: bool) -> i32 {
    if completed { 0 } else { 2 }
}

// Exit 0: completed; 2: refused (an error, not an overflow).
fn child(phase: &str, shape: &str, depth: usize, kib: usize) -> i32 {
    let pattern = pattern(shape, depth);
    let small = kib * 1024;
    match phase {
        "regex-new" => {
            code(on_stack(small, move || jsonschema::ob_ecma::Regex::new(&pattern).is_ok()))
        }
        "regex-match" => {
            let Ok(regex) = on_stack(LARGE, move || jsonschema::ob_ecma::Regex::new(&pattern))
            else {
                return 2;
            };
            let matched = on_stack(small, move || {
                let result =
                    jsonschema::ob_ecma::top_level(2_000_000, || regex.is_match("a").is_ok());
                std::mem::forget(regex);
                result
            });
            code(matches!(matched, Ok(true)))
        }
        "doc" => code(on_stack(small, move || {
            let text = document(&schema("pattern", &pattern));
            let proof = Document::parse(text)
                .unwrap()
                .validate()
                .unwrap()
                .conformant()
                .unwrap();
            let operation = proof.resolve_operation("op").unwrap();
            operation.input().is_some()
        })),
        "prepare-pattern" | "prepare-pp" => {
            let keyword = if phase == "prepare-pattern" { "pattern" } else { "pp" };
            let schema = schema(keyword, &pattern);
            // Drop on the small stack too, as the test does.
            code(on_stack(small, move || prepare(&schema).is_ok()))
        }
        "check-pattern" | "check-pp" => {
            let (keyword, value) = if phase == "check-pattern" {
                ("pattern", r#""a""#)
            } else {
                ("pp", r#"{"a":1}"#)
            };
            let schema = schema(keyword, &pattern);
            let Ok(contract) = on_stack(LARGE, move || prepare(&schema)) else {
                return 2;
            };
            let value = JsonValue::parse(value).unwrap();
            let decided = on_stack(small, move || {
                let outcome = contract.validate(&value);
                // Keep the drop of the contract off the measured stack.
                std::mem::forget(contract);
                !matches!(outcome, ValueOutcome::NoVerdict { .. })
            });
            code(decided)
        }
        _ => unreachable!(),
    }
}

fn run(phase: &str, shape: &str, depth: usize, kib: usize) -> &'static str {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["child", phase, shape, &depth.to_string(), &kib.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    match status.code() {
        Some(0) => "ok",
        Some(2) => "refused",
        _ => "overflow",
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("child") {
        let code = child(
            &args[2],
            &args[3],
            args[4].parse().unwrap(),
            args[5].parse().unwrap(),
        );
        std::process::exit(code);
    }
    let only: Option<Vec<String>> = std::env::var("PROBE_PHASES")
        .ok()
        .map(|v| v.split(',').map(str::to_owned).collect());
    let stacks: Vec<usize> = std::env::var("PROBE_STACKS")
        .unwrap_or_else(|_| "256,512,1024".into())
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    println!(
        "evaluator-phases profile={} os={} arch={}",
        if cfg!(debug_assertions) { "debug" } else { "release" },
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("phase shape stack_kib deepest_ok first_failure");
    for phase in PHASES {
        if only.as_ref().is_some_and(|o| !o.iter().any(|p| p == phase)) {
            continue;
        }
        for (shape, _, _) in SHAPES {
            for &kib in &stacks {
                // Largest depth that completes, assuming deeper is never cheaper.
                if run(phase, shape, MAX_DEPTH, kib) == "ok" {
                    println!("{phase} {shape} {kib} {MAX_DEPTH} none");
                    continue;
                }
                let (mut lo, mut hi) = (0usize, MAX_DEPTH);
                while hi - lo > 1 {
                    let mid = (lo + hi) / 2;
                    if run(phase, shape, mid, kib) == "ok" {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let failure = run(phase, shape, hi, kib);
                println!("{phase} {shape} {kib} {lo} {failure}@{hi}");
            }
        }
    }
}
