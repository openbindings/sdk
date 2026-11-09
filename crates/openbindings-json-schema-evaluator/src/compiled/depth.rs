//! A static bound on how deeply evaluation nests.
//!
//! The library evaluates by recursion: each schema object it enters is at
//! least one call deeper. Preparation computes, from the bundled definitions
//! and their references, a bound on the schema objects any evaluation path can
//! hold at once, as a function of how deep the value is. A value is then
//! checked against the evaluator's depth limit before evaluation, in constant
//! time, so evaluation itself carries no depth accounting.
//!
//! References that apply to the same instance are acyclic in the admitted
//! value lane; other interpretations retain Plan. Every qualified cycle passes
//! a keyword that applies to a
//! child instance, and a path can pass such keywords at most as many times as
//! the value is deep. With the strongly connected components of the references,
//! a path stays in each component for some descents, entering at most the
//! component's longest same-instance run of objects between them. So the
//! objects a path holds are at most `base + per_level * depth`, where
//! `per_level` is the most any component adds per descent (its longest
//! same-instance run plus its longest descending reference) and `base` is the
//! longest path through the components, each counted once with its run.
use crate::compiled::bundle::Definition;

/// The objects an evaluation of a value `depth` levels deep can hold at once:
/// the lesser of two linear bounds, one from the reference graph's components
/// and one from its descents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bound {
    components: Linear,
    descents: Linear,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Linear {
    base: u64,
    per_level: u64,
}
impl Linear {
    fn deepest(self, limit: u64) -> Option<usize> {
        let spare = limit.checked_sub(self.base)?;
        Some(match spare.checked_div(self.per_level) {
            Some(levels) => usize::try_from(levels).unwrap_or(usize::MAX),
            None => usize::MAX,
        })
    }
}
impl Bound {
    /// The deepest value whose bound is at most `limit`, if any is;
    /// `usize::MAX` when every value's is.
    pub(crate) fn deepest(self, limit: u64) -> Option<usize> {
        match (self.components.deepest(limit), self.descents.deepest(limit)) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }
}

/// References that apply to the same instance form a cycle.
#[derive(Debug)]
pub(crate) struct Cycle;

/// The bound for evaluating `definitions` from `entry`, which evaluation
/// reaches holding `entry_weight` objects, the entry's own included.
pub(crate) fn bound(
    definitions: &[Definition],
    entry: u32,
    entry_weight: u32,
) -> Result<Bound, Cycle> {
    let weight = |edge: &crate::compiled::bundle::Edge| u64::from(edge.weight);
    let components = components(definitions, |_| true);
    let count = components.count;
    // Each component's longest same-instance run and longest descending
    // reference within it.
    let mut run = vec![0u64; count];
    let mut descent = vec![0u64; count];
    let mut recursive = vec![false; count];
    // Kahn's algorithm per component over its same-instance references.
    let mut pending = vec![0u32; definitions.len()];
    let mut longest = vec![0u64; definitions.len()];
    for (from, definition) in definitions.iter().enumerate() {
        let component = components.of[from];
        for edge in &definition.edges {
            if components.of[edge.to as usize] != component {
                continue;
            }
            recursive[component as usize] = true;
            if edge.descends {
                let slot = &mut descent[component as usize];
                *slot = (*slot).max(weight(edge));
            } else {
                pending[edge.to as usize] += 1;
            }
        }
    }
    let mut ready: Vec<u32> = (0..definitions.len() as u32)
        .filter(|&node| pending[node as usize] == 0)
        .collect();
    let mut visited = 0usize;
    while let Some(node) = ready.pop() {
        visited += 1;
        let component = components.of[node as usize];
        let at = longest[node as usize];
        run[component as usize] = run[component as usize].max(at);
        for edge in &definitions[node as usize].edges {
            if edge.descends || components.of[edge.to as usize] != component {
                continue;
            }
            let target = edge.to as usize;
            longest[target] = longest[target].max(at + weight(edge));
            pending[target] -= 1;
            if pending[target] == 0 {
                ready.push(edge.to);
            }
        }
    }
    if visited != definitions.len() {
        return Err(Cycle);
    }
    let per_level = (0..count)
        .filter(|&component| recursive[component])
        .map(|component| run[component] + descent[component])
        .max()
        .unwrap_or(0);
    // Components complete sinks first, so every reference out of a component
    // reaches one whose path is already known.
    let mut path = vec![0u64; count];
    for component in 0..count {
        let mut best = 0u64;
        for &node in
            &components.members[components.starts[component]..components.starts[component + 1]]
        {
            for edge in &definitions[node as usize].edges {
                let target = components.of[edge.to as usize] as usize;
                if target != component {
                    best = best.max(weight(edge) + path[target]);
                }
            }
        }
        path[component] = run[component] + best;
    }
    // A path alternates runs of same-instance references with descents, and a
    // value `depth` levels deep allows at most `depth` descents; so it holds at
    // most the entry's run, then for each descent the heaviest descent and the
    // longest same-instance run anywhere.
    // The same-instance references form a DAG; in its topological order,
    // `ending` is the heaviest same-instance run ending at each definition and
    // `from_entry` the heaviest that starts at the entry.
    let mut pending = vec![0u32; definitions.len()];
    let mut heaviest_descent = 0u64;
    for definition in definitions {
        for edge in &definition.edges {
            if edge.descends {
                heaviest_descent = heaviest_descent.max(weight(edge));
            } else {
                pending[edge.to as usize] += 1;
            }
        }
    }
    let mut ending = vec![0u64; definitions.len()];
    let mut from_entry: Vec<Option<u64>> = vec![None; definitions.len()];
    from_entry[entry as usize] = Some(0);
    let mut ready: Vec<u32> = (0..definitions.len() as u32)
        .filter(|&node| pending[node as usize] == 0)
        .collect();
    let (mut longest_run, mut entry_run) = (0u64, 0u64);
    while let Some(node) = ready.pop() {
        let (at, reached) = (ending[node as usize], from_entry[node as usize]);
        longest_run = longest_run.max(at);
        if let Some(reached) = reached {
            entry_run = entry_run.max(reached);
        }
        for edge in &definitions[node as usize].edges {
            if edge.descends {
                continue;
            }
            let target = edge.to as usize;
            ending[target] = ending[target].max(at + weight(edge));
            if let Some(reached) = reached {
                let next = reached + weight(edge);
                from_entry[target] = Some(from_entry[target].map_or(next, |known| known.max(next)));
            }
            pending[target] -= 1;
            if pending[target] == 0 {
                ready.push(edge.to);
            }
        }
    }
    let entered = u64::from(entry_weight);
    Ok(Bound {
        components: Linear {
            base: entered + path[components.of[entry as usize] as usize],
            per_level,
        },
        descents: Linear {
            base: entered + entry_run,
            per_level: if heaviest_descent == 0 {
                0
            } else {
                heaviest_descent + longest_run
            },
        },
    })
}

