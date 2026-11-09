//! Completed application-graph facts, independent of any schema shape.
use super::*;

// Deleting sinks leaves exactly the nodes that can reach a directed cycle.
// Every edge occurrence is counted, including parallel different-kind edges.
pub(super) fn productive(
    nodes: &[usize],
    edges: &HashMap<usize, Vec<Edge>>,
    control: &dyn Control,
) -> Result<Option<HashSet<usize>>, PreparationFailure> {
    control.check().map_err(PreparationFailure::from)?;
    let mut remaining: HashMap<_, _> = nodes.iter().map(|&node| (node, 0usize)).collect();
    let mut predecessors: HashMap<usize, Vec<usize>> = HashMap::new();
    for (&from, children) in edges {
        control.check().map_err(PreparationFailure::from)?;
        let Some(count) = remaining.get_mut(&from) else {
            return Ok(None);
        };
        *count = children.len();
        for edge in children {
            control.check().map_err(PreparationFailure::from)?;
            if !remaining.contains_key(&edge.to) {
                return Ok(None);
            }
            predecessors.entry(edge.to).or_default().push(from);
        }
    }
    let mut ready: Vec<_> = nodes
        .iter()
        .copied()
        .filter(|node| remaining[node] == 0)
        .collect();
    while let Some(node) = ready.pop() {
        control.check().map_err(PreparationFailure::from)?;
        for parent in predecessors.get(&node).into_iter().flatten() {
            control.check().map_err(PreparationFailure::from)?;
            let count = remaining.get_mut(parent).expect("completed predecessor");
            *count -= 1;
            if *count == 0 {
                ready.push(*parent);
            }
        }
    }
    let mut productive = HashSet::new();
    for &node in nodes {
        control.check().map_err(PreparationFailure::from)?;
        if remaining[&node] != 0 {
            productive.insert(node);
        }
    }
    Ok(Some(productive))
}

#[derive(Clone, Copy)]
struct Weight {
    exact: usize,
    applications: usize,
    coefficient: usize,
}

