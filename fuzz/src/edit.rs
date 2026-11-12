//! Independent tree rebuilding oracle for snapshot-addressed edit batches.
use openbindings::{Document, Edit, JsonLimits, JsonRef, JsonValue};

#[derive(Clone)]
struct Node {
    // Untouched nodes are checked byte for byte, even below changed containers.
    raw: Option<String>,
    shape: Shape,
}
#[derive(Clone)]
enum Shape {
    Object(Vec<(Vec<u16>, String, Node)>),
    Array(Vec<Node>),
    Atom(String),
}
impl Node {
    fn read(value: JsonRef<'_>) -> Self {
        let shape = if let Some(members) = value.members() {
            Shape::Object(
                members
                    .map(|m| {
                        (
                            m.name.as_string().unwrap().utf16_units().collect(),
                            m.name.text().into(),
                            Self::read(m.value),
                        )
                    })
                    .collect(),
            )
        } else if let Some(elements) = value.elements() {
            Shape::Array(elements.map(Self::read).collect())
        } else {
            Shape::Atom(value.text().into())
        };
        Self {
            raw: Some(value.text().into()),
            shape,
        }
    }
    fn text(&self) -> String {
        if let Some(raw) = &self.raw {
            return raw.clone();
        }
        match &self.shape {
            Shape::Atom(text) => text.clone(),
            Shape::Array(values) => format!(
                "[{}]",
                values.iter().map(Self::text).collect::<Vec<_>>().join(",")
            ),
            Shape::Object(members) => format!(
                "{{{}}}",
                members
                    .iter()
                    .map(|(_, key, value)| format!("{key}:{}", value.text()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }
    fn check_bytes(&self, actual: JsonRef<'_>) {
        if let Some(raw) = &self.raw {
            assert_eq!(actual.text(), raw, "untouched value bytes changed");
            return;
        }
        match &self.shape {
            Shape::Object(members) => {
                let actual: Vec<_> = actual.members().unwrap().collect();
                assert_eq!(members.len(), actual.len());
                for (name, key, value) in members {
                    let member = actual
                        .iter()
                        .find(|m| {
                            m.name
                                .as_string()
                                .unwrap()
                                .utf16_units()
                                .eq(name.iter().copied())
                        })
                        .unwrap();
                    assert_eq!(member.name.text(), key, "member name bytes changed");
                    value.check_bytes(member.value);
                }
            }
            Shape::Array(values) => {
                let actual = actual.elements().unwrap();
                assert_eq!(values.len(), actual.len());
                for (value, actual) in values.iter().zip(actual) {
                    value.check_bytes(actual);
                }
            }
            Shape::Atom(_) => unreachable!(),
        }
    }
}
struct Patch {
    path: Vec<Vec<u16>>,
    value: Option<Node>,
}
fn pointer(text: &str) -> Option<Vec<Vec<u16>>> {
    if text.is_empty() {
        return Some(Vec::new());
    }
    text.strip_prefix('/')?
        .split('/')
        .map(|segment| {
            let mut decoded = String::new();
            let mut chars = segment.chars();
            while let Some(c) = chars.next() {
                decoded.push(if c == '~' {
                    match chars.next()? {
                        '0' => '~',
                        '1' => '/',
                        _ => return None,
                    }
                } else {
                    c
                });
            }
            Some(decoded.encode_utf16().collect())
        })
        .collect()
}
// Rebuild from the original tree, so removing array elements never shifts later
// addresses in this batch. This model knows nothing about splices or separators.
fn apply(
    node: &Node,
    path: &mut Vec<Vec<u16>>,
    patches: &[Patch],
    used: &mut [bool],
) -> Option<Node> {
    if let Some((i, patch)) = patches.iter().enumerate().find(|(_, p)| p.path == *path) {
        for (j, p) in patches.iter().enumerate() {
            if p.path == *path {
                used[j] = true;
            }
        }
        used[i] = true;
        return patch.value.clone();
    }
    if !patches.iter().any(|p| p.path.starts_with(path)) {
        return Some(node.clone());
    }
    let shape = match &node.shape {
        Shape::Object(members) => {
            let mut out = Vec::new();
            for (name, key, child) in members {
                path.push(name.clone());
                if let Some(value) = apply(child, path, patches, used) {
                    out.push((name.clone(), key.clone(), value));
                }
                path.pop();
            }
            for (i, p) in patches.iter().enumerate() {
                if p.path.len() == path.len() + 1
                    && p.path.starts_with(path)
                    && !members
                        .iter()
                        .any(|(name, _, _)| *name == p.path[path.len()])
                {
                    let name = p.path.last().unwrap().clone();
                    let key = JsonValue::string(&String::from_utf16(&name).unwrap())
                        .unwrap()
                        .text()
                        .to_owned();
                    out.push((
                        name,
                        key,
                        p.value.clone().expect("successful insertion has a value"),
                    ));
                    used[i] = true;
                }
            }
            Shape::Object(out)
        }
        Shape::Array(values) => {
            let mut out = Vec::new();
            for (i, child) in values.iter().enumerate() {
                path.push(i.to_string().encode_utf16().collect());
                if let Some(value) = apply(child, path, patches, used) {
                    out.push(value);
                }
                path.pop();
            }
            for (i, p) in patches.iter().enumerate() {
                if p.path.len() == path.len() + 1
                    && p.path.starts_with(path)
                    && p.path.last().unwrap() == &[u16::from(b'-')]
                {
                    out.push(p.value.clone().expect("successful append has a value"));
                    used[i] = true;
                }
            }
            Shape::Array(out)
        }
        Shape::Atom(_) => node.shape.clone(),
    };
    Some(Node { raw: None, shape })
}

/// Input: [arbitrary document, [["set", pointer, value] | ["remove", pointer], ...], optional [byte, node, depth limits]].
/// Successful edits are compared with an independent snapshot tree model.
pub fn edit(data: &[u8]) -> bool {
    if data.len() > crate::MAX_INPUT {
        return false;
    }
    let Ok(input) = JsonValue::parse(data) else {
        return false;
    };
    let Some(original) = input.view().element(0) else {
        return false;
    };
    let Some(batch) = input.view().element(1).and_then(|v| v.elements()) else {
        return false;
    };
    let mut limits = JsonLimits {
        max_bytes: crate::MAX_INPUT * 2,
        max_nodes: crate::MAX_INPUT,
        max_depth: 64,
    };
    if let Some(selected) = input.view().element(2) {
        for (i, limit) in [
            &mut limits.max_bytes,
            &mut limits.max_nodes,
            &mut limits.max_depth,
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(n) = selected
                .element(i)
                .and_then(|v| v.number_text())
                .and_then(|n| n.parse::<usize>().ok())
            {
                *limit = (*limit).min(n);
            }
        }
    }
    let Ok(document) = Document::parse_with_limits(original.text(), limits) else {
        return false;
    };
    let before = document.as_bytes().to_vec();
    let mut edits = Vec::new();
    let mut patches = Vec::new();
    for request in batch {
        let Some(path) = request.element(1).and_then(|v| v.as_str()) else {
            return false;
        };
        let value = match request.element(0).and_then(|v| v.as_str()) {
            Some("set") => {
                let Some(value) = request.element(2) else {
                    return false;
                };
                // Keep model recursion bounded independently of the outer parser.
                let Ok(value) = JsonValue::parse_with_limits(value.text(), limits) else {
                    return false;
                };
                edits.push(Edit::set_at(path, value.clone()));
                Some(Node::read(value.view()))
            }
            Some("remove") => {
                edits.push(Edit::remove_at(path));
                None
            }
            _ => return false,
        };
        patches.push((pointer(path), value));
    }
    let result = document.edit(&edits);
    assert_eq!(
        document.as_bytes(),
        before,
        "edit changed the original snapshot"
    );
    let Ok(result) = result else {
        return false;
    };
    let reparsed = Document::parse_with_limits(result.as_bytes(), document.limits())
        .expect("a successful edit reparses at its limits");
    assert_eq!(reparsed.as_bytes(), result.as_bytes());
    let patches: Vec<_> = patches
        .into_iter()
        .map(|(path, value)| Patch {
            path: path.expect("successful pointer is valid"),
            value,
        })
        .collect();
    let mut used = vec![false; patches.len()];
    let model = apply(
        &Node::read(document.value().view()),
        &mut Vec::new(),
        &patches,
        &mut used,
    )
    .expect("successful edit retains a root");
    assert!(
        used.into_iter().all(|used| used),
        "successful edit ignored a request"
    );
    let expected = JsonValue::parse(model.text()).unwrap();
    // Only an empty batch may succeed on a document with repeated names; such a
    // result has no semantic equality verdict and is checked byte for byte below.
    match result.value().semantic_eq(&expected) {
        Some(equal) => assert!(equal, "edit differs from tree model"),
        None => assert!(edits.is_empty(), "a successful edit has repeated names"),
    }
    model.check_bytes(result.value().view());
    true
}
