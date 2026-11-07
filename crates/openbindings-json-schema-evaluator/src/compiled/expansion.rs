//! Memoized finite application expansion, before entering the compiler.
use super::*;
use openbindings_internal_schema::Control;
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(super) struct Own {
    pub scratch: usize,
    pub applications: usize,
}

pub(super) struct Weight {
    pub scratch: usize,
    pub applications: usize,
    pub coefficient: usize,
}

fn weight(
    own: Own,
    children: &[recursion::Edge],
    complete: &HashMap<usize, Weight>,
    control: &dyn Control,
) -> Result<Weight, PreparationFailure> {
    let mut weight = Weight {
        scratch: own.scratch,
        applications: 1,
        coefficient: own.applications,
    };
    let mut exclusive = 0;
    // Keep the original structural/scratch sums for preparation admission.
    // The weighted evaluation coefficient can tighten sibling properties and
    // prefixItems: their selected subtrees are disjoint, so max(child weight)
    // times their total nodes covers all of them. Object and array selectors
    // are mutually exclusive too. Only this local group takes max; references,
    // separate applicator sites and overlapping selectors still sum. Induction
    // over the DAG then bounds total work by coefficient * root value nodes.
    for child in children {
        control.check().map_err(PreparationFailure::from)?;
        let next = &complete[&child.to];
        if child.exclusive {
            exclusive = exclusive.max(next.coefficient);
        } else {
            weight.coefficient = weight.coefficient.saturating_add(next.coefficient);
        }
        weight.scratch = weight.scratch.saturating_add(next.scratch);
        weight.applications = weight
            .applications
            .saturating_add(next.applications)
            .min(MAXIMUM_NODES + 1);
    }
    weight.coefficient = weight.coefficient.saturating_add(exclusive);
    Ok(weight)
}

// None leaves recursive admission entirely to its existing proof. Complete the
// cycle check even after a saturated count so recursion's route stays unchanged.
// Every node is folded once and each edge occurrence is visited a constant
// number of times. Keys are prepared numeric identities, never input text.
pub(super) fn fold(
    nodes: &[usize],
    edges: &HashMap<usize, Vec<recursion::Edge>>,
    own: &HashMap<usize, Own>,
    control: &dyn Control,
) -> Result<Option<Weight>, PreparationFailure> {
    control.check().map_err(PreparationFailure::from)?;
    let mut complete = HashMap::<usize, Weight>::new();
    let mut active = HashSet::new();
    let mut maximum = Weight {
        scratch: 0,
        applications: 0,
        coefficient: 0,
    };
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
                let weight = weight(
                    *own.get(&node).unwrap_or(&Own {
                        scratch: 0,
                        applications: 1,
                    }),
                    children,
                    &complete,
                    control,
                )?;
                maximum.coefficient = maximum.coefficient.max(weight.coefficient);
                maximum.scratch = maximum.scratch.max(weight.scratch);
                maximum.applications = maximum.applications.max(weight.applications);
                complete.insert(node, weight);
            } else {
                if !active.insert(node) {
                    return Ok(None);
                }
                pending.push((node, true));
                for child in children {
                    control.check().map_err(PreparationFailure::from)?;
                    if !complete.contains_key(&child.to) {
                        pending.push((child.to, false));
                    }
                }
            }
        }
    }
    Ok(Some(maximum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings_internal_schema::preparation::Attempt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn parallel_and_advancing_paths_saturate_in_linear_work() {
        let nodes: Vec<_> = (0..1024).collect();
        let edges: HashMap<_, _> = (0..1023)
            .map(|n| {
                (
                    n,
                    vec![
                        recursion::Edge {
                            to: n + 1,
                            advances: n % 2 == 0,
                            exclusive: false
                        };
                        2
                    ],
                )
            })
            .collect();
        let polls = AtomicUsize::new(0);
        let control = || {
            polls.fetch_add(1, Ordering::Relaxed);
            false
        };
        let weight = fold(
            &nodes,
            &edges,
            &HashMap::from([(
                1023,
                Own {
                    scratch: 1,
                    applications: 1,
                },
            )]),
            &control,
        )
        .unwrap()
        .unwrap();
        assert_eq!(weight.applications, MAXIMUM_NODES + 1);
        assert_eq!(weight.scratch, usize::MAX);
        assert!(polls.load(Ordering::Relaxed) < 8 * nodes.len());
    }

    #[test]
    fn cycles_still_belong_to_recursive_admission_after_saturation() {
        let nodes: Vec<_> = (0..64).collect();
        let mut edges: HashMap<_, _> = (0..62)
            .map(|n| {
                (
                    n,
                    vec![
                        recursion::Edge {
                            to: n + 1,
                            advances: false,
                            exclusive: false
                        };
                        2
                    ],
                )
            })
            .collect();
        edges.insert(
            63,
            vec![recursion::Edge {
                to: 63,
                advances: true,
                exclusive: false,
            }],
        );
        assert!(
            fold(&nodes, &edges, &HashMap::new(), &|| false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn cancellation_and_inherited_work_failure_do_not_become_declines() {
        let nodes = [0, 1];
        let edges = HashMap::from([(
            0,
            vec![recursion::Edge {
                to: 1,
                advances: false,
                exclusive: false,
            }],
        )]);
        assert!(fold(&nodes, &edges, &HashMap::new(), &|| true).is_err());
        let result = Attempt::run(0, usize::MAX, &|| false, |attempt| {
            let _ = attempt.charge_work(1);
            assert!(fold(&nodes, &edges, &HashMap::new(), attempt).is_err());
        });
        assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
        assert_eq!(
            fold(&nodes, &edges, &HashMap::new(), &|| false)
                .unwrap()
                .unwrap()
                .applications,
            2
        );
    }
}
