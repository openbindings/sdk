//! Shared environment names and request-local compact identifier selection.
use super::*;
use crate::query_identifiers::Identifiers;

pub(super) struct Inventory {
    ids: Identifiers<'static>,
    names: Vec<(u32, NameSelection)>,
}
impl Inventory {
    pub(super) fn new(space: &SchemaSpace) -> Self {
        let mut ids = Identifiers::new(usize::MAX);
        let mut names = Vec::new();
        for name in space
            .local
            .names
            .keys()
            .map(String::as_str)
            .chain(space.packaged.names())
        {
            // Queries select the resource part, so a nonempty fragment in a
            // declaration cannot become a resource association here.
            if name.contains('#') {
                continue;
            }
            if let Some(handle) = ids.absolute_owned(name) {
                names.push((handle, space.name(name).expect("inventoried name")));
            }
        }
        names.sort_by(|a, b| ids.grouping(a.0, b.0));
        names.dedup_by(|a, b| ids.grouping(a.0, b.0).is_eq());
        Self { ids, names }
    }
    fn query(&self) -> Names<'_> {
        Names {
            ids: self.ids.overlay(),
            bases: HashMap::new(),
            names: &self.names,
        }
    }
}

pub(super) struct Names<'a> {
    ids: Identifiers<'a>,
    bases: HashMap<usize, Option<u32>>,
    names: &'a [(u32, NameSelection)],
}
impl<'a> Names<'a> {
    pub(super) fn new(space: &'a SchemaSpace) -> Self {
        space
            .names
            .get_or_init(|| Box::new(Inventory::new(space)))
            .query()
    }
    fn selection(&self, handle: u32) -> Option<NameSelection> {
        self.names
            .binary_search_by(|(name, _)| self.ids.grouping(*name, handle))
            .ok()
            .map(|at| self.names[at].1)
    }
    pub(super) fn select(
        &mut self,
        resource: usize,
        base: Option<&str>,
        reference: &str,
    ) -> Result<(u32, Option<NameSelection>), QueryFailure> {
        let parent = *self
            .bases
            .entry(resource)
            .or_insert_with(|| base.and_then(|base| self.ids.absolute_owned(base)));
        let reference = reference
            .split_once('#')
            .map_or(reference, |(name, _)| name);
        let handle = self
            .ids
            .compared_owned(parent, reference)
            .ok_or(QueryFailure::InvalidIdentifier)?;
        if !self.ids.valid_absolute(handle) {
            return Err(QueryFailure::MalformedReference);
        }
        let exact = self.selection(handle).filter(|s| s.exact_contained);
        let selected = exact.or_else(|| {
            let canonical = self.ids.canonical(handle)?;
            self.selection(canonical)
        });
        Ok((handle, selected))
    }
    pub(super) fn length(&self, handle: u32) -> usize {
        self.ids.length(handle)
    }
    pub(super) fn spell(&self, handle: u32) -> String {
        self.ids.spelled(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_admission_precedes_missing_uri_spelling() {
        for absolute in [false, true] {
            let base = format!("https://e.test/{}/", "a".repeat(1024 * 1024));
            let document =
                RawDocument::parse(format!(r#"{{"schemas":{{"entry":{{"$id":"{base}"}}}}}}"#))
                    .unwrap();
            let space = SchemaSpace::new(document, Supplied::default()).unwrap();
            let holder = space.local_nodes().next().unwrap().0;
            let mut resolver = space.reference_resolver();
            let reference = if absolute {
                format!("{base}s0")
            } else {
                "s0".into()
            };
            // Warm the name inventory and identical compact query input.
            resolver
                .query_for_report(holder, &reference, &mut |_, _| Ok(()))
                .unwrap()
                .unwrap_err();
            crate::resolution_work::take();
            let mut length = 0;
            let refused = resolver.query_for_report(holder, &reference, &mut |query, bytes| {
                assert!(
                    matches!(query, Err(QueryFailure::ResourceUnavailable(uri)) if uri.is_empty())
                );
                length = bytes;
                Err(Failure::new(
                    FailureReason::LimitExceeded,
                    "test-limit",
                    "test admission",
                ))
            });
            assert_eq!(refused.unwrap_err().code, "test-limit");
            assert_eq!(length, base.len() + 2);
            let retained = resolver
                .query_for_report(holder, &reference, &mut |_, bytes| {
                    assert_eq!(bytes, length);
                    Ok(())
                })
                .unwrap()
                .unwrap_err();
            assert_eq!(
                retained,
                QueryFailure::ResourceUnavailable(format!("{base}s0"))
            );
        }
    }

    #[test]
    fn inspection_resolution_bytes_do_not_multiply_inherited_base_text() {
        for shape in [
            "identical",
            "fragments",
            "leaf",
            "directory-supplied",
            "leaf-supplied",
        ] {
            let mut work = Vec::new();
            let mut cold = Vec::new();
            for length in [1024, 65_536, 1_048_576] {
                let base = format!(
                    "https://e.test/{}{}",
                    "a".repeat(length),
                    if shape == "directory-supplied" {
                        "/"
                    } else {
                        ""
                    }
                );
                let refs = 256;
                let defs = (0..refs)
                    .map(|i| format!("\"s{i}\":true"))
                    .collect::<Vec<_>>()
                    .join(",");
                let items = (0..refs)
                    .map(|i| {
                        let reference = match shape {
                            "identical" => "#".into(),
                            "fragments" => format!("#/$defs/s{i}"),
                            "directory-supplied" => "s0".into(),
                            _ => format!("s{i}"),
                        };
                        format!(r##"{{"$ref":"{reference}"}}"##)
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let document = RawDocument::parse(format!(r##"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{{"$id":"{base}","$defs":{{{defs}}},"allOf":[{items}]}}}}}}"##)).unwrap();
                let supplied = if shape.ends_with("supplied") {
                    Supplied::new(
                        (0..if shape == "directory-supplied" {
                            1
                        } else {
                            refs
                        })
                            .map(|i| ProjectedResource {
                                uri: if shape == "directory-supplied" {
                                    format!("https://fetch.test/s{i}")
                                } else {
                                    format!("https://e.test/s{i}")
                                },
                                // Keep retrieval coordinates short so this work
                                // probe still inspects every reference under the
                                // report allowance. The resolved identity stays long.
                                document: JsonValue::parse(if shape == "directory-supplied" {
                                    format!(r#"{{"$id":"{base}s{i}"}}"#)
                                } else {
                                    "true".into()
                                })
                                .unwrap(),
                            }),
                    )
                    .unwrap()
                } else {
                    Supplied::default()
                };
                let space = SchemaSpace::new(document, supplied).unwrap();
                let mut resolver = space.reference_resolver();
                // Initialize the name inventory once. The measured pass must
                // still resolve every occurrence, with no query-result cache.
                crate::resolution_work::take();
                assert!(space.references_with(&mut resolver, &|| false).complete);
                cold.push(crate::resolution_work::take());
                let report = space.references_with(&mut resolver, &|| false);
                work.push(crate::resolution_work::take());
                assert!(report.complete);
                assert_eq!(report.references.len(), refs);
                // A new run borrows only the immutable environment inventory.
                // It still starts with no bases or query inputs.
                drop(resolver);
                let mut resolver = space.reference_resolver();
                assert!(space.references_with(&mut resolver, &|| false).complete);
                drop(resolver);
                let mut resolver = space.reference_resolver();
                assert!(space.references_with(&mut resolver, &|| false).complete);
                if shape != "leaf" {
                    assert!(
                        report
                            .references
                            .iter()
                            .all(|r| matches!(r.resolution, ReferenceResolution::Located { .. }))
                    );
                }
            }
            // Inventory construction may process each input name once. Once
            // initialized, every occurrence must do exactly the same amount of
            // text work at all three base lengths, without result memoization.
            println!("{shape}: cold={cold:?}, initialized={work:?}");
        }
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;

    #[test]
    fn compact_queries_keep_exact_priority_normalized_aliases_and_conflicts() {
        for base in [
            "https://EXAMPLE.test:443/a/%7e/b",
            "s:a/b",
            "s:/a/%2e%2e//b",
            "https://example.test/a/b",
        ] {
            for conflict in [false, true] {
                let text = serde_json::json!({"openbindings":"0.2.0","operations":{},"schemas":{
                    "entry":{"$id":base,"$defs":{"s":true},"$anchor":"anchor"},
                    "other":{"$id": if conflict { base } else { "https://example.test/a/target" } }
                }})
                .to_string();
                let supplied = Supplied::new([ProjectedResource {
                    uri: "https://example.test/a/supplied".into(),
                    document: JsonValue::parse(r##"{"$id":"https://EXAMPLE.test:443/a/%73upplied","$defs":{"s":true},"$anchor":"anchor"}"##).unwrap(),
                }]).unwrap();
                let space = SchemaSpace::new(RawDocument::parse(text).unwrap(), supplied).unwrap();
                for (holder, _) in space.local_nodes() {
                    for reference in [
                        "#",
                        "#/$defs/s",
                        "#missing",
                        "#anchor",
                        "target",
                        "supplied",
                        "supplied#/$defs/s",
                        "../a/%73upplied",
                        "missing",
                        "https://EXAMPLE.test:443/a/target",
                        "https://example.test/a/%74arget",
                        "%",
                        "s:/.//host:word",
                    ] {
                        let node = space.node(holder);
                        let current = space.resource(node.resource);
                        let old = (|| {
                            if !uri::valid(reference) {
                                return Err(QueryFailure::MalformedReference);
                            }
                            if !space.eligible(node.resource) {
                                return Err(QueryFailure::IdentityConflict);
                            }
                            let resource = if current.anonymous && !uri::absolute(reference) {
                                if !reference.is_empty() && !reference.starts_with('#') {
                                    return Err(QueryFailure::InvalidIdentifier);
                                }
                                node.resource
                            } else {
                                let name = uri::compared_id(current.base.as_deref(), reference)
                                    .ok_or(QueryFailure::InvalidIdentifier)?;
                                space.named_resource(&name)?
                            };
                            space.query_fragment(resource, reference)
                        })()
                        .map(|t| (t.node, t.dynamic_name));
                        let new = space
                            .query(holder, reference)
                            .map(|t| (t.node, t.dynamic_name));
                        assert_eq!(new, old, "{base} / {conflict} / {holder} / {reference}");
                    }
                }
            }
        }
    }
}
