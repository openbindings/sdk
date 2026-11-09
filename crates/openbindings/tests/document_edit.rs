//! Document edits preserve exact bytes and retained proof ownership.
use openbindings::*;
use std::{ops::Range, path::Path};

fn exact(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn escape(text: &str) -> String {
    text.replace('~', "~0").replace('/', "~1")
}

#[test]
fn atomic_failure_keeps_bytes_caches_views_and_proof() {
    let mut doc =
        Document::parse(" {\n\"openbindings\":\"0.2.0\",\"operations\":{\"go\":{}}} ").unwrap();
    let proof = doc.validate().unwrap().conformant().unwrap();
    let view = proof.resolve_operation("go").unwrap();
    let bytes = doc.as_bytes().to_vec();
    let value = exact("42");
    for edits in [
        vec![
            Edit::set_at("/name", (value).clone()),
            Edit::remove_at("/missing"),
        ],
        vec![
            Edit::set_at("/operations", (value).clone()),
            Edit::remove_at("/operations/go"),
        ],
        vec![
            Edit::set_at("/new", (value).clone()),
            Edit::set_at("/new", (value).clone()),
        ],
        vec![Edit::set_at("/~2", (value).clone())],
        vec![Edit::remove_at("")],
    ] {
        assert!(doc.edit(&edits).is_err());
        assert_eq!(doc.as_bytes(), bytes);
        assert_eq!(
            doc.validate().unwrap().report().conclusion,
            Conformance::Conformant
        );
    }
    doc = doc
        .edit(&[Edit::set(
            &["operations", "go", "description"],
            JsonValue::string("new\nline").unwrap(),
        )])
        .unwrap();
    assert_eq!(
        doc.value()
            .at("/operations/go/description")
            .unwrap()
            .as_str(),
        Some("new\nline")
    );
    assert_eq!(proof.document().as_bytes(), bytes);
    assert!(view.description().is_none());
    let mut doc = proof.into_inner();
    doc = doc
        .set("/name", &JsonValue::string("editable again").unwrap())
        .unwrap();
    assert_eq!(
        doc.value().get("name").unwrap().as_str(),
        Some("editable again")
    );
}

#[test]
fn sibling_insertions_coalesce_in_batch_order_and_escape_keys() {
    for (input, expected) in [
        ("{ }", "{ \"a/b\":1e+03,\"c~d\":-0}"),
        (
            "{\"old\": 2.00 }",
            "{\"old\": 2.00 ,\"a/b\":1e+03,\"c~d\":-0}",
        ),
    ] {
        let mut doc = Document::parse(input).unwrap();
        doc = doc
            .edit(&[
                Edit::set_at("/a~1b", (exact("1e+03")).clone()),
                Edit::set_at("/c~0d", (exact("-0")).clone()),
            ])
            .unwrap();
        assert_eq!(doc.as_bytes(), expected.as_bytes());
    }
    let mut doc = Document::parse("[ ]").unwrap();
    doc = doc
        .edit(&[
            Edit::set_at("/-", (exact("1")).clone()),
            Edit::set_at("/-", (exact("2")).clone()),
        ])
        .unwrap();
    assert_eq!(doc.as_bytes(), b"[ 1,2]");
    let mut doc = Document::new();
    let pointers: Vec<_> = (0..1000).map(|i| format!("/operations/op{i}")).collect();
    let value = exact("{}");
    doc = doc
        .edit(
            &pointers
                .iter()
                .map(|pointer| Edit::set_at(pointer, (value).clone()))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        doc.validate()
            .unwrap()
            .conformant()
            .unwrap()
            .operations()
            .len(),
        1000
    );
    assert!(doc.validate().unwrap().conformant().is_some());
}

#[test]
fn duplicates_remain_inspectable_but_cannot_be_edited_or_proven() {
    let doc =
        Document::parse(r#"{"openbindings":"0.2.0","operations":{},"x":1,"\u0078":2}"#).unwrap();
    let before = doc.as_bytes().to_vec();
    assert!(doc.validate().unwrap().conformant().is_none());
    assert_eq!(
        doc.set("/x", &exact("3")).unwrap_err().code(),
        "duplicate-members"
    );
    assert_eq!(doc.as_bytes(), before);
    let doc = Document::new();
    assert_eq!(
        doc.set("/x", &exact(r#"{"a":1,"a":2}"#))
            .unwrap_err()
            .code(),
        "duplicate-members"
    );
}

#[test]
fn positions_follow_current_bytes_and_untouched_extensions_stay_exact() {
    let mut doc = Document::parse("{\n\"openbindings\":\"0.2.0\",\"description\":\"old\",\n\"operations\":{}, \"x\" : [ -0, 1e+002, 9007199254740993, \"\\ud800\" ]\n}").unwrap();
    let extension = doc.value().get("x").unwrap().text().to_owned();
    doc = doc
        .set(
            "/description",
            &JsonValue::string("different length").unwrap(),
        )
        .unwrap();
    assert_eq!(doc.value().get("x").unwrap().text(), extension);
    let operations = doc.value().get("operations").unwrap().location();
    assert_eq!(
        &doc.as_bytes()[operations.byte_offset..operations.byte_offset + 2],
        b"{}"
    );
    assert_eq!(operations.line, 3);
    doc = doc.set("/operations", &exact("null")).unwrap();
    let location = doc.value().get("operations").unwrap().location();
    assert!(doc.validate().unwrap().conformant().is_none());
    assert_eq!(
        &doc.as_bytes()[location.byte_offset..location.byte_offset + 4],
        b"null"
    );
    let fresh = Document::parse(doc.as_bytes()).unwrap();
    assert_eq!(
        serde_json::to_value(doc.validate().unwrap().report()).unwrap(),
        serde_json::to_value(fresh.validate().unwrap().report()).unwrap()
    );
}

#[test]
fn exact_parts_preserve_absence_null_and_original_values() {
    let doc = Document::new()
        .edit(&[
            Edit::set(
                &["operations", "go"],
                exact(r#"{"input":true,"examples":{"one":{"input":null}}}"#),
            ),
            Edit::set(
                &["sources"],
                exact(r#"{"s":{"kind":"custom","data":null}}"#),
            ),
            Edit::set(
                &["bindings"],
                exact(r#"{"b":{"operation":"go","source":"s"}}"#),
            ),
            Edit::set(&["dependencies"], exact(r#"{"d":{"operation":"go"}}"#)),
            Edit::set(&["x-number"], exact("1e9999999999999999")),
        ])
        .unwrap();
    let proof = doc.validate().unwrap().conformant().unwrap();
    assert!(proof.name().is_none());
    assert_eq!(proof.source("s").unwrap().data().unwrap().text(), "null");
    assert_eq!(
        proof
            .operation("go")
            .unwrap()
            .example("one")
            .unwrap()
            .input()
            .unwrap()
            .text(),
        "null"
    );
    assert_eq!(
        doc.value().get("x-number").unwrap().text(),
        "1e9999999999999999"
    );
    assert!(proof.binding("b").unwrap().data().is_none());
    assert!(proof.dependency("d").unwrap().kinds().is_none());
}

// Independent text reference: parse the expected revision for every step, edit
// it with String::replace_range, then parse it again. The SDK never supplies the
// expected document, and untouched prefix/suffix bytes are checked separately.
fn reference(text: &str, pointer: &str, value: Option<&str>) -> (String, Range<usize>, usize) {
    let root = exact(text);
    let target = root.at(pointer);
    let (parent_path, token) = pointer.rsplit_once('/').unwrap();
    let parent = root.at(parent_path).unwrap();
    let (range, replacement) = if let Some(target) = target {
        let start = target.location().byte_offset;
        if let Some(value) = value {
            (start..start + target.text().len(), value.to_owned())
        } else {
            let children: Vec<_> = if let Some(members) = parent.members() {
                members
                    .map(|m| {
                        m.name.location().byte_offset
                            ..m.value.location().byte_offset + m.value.text().len()
                    })
                    .collect()
            } else {
                parent
                    .elements()
                    .unwrap()
                    .map(|v| v.location().byte_offset..v.location().byte_offset + v.text().len())
                    .collect()
            };
            let index = children
                .iter()
                .position(|r| r.end == start + target.text().len())
                .unwrap();
            let range = if index + 1 < children.len() {
                children[index].start..children[index + 1].start
            } else if index > 0 {
                children[index - 1].end..children[index].end
            } else {
                children[index].clone()
            };
            (range, String::new())
        }
    } else {
        let key = token.replace("~1", "/").replace("~0", "~");
        let mut insert = if parent.is_empty() == Some(true) {
            String::new()
        } else {
            ",".into()
        };
        if parent.kind() == JsonKind::Object {
            insert.push_str(&serde_json::to_string(&key).unwrap());
            insert.push(':');
        }
        insert.push_str(value.unwrap());
        let end = parent.location().byte_offset + parent.text().len() - 1;
        (end..end, insert)
    };
    let mut expected = text.to_owned();
    expected.replace_range(range.clone(), &replacement);
    let _ = exact(&expected);
    (expected, range, replacement.len())
}
fn paths(value: JsonRef<'_>, path: String, out: &mut Vec<String>) {
    if !path.is_empty() {
        out.push(path.clone());
    }
    if let Some(members) = value.members() {
        for member in members {
            if let Some(name) = member.name.as_str() {
                paths(member.value, format!("{path}/{}", escape(name)), out);
            }
        }
    } else if let Some(items) = value.elements() {
        for (i, value) in items.enumerate() {
            paths(value, format!("{path}/{i}"), out);
        }
    }
}
fn base64(text: &str) -> Vec<u8> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut bits, mut n, mut out) = (0u32, 0, Vec::new());
    for byte in text.bytes().take_while(|&b| b != b'=') {
        bits = (bits << 6) | alphabet.iter().position(|&b| b == byte).unwrap() as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    out
}
fn corpus_documents(directory: &Path, out: &mut Vec<Vec<u8>>) {
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|v| v.unwrap().path())
        .collect();
    files.sort();
    for file in files {
        let fixture = JsonValue::parse(std::fs::read(file).unwrap()).unwrap();
        let cases = fixture
            .get("tests")
            .or_else(|| fixture.get("scenarios"))
            .unwrap();
        for case in cases.elements().unwrap() {
            let given = case.get("given").unwrap_or(case);
            out.push(if let Some(value) = given.get("document") {
                value.text().as_bytes().to_vec()
            } else if let Some(value) = given.get("documentText") {
                value.as_str().unwrap().as_bytes().to_vec()
            } else {
                base64(given.get("documentBase64").unwrap().as_str().unwrap())
            });
        }
    }
}

#[test]
fn generated_edit_sequences_over_every_corpus_document_match_reparsed_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/qualification/inputs/spec/conformance");
    let mut corpus = Vec::new();
    corpus_documents(&root.join("document"), &mut corpus);
    corpus_documents(&root.join("scenarios"), &mut corpus);
    assert_eq!(corpus.len(), 505);
    let mut admitted = 0;
    let mut seed = 0x67cd_f829_302a_19e5u64;
    for bytes in corpus {
        let Ok(mut doc) = Document::parse(&bytes) else {
            continue;
        };
        if doc.value().has_duplicate_names() {
            assert!(doc.set("", &exact("{}")).is_err());
            continue;
        }
        admitted += 1;
        if doc.value().kind() != JsonKind::Object {
            doc = doc.set("", &exact("{}")).unwrap();
        }
        let mut expected = String::from_utf8(doc.as_bytes().to_vec()).unwrap();
        for step in 0..24 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut choices = Vec::new();
            paths(doc.value().view(), String::new(), &mut choices);
            let value = [
                "null",
                "-0",
                "1e+002",
                "900719925474099312345",
                "{\"opaque\": [ true, 3.00 ]}",
                "\"\\ud800\"",
            ][(seed as usize >> 8) % 6];
            let (pointer, replacement) = if step % 5 == 0 {
                (
                    "/operations".to_owned(),
                    Some(format!(r#"{{"op{step}":{{"input":{value}}}}}"#)),
                )
            } else if !choices.is_empty() && !seed.is_multiple_of(3) {
                (
                    choices[seed as usize % choices.len()].clone(),
                    (seed & 4 != 0).then(|| value.to_owned()),
                )
            } else {
                (format!("/x~1{step}~0"), Some(value.to_owned()))
            };
            let before = expected.clone();
            let (next, range, inserted) = reference(&expected, &pointer, replacement.as_deref());
            if let Some(value) = replacement {
                doc = doc.set(&pointer, &exact(&value)).unwrap();
            } else {
                doc = doc.remove(&pointer).unwrap();
            }
            assert_eq!(
                &doc.as_bytes()[..range.start],
                &before.as_bytes()[..range.start]
            );
            assert_eq!(
                &doc.as_bytes()[range.start + inserted..],
                &before.as_bytes()[range.end..]
            );
            assert_eq!(doc.as_bytes(), next.as_bytes(), "step {step} at {pointer}");
            assert_eq!(doc.value().semantic_eq(&exact(&next)), Some(true));
            expected = next;
        }
    }
    assert!(admitted > 300, "{admitted}");
}

#[test]
fn removing_the_only_child_and_inserting_siblings_uses_one_separator_group() {
    // Comma planning depends on the batch, not just the old map.
    for (source, removed, inserted, expected) in [
        (r#"{"a":1 }"#, "/a", "/b", r#"{ "b":2}"#),
        ("[1 ]", "/0", "/-", "[ 2]"),
    ] {
        let mut document = Document::parse(source).unwrap();
        document = document
            .edit(&[
                Edit::remove_at(removed),
                Edit::set_at(inserted, (exact("2")).clone()),
            ])
            .unwrap();
        assert_eq!(document.as_bytes(), expected.as_bytes());
    }
}

#[test]
fn removing_any_set_of_siblings_in_one_batch_matches_removing_them_one_at_a_time() {
    // Adjacent removals, up to every child, are not overlaps. The independent
    // text reference removes one child at a time, from the last to the first,
    // so earlier pointers stay valid; an optional append then lands at the
    // parent's end, with a comma only when a child remains.
    let containers = [
        (
            r#"{"a":1,"b":2,"c":3,"d":4}"#,
            ["/a", "/b", "/c", "/d"],
            "/z",
        ),
        (
            " { \"a\" : 1 , \"b\" : [2] ,\"c\":{\"x\":3},\n\t\"d\":4 } ",
            ["/a", "/b", "/c", "/d"],
            "/z",
        ),
        ("[1,2,3,4]", ["/0", "/1", "/2", "/3"], "/-"),
        (
            r#"[ [1] , {"a":2},3 ,"d" ]"#,
            ["/0", "/1", "/2", "/3"],
            "/-",
        ),
    ];
    for (source, children, append) in containers {
        for subset in 1..16u32 {
            let removed: Vec<_> = (0..4).filter(|i| subset & (1 << i) != 0).collect();
            for insert in [false, true] {
                let mut expected = source.to_owned();
                for &i in removed.iter().rev() {
                    expected = reference(&expected, children[i], None).0;
                }
                if insert {
                    expected = reference(&expected, append, Some("5")).0;
                }
                // Request order does not matter: plan both orders.
                for reversed in [false, true] {
                    let mut edits: Vec<_> = removed
                        .iter()
                        .map(|&i| Edit::remove_at(children[i]))
                        .collect();
                    if reversed {
                        edits.reverse();
                    }
                    let five = exact("5");
                    if insert {
                        edits.push(Edit::set_at(append, (five).clone()));
                    }
                    let mut document = Document::parse(source).unwrap();
                    document = document.edit(&edits).unwrap();
                    assert_eq!(
                        String::from_utf8_lossy(document.as_bytes()),
                        expected,
                        "{source} without {removed:?}, append {insert}"
                    );
                }
            }
        }
    }
    let mut document = Document::parse(r#"{"a":1,"b":2,"c":3}"#).unwrap();
    document = document
        .edit(&[Edit::remove_at("/b"), Edit::remove_at("/c")])
        .unwrap();
    assert_eq!(document.as_bytes(), br#"{"a":1}"#);
}

#[test]
fn overlapping_edits_are_refused_without_changing_bytes() {
    let two = exact("2");
    let source = r#"{"a":{"b":1,"c":2},"d":3}"#;
    for edits in [
        vec![Edit::remove_at("/d"), Edit::remove_at("/d")],
        vec![Edit::remove_at("/a/b"), Edit::remove_at("/a")],
        vec![
            Edit::remove_at("/a"),
            Edit::remove_at("/a/b"),
            Edit::remove_at("/a/c"),
        ],
        vec![Edit::remove_at("/d"), Edit::set_at("/d", (two).clone())],
        vec![Edit::remove_at("/a"), Edit::set_at("/a/b", (two).clone())],
        vec![
            Edit::set_at("/z", (two).clone()),
            Edit::set_at("/z", (two).clone()),
        ],
    ] {
        let document = Document::parse(source).unwrap();
        let error = document.edit(&edits).unwrap_err();
        assert_eq!(error.code(), "overlapping-edits", "{edits:?}");
        assert_eq!(document.as_bytes(), source.as_bytes());
    }
}

#[test]
fn every_namespace_edit_matches_an_exact_reference_sequence() {
    // Exercise all construction namespaces with escaped keys.
    let mut seed = 0x9876_5432u64;
    let mut document = Document::new();
    let mut expected = String::from_utf8(document.as_bytes().to_vec()).unwrap();
    for step in 0..200 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let key = format!("item{}/~", seed % 7);
        let encoded_key = serde_json::to_string(&key).unwrap();
        let (namespace, part) = match seed % 4 {
            0 => ("operations", format!(r#"{{"description":"step {step}"}}"#)),
            1 => (
                "sources",
                String::from(r#"{"kind":"custom","data":[-0,1e+3]}"#),
            ),
            2 => (
                "bindings",
                format!(r#"{{"operation":{encoded_key},"source":{encoded_key}}}"#),
            ),
            _ => ("dependencies", format!(r#"{{"operation":{encoded_key}}}"#)),
        };
        let (pointer, replacement) = if exact(&expected).get(namespace).is_some() {
            (format!("/{namespace}/{}", escape(&key)), part)
        } else {
            (
                format!("/{namespace}"),
                format!("{{{}:{part}}}", serde_json::to_string(&key).unwrap()),
            )
        };
        let (next, range, inserted) = reference(&expected, &pointer, Some(&replacement));
        document = document
            .edit(&[Edit::set_at(&pointer, exact(&replacement))])
            .unwrap();
        assert_eq!(document.as_bytes(), next.as_bytes());
        assert_eq!(
            &document.as_bytes()[..range.start],
            &expected.as_bytes()[..range.start]
        );
        assert_eq!(
            &document.as_bytes()[range.start + inserted..],
            &expected.as_bytes()[range.end..]
        );
        expected = next;
    }
    let edit = Edit::remove_at("/private-token");
    assert!(!format!("{edit:?}").contains("private-token"));
}

#[test]
fn exact_namespace_batch_preserves_exact_bytes_and_is_atomic() {
    // Complete maps and leaf edits share a single atomic revision.
    let mut document =
        Document::parse(" {\n\"openbindings\":\"0.2.0\",\"operations\":{},\"x-keep\": 1e+00 } ")
            .unwrap();
    let operations = exact(r#"{"run":{"input":true,"x-number":1e+003}}"#);
    let sources = exact(r#"{"s":{"kind":"custom","data":9007199254740993}}"#);
    let bindings = exact(r#"{"b":{"operation":"run","source":"s"}}"#);
    let dependencies = exact(r#"{"d":{"operation":"run"}}"#);
    let name = exact("\"Batch\"");
    document = document
        .edit(&[
            Edit::set(&["operations"], operations),
            Edit::set_at("/name", (name).clone()),
            Edit::set(&["dependencies"], dependencies),
            Edit::set(&["sources"], sources),
            Edit::set(&["bindings"], bindings),
        ])
        .unwrap();
    assert!(document.validate().unwrap().conformant().is_some());
    let expected = " {\n\"openbindings\":\"0.2.0\",\"operations\":{\"run\":{\"input\":true,\"x-number\":1e+003}},\"x-keep\": 1e+00 ,\"name\":\"Batch\",\"dependencies\":{\"d\":{\"operation\":\"run\"}},\"sources\":{\"s\":{\"kind\":\"custom\",\"data\":9007199254740993}},\"bindings\":{\"b\":{\"operation\":\"run\",\"source\":\"s\"}}} ";
    assert_eq!(document.as_bytes(), expected.as_bytes());
    let before = document.as_bytes().to_vec();
    assert!(
        document
            .edit(&[
                Edit::set(&["operations"], exact("{}")),
                Edit::remove_at("/missing")
            ])
            .is_err()
    );
    assert_eq!(document.as_bytes(), before);
}

#[test]
fn a_finding_pointer_through_an_unpaired_name_edits_the_member_it_reports() {
    // No Rust text spells this member's name; its OBI-01 finding's exact
    // pointer addresses it, alone or in a batch, and repairs the document.
    let text = r#"{"openbindings":"0.2.0","operations":{},"x-a":{"\ud800":1,"k":2}}"#;
    let document = Document::parse(text).unwrap();
    let validation = document.validate().unwrap();
    let finding = &validation.report().findings[0];
    assert_eq!(
        (finding.rule, finding.code),
        ("OBI-01", "unpaired-surrogate")
    );
    let pointer = finding.location.as_ref().unwrap().pointer.clone().unwrap();
    assert_eq!(pointer.as_str(), None);
    let conclusion = |doc: &Document| doc.validate().unwrap().report().conclusion;

    let mut removed = document.clone();
    removed = removed.remove(&pointer).unwrap();
    assert_eq!(
        removed.as_bytes(),
        br#"{"openbindings":"0.2.0","operations":{},"x-a":{"k":2}}"#
    );
    assert_eq!(conclusion(&removed), Conformance::Conformant);

    let mut replaced = document.clone();
    replaced = replaced.set(&pointer, &exact("[3]")).unwrap();
    assert_eq!(
        replaced.as_bytes(),
        br#"{"openbindings":"0.2.0","operations":{},"x-a":{"\ud800":[3],"k":2}}"#
    );
    let inside = Pointer::from_utf16(
        &pointer
            .utf16_units()
            .chain("/0".encode_utf16())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    replaced = replaced.set(&inside, &exact("4")).unwrap();
    assert_eq!(replaced.value().at_pointer(&inside).unwrap().text(), "4");

    let mut renamed = document.clone();
    let value = exact("1");
    renamed = renamed
        .edit(&[
            Edit::remove_at(&pointer),
            Edit::set_at("/x-a/fixed", (value).clone()),
        ])
        .unwrap();
    assert_eq!(
        renamed.as_bytes(),
        br#"{"openbindings":"0.2.0","operations":{},"x-a":{"k":2,"fixed":1}}"#
    );
    assert_eq!(conclusion(&renamed), Conformance::Conformant);
    assert_eq!(document.as_bytes(), text.as_bytes());
}

#[test]
fn an_exact_pointer_inserts_and_appends_like_its_text() {
    let units = |text: &str| text.encode_utf16().collect::<Vec<_>>();
    let exact_pointer = |parts: &[&[u16]]| Pointer::from_utf16(&parts.concat()).unwrap();
    let mut doc = Document::parse(r#"{"x":{"\ud800":{"list":[]}}}"#).unwrap();
    let lone = [0xd800];
    // A missing name with an unpaired unit, decoded from its escapes, is
    // written as a JSON escape; its own pointer then finds it.
    let inserted = exact_pointer(&[&units("/x/"), &lone, &units("~0~1y")]);
    doc = doc.set(&inserted, &exact("true")).unwrap();
    let appended = exact_pointer(&[&units("/x/"), &lone, &units("/list/-")]);
    doc = doc.set(&appended, &exact("5")).unwrap();
    assert_eq!(
        doc.as_bytes(),
        br#"{"x":{"\ud800":{"list":[5]},"\ud800~/y":true}}"#
    );
    assert_eq!(doc.value().at_pointer(&inserted).unwrap().text(), "true");
    // An exact pointer whose units all pair is the same as its text.
    let paired: Pointer = "/x/plain".parse().unwrap();
    doc = doc.set(&paired, &exact("0")).unwrap();
    assert_eq!(doc.value().at("/x/plain").unwrap().text(), "0");
    // Its refusals match the text form's.
    let missing = exact_pointer(&[&units("/none/"), &lone]);
    assert_eq!(
        doc.set(&missing, &exact("0")).unwrap_err(),
        EditError::MissingParent
    );
    assert_eq!(doc.remove(&missing).unwrap_err(), EditError::MissingValue);
    let into_array = exact_pointer(&[&units("/x/"), &lone, &units("/list/"), &lone]);
    assert_eq!(
        doc.set(&into_array, &exact("0")).unwrap_err(),
        EditError::InvalidPointer
    );
    let duplicate = exact_pointer(&[&units("/x/"), &lone, &units("/n")]);
    let value = exact("1");
    let twice = Edit::set_at(&duplicate, (value).clone());
    assert_eq!(
        doc.edit(&[twice.clone(), twice]).unwrap_err(),
        EditError::Overlap
    );
}

#[test]
fn successful_edits_preserve_original_snapshot_and_exact_owners() {
    let source = " {\"openbindings\":\"0.2.0\",\"operations\":{},\"x-kept\":1e+003} ";
    let original = Document::parse(source).unwrap();
    let proof = original.validate().unwrap().conformant().unwrap();
    let revision = original.set("/description", &JsonValue::null()).unwrap();
    let empty = original.edit(&[]).unwrap();
    assert_eq!(original.as_bytes(), source.as_bytes());
    assert_eq!(proof.document().as_bytes(), source.as_bytes());
    assert_eq!(empty.as_bytes(), source.as_bytes());
    assert!(original.value().get("description").is_none());
    assert_eq!(
        revision.value().get("description").unwrap().kind(),
        JsonKind::Null
    );
    let retained = revision.value().get("x-kept").unwrap().to_owned();
    drop(original);
    drop(revision);
    assert_eq!(retained.text(), "1e+003");
    assert_eq!(proof.document().as_bytes(), source.as_bytes());
}

#[test]
fn owned_edits_retain_paths_and_exact_values_across_reuse() {
    fn owned<T: Clone + Send + Sync + 'static>() {}
    owned::<Edit>();
    let edits = {
        let key = String::from("look/up~");
        let value = exact(r#"{ "input": false, "x-number": 1e+003 }"#);
        vec![Edit::set(&["operations", &key], value)]
    };
    let original = Document::new();
    let first = original.edit(&edits).unwrap();
    let second = original.edit(&edits.clone()).unwrap();
    assert_eq!(first.as_bytes(), second.as_bytes());
    assert!(original.value().at("/operations/look~1up~0").is_none());
    assert_eq!(
        first
            .value()
            .at("/operations/look~1up~0/x-number")
            .unwrap()
            .text(),
        "1e+003"
    );
    let removed = first
        .edit(&[Edit::remove(&["operations", "look/up~"])])
        .unwrap();
    assert!(removed.value().at("/operations/look~1up~0").is_none());
    assert!(first.value().at("/operations/look~1up~0").is_some());
}

#[test]
fn edit_batches_accept_the_node_limit_and_refuse_larger_requests() {
    let document = Document::parse_with_limits(
        "[0,0,0,0]",
        JsonLimits {
            max_nodes: 5,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    let mut edits: Vec<_> = (0..4)
        .map(|i| Edit::remove_at(format!("/{i}").as_str()))
        .collect();
    edits.push(Edit::set_at("/-", exact("1")));
    assert_eq!(document.edit(&edits).unwrap().as_bytes(), b"[1]");
    for _ in 0..2 {
        edits.push(Edit::set_at("/-", exact("1")));
        assert_eq!(document.edit(&edits).unwrap_err().code(), "node-limit");
    }
    assert_eq!(document.as_bytes(), b"[0,0,0,0]");
}

#[test]
fn edit_pointer_byte_and_utf16_limits_are_inclusive() {
    let document = Document::parse_with_limits(
        "{}",
        JsonLimits {
            max_bytes: 8,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    for length in [8, 9, 10] {
        let text = format!("/{}", "a".repeat(length - 1));
        let expected = if length == 8 {
            "missing-value"
        } else {
            "byte-limit"
        };
        assert_eq!(document.remove(text.as_str()).unwrap_err().code(), expected);
        let source = exact(&format!(r#"{{"{}\ud800":0}}"#, "a".repeat(length - 2)));
        let pointer = source
            .view()
            .members()
            .unwrap()
            .next()
            .unwrap()
            .value
            .location()
            .pointer
            .unwrap();
        assert_eq!(pointer.utf16_units().count(), length);
        assert_eq!(document.remove(&pointer).unwrap_err().code(), expected);
    }
}

#[test]
fn inserted_names_and_replacements_obey_the_final_byte_limit() {
    for name in [
        "plain",
        "quote\"slash\\line\nreturn\rtab\t\u{8}\u{c}",
        "control\u{1}",
        "é😀",
    ] {
        let expected = format!("{{{}:1e+03}}", serde_json::to_string(name).unwrap());
        for maximum in [expected.len(), expected.len() - 1] {
            let document = Document::parse_with_limits(
                "{}",
                JsonLimits {
                    max_bytes: maximum,
                    ..JsonLimits::default()
                },
            )
            .unwrap();
            let result = document.edit(&[Edit::set(&[name], exact("1e+03"))]);
            if maximum == expected.len() {
                assert_eq!(result.unwrap().as_bytes(), expected.as_bytes());
            } else {
                assert_eq!(result.unwrap_err().code(), "byte-limit");
            }
            assert_eq!(document.as_bytes(), b"{}");
        }
    }
    let document = Document::parse_with_limits(
        r#"{"a":0}"#,
        JsonLimits {
            max_bytes: 14,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    let edits = [Edit::remove_at("/a"), Edit::set_at("/long", exact("1e+03"))];
    assert_eq!(
        document.edit(&edits).unwrap().as_bytes(),
        br#"{"long":1e+03}"#
    );
    let oversized = [
        Edit::remove_at("/a"),
        Edit::set_at("/long", exact("1e+003")),
    ];
    assert_eq!(document.edit(&oversized).unwrap_err().code(), "byte-limit");
}

#[test]
fn edit_input_errors_expose_the_underlying_error_as_their_source() {
    use std::error::Error;
    let document = Document::parse_with_limits(
        "{}",
        JsonLimits {
            max_bytes: 2,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    let error = document.set("/x", &exact("0")).unwrap_err();
    let EditError::Input(input) = &error else {
        panic!("expected input refusal")
    };
    let source = error
        .source()
        .unwrap()
        .downcast_ref::<InputError>()
        .unwrap();
    assert_eq!(source, input);
    assert_eq!(source.code(), "byte-limit");
    assert!(EditError::InvalidPointer.source().is_none());
}

#[test]
fn duplicate_replacements_keep_their_refusal_under_tight_byte_limits() {
    let replacement = exact(r#"{"a":0,"a":1}"#);
    for maximum in [2, 128] {
        let document = Document::parse_with_limits(
            "{}",
            JsonLimits {
                max_bytes: maximum,
                ..JsonLimits::default()
            },
        )
        .unwrap();
        assert_eq!(
            document.set("/x", &replacement).unwrap_err().code(),
            "duplicate-members"
        );
        assert_eq!(document.as_bytes(), b"{}");
    }
}
