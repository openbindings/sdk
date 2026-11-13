//! Pre-entry proof that each immediate child has at most one productive
//! continuation, including all same-instance reference/applicator paths.
use super::*;
use openbindings_internal_schema::{Control, Reach, SchemaSpace};
use std::collections::HashSet;

mod graph;
#[derive(Clone, Copy)]
pub(super) struct Edge {
    pub to: usize,
    pub advances: bool,
    pub exclusive: bool,
}
pub(super) struct Proof {
    pub exact: usize,
    pub applications: usize,
}

pub(super) fn qualifies(
    space: &SchemaSpace,
    entry: usize,
    nodes: &[usize],
    reach: &Reach,
    applications: &HashMap<usize, Vec<Edge>>,
    own: &HashMap<usize, expansion::Own>,
    control: &dyn Control,
) -> Result<Result<Proof, &'static str>, PreparationFailure> {
    // Only scalar proof weights escape; all graph and selector storage drops
    // before this scope releases its conservative reservations.
    let scratch = openbindings_internal_schema::preparation::Scratch::new(control);
    let control: &dyn Control = &scratch;
    // Cycle reachability, reverse edges, finite folds and entry sets are
    // preparation owners only. The existing selector scan prepays its own data.
    control
        .charge_allocation(nodes.len().saturating_mul(768).saturating_add(512))
        .map_err(PreparationFailure::from)?;
    let Some(productive) = graph::productive(nodes, applications, control)? else {
        return Ok(Err("recursive-application-graph"));
    };
    let Some(entries) = partition(space, entry, nodes, reach, &productive, control)? else {
        return Ok(Err("recursive-child-partition-profile"));
    };
    let Some(proof) = graph::fold(nodes, applications, &productive, own, &entries, control)? else {
        return Ok(Err("recursive-finite-expansion-profile"));
    };
    Ok(Ok(proof))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Exact,
    Prefix,
    ObjectValues,
    ObjectNames,
    Array { start: usize, end: usize },
}
struct Site {
    kind: Kind,
    text: String,
}

// Fallible heap sort keeps text comparisons inside the caller's work and
// cancellation controls without allocating a second selector collection.
fn sort_objects(objects: &mut [&Site], control: &dyn Control) -> Result<(), PreparationFailure> {
    fn greater(a: &Site, b: &Site, control: &dyn Control) -> Result<bool, PreparationFailure> {
        control
            .charge_work(a.text.len().min(b.text.len()).saturating_add(1))
            .map_err(PreparationFailure::from)?;
        let order = a
            .text
            .cmp(&b.text)
            .then_with(|| (a.kind == Kind::Exact).cmp(&(b.kind == Kind::Exact)));
        control.check().map_err(PreparationFailure::from)?;
        Ok(order.is_gt())
    }
    fn sift(
        objects: &mut [&Site],
        mut root: usize,
        control: &dyn Control,
    ) -> Result<(), PreparationFailure> {
        while let Some(mut child) = root
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .filter(|&n| n < objects.len())
        {
            if child + 1 < objects.len() && greater(objects[child + 1], objects[child], control)? {
                child += 1;
            }
            if !greater(objects[child], objects[root], control)? {
                break;
            }
            objects.swap(root, child);
            root = child;
        }
        Ok(())
    }
    for root in (0..objects.len() / 2).rev() {
        sift(objects, root, control)?;
    }
    for end in (1..objects.len()).rev() {
        objects.swap(0, end);
        sift(&mut objects[..end], 0, control)?;
    }
    Ok(())
}

