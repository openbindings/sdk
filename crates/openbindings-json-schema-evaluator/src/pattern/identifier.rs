//! Unicode membership for group names, without runtime table construction.
mod ranges;

fn contains(ranges: &[(u32, u32)], c: char) -> bool {
    let scalar = u32::from(c);
    ranges
        .binary_search_by(|&(start, end)| {
            if end < scalar {
                std::cmp::Ordering::Less
            } else if start > scalar {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

pub(crate) fn start(c: char) -> bool {
    c.is_ascii_alphabetic() || (!c.is_ascii() && contains(ranges::START, c))
}
pub(crate) fn part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || (!c.is_ascii() && contains(ranges::PART, c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex_syntax::hir::{Class, HirKind};

    #[test]
    fn named_group_grammar_keeps_explicit_additions_and_decoded_identity() {
        use crate::pattern::{Reading, Refusal, read};
        for name in [
            "é",
            "éé",
            "_",
            "$",
            "a1",
            "a\u{200C}",
            "a\u{200D}",
            "\u{2118}",
            "a\u{B7}",
            "𐐀",
            r"\u{10400}",
            r"\u00e9",
        ] {
            let source = format!("(?<{name}>x)");
            assert!(
                matches!(read(&source, 32, 1 << 20), Ok(Reading::Linear(_))),
                "{source}"
            );
        }
        for name in ["1a", "\u{301}x", "😀", r"\ud800", r"\udc00", "a-"] {
            let source = format!("(?<{name}>x)");
            assert!(
                matches!(read(&source, 32, 1 << 20), Err(Refusal::Invalid)),
                "{source}"
            );
        }
        for source in [r"(?<é>x)(?<\u00e9>y)", r"(?<𐐀>x)(?<\u{10400}>y)"] {
            assert!(
                matches!(read(source, 32, 1 << 20), Err(Refusal::Invalid)),
                "{source}"
            );
        }
    }

    #[test]
    fn pinned_tables_and_all_scalar_membership_match_the_property_parser() {
        for (table, property, member) in [
            (ranges::START, "ID_Start", start as fn(char) -> bool),
            (ranges::PART, "ID_Continue", part as fn(char) -> bool),
        ] {
            let hir = regex_syntax::parse(&format!(r"\p{{{property}}}")).unwrap();
            let HirKind::Class(Class::Unicode(class)) = hir.into_kind() else {
                panic!()
            };
            let expected: Vec<_> = class
                .ranges()
                .iter()
                .map(|r| (u32::from(r.start()), u32::from(r.end())))
                .collect();
            assert_eq!(table, expected);
            assert!(table.windows(2).all(|pair| pair[0].1 < pair[1].0));
            // A monotone interval sweep provides an independent oracle for the
            // binary search, including every boundary and neighboring scalar.
            let mut at = 0;
            for scalar in 0..=0x10FFFF {
                let Some(c) = char::from_u32(scalar) else {
                    continue;
                };
                while at < expected.len() && expected[at].1 < scalar {
                    at += 1;
                }
                let matches = at < expected.len() && expected[at].0 <= scalar;
                assert_eq!(member(c), matches, "{property} U+{scalar:X}");
            }
        }
    }
}