/// Strongly connected components of the references `follows` admits,
/// numbered in the order Tarjan's algorithm completes them: every such
/// reference out of a component reaches an earlier one.
pub(crate) struct Components {
    pub(crate) count: usize,
    pub(crate) of: Vec<u32>,
    /// Definitions by component, `starts[c]..starts[c + 1]`.
    pub(crate) members: Vec<u32>,
    pub(crate) starts: Vec<usize>,
}
pub(crate) fn components(
    definitions: &[Definition],
    follows: impl Fn(&crate::compiled::bundle::Edge) -> bool,
) -> Components {
    const NONE: u32 = u32::MAX;
    let n = definitions.len();
    let mut index = vec![NONE; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut of = vec![NONE; n];
    let mut members = Vec::with_capacity(n);
    let mut starts = vec![0];
    let mut next = 0u32;
    // (node, next edge to visit)
    let mut calls: Vec<(u32, usize)> = Vec::new();
    for root in 0..n as u32 {
        if index[root as usize] != NONE {
            continue;
        }
        calls.push((root, 0));
        index[root as usize] = next;
        low[root as usize] = next;
        next += 1;
        stack.push(root);
        on_stack[root as usize] = true;
        while let Some(&mut (node, ref mut position)) = calls.last_mut() {
            let edges = &definitions[node as usize].edges;
            if let Some(edge) = edges.get(*position) {
                *position += 1;
                if !follows(edge) {
                    continue;
                }
                let target = edge.to;
                if index[target as usize] == NONE {
                    index[target as usize] = next;
                    low[target as usize] = next;
                    next += 1;
                    stack.push(target);
                    on_stack[target as usize] = true;
                    calls.push((target, 0));
                } else if on_stack[target as usize] {
                    low[node as usize] = low[node as usize].min(index[target as usize]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent as usize] = low[parent as usize].min(low[node as usize]);
            }
            if low[node as usize] == index[node as usize] {
                let component = (starts.len() - 1) as u32;
                loop {
                    let member = stack.pop().expect("tarjan stack");
                    on_stack[member as usize] = false;
                    of[member as usize] = component;
                    members.push(member);
                    if member == node {
                        break;
                    }
                }
                starts.push(members.len());
            }
        }
    }
    Components {
        count: starts.len() - 1,
        of,
        members,
        starts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiled::bundle::Edge;
    fn graph(edges: &[(u32, u32, u32, bool)], n: usize) -> Vec<Definition> {
        let mut definitions: Vec<Definition> =
            (0..n).map(|_| Definition { edges: Vec::new() }).collect();
        for &(from, to, weight, descends) in edges {
            definitions[from as usize].edges.push(Edge {
                to,
                weight,
                descends,
            });
        }
        definitions
    }
    #[test]
    fn a_long_descending_chain_admits_shallow_values() {
        // entry -items-> 1 -items-> 2 ... a chain of 5,000 descents.
        let n = 5_000;
        let chain: Vec<_> = (0..n - 1).map(|i| (i, i + 1, 2, true)).collect();
        bound(&graph(&chain, n as usize), 0, 1).unwrap();
    }
    #[test]
    fn same_instance_cycles_are_refused() {
        assert!(bound(&graph(&[(0, 1, 1, false), (1, 0, 1, false)], 2), 0, 1).is_err());
        assert!(bound(&graph(&[(0, 0, 1, false)], 1), 0, 1).is_err());
    }
    #[test]
    fn long_chains_need_no_recursion() {
        let n = 200_000;
        let chain: Vec<_> = (0..n - 1).map(|i| (i, i + 1, 1, false)).collect();
        bound(&graph(&chain, n as usize), 0, 1).unwrap();
    }
}
