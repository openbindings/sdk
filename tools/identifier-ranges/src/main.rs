//! Offline, explicit reproduction of the runtime identifier property tables.
use regex_syntax::hir::{Class, HirKind};
use std::{fmt::Write, path::PathBuf};

fn generated() -> String {
    let mut out = String::from(
        "// Generated from regex-syntax 0.8.11 public HIR properties, Unicode 16.0.0.\n\
         // Reproduce with tools/identifier-ranges; never generated during a build.\n\
         // Unicode data terms: LICENSE-UNICODE in this directory.\n\n",
    );
    for (name, property) in [("START", "ID_Start"), ("PART", "ID_Continue")] {
        let hir = regex_syntax::parse(&format!(r"\p{{{property}}}")).unwrap();
        let HirKind::Class(Class::Unicode(class)) = hir.into_kind() else {
            panic!("fixed property is a Unicode range class")
        };
        writeln!(out, "pub(super) const {name}: &[(u32, u32)] = &[").unwrap();
        for range in class.ranges() {
            writeln!(
                out,
                "    (0x{:X}, 0x{:X}),",
                u32::from(range.start()),
                u32::from(range.end())
            )
            .unwrap();
        }
        out.push_str("];\n");
        if name == "START" {
            out.push('\n');
        }
    }
    out
}

fn main() {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/openbindings-json-schema-evaluator/src/pattern/identifier/ranges.rs");
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--write" => std::fs::write(target, generated()).unwrap(),
        [flag] if flag == "--check" => {
            assert!(
                std::fs::read_to_string(target).unwrap() == generated(),
                "pinned identifier ranges drifted; review dependency/data changes explicitly"
            );
            println!(
                "Pinned identifier range reproduction passed (regex-syntax 0.8.11, Unicode 16.0.0)."
            );
        }
        _ => panic!("use --check or explicitly regenerate with --write"),
    }
}