pub(super) fn fold(
    nodes: &[usize],
    edges: &HashMap<usize, Vec<Edge>>,
    productive: &HashSet<usize>,
    own: &HashMap<usize, expansion::Own>,
    entries: &[usize],
    control: &dyn Control,
) -> Result<Option<Proof>, PreparationFailure> {
    control.check().map_err(PreparationFailure::from)?;
    let retained = |edge: &&Edge| !(edge.advances && productive.contains(&edge.to));
    let mut complete: HashMap<usize, Weight> = HashMap::new();
    let mut active = HashSet::new();
    for &root in nodes {
        let mut pending = vec![(root, false)];
        while let Some((node, leaving)) = pending.pop() {
            control.check().map_err(PreparationFailure::from)?;
            if complete.contains_key(&node) {
                continue;
            }
            let children = edges.get(&node).map(Vec::as_slice).unwrap_or(&[]);
            if leaving {
                active.remove(&node);
                let mut weight = Weight {
                    exact: own.get(&node).map_or(0, |w| w.scratch),
                    applications: 1,
                    coefficient: own.get(&node).map_or(1, |w| w.applications),
                };
                let mut exclusive = 0;
                for edge in children.iter().filter(retained) {
                    control.check().map_err(PreparationFailure::from)?;
                    let Some(next) = complete.get(&edge.to) else {
                        return Ok(None);
                    };
                    if edge.exclusive {
                        exclusive = exclusive.max(next.coefficient);
                    } else {
                        weight.coefficient = weight.coefficient.saturating_add(next.coefficient);
                    }
                    let Some(exact) = weight.exact.checked_add(next.exact) else {
                        return Ok(None);
                    };
                    weight.exact = exact;
                    weight.applications = weight
                        .applications
                        .saturating_add(next.applications)
                        .min(MAXIMUM_NODES + 1);
                }
                weight.coefficient = weight.coefficient.saturating_add(exclusive);
                complete.insert(node, weight);
            } else {
                if !active.insert(node) {
                    return Ok(None);
                }
                pending.push((node, true));
                for edge in children.iter().filter(retained) {
                    control.check().map_err(PreparationFailure::from)?;
                    if !complete.contains_key(&edge.to) {
                        pending.push((edge.to, false));
                    }
                }
            }
        }
    }
    let mut exact = 0usize;
    let mut applications = 0usize;
    let mut coefficient = 0usize;
    // Different entry types may send terminal paths from different ancestors
    // to the same subject. Sum their path weights; max would undercount them.
    for entry in entries {
        control.check().map_err(PreparationFailure::from)?;
        let Some(weight) = complete.get(entry) else {
            return Ok(None);
        };
        let Some(sum) = exact.checked_add(weight.exact) else {
            return Ok(None);
        };
        exact = sum;
        coefficient = coefficient.saturating_add(weight.coefficient);
        applications = applications.saturating_add(weight.applications);
        if applications > MAXIMUM_NODES {
            return Ok(None);
        }
    }
    #[cfg(test)]
    entries::record_expansion(applications);
    Ok(Some(Proof {
        exact,
        applications: coefficient,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edge(to: usize, advances: bool) -> Edge {
        Edge {
            to,
            advances,
            exclusive: false,
        }
    }
    #[test]
    fn finite_expansion_exact_cap_and_one_beyond_stop_before_execution() {
        for count in [MAXIMUM_NODES - 1, MAXIMUM_NODES] {
            let edges = HashMap::from([(0, vec![edge(1, false); count])]);
            let result = fold(
                &[0, 1],
                &edges,
                &HashSet::new(),
                &HashMap::new(),
                &[0],
                &|| false,
            )
            .unwrap();
            assert_eq!(result.is_some(), count < MAXIMUM_NODES);
        }
        let cycle = HashMap::from([(0, vec![edge(1, false)]), (1, vec![edge(0, false)])]);
        assert!(
            fold(
                &[0, 1],
                &cycle,
                &HashSet::from([0, 1]),
                &HashMap::new(),
                &[0],
                &|| false
            )
            .unwrap()
            .is_none()
        );
        let overflow = HashMap::from([(0, vec![edge(1, false), edge(1, false)])]);
        assert!(
            fold(
                &[0, 1],
                &overflow,
                &HashSet::new(),
                &HashMap::from([(
                    1,
                    expansion::Own {
                        scratch: usize::MAX,
                        applications: 1
                    }
                )]),
                &[0],
                &|| false
            )
            .unwrap()
            .is_none()
        );
    }
}

#[cfg(test)]
mod control_tests {
    use super::*;
    use openbindings_internal_schema::preparation::Attempt;
    #[test]
    fn inherited_work_and_storage_poison_are_failures_not_profile_declines() {
        let edges = HashMap::from([(
            0,
            vec![Edge {
                to: 0,
                advances: true,
                exclusive: false,
            }],
        )]);
        for storage in [false, true] {
            let mut failures = [false; 2];
            let result = Attempt::run(
                if storage { usize::MAX } else { 1 },
                if storage { 0 } else { usize::MAX },
                &|| false,
                |attempt| {
                    if storage {
                        let _ = attempt.charge_allocation(1);
                    } else {
                        let _ = attempt.charge_work(2);
                    }
                    failures[0] = productive(&[0], &edges, attempt).is_err();
                    failures[1] = fold(
                        &[0],
                        &edges,
                        &HashSet::from([0]),
                        &HashMap::new(),
                        &[0],
                        attempt,
                    )
                    .is_err();
                },
            );
            assert_eq!(failures, [true, true]);
            assert_eq!(
                result.unwrap_err().code,
                if storage {
                    "preparation-sdk-reservation-bytes"
                } else {
                    "preparation-sdk-work-units"
                }
            );
        }
        assert_eq!(
            productive(&[0], &edges, &|| false).unwrap().unwrap(),
            HashSet::from([0])
        );
        fold(
            &[0],
            &edges,
            &HashSet::from([0]),
            &HashMap::new(),
            &[0],
            &|| false,
        )
        .unwrap()
        .unwrap();
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn cancellation_during_each_graph_walk_is_not_a_decline_and_retry_is_clean() {
        let nodes: Vec<_> = (0..32).collect();
        let edges: HashMap<_, _> = nodes
            .iter()
            .map(|&n| {
                (
                    n,
                    vec![Edge {
                        to: (n + 1) % 32,
                        advances: true,
                        exclusive: false,
                    }],
                )
            })
            .collect();
        let productive_set: HashSet<_> = nodes.iter().copied().collect();
        let own = HashMap::from([(
            0,
            expansion::Own {
                scratch: 1,
                applications: 1,
            },
        )]);
        for stage in [0, 1] {
            let polls = AtomicUsize::new(0);
            let cancel = || polls.fetch_add(1, Ordering::SeqCst) >= 10;
            if stage == 0 {
                assert!(productive(&nodes, &edges, &cancel).is_err());
            } else {
                assert!(fold(&nodes, &edges, &productive_set, &own, &nodes, &cancel).is_err());
            }
            assert_eq!(
                productive(&nodes, &edges, &|| false).unwrap().unwrap(),
                productive_set
            );
            fold(&nodes, &edges, &productive_set, &own, &nodes, &|| false)
                .unwrap()
                .unwrap();
        }
    }
}
