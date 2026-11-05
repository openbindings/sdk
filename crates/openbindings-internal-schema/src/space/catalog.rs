//! Shared pinned catalog bodies and indexes, loaded by resource on demand.
//! Constant inventory extents keep original dense identifiers and budgets stable.
use super::*;
use std::sync::OnceLock;
const CORE: usize = 8;
struct Entry {
    uri: &'static str,
    text: &'static str,
    nodes: usize,
    offset: usize,
}
static ENTRIES: [Entry; 17] = [
    Entry {
        uri: "https://json-schema.org/draft/2020-12/schema",
        text: crate::fixed_schema::META_FILES[0],
        nodes: 16,
        offset: 0,
    },
    Entry {
        uri: "https://json-schema.org/draft/2020-12/meta/core",
        text: crate::fixed_schema::META_FILES[1],
        nodes: 16,
        offset: 16,
    },
    Entry {
        uri: "https://json-schema.org/draft/2020-12/meta/applicator",
        text: crate::fixed_schema::META_FILES[2],
        nodes: 22,
        offset: 32,
    },
    Entry {
        uri: "https://json-schema.org/draft/2020-12/meta/unevaluated",
        text: crate::fixed_schema::META_FILES[3],
        nodes: 3,
        offset: 54,
    },
    Entry {
        uri: "https://json-schema.org/draft/2020-12/meta/validation",
        text: crate::fixed_schema::META_FILES[4],
        nodes: 31,
        offset: 57,
    },
    Entry {
        uri: "https://json-schema.org/draft/2020-12/meta/meta-data",
        text: crate::fixed_schema::META_FILES[5],
        nodes: 9,
        offset: 88,
    },
    Entry {
        uri: "https://json-schema.org/draft/2020-12/meta/format-annotation",
        text: crate::fixed_schema::META_FILES[6],
        nodes: 2,
        offset: 97,
    },
    Entry {
        uri: "https://json-schema.org/draft/2020-12/meta/content",
        text: crate::fixed_schema::META_FILES[7],
        nodes: 4,
        offset: 99,
    },
    Entry {
        uri: "http://json-schema.org/draft-07/schema#",
        text: crate::fixed_schema::DRAFT7_FILES[0],
        nodes: 70,
        offset: 103,
    },
    Entry {
        uri: "https://json-schema.org/draft/2019-09/schema",
        text: crate::fixed_schema::DRAFT2019_FILES[0],
        nodes: 13,
        offset: 173,
    },
    Entry {
        uri: "https://json-schema.org/draft/2019-09/meta/core",
        text: crate::fixed_schema::DRAFT2019_FILES[1],
        nodes: 13,
        offset: 186,
    },
    Entry {
        uri: "https://json-schema.org/draft/2019-09/meta/applicator",
        text: crate::fixed_schema::DRAFT2019_FILES[2],
        nodes: 26,
        offset: 199,
    },
    Entry {
        uri: "https://json-schema.org/draft/2019-09/meta/validation",
        text: crate::fixed_schema::DRAFT2019_FILES[3],
        nodes: 31,
        offset: 225,
    },
    Entry {
        uri: "https://json-schema.org/draft/2019-09/meta/meta-data",
        text: crate::fixed_schema::DRAFT2019_FILES[4],
        nodes: 9,
        offset: 256,
    },
    Entry {
        uri: "https://json-schema.org/draft/2019-09/meta/format",
        text: crate::fixed_schema::DRAFT2019_FILES[5],
        nodes: 2,
        offset: 265,
    },
    Entry {
        uri: "https://json-schema.org/draft/2019-09/meta/content",
        text: crate::fixed_schema::DRAFT2019_FILES[6],
        nodes: 4,
        offset: 267,
    },
    Entry {
        uri: "https://json-schema.org/draft/2020-12/meta/format-assertion",
        text: crate::fixed_schema::FORMAT_ASSERTION_FILES[0],
        nodes: 2,
        offset: 271,
    },
];
static VALUES: [OnceLock<ProjectedResource>; 17] = [const { OnceLock::new() }; 17];
static PARTS: [OnceLock<Part>; 9] = [const { OnceLock::new() }; 9];

