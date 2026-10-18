//! Diagnostic probe, not for merge: which regress stage uses stack per
//! nesting level. Each trial runs in a child process because a stack
//! overflow aborts the process.
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
const STAGES: [&str; 5] = ["parse", "optimize", "emit", "drop-ir", "match"];
const MAX_DEPTH: usize = 300;
const LARGE: usize = 256 * 1024 * 1024;

fn flags() -> regress::Flags {
    regress::Flags {
        unicode: true,
        es2020: true,
        ..Default::default()
    }
}

fn pattern(shape: &str, depth: usize) -> String {
    let (_, open, close) = SHAPES.iter().find(|(name, _, _)| *name == shape).unwrap();
    format!("{}a{}", open.repeat(depth), close.repeat(depth))
}

fn on_stack<T: Send + 'static>(bytes: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn child(stage: &str, shape: &str, depth: usize, kib: usize) -> i32 {
    use regress::backends::{emit, optimize, try_parse};
    let text = pattern(shape, depth);
    let small = kib * 1024;
    let parse = move || try_parse(text.chars().map(u32::from), flags());
    match stage {
        "parse" => {
            if on_stack(small, move || parse().map(std::mem::forget).is_ok()) { 0 } else { 2 }
        }
        "optimize" => {
            let Ok(mut ir) = on_stack(LARGE, parse) else { return 2 };
            let ir = on_stack(small, move || {
                optimize(&mut ir);
                ir
            });
            std::mem::forget(ir);
            0
        }
        "emit" => {
            let Ok(ir) = on_stack(LARGE, move || {
                let mut ir = parse()?;
                optimize(&mut ir);
                Ok::<_, regress::Error>(ir)
            }) else {
                return 2;
            };
            let ir = on_stack(small, move || {
                std::mem::forget(emit(&ir));
                ir
            });
            std::mem::forget(ir);
            0
        }
        "drop-ir" => {
            let Ok(mut ir) = on_stack(LARGE, parse) else { return 2 };
            optimize(&mut ir);
            on_stack(small, move || drop(ir));
            0
        }
        "match" => {
            let source = pattern(shape, depth);
            let Ok(regex) = on_stack(LARGE, move || regress::Regex::with_flags(&source, flags()))
            else {
                return 2;
            };
            // Completion, not whether it matched, is what is measured.
            on_stack(small, move || {
                let _ = regex.find("a").is_some();
                std::mem::forget(regex);
            });
            0
        }
        _ => unreachable!(),
    }
}

fn run(stage: &str, shape: &str, depth: usize, kib: usize) -> &'static str {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["child", stage, shape, &depth.to_string(), &kib.to_string()])
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
        let code = child(&args[2], &args[3], args[4].parse().unwrap(), args[5].parse().unwrap());
        std::process::exit(code);
    }
    let stacks: Vec<usize> = std::env::var("PROBE_STACKS")
        .unwrap_or_else(|_| "256,512,1024".into())
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    println!(
        "regress-stages profile={} os={} arch={}",
        if cfg!(debug_assertions) { "debug" } else { "release" },
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("stage shape stack_kib deepest_ok first_failure");
    for stage in STAGES {
        for (shape, _, _) in SHAPES {
            for &kib in &stacks {
                if run(stage, shape, MAX_DEPTH, kib) == "ok" {
                    println!("{stage} {shape} {kib} {MAX_DEPTH} none");
                    continue;
                }
                let (mut lo, mut hi) = (0usize, MAX_DEPTH);
                while hi - lo > 1 {
                    let mid = (lo + hi) / 2;
                    if run(stage, shape, mid, kib) == "ok" {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let failure = run(stage, shape, hi, kib);
                println!("{stage} {shape} {kib} {lo} {failure}@{hi}");
            }
        }
    }
}