fn partition(
    space: &SchemaSpace,
    entry: usize,
    nodes: &[usize],
    reach: &Reach,
    productive: &HashSet<usize>,
    control: &dyn Control,
) -> Result<Option<Vec<usize>>, PreparationFailure> {
    control
        .charge_allocation(nodes.len().saturating_mul(512).saturating_add(512))
        .map_err(PreparationFailure::from)?;
    let mut sites: HashMap<usize, Vec<Site>> = HashMap::new();
    let mut entries = HashSet::from([entry]);
    let mut text_bytes = 0usize;
    for &node in nodes {
        control.check().map_err(PreparationFailure::from)?;
        let original = space.node(node);
        let profile = original.profile.expect("admitted original profile");
        let value = original.value.view();
        let mut add = |child: JsonRef<'_>, kind, text: &str| -> Result<bool, PreparationFailure> {
            // Constant schemas terminate here; they remain in actual lowering.
            if child.as_bool().is_some() {
                return Ok(true);
            }
            let Some(&target) = space
                .source(original.source)
                .positions
                .get(&backend::ref_id(child))
            else {
                return Ok(false);
            };
            if !reach.nodes.contains(&target) {
                return Ok(false);
            }
            // Terminal constraints remain in lowering and the finite fold;
            // only routes that can reach recursion need a child partition.
            if !productive.contains(&target) {
                return Ok(true);
            }
            text_bytes = text_bytes.saturating_add(text.len());
            if text_bytes > openbindings_internal_schema::PROGRAM_TEXT_LIMIT {
                return Ok(false);
            }
            control
                .charge_allocation(text.len().saturating_add(32))
                .map_err(PreparationFailure::from)?;
            control
                .charge_work(text.len().saturating_add(1))
                .map_err(PreparationFailure::from)?;
            sites.entry(node).or_default().push(Site {
                kind,
                text: text.to_owned(),
            });
            entries.insert(target);
            Ok(true)
        };
        for keyword in ["properties", "patternProperties"] {
            if let Some(members) = profile.get(value, keyword).and_then(|v| v.members()) {
                for member in members {
                    control.check().map_err(PreparationFailure::from)?;
                    let Some(name) = member.name.as_str() else {
                        return Ok(None);
                    };
                    let (kind, text) = if keyword == "properties" {
                        (Kind::Exact, name)
                    } else if let Some(prefix) = name.strip_prefix('^').filter(|p| {
                        !p.is_empty()
                            && p.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                    }) {
                        (Kind::Prefix, prefix)
                    } else {
                        // An arbitrary admitted pattern selects a subset of
                        // object values. Treat it as all values for overlap.
                        (Kind::ObjectValues, "")
                    };
                    if !add(member.value, kind, text)? {
                        return Ok(None);
                    }
                }
            }
        }
        let prefix = profile.get(value, "prefixItems").and_then(|v| v.elements());
        let mut prefix_len = 0;
        if let Some(children) = prefix {
            for (i, child) in children.enumerate() {
                prefix_len = i + 1;
                if !add(
                    child,
                    Kind::Array {
                        start: i,
                        end: i + 1,
                    },
                    "",
                )? {
                    return Ok(None);
                }
            }
        }
        for (keyword, kind) in [
            (
                "items",
                Kind::Array {
                    start: prefix_len,
                    end: usize::MAX,
                },
            ),
            (
                "contains",
                Kind::Array {
                    start: 0,
                    end: usize::MAX,
                },
            ),
            ("additionalProperties", Kind::ObjectValues),
            ("propertyNames", Kind::ObjectNames),
        ] {
            let Some(child) = profile.get(value, keyword) else {
                continue;
            };
            if !add(child, kind, "")? {
                return Ok(None);
            }
        }
    }
    // Same-instance acyclicity is independent of the child partition proof.
    // Keep parallel edges: duplicate paths cannot become a cache hit.
    let mut pending: HashMap<_, usize> = nodes.iter().map(|&n| (n, 0)).collect();
    for children in reach.edges.values() {
        for child in children {
            control.check().map_err(PreparationFailure::from)?;
            *pending.get_mut(child).expect("completed child membership") += 1;
        }
    }
    let mut ready: Vec<_> = nodes.iter().copied().filter(|n| pending[n] == 0).collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(node) = ready.pop() {
        control.check().map_err(PreparationFailure::from)?;
        order.push(node);
        for child in reach.edges.get(&node).into_iter().flatten() {
            control.check().map_err(PreparationFailure::from)?;
            let n = pending.get_mut(child).expect("completed child membership");
            *n -= 1;
            if *n == 0 {
                ready.push(*child)
            }
        }
    }
    if order.len() != nodes.len() {
        return Ok(None);
    }
    // Reuse the count map for whether a closure contains a child site.
    for node in order.into_iter().rev() {
        let active = sites.contains_key(&node)
            || reach
                .edges
                .get(&node)
                .into_iter()
                .flatten()
                .any(|n| pending[n] != 0);
        pending.insert(node, usize::from(active));
    }
    let mut entries: Vec<_> = entries.into_iter().collect();
    entries.sort_unstable();
    let mut seen = HashSet::new();
    let mut queue = Vec::new();
    let mut objects = Vec::new();
    let mut arrays = Vec::new();
    let mut occurrences = 0usize;
    for &entry in &entries {
        if pending[&entry] == 0 {
            continue;
        }
        seen.clear();
        queue.clear();
        objects.clear();
        arrays.clear();
        queue.push(entry);
        let mut all_objects = 0;
        let mut object_names = 0;
        while let Some(node) = queue.pop() {
            control.check().map_err(PreparationFailure::from)?;
            occurrences = occurrences.saturating_add(1);
            if occurrences > super::MAXIMUM_NODES || !seen.insert(node) {
                return Ok(None);
            }
            if let Some(local) = sites.get(&node) {
                occurrences = occurrences.saturating_add(local.len());
                if occurrences > super::MAXIMUM_NODES {
                    return Ok(None);
                }
                control
                    .charge_work(local.len())
                    .map_err(PreparationFailure::from)?;
                for site in local {
                    match site.kind {
                        Kind::Exact | Kind::Prefix => objects.push(site),
                        Kind::ObjectValues => all_objects += 1,
                        Kind::ObjectNames => object_names += 1,
                        Kind::Array { start, end } => arrays.push((start, end)),
                    }
                }
            }
            for child in reach.edges.get(&node).into_iter().flatten() {
                control.check().map_err(PreparationFailure::from)?;
                if pending[child] != 0 {
                    queue.push(*child)
                }
            }
        }
        if object_names > 1 || all_objects > 1 || (all_objects == 1 && !objects.is_empty()) {
            return Ok(None);
        }
        sort_objects(&mut objects, control)?;
        for pair in objects.windows(2) {
            control
                .charge_work(
                    pair[0]
                        .text
                        .len()
                        .min(pair[1].text.len())
                        .saturating_mul(2)
                        .saturating_add(1),
                )
                .map_err(PreparationFailure::from)?;
            let overlaps = pair[0].text == pair[1].text
                || (pair[0].kind == Kind::Prefix && pair[1].text.starts_with(&pair[0].text));
            control.check().map_err(PreparationFailure::from)?;
            if overlaps {
                return Ok(None);
            }
        }
        arrays.sort_unstable();
        if arrays.windows(2).any(|p| p[1].0 < p[0].1) {
            return Ok(None);
        }
        control.check().map_err(PreparationFailure::from)?;
    }
    Ok(Some(entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings_internal_schema::preparation::Attempt;

    #[test]
    fn selector_text_sort_observes_shared_work_and_cancellation() {
        let common = "a".repeat(4096);
        let sites = [
            Site {
                kind: Kind::Exact,
                text: format!("{common}z"),
            },
            Site {
                kind: Kind::Exact,
                text: common.clone(),
            },
            Site {
                kind: Kind::Prefix,
                text: common,
            },
        ];
        let failure = Attempt::run(1, usize::MAX, &|| false, |attempt| {
            sort_objects(&mut sites.iter().collect::<Vec<_>>(), attempt)
        })
        .unwrap_err();
        assert_eq!(failure.code, "preparation-sdk-work-units");
        assert!(sort_objects(&mut sites.iter().collect::<Vec<_>>(), &|| true).is_err());
        let mut sorted = sites.iter().collect::<Vec<_>>();
        sort_objects(&mut sorted, &|| false).unwrap();
        assert!(sorted[0].kind == Kind::Prefix);
        assert_eq!(sorted[0].text, sorted[1].text);
        assert!(sorted[1].text < sorted[2].text);
    }
}
