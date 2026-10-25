//! A static bound on how deeply evaluation nests.
//!
//! The library evaluates by recursion: each schema object it enters is at
//! least one call deeper. Preparation computes, from the bundled definitions
//! and their references, a bound on the schema objects any evaluation path can
//! hold at once, as a function of how deep the value is. A value is then
//! checked against the evaluator's depth limit before evaluation, in constant
//! time, so evaluation itself carries no depth accounting.
//!
//! References that apply to the same instance never form a cycle: preparation
//! unfolds each such cycle so that a schema applying again is a stand-in
//! (`crate::cycles`). Every cycle therefore passes a keyword that applies to a
//! child instance, and a path can pass such keywords at most as many times as
//! the value is deep. With the strongly connected components of the references,
//! a path stays in each component for some descents, entering at most the
//! component's longest same-instance run of objects between them. So the
//! objects a path holds are at most `base + per_level * depth`, where
//! `per_level` is the most any component adds per descent (its longest
//! same-instance run plus its longest descending reference) and `base` is the
//! longest path through the components, each counted once with its run.
use crate::bundle::Definition;

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
    #[cfg(test)]
    fn levels(self, depth: usize) -> u64 {
        self.base
            .saturating_add(self.per_level.saturating_mul(depth as u64))
    }
    fn deepest(self, limit: u64) -> Option<usize> {
        let spare = limit.checked_sub(self.base)?;
        Some(match spare.checked_div(self.per_level) {
            Some(levels) => usize::try_from(levels).unwrap_or(usize::MAX),
            None => usize::MAX,
        })
    }
}
impl Bound {
    /// The bound for a value whose nodes are at most `depth` levels below it.
    #[cfg(test)]
    pub(crate) fn levels(self, depth: usize) -> u64 {
        self.components
            .levels(depth)
            .min(self.descents.levels(depth))
    }
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

/// The heaviest run of references an `unevaluatedProperties` or
/// `unevaluatedItems` keyword's annotation walk follows from its holder:
/// references through `$ref`, `$dynamicRef`, `allOf`, `anyOf`, `oneOf`, `if`,
/// `then`, `else` and `dependentSchemas`, which apply to the same instance,
/// each weighing what its [`Edge::walk`](crate::bundle::Edge::walk) adds, and
/// [`HOLDER_WALK`](crate::bundle::HOLDER_WALK) more when it reaches another
/// holder. The library compiles that walk by recursion, so its weight bounds
/// the stack preparation takes. Returns the weight and the holder whose walk
/// it is, if any holder walks. The definitions' same-instance references are
/// acyclic, which [`bound`] establishes first.
pub(crate) fn longest_walk(definitions: &[Definition], holders: &[bool]) -> Option<(u32, u32)> {
    // The longest walked path from any holder ending at each definition, with
    // the holder it starts from, propagated in topological order of the walked
    // references.
    let walked = |edge: &crate::bundle::Edge| edge.walked && !edge.descends;
    let mut pending = vec![0u32; definitions.len()];
    for definition in definitions {
        for edge in definition.edges.iter().filter(|edge| walked(edge)) {
            pending[edge.to as usize] += 1;
        }
    }
    let mut ending: Vec<Option<(u32, u32)>> = holders
        .iter()
        .enumerate()
        .map(|(holder, &holds)| holds.then_some((0, holder as u32)))
        .collect();
    let mut ready: Vec<u32> = (0..definitions.len() as u32)
        .filter(|&node| pending[node as usize] == 0)
        .collect();
    let mut longest: Option<(u32, u32)> = None;
    while let Some(node) = ready.pop() {
        let at = ending[node as usize];
        if let Some(at) = at
            && longest.is_none_or(|known| at.0 > known.0)
        {
            longest = Some(at);
        }
        for edge in definitions[node as usize]
            .edges
            .iter()
            .filter(|edge| walked(edge))
        {
            let target = edge.to as usize;
            if let Some((length, holder)) = at {
                let reached = if holders[target] {
                    crate::bundle::HOLDER_WALK
                } else {
                    0
                };
                let next = (length + edge.walk + reached, holder);
                if ending[target].is_none_or(|known| next.0 > known.0) {
                    ending[target] = Some(next);
                }
            }
            pending[target] -= 1;
            if pending[target] == 0 {
                ready.push(edge.to);
            }
        }
    }
    longest
}

/// The bound for evaluating `definitions` from `entry`, which evaluation
/// reaches holding `entry_weight` objects, the entry's own included.
pub(crate) fn bound(
    definitions: &[Definition],
    entry: u32,
    entry_weight: u32,
) -> Result<Bound, Cycle> {
    let weight = |edge: &crate::bundle::Edge| u64::from(edge.weight);
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
    follows: impl Fn(&crate::bundle::Edge) -> bool,
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
    use crate::bundle::Edge;
    fn graph(edges: &[(u32, u32, u32, bool)], n: usize) -> Vec<Definition> {
        let mut definitions: Vec<Definition> = (0..n)
            .map(|i| Definition {
                number: i as u32,
                edges: Vec::new(),
            })
            .collect();
        for &(from, to, weight, descends) in edges {
            definitions[from as usize].edges.push(Edge {
                to,
                weight,
                descends,
                walked: !descends,
                walk: crate::bundle::REFERENCE_WALK,
            });
        }
        definitions
    }
    #[test]
    fn a_tree_is_bounded_by_its_longest_path_whatever_the_value() {
        let tree = graph(&[(0, 1, 2, true), (1, 2, 2, false), (0, 3, 1, false)], 4);
        let bound = bound(&tree, 0, 1).unwrap();
        assert_eq!(bound.deepest(5), Some(usize::MAX));
        // A value with no descents reaches only the entry's same-instance run.
        assert_eq!(bound.levels(0), 2);
        assert_eq!(bound.levels(10_000), 5);
    }
    #[test]
    fn a_long_descending_chain_admits_shallow_values() {
        // entry -items-> 1 -items-> 2 ... a chain of 5,000 descents.
        let n = 5_000;
        let chain: Vec<_> = (0..n - 1).map(|i| (i, i + 1, 2, true)).collect();
        let bound = bound(&graph(&chain, n as usize), 0, 1).unwrap();
        assert_eq!(bound.levels(0), 1);
        assert_eq!(bound.levels(3), 7);
        assert_eq!(bound.deepest(1024), Some(511));
    }
    #[test]
    fn recursion_adds_its_cycle_per_level() {
        // entry -> node; node -anyOf-> array branch; branch -items-> wrapper -> node.
        let recursive = graph(
            &[
                (0, 1, 1, false),
                (1, 2, 2, false),
                (1, 3, 2, false),
                (3, 4, 2, true),
                (4, 1, 1, false),
            ],
            5,
        );
        let bound = bound(&recursive, 0, 1).unwrap();
        for limit in [0, 6, 7, 11, 12, 1024] {
            let deepest = bound.deepest(limit);
            assert_eq!(deepest.is_some(), bound.levels(0) <= limit);
            if let Some(deepest) = deepest {
                assert!(bound.levels(deepest) <= limit && bound.levels(deepest + 1) > limit);
            }
        }
        // The cycle node -> branch -> wrapper -> node holds 5 objects per level;
        // a value without descents reaches entry -> node -> branch.
        assert_eq!(bound.levels(1) - bound.levels(0), 5);
        assert_eq!(bound.levels(0), 4);
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
        let bound = bound(&graph(&chain, n as usize), 0, 1).unwrap();
        assert_eq!(bound.levels(0), u64::from(n));
    }
}