fn identifier(entry: &Entry) -> &str {
    entry.uri.strip_suffix('#').unwrap_or(entry.uri)
}
pub(super) fn contains(name: &str) -> bool {
    ENTRIES.iter().any(|entry| identifier(entry) == name)
}
pub(super) fn resources() -> impl ExactSizeIterator<Item = &'static ProjectedResource> {
    (0..ENTRIES.len()).map(value)
}
fn value(index: usize) -> &'static ProjectedResource {
    VALUES[index].get_or_init(|| {
        let entry = &ENTRIES[index];
        let document = JsonValue::parse(entry.text).expect("pinned meta-schema JSON");
        assert_eq!(
            document.get("$id").and_then(|v| v.as_str()),
            Some(entry.uri)
        );
        ProjectedResource {
            uri: entry.uri.into(),
            document,
        }
    })
}
struct Part {
    nodes: Vec<SpaceNode>,
    resources: Vec<Resource>,
    sources: Vec<Source>,
}
fn part(index: usize) -> &'static Part {
    PARTS[index - CORE].get_or_init(|| {
        let resource = value(index);
        let mut layer = Layer::default();
        let inventory = SchemaIndex::supplied_nodes(&resource.document);
        layer.add_source(
            Some(resource.uri.clone()),
            resource.document.clone(),
            &inventory,
            ResourceOrigin::Standard,
            &HashSet::new(),
        );
        layer.finalize(|names, name| names.get(name).copied());
        let entry = &ENTRIES[index];
        assert_eq!(layer.nodes.len(), entry.nodes, "pinned catalog node extent");
        assert_eq!(layer.resources.len(), 2, "pinned catalog resource extent");
        assert_eq!(layer.sources.len(), 1, "pinned catalog source extent");
        assert_eq!(layer.names.len(), 1, "pinned catalog identifier extent");
        assert!(layer.names.contains_key(identifier(entry)));
        let node_base = PACKAGED + entry.offset;
        let resource_base = PACKAGED + 2 * index;
        let source_base = PACKAGED + index;
        for node in &mut layer.nodes {
            node.resource += resource_base;
            node.source += source_base;
        }
        for resource in &mut layer.resources {
            resource.root = resource.root.map(|node| node + node_base);
            for positions in resource.anchors.values_mut() {
                for (node, _) in positions {
                    *node += node_base;
                }
            }
        }
        for source in &mut layer.sources {
            for node in source.positions.values_mut() {
                *node += node_base;
            }
        }
        Part {
            nodes: layer.nodes,
            resources: layer.resources,
            sources: layer.sources,
        }
    })
}
// Only the normative eight-file2020-12 profile initializes with a schema space.
// Other editions and the optional format-assertion supplement initialize on use.
pub(super) struct Packaged {
    core: Layer,
    supplemental: bool,
}
impl Packaged {
    pub(super) fn new() -> Self {
        let mut core = Layer {
            base: PACKAGED,
            ..Layer::default()
        };
        for (index, entry) in ENTRIES.iter().enumerate().take(CORE) {
            let resource = value(index);
            let inventory = SchemaIndex::supplied_nodes(&resource.document);
            assert_eq!(inventory.nodes.len(), entry.nodes);
            core.add_source(
                Some(resource.uri.clone()),
                resource.document.clone(),
                &inventory,
                ResourceOrigin::Standard,
                &HashSet::new(),
            );
        }
        core.finalize(|names, name| names.get(name).copied());
        assert_eq!(core.nodes.len(), ENTRIES[CORE].offset);
        assert_eq!(core.resources.len(), 2 * CORE);
        assert_eq!(core.sources.len(), CORE);
        Self {
            core,
            supplemental: true,
        }
    }
    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self {
            core: Layer {
                base: PACKAGED,
                ..Layer::default()
            },
            supplemental: false,
        }
    }
    pub(super) fn node_count(&self) -> usize {
        if self.supplemental {
            let last = &ENTRIES[ENTRIES.len() - 1];
            last.offset + last.nodes
        } else {
            self.core.nodes.len()
        }
    }
    pub(super) fn identity_bytes(&self) -> usize {
        if self.supplemental {
            ENTRIES.iter().map(|e| 2 * identifier(e).len()).sum()
        } else {
            self.core
                .resources
                .iter()
                .map(|r| r.base.as_ref().map_or(0, String::len))
                .sum()
        }
    }
    pub(super) fn name(&self, name: &str) -> Option<NameSelection> {
        self.core.names.get(name).copied().or_else(|| {
            self.supplemental
                .then(|| {
                    ENTRIES
                        .iter()
                        .enumerate()
                        .skip(CORE)
                        .find(|(_, entry)| identifier(entry) == name)
                        .map(|(index, _)| NameSelection {
                            origin: ResourceOrigin::Standard,
                            carrier: Some(PACKAGED + 2 * index + 1),
                            exact_contained: false,
                        })
                })
                .flatten()
        })
    }
    #[cfg(test)]
    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        let supplemental = self.supplemental;
        self.core.names.keys().map(String::as_str).chain(
            ENTRIES
                .iter()
                .skip(CORE)
                .filter(move |_| supplemental)
                .map(identifier),
        )
    }
    pub(super) fn node(&self, id: usize) -> &SpaceNode {
        let index = id - PACKAGED;
        if index < self.core.nodes.len() {
            return &self.core.nodes[index];
        }
        assert!(self.supplemental);
        let file = ENTRIES
            .iter()
            .enumerate()
            .skip(CORE)
            .find(|(_, e)| index >= e.offset && index < e.offset + e.nodes)
            .map(|(i, _)| i)
            .expect("packaged node");
        &part(file).nodes[index - ENTRIES[file].offset]
    }
    pub(super) fn resource(&self, id: usize) -> &Resource {
        let index = id - PACKAGED;
        if index < self.core.resources.len() {
            &self.core.resources[index]
        } else {
            assert!(self.supplemental);
            &part(index / 2).resources[index % 2]
        }
    }
    pub(super) fn source(&self, id: usize) -> &Source {
        let index = id - PACKAGED;
        if index < self.core.sources.len() {
            &self.core.sources[index]
        } else {
            assert!(self.supplemental);
            &part(index).sources[0]
        }
    }
    pub(super) fn nodes(&self) -> impl Iterator<Item = (usize, &SpaceNode)> {
        let supplemental = self.supplemental;
        self.core.nodes().chain(
            ENTRIES
                .iter()
                .enumerate()
                .skip(CORE)
                .filter(move |_| supplemental)
                .flat_map(|(index, entry)| {
                    part(index)
                        .nodes
                        .iter()
                        .enumerate()
                        .map(move |(at, node)| (PACKAGED + entry.offset + at, node))
                }),
        )
    }
    pub(super) fn resources(&self) -> impl Iterator<Item = (usize, &Resource)> {
        let supplemental = self.supplemental;
        self.core.resources().chain(
            ENTRIES
                .iter()
                .enumerate()
                .skip(CORE)
                .filter(move |_| supplemental)
                .flat_map(|(index, _)| {
                    part(index)
                        .resources
                        .iter()
                        .enumerate()
                        .map(move |(at, resource)| (PACKAGED + 2 * index + at, resource))
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_extents_match_every_original_catalog_index() {
        let mut total = 0;
        for (index, entry) in ENTRIES.iter().enumerate() {
            assert_eq!(entry.offset, total);
            let resource = value(index);
            let inventory = SchemaIndex::supplied_nodes(&resource.document);
            assert_eq!(entry.nodes, inventory.nodes.len());
            let mut layer = Layer::default();
            layer.add_source(
                Some(resource.uri.clone()),
                resource.document.clone(),
                &inventory,
                ResourceOrigin::Standard,
                &HashSet::new(),
            );
            assert_eq!(layer.resources.len(), 2);
            assert_eq!(layer.sources.len(), 1);
            total += entry.nodes;
        }
        assert_eq!(total, 273);
        let catalog = packaged();
        assert_eq!(catalog.node_count(), total);
        assert_eq!(catalog.nodes().count(), total);
        assert_eq!(catalog.resources().count(), 2 * ENTRIES.len());
        let bytes: usize = catalog
            .resources()
            .map(|(_, r)| r.base.as_ref().map_or(0, String::len))
            .sum();
        assert_eq!(catalog.identity_bytes(), bytes);
    }

    #[test]
    fn plain_identifiers_match_the_retired_registry_membership_policy() {
        #[derive(Clone)]
        struct NoRetrieval;
        impl jsonschema::Retrieve for NoRetrieval {
            fn retrieve(
                &self,
                _: &jsonschema::Uri<String>,
            ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
                Err("no retrieval".into())
            }
        }
        let mut builder = jsonschema::Registry::new()
            .draft(jsonschema::Draft::Draft202012)
            .retriever(NoRetrieval);
        for resource in resources() {
            builder = builder
                .add(
                    &resource.uri,
                    serde_json::from_str::<serde_json::Value>(resource.document.text()).unwrap(),
                )
                .unwrap();
        }
        let old = builder.prepare().unwrap();
        let mut names = Vec::new();
        for entry in &ENTRIES {
            names.extend([
                entry.uri.to_owned(),
                identifier(entry).to_owned(),
                format!("{}#", identifier(entry)),
                format!("{}/", identifier(entry)),
                format!("{}#meta", identifier(entry)),
                entry.uri.replace("json-schema.org", "JSON-SCHEMA.ORG"),
                entry.uri.replace("draft/2020-12", "draft/%32%30%32%30-12"),
            ]);
        }
        names.extend([
            "https://json-schema.org/draft/2020-12/absent".into(),
            "http://json-schema.org/draft-04/schema".into(),
            "https://example.test/custom".into(),
            "not an absolute URI".into(),
        ]);
        for raw in names {
            if let Some(name) = crate::input_policy::canonical_uri(&raw) {
                assert_eq!(
                    contains(&name),
                    old.contains_resource(&name),
                    "{raw} -> {name}"
                );
            }
        }
    }

    #[test]
    fn ordinary_spaces_leave_supplemental_bodies_uninitialized() {
        const NAME: &str =
            "space::catalog::tests::ordinary_spaces_leave_supplemental_bodies_uninitialized";
        if std::env::var_os("OB_CATALOG_FRESH_CHECK").is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture"])
                .env("OB_CATALOG_FRESH_CHECK", "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stdout)
            );
            return;
        }
        assert!(VALUES.iter().all(|v| v.get().is_none()));
        for entry in &ENTRIES {
            assert!(contains(identifier(entry)));
        }
        let supplied = Supplied::new([ProjectedResource {
            uri: ENTRIES[8].uri.into(),
            document: JsonValue::parse("true").unwrap(),
        }])
        .unwrap();
        assert!(supplied.policy.protected.contains(identifier(&ENTRIES[8])));
        assert!(
            VALUES.iter().all(|v| v.get().is_none()),
            "membership must not parse any body"
        );
        let document =
            RawDocument::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":true}}}"#)
                .unwrap();
        let space = SchemaSpace::new(document, Supplied::default()).unwrap();
        assert_eq!(space.node_count(), 274);
        assert!(VALUES[..CORE].iter().all(|v| v.get().is_some()));
        assert!(VALUES[CORE..].iter().all(|v| v.get().is_none()));
        assert!(PARTS.iter().all(|v| v.get().is_none()));
        let resource = space
            .name(identifier(&ENTRIES[8]))
            .unwrap()
            .carrier
            .unwrap();
        assert!(
            VALUES[CORE..].iter().all(|v| v.get().is_none()),
            "name lookup is only an identifier lookup"
        );
        let root = space.resource(resource).root.unwrap();
        assert_eq!(
            space.node(root).value.get("$id").and_then(|v| v.as_str()),
            Some(ENTRIES[8].uri)
        );
        assert!(VALUES[8].get().is_some());
        assert!(PARTS[0].get().is_some());
        assert!(VALUES[9..].iter().all(|v| v.get().is_none()));
        assert!(PARTS[1..].iter().all(|v| v.get().is_none()));
    }
}
