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
    if shape == "none" {
        return String::new();
    }
    let (_, open, close) = SHAPES.iter().find(|(name, _, _)| *name == shape).unwrap();
    format!("{}a{}", open.repeat(depth), close.repeat(depth))
}

fn schema(keyword: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return r#"{"type":["string","object"]}"#.into();
    }
    let quoted = serde_json::to_string(pattern).unwrap();
    match keyword {
        "pattern" => format!(r#"{{"pattern":{quoted}}}"#),
        _ => format!(r#"{{"patternProperties":{{{quoted}:true}}}}"#),
    }
}

fn document(schema: &str) -> String {
    format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#)
}

fn evaluator() -> DefaultEvaluator {
    DefaultEvaluator::with_limits(Limits {
        pattern_depth: usize::MAX,
        ..Limits::default()
    })
}

fn prepare(schema: &str) -> Result<PreparedContract, NoVerdict> {
    let proof = Document::parse(document(schema))
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    operation.input().unwrap().prepare(&evaluator())
}

// Validate the document on a large stack and run only `prepare` on the small one.
fn prepare_only(schema: &str, small: usize) -> bool {
    let proof = on_stack(LARGE, {
        let text = document(schema);
        move || {
            Document::parse(text)
                .unwrap()
                .validate()
                .unwrap()
                .conformant()
                .unwrap()
        }
    });
    let operation = proof.resolve_operation("op").unwrap();
    let input = operation.input().unwrap();
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(small)
            .spawn_scoped(scope, || {
                let prepared = input.prepare(&evaluator());
                let ok = prepared.is_ok();
                std::mem::forget(prepared);
                ok
            })
            .unwrap()
            .join()
            .unwrap()
    })
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
        // Warm the process with one complete preparation of the same schema on a
        // large stack, then repeat the phase on the small stack.
        "warm-doc" | "warm-prepare-pattern" | "warm-prepare-only-pattern" => {
            let text = schema("pattern", &pattern);
            let warm = text.clone();
            assert!(on_stack(LARGE, move || prepare(&warm).is_ok()));
            match phase {
                "warm-doc" => code(on_stack(small, move || {
                    let proof = Document::parse(document(&text))
                        .unwrap()
                        .validate()
                        .unwrap()
                        .conformant()
                        .unwrap();
                    proof.resolve_operation("op").unwrap().input().is_some()
                })),
                "warm-prepare-pattern" => code(on_stack(small, move || prepare(&text).is_ok())),
                _ => code(prepare_only(&text, small)),
            }
        }
        // The engine alone, warmed once on a large stack: meta-schema check, and
        // a whole build (which includes the meta-schema check).
        "js-meta" | "js-build" | "js-build-id" => {
            let mut value: serde_json::Value =
                serde_json::from_str(&schema("pattern", &pattern)).unwrap();
            if phase == "js-build-id" {
                // A projected resource carries an `$id`, whose meta-schema
                // entry includes a pattern with a loop.
                value["$id"] = "urn:example:probe".into();
            }
            let build = |value: &serde_json::Value| {
                jsonschema::options()
                    .with_draft(jsonschema::Draft::Draft202012)
                    .should_validate_formats(false)
                    .build(value)
                    .map(std::mem::forget)
                    .is_ok()
            };
            let warm = value.clone();
            assert!(on_stack(LARGE, move || build(&warm)));
            if phase == "js-meta" {
                code(on_stack(small, move || jsonschema::meta::is_valid(&value)))
            } else {
                code(on_stack(small, move || build(&value)))
            }
        }
        "prepare-only-pattern" | "prepare-only-pp" => {
            let keyword = if phase == "prepare-only-pattern" { "pattern" } else { "pp" };
            code(prepare_only(&schema(keyword, &pattern), small))
        }
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
    if std::env::var("PROBE_MODE").as_deref() == Ok("min-stack") {
        min_stack();
        return;
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

// The smallest thread stack, in 4 KiB steps, on which a fixed workload completes.
fn min_stack() {
    println!(
        "evaluator-min-stack profile={} os={} arch={}",
        if cfg!(debug_assertions) { "debug" } else { "release" },
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("phase shape depth min_stack_kib");
    let mut workloads: Vec<(&str, &str, usize)> = vec![
        ("doc", "none", 0),
        ("prepare-pattern", "none", 0),
        ("prepare-only-pattern", "none", 0),
        ("warm-doc", "none", 0),
        ("warm-prepare-pattern", "none", 0),
        ("warm-prepare-only-pattern", "none", 0),
        ("check-pattern", "none", 0),
        ("js-meta", "none", 0),
        ("js-build", "none", 0),
        ("js-meta", "group", 1),
        ("js-build", "group", 1),
        ("js-build", "group", 32),
        ("js-build-id", "none", 0),
    ];
    for depth in [1, 2, 4, 8, 16, 32] {
        for shape in ["group", "lookahead"] {
            workloads.push(("prepare-pattern", shape, depth));
            workloads.push(("prepare-only-pattern", shape, depth));
            workloads.push(("warm-prepare-pattern", shape, depth));
            workloads.push(("warm-prepare-only-pattern", shape, depth));
            workloads.push(("regex-new", shape, depth));
        }
    }
    for depth in [1, 2, 4] {
        for shape in ["group", "noncap-star", "lookahead", "neg-lookahead", "lookbehind"] {
            workloads.push(("check-pattern", shape, depth));
            workloads.push(("regex-match", shape, depth));
        }
    }
    workloads.push(("check-pattern", "group", 32));
    workloads.push(("check-pattern", "noncap-star", 32));
    for (phase, shape, depth) in workloads {
        let (mut lo, mut hi) = (0usize, 1024usize); // in 4 KiB units
        if run(phase, shape, depth, hi * 4) != "ok" {
            println!("{phase} {shape} {depth} >4096");
            continue;
        }
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if run(phase, shape, depth, mid * 4) == "ok" {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        println!("{phase} {shape} {depth} {}", hi * 4);
    }
}
