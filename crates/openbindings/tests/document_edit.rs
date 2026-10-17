//! Document edits preserve exact bytes and retained proof ownership.
use openbindings::*;
use std::{collections::BTreeMap, ops::Range, path::Path};

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
    let view = doc.resolve_operation("go").unwrap();
    let bytes = doc.to_bytes().to_vec();
    let value = exact("42");
    for edits in [
        vec![
            Edit::Set {
                pointer: "/name",
                value: &value,
            },
            Edit::Remove {
                pointer: "/missing",
            },
        ],
        vec![
            Edit::Set {
                pointer: "/operations",
                value: &value,
            },
            Edit::Remove {
                pointer: "/operations/go",
            },
        ],
        vec![
            Edit::Set {
                pointer: "/new",
                value: &value,
            },
            Edit::Set {
                pointer: "/new",
                value: &value,
            },
        ],
        vec![Edit::Set {
            pointer: "/~2",
            value: &value,
        }],
        vec![Edit::Remove { pointer: "" }],
    ] {
        assert!(doc.edit(&edits).is_err());
        assert_eq!(doc.to_bytes(), bytes);
        assert_eq!(
            doc.validate().unwrap().report().conclusion,
            Conformance::Conformant
        );
    }
    doc.set_operation_description("go", "new\nline").unwrap();
    assert_eq!(proof.as_document().to_bytes(), bytes);
    let OperationSelection::Found(view) = view else {
        panic!()
    };
    assert!(view.description().unwrap().is_none());
    let mut doc = proof.into_inner();
    doc.set_name("editable again").unwrap();
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
        doc.edit(&[
            Edit::Set {
                pointer: "/a~1b",
                value: &exact("1e+03"),
            },
            Edit::Set {
                pointer: "/c~0d",
                value: &exact("-0"),
            },
        ])
        .unwrap();
        assert_eq!(doc.to_bytes(), expected.as_bytes());
    }
    let mut doc = Document::parse("[ ]").unwrap();
    doc.edit(&[
        Edit::Set {
            pointer: "/-",
            value: &exact("1"),
        },
        Edit::Set {
            pointer: "/-",
            value: &exact("2"),
        },
    ])
    .unwrap();
    assert_eq!(doc.to_bytes(), b"[ 1,2]");
    let mut doc = Document::new();
    let pointers: Vec<_> = (0..1000).map(|i| format!("/operations/op{i}")).collect();
    let value = exact("{}");
    doc.edit(
        &pointers
            .iter()
            .map(|pointer| Edit::Set {
                pointer,
                value: &value,
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(doc.operations().unwrap().len(), 1000);
    assert!(doc.validate().unwrap().conformant().is_some());
}

#[test]
fn duplicates_remain_inspectable_but_cannot_be_edited_or_proven() {
    let mut doc =
        Document::parse(r#"{"openbindings":"0.2.0","operations":{},"x":1,"\u0078":2}"#).unwrap();
    let before = doc.to_bytes().to_vec();
    assert!(doc.operations().is_err());
    assert!(doc.validate().unwrap().conformant().is_none());
    assert_eq!(
        doc.set("/x", &exact("3")).unwrap_err().code(),
        "duplicate-members"
    );
    assert_eq!(doc.to_bytes(), before);
    let mut doc = Document::new();
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
    doc.set_description("different length").unwrap();
    assert_eq!(doc.value().get("x").unwrap().text(), extension);
    let operations = doc.value().get("operations").unwrap().location();
    assert_eq!(
        &doc.to_bytes()[operations.byte_offset..operations.byte_offset + 2],
        b"{}"
    );
    assert_eq!(operations.line, 3);
    doc.set("/operations", &exact("null")).unwrap();
    let error = doc.operations().unwrap_err();
    let location = error.source_location().unwrap();
    assert_eq!(
        &doc.to_bytes()[location.byte_offset..location.byte_offset + 4],
        b"null"
    );
    let fresh = Document::parse(doc.to_bytes()).unwrap();
    assert_eq!(
        serde_json::to_value(doc.validate().unwrap().report()).unwrap(),
        serde_json::to_value(fresh.validate().unwrap().report()).unwrap()
    );
}

#[test]
fn whole_parts_preserve_absence_null_exact_values_and_collision_paths() {
    let mut doc = Document::new();
    doc.set_operation(
        "go",
        &Operation {
            input: Some(exact("true")),
            ..Default::default()
        },
    )
    .unwrap();
    doc.set_example(
        "go",
        "one",
        &Example {
            input: Some(exact("null")),
            ..Default::default()
        },
    )
    .unwrap();
    doc.set_source(
        "s",
        &Source {
            kind: "custom".into(),
            content: Some(exact("null")),
            ..Default::default()
        },
    )
    .unwrap();
    doc.set_binding(
        "b",
        &Binding {
            operation: "go".into(),
            source: "s".into(),
            ..Default::default()
        },
    )
    .unwrap();
    doc.set_dependency(
        "d",
        &Dependency {
            operation: "go".into(),
            ..Default::default()
        },
    )
    .unwrap();
    doc.set("/x-number", &exact("1e9999999999999999")).unwrap();
    let proof = doc.validate().unwrap().conformant().unwrap();
    assert!(proof.name().is_none());
    assert_eq!(proof.source("s").unwrap().content().unwrap().text(), "null");
    assert_eq!(
        proof
            .resolve_operation("go")
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
    let before = doc.to_bytes().to_vec();
    let bad = Source {
        additional_fields: BTreeMap::from([("kind".into(), exact("null"))]),
        ..Default::default()
    };
    let EditError::Authoring(error) = doc.set_source("s~/", &bad).unwrap_err() else {
        panic!()
    };
    assert_eq!(
        error.draft_pointer(),
        Some("/sources/s~0~1/additional_fields/kind")
    );
    assert_eq!(doc.to_bytes(), before);
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
    assert_eq!(corpus.len(), 440);
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
            doc.set("", &exact("{}")).unwrap();
        }
        let mut expected = String::from_utf8(doc.to_bytes().to_vec()).unwrap();
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
            let (pointer, replacement, part) = if step % 5 == 0 {
                let op = Operation {
                    input: Some(exact(value)),
                    ..Default::default()
                };
                let mut operations = BTreeMap::new();
                operations.insert(format!("op{step}"), op);
                (
                    "/operations".to_owned(),
                    Some(String::from_utf8(serde_json::to_vec(&operations).unwrap()).unwrap()),
                    Some(operations),
                )
            } else if !choices.is_empty() && !seed.is_multiple_of(3) {
                (
                    choices[seed as usize % choices.len()].clone(),
                    (seed & 4 != 0).then(|| value.to_owned()),
                    None,
                )
            } else {
                (format!("/x~1{step}~0"), Some(value.to_owned()), None)
            };
            let before = expected.clone();
            let (next, range, inserted) = reference(&expected, &pointer, replacement.as_deref());
            if let Some(operations) = part {
                doc.set_operations(&operations).unwrap();
            } else if let Some(value) = replacement {
                doc.set(&pointer, &exact(&value)).unwrap();
            } else {
                doc.remove(&pointer).unwrap();
            }
            assert_eq!(
                &doc.to_bytes()[..range.start],
                &before.as_bytes()[..range.start]
            );
            assert_eq!(
                &doc.to_bytes()[range.start + inserted..],
                &before.as_bytes()[range.end..]
            );
            assert_eq!(doc.to_bytes(), next.as_bytes(), "step {step} at {pointer}");
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
        document
            .edit(&[
                Edit::Remove { pointer: removed },
                Edit::Set {
                    pointer: inserted,
                    value: &exact("2"),
                },
            ])
            .unwrap();
        assert_eq!(document.to_bytes(), expected.as_bytes());
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
                        .map(|&i| Edit::Remove {
                            pointer: children[i],
                        })
                        .collect();
                    if reversed {
                        edits.reverse();
                    }
                    let five = exact("5");
                    if insert {
                        edits.push(Edit::Set {
                            pointer: append,
                            value: &five,
                        });
                    }
                    let mut document = Document::parse(source).unwrap();
                    document.edit(&edits).unwrap();
                    assert_eq!(
                        String::from_utf8_lossy(document.to_bytes()),
                        expected,
                        "{source} without {removed:?}, append {insert}"
                    );
                }
            }
        }
    }
    let mut document = Document::parse(r#"{"a":1,"b":2,"c":3}"#).unwrap();
    document
        .edit(&[
            Edit::Remove { pointer: "/b" },
            Edit::Remove { pointer: "/c" },
        ])
        .unwrap();
    assert_eq!(document.to_bytes(), br#"{"a":1}"#);
}

#[test]
fn overlapping_edits_are_refused_without_changing_bytes() {
    let two = exact("2");
    let source = r#"{"a":{"b":1,"c":2},"d":3}"#;
    for edits in [
        vec![
            Edit::Remove { pointer: "/d" },
            Edit::Remove { pointer: "/d" },
        ],
        vec![
            Edit::Remove { pointer: "/a/b" },
            Edit::Remove { pointer: "/a" },
        ],
        vec![
            Edit::Remove { pointer: "/a" },
            Edit::Remove { pointer: "/a/b" },
            Edit::Remove { pointer: "/a/c" },
        ],
        vec![
            Edit::Remove { pointer: "/d" },
            Edit::Set {
                pointer: "/d",
                value: &two,
            },
        ],
        vec![
            Edit::Remove { pointer: "/a" },
            Edit::Set {
                pointer: "/a/b",
                value: &two,
            },
        ],
        vec![
            Edit::Set {
                pointer: "/z",
                value: &two,
            },
            Edit::Set {
                pointer: "/z",
                value: &two,
            },
        ],
    ] {
        let mut document = Document::parse(source).unwrap();
        let error = document.edit(&edits).unwrap_err();
        assert_eq!(error.code(), "overlapping-edits", "{edits:?}");
        assert_eq!(document.to_bytes(), source.as_bytes());
    }
}

#[test]
fn every_typed_part_setter_matches_an_exact_reference_sequence() {
    // Exercise all construction namespaces with escaped keys.
    let mut seed = 0x9876_5432u64;
    let mut document = Document::new();
    let mut expected = String::from_utf8(document.to_bytes().to_vec()).unwrap();
    for step in 0..200 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let key = format!("item{}/~", seed % 7);
        let operation = Operation {
            description: Some(format!("step {step}")),
            ..Default::default()
        };
        let source = Source {
            kind: "custom".into(),
            content: Some(exact("[-0,1e+3]")),
            ..Default::default()
        };
        let binding = Binding {
            operation: key.clone(),
            source: key.clone(),
            ..Default::default()
        };
        let dependency = Dependency {
            operation: key.clone(),
            ..Default::default()
        };
        let (namespace, part) = match seed % 4 {
            0 => ("operations", serde_json::to_string(&operation).unwrap()),
            1 => ("sources", serde_json::to_string(&source).unwrap()),
            2 => ("bindings", serde_json::to_string(&binding).unwrap()),
            _ => ("dependencies", serde_json::to_string(&dependency).unwrap()),
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
        match seed % 4 {
            0 => document.set_operation(&key, &operation).unwrap(),
            1 => document.set_source(&key, &source).unwrap(),
            2 => document.set_binding(&key, &binding).unwrap(),
            _ => document.set_dependency(&key, &dependency).unwrap(),
        }
        assert_eq!(document.to_bytes(), next.as_bytes());
        assert_eq!(
            &document.to_bytes()[..range.start],
            &expected.as_bytes()[..range.start]
        );
        assert_eq!(
            &document.to_bytes()[range.start + inserted..],
            &expected.as_bytes()[range.end..]
        );
        expected = next;
    }
    let edit = Edit::Remove {
        pointer: "/private-token",
    };
    assert!(!format!("{edit:?}").contains("private-token"));
}

#[test]
fn typed_namespace_batch_preserves_exact_bytes_and_is_atomic() {
    // Typed maps and pointer edits share a single atomic revision.
    let mut document =
        Document::parse(" {\n\"openbindings\":\"0.2.0\",\"operations\":{},\"x-keep\": 1e+00 } ")
            .unwrap();
    let operations = BTreeMap::from([(
        "run".into(),
        Operation {
            input: Some(exact("true")),
            additional_fields: BTreeMap::from([("x-number".into(), exact("1e+003"))]),
            ..Default::default()
        },
    )]);
    let sources = BTreeMap::from([(
        "s".into(),
        Source {
            kind: "custom".into(),
            content: Some(exact("9007199254740993")),
            ..Default::default()
        },
    )]);
    let bindings = BTreeMap::from([(
        "b".into(),
        Binding {
            operation: "run".into(),
            source: "s".into(),
            ..Default::default()
        },
    )]);
    let dependencies = BTreeMap::from([(
        "d".into(),
        Dependency {
            operation: "run".into(),
            ..Default::default()
        },
    )]);
    let name = exact("\"Batch\"");
    document
        .edit(&[
            Edit::SetOperations { value: &operations },
            Edit::Set {
                pointer: "/name",
                value: &name,
            },
            Edit::SetDependencies {
                value: &dependencies,
            },
            Edit::SetSources { value: &sources },
            Edit::SetBindings { value: &bindings },
        ])
        .unwrap();
    assert!(document.validate().unwrap().conformant().is_some());
    let expected = " {\n\"openbindings\":\"0.2.0\",\"operations\":{\"run\":{\"input\":true,\"x-number\":1e+003}},\"x-keep\": 1e+00 ,\"name\":\"Batch\",\"dependencies\":{\"d\":{\"operation\":\"run\"}},\"sources\":{\"s\":{\"kind\":\"custom\",\"content\":9007199254740993}},\"bindings\":{\"b\":{\"operation\":\"run\",\"source\":\"s\"}}} ";
    assert_eq!(document.to_bytes(), expected.as_bytes());
    let before = document.to_bytes().to_vec();
    assert!(
        document
            .edit(&[
                Edit::SetOperations {
                    value: &BTreeMap::new()
                },
                Edit::Remove {
                    pointer: "/missing"
                }
            ])
            .is_err()
    );
    assert_eq!(document.to_bytes(), before);
    let invalid = BTreeMap::from([(
        "run".into(),
        Operation {
            additional_fields: BTreeMap::from([("input".into(), exact("true"))]),
            ..Default::default()
        },
    )]);
    assert_eq!(
        document
            .edit(&[Edit::SetOperations { value: &invalid }])
            .unwrap_err()
            .code(),
        "field-collision"
    );
    assert_eq!(document.to_bytes(), before);
}
