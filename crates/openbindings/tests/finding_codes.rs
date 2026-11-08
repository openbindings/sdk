use std::collections::BTreeSet;

// Read the existing source instead of introducing another production code list.
const SOURCE: &str = include_str!("../src/conformance.rs");
const REFERENCE: &str = include_str!("../README.md");

fn quoted_strings(source: &str) -> BTreeSet<String> {
    source
        .split('"')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, text)| text.to_owned())
        .collect()
}

// The emission code argument is a literal, a forwarded code, or an if expression.
// Respect nested delimiters and strings when separating the call's arguments.
fn argument(call: &str, index: usize) -> &str {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    let mut current = 0;
    for (offset, ch) in call.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '(' | '[' | '{' => depth += 1,
            ',' | ')' if depth == 0 => {
                if current == index {
                    return call[start..offset].trim();
                }
                assert_ne!(ch, ')', "missing code argument: {call}");
                current += 1;
                start = offset + 1;
            }
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    panic!("unterminated emission: {call}");
}

#[test]
fn documented_finding_codes_match_source() {
    let production = SOURCE.split("#[cfg(test)]").next().unwrap();
    let debug = production
        .split_once("let code = match self.code {")
        .unwrap()
        .1
        .split_once("=> self.code")
        .unwrap()
        .0;
    // Debug enumerates the parser codes forwarded to OBI-01 as well as fixed
    // emissions. Scan emission sites too, so a code Debug would redact fails here.
    let debug = quoted_strings(debug);
    assert!(!debug.contains("input-limit"));
    for code in ["byte-limit", "depth-limit", "node-limit"] {
        assert!(debug.contains(code), "missing parser limit code: {code}");
    }
    let mut emitted = debug.clone();
    for (method, index) in [
        (".violation(", 2),
        (".mark(", 2),
        (".mark_at(", 3),
        (".mark_at_fact(", 3),
    ] {
        for call in production.split(method).skip(1) {
            let code = argument(call, index);
            match code {
                "code" | "error.code()" => {}
                code if code.starts_with('"') => emitted.extend(quoted_strings(code)),
                code if code.starts_with("if ") => {
                    // Discard the condition's field name, retaining both branch codes.
                    emitted.extend(quoted_strings(code.split_once('{').unwrap().1));
                }
                other => panic!("unrecognized finding code expression: {other}"),
            }
        }
    }
    // `lines` strips both line endings, so a CRLF checkout reads the same table.
    let table: Vec<_> = REFERENCE
        .lines()
        .skip_while(|line| *line != "## Document finding codes")
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect();
    assert!(!table.is_empty(), "finding-code section missing");
    let mut documented = BTreeSet::new();
    for row in table.iter().filter(|line| line.starts_with("| `")) {
        let columns: Vec<_> = row.split('|').map(str::trim).collect();
        assert_eq!(columns.len(), 5, "malformed code row: {row}");
        assert!(columns[2].starts_with("OBI-"), "missing rule: {row}");
        assert!(!columns[3].is_empty(), "missing meaning: {row}");
        assert!(documented.insert(columns[1].trim_matches('`').to_owned()));
    }
    assert_eq!(
        documented, emitted,
        "document finding code inventory drifted"
    );
    assert_eq!(
        debug, emitted,
        "Finding's Debug would redact an emitted code"
    );
}
