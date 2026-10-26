//! In-place cycles: schemas that could apply again to the same instance.
//!
//! JSON Schema leaves evaluating a cycle that recurses without consuming the
//! instance undefined (JSON Schema Core §9.4.1), and the library would recurse
//! without end. Preparation therefore unfolds each cycle of the bundled
//! references that apply to the same instance, so that a schema applying again
//! is a stand-in instead of a reference back: a pass that reaches it records
//! the stand-in, which names that schema's original location, and goes on as
//! if it held. The pass then gives no verdict unless its value's verdict cannot
//! depend on the stand-in (`crate::bounds`); a value whose evaluation never
//! reaches it keeps its verdict.
//!
//! Each strongly connected component of same-instance references has entries:
//! the bundle's entry, and each of its schemas that a reference from outside
//! it reaches, or one from inside it through a keyword that applies to a child
//! instance. From each entry the component unfolds along every path that does
//! not return to a schema already on it: each schema such a path reaches is a
//! copy of its own, and a same-instance reference to a schema on the path is
//! the stand-in for that schema. So the stand-in is exactly where a schema
//! would apply again to the instance it is already being applied to, whatever
//! order keywords are evaluated in. A component has as many such copies as
//! paths, which grows quickly in a dense component. The unfolding of all
//! components together may enumerate at most 1,024 copies, holding at most
//! the larger of 2 MiB and four times the bytes of the components' own
//! schemas and no more than the program budget leaves; the copies a component
//! enumerates count whether or not it unfolds. A component whose unfolding would pass that stands in at
//! each of its same-instance references instead, which is coarser and as
//! sound.
use crate::bundle::{self, Bundle, Definition};
use openbindings::{NoVerdict, WorkControl};
use std::collections::HashMap;

/// The copies the unfolding of all components may enumerate.
const COPIES: usize = 1024;
/// How many times the bytes of the components' own schemas their copies may
/// hold.
const COPY_BYTES: usize = 4;
/// The bytes the copies may hold whatever the components hold.
const COPY_BYTES_FLOOR: usize = 2 * 1024 * 1024;
/// The unfolding steps between checks for cancellation.
const CHECK_EVERY: usize = 1024;

/// How a unit of the unfolding reaches a schema of its component.
#[derive(Clone, Copy)]
enum Reach {
    /// The stand-in for that schema, which applies again.
    Again(u32),
    /// The copy for that schema along this path, by unit.
    Unit(usize),
}
/// The entry's own definition, or one copy, of an unfolding.
struct Unit {
    node: u32,
    // What each same-instance reference to a schema of the component reaches,
    // by that schema.
    reaches: Vec<(u32, Reach)>,
    // The copy's bundled index once added; `None` for the entry's own
    // definition.
    index: Option<u32>,
}

/// Unfold every in-place cycle of `bundle`, within `bytes` of copies as
/// [`bundle::definition_charge`] counts them. Returns, by each cycle
/// stand-in's index, the projected number of the schema it stands for, and
/// the bytes the copies were charged; an error once `control` cancels.
pub(crate) fn unfold(
    bundle: &mut Bundle,
    bytes: usize,
    control: &dyn WorkControl,
) -> Result<(Vec<u32>, usize), NoVerdict> {
    let components = crate::depth::components(&bundle.definitions, |edge| !edge.descends);
    let count = bundle.definitions.len();
    // Components holding a same-instance cycle: more than one schema, or one
    // that references itself.
    let mut cyclic = vec![false; components.count];
    for (from, definition) in bundle.definitions.iter().enumerate() {
        let component = components.of[from];
        for edge in &definition.edges {
            if !edge.descends && components.of[edge.to as usize] == component {
                let size = components.starts[component as usize + 1]
                    - components.starts[component as usize];
                if size > 1 || edge.to as usize == from {
                    cyclic[component as usize] = true;
                }
            }
        }
    }
    if !cyclic.iter().any(|&cyclic| cyclic) {
        return Ok((Vec::new(), 0));
    }
    let mut entry = vec![false; count];
    entry[bundle.entry as usize] = true;
    for (from, definition) in bundle.definitions.iter().enumerate() {
        for edge in &definition.edges {
            if edge.descends || components.of[edge.to as usize] != components.of[from] {
                entry[edge.to as usize] = true;
            }
        }
    }
    let mut unfolding = Unfolding {
        bundle,
        of: &components.of,
        entry: &entry,
        again: HashMap::new(),
        numbers: Vec::new(),
        added: Vec::new(),
        rewritten: Vec::new(),
        budget: COPIES,
        copies: 0,
        bytes: 0,
        charged: 0,
        charges: HashMap::new(),
        control,
        steps: 0,
    };
    // The copies may hold a multiple of the cyclic components' own bytes, or
    // a fixed allowance for small components, within what the program budget
    // leaves.
    let mut own = 0usize;
    for component in (0..components.count).filter(|&component| cyclic[component]) {
        for &member in
            &components.members[components.starts[component]..components.starts[component + 1]]
        {
            own = own.saturating_add(unfolding.charge(member));
        }
    }
    unfolding.bytes = own
        .saturating_mul(COPY_BYTES)
        .max(COPY_BYTES_FLOOR)
        .min(bytes);
    for component in (0..components.count).filter(|&component| cyclic[component]) {
        let members =
            &components.members[components.starts[component]..components.starts[component + 1]];
        let mut entries: Vec<u32> = members
            .iter()
            .copied()
            .filter(|&member| entry[member as usize])
            .collect();
        entries.sort_unstable();
        unfolding.component(component as u32, members, &entries)?;
    }
    Ok(unfolding.finish())
}

struct Unfolding<'b> {
    bundle: &'b mut Bundle,
    of: &'b [u32],
    // Whether each definition is an entry of its component.
    entry: &'b [bool],
    // Each stand-in's bundled index, by the schema it stands for.
    again: HashMap<u32, u32>,
    // Each stand-in's projected number, by its index.
    numbers: Vec<u32>,
    // The definitions added, as their bundled index.
    added: Vec<u32>,
    // The definitions whose references were rewritten.
    rewritten: Vec<u32>,
    // The copies the unfolding may still add, and those it added.
    budget: usize,
    copies: usize,
    // The bytes of copies the unfolding may still add, and those it added,
    // with each definition's charge once computed.
    bytes: usize,
    charged: usize,
    charges: HashMap<u32, usize>,
    control: &'b dyn WorkControl,
    // The enumeration steps taken since the last check for cancellation.
    steps: usize,
}
impl Unfolding<'_> {
    // The stand-in for `node`, added once.
    fn again(&mut self, node: u32) -> u32 {
        if let Some(&index) = self.again.get(&node) {
            return index;
        }
        let at = self.numbers.len() as u32;
        let number = self.bundle.definitions[node as usize].number;
        let name = format!("{}-r", self.bundle.names[node as usize]);
        let index = self.add(
            name,
            number,
            serde_json::json!({ crate::operands::CYCLE: at }),
        );
        self.numbers.push(number);
        self.again.insert(node, index);
        index
    }
    fn add(&mut self, name: String, number: u32, schema: serde_json::Value) -> u32 {
        let index = self.bundle.names.len() as u32;
        self.bundle.names.push(name);
        self.bundle.schemas.push(schema);
        self.bundle.definitions.push(Definition {
            number,
            edges: Vec::new(),
        });
        self.added.push(index);
        index
    }
    // The schemas a same-instance reference from `node` reaches within
    // `component`, each once, in reference order.
    fn within(&self, node: u32, component: u32) -> Vec<u32> {
        let mut targets = Vec::new();
        for edge in &self.bundle.definitions[node as usize].edges {
            if !edge.descends
                && self.of[edge.to as usize] == component
                && !targets.contains(&edge.to)
            {
                targets.push(edge.to);
            }
        }
        targets
    }
    fn component(
        &mut self,
        component: u32,
        members: &[u32],
        entries: &[u32],
    ) -> Result<(), NoVerdict> {
        self.control.check()?;
        match self.units(component, entries)? {
            Some(mut units) => {
                // Every copy is added before any reference is rewritten, so
                // each copies its schema as projected.
                for unit in &mut units {
                    if unit.index.is_some() {
                        let node = unit.node as usize;
                        let number = self.bundle.definitions[node].number;
                        let name = format!("{}-c{}", self.bundle.names[node], self.copies);
                        self.copies += 1;
                        let schema = self.bundle.schemas[node].clone();
                        unit.index = Some(self.add(name, number, schema));
                    }
                }
                for at in 0..units.len() {
                    let mut targets = Vec::with_capacity(units[at].reaches.len());
                    for &(node, reach) in &units[at].reaches {
                        let index = match reach {
                            Reach::Again(again) => self.again(again),
                            Reach::Unit(unit) => units[unit].index.unwrap_or(units[unit].node),
                        };
                        targets.push((node, index));
                    }
                    let index = units[at].index.unwrap_or(units[at].node);
                    self.redirect(index, component, &targets);
                }
                // A schema of the component that is no entry is reached only
                // through its copies.
                for &member in members {
                    if !self.entry[member as usize] {
                        self.dead(member);
                    }
                }
            }
            None => {
                // Too many paths: every same-instance reference within the
                // component is a stand-in for its target.
                for &member in members {
                    if !self.entry[member as usize] {
                        self.dead(member);
                        continue;
                    }
                    let targets: Vec<(u32, u32)> = self
                        .within(member, component)
                        .into_iter()
                        .map(|node| (node, self.again(node)))
                        .collect();
                    self.redirect(member, component, &targets);
                }
            }
        }
        Ok(())
    }
    // The units of the component's unfolding from each entry, depth first;
    // `None` when they would pass the budget, which the copies enumerated
    // spend either way. Each copy unit's `index` holds `Some(0)` until it is
    // added.
    fn units(&mut self, component: u32, entries: &[u32]) -> Result<Option<Vec<Unit>>, NoVerdict> {
        let mut units: Vec<Unit> = Vec::new();
        let mut on_path = std::collections::HashSet::new();
        let (mut copies, mut charged) = (0usize, 0usize);
        for &entry in entries {
            // (unit, its targets within the component, the next to visit)
            let root = units.len();
            units.push(Unit {
                node: entry,
                reaches: Vec::new(),
                index: None,
            });
            let mut stack = vec![(root, self.within(entry, component), 0usize)];
            on_path.insert(entry);
            while let Some((unit, targets, next)) = stack.last_mut() {
                self.steps += 1;
                if self.steps == CHECK_EVERY {
                    self.steps = 0;
                    self.control.check()?;
                }
                let unit = *unit;
                let Some(&target) = targets.get(*next) else {
                    on_path.remove(&units[unit].node);
                    stack.pop();
                    continue;
                };
                *next += 1;
                if on_path.contains(&target) {
                    units[unit].reaches.push((target, Reach::Again(target)));
                    continue;
                }
                copies += 1;
                charged = charged.saturating_add(self.charge(target));
                if copies > self.budget || charged > self.bytes {
                    self.budget = 0;
                    self.bytes = 0;
                    return Ok(None);
                }
                let child = units.len();
                units.push(Unit {
                    node: target,
                    reaches: Vec::new(),
                    index: Some(0),
                });
                units[unit].reaches.push((target, Reach::Unit(child)));
                on_path.insert(target);
                let targets = self.within(target, component);
                stack.push((child, targets, 0));
            }
        }
        self.budget -= copies;
        self.bytes -= charged;
        self.charged = self.charged.saturating_add(charged);
        Ok(Some(units))
    }
    // What a copy of definition `node` is charged, as specialization charges
    // its copies.
    fn charge(&mut self, node: u32) -> usize {
        *self
            .charges
            .entry(node)
            .or_insert_with(|| bundle::definition_charge(&self.bundle.schemas[node as usize]))
    }
    // Rewrite each same-instance reference from definition `index` to a
    // schema of `component` as `targets` maps it.
    fn redirect(&mut self, index: u32, component: u32, targets: &[(u32, u32)]) {
        let names = &self.bundle.names;
        let lookup: HashMap<&str, u32> = targets
            .iter()
            .map(|&(node, _)| (names[node as usize].as_str(), node))
            .collect();
        let rename: Vec<(u32, String)> = targets
            .iter()
            .map(|&(node, to)| (node, format!("#/$defs/{}", names[to as usize])))
            .collect();
        let of = self.of;
        let mut schema = std::mem::take(&mut self.bundle.schemas[index as usize]);
        bundle::references(&mut schema, false, &mut |reference, descends| {
            if descends {
                return;
            }
            let Some(&node) = reference
                .strip_prefix("#/$defs/")
                .and_then(|name| lookup.get(name))
            else {
                return;
            };
            if of[node as usize] != component {
                return;
            }
            if let Some((_, to)) = rename.iter().find(|(target, _)| *target == node) {
                reference.clone_from(to);
            }
        });
        self.bundle.schemas[index as usize] = schema;
        self.rewritten.push(index);
    }
    // A definition no reference reaches any more.
    fn dead(&mut self, index: u32) {
        self.bundle.schemas[index as usize] = serde_json::Value::Bool(true);
        self.bundle.definitions[index as usize].edges.clear();
    }
    // Recount the edges of every definition the unfolding wrote.
    fn finish(self) -> (Vec<u32>, usize) {
        let Unfolding {
            bundle,
            numbers,
            added,
            rewritten,
            charged,
            ..
        } = self;
        let indices: HashMap<String, u32> = bundle
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as u32))
            .collect();
        for index in rewritten.into_iter().chain(added) {
            let mut resolve = |reference: &str| {
                reference
                    .strip_prefix("#/$defs/")
                    .and_then(|name| indices.get(name))
                    .copied()
            };
            bundle.definitions[index as usize].edges =
                bundle::edges(&mut bundle.schemas[index as usize], &mut resolve);
        }
        (numbers, charged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    struct Control(bool);
    impl WorkControl for Control {
        fn is_cancelled(&self) -> bool {
            self.0
        }
    }

    // A bundle of `components` disjoint components of `size` schemas, each
    // naming every schema of its component in an `anyOf`, entered from a root
    // that names the first schema of each.
    fn dense(components: usize, size: usize) -> Bundle {
        let mut names = vec!["n0".to_owned()];
        let mut schemas = vec![json!({"allOf": (0..components)
            .map(|c| json!({"$ref": format!("#/$defs/n{}", 1 + c * size)}))
            .collect::<Vec<_>>()})];
        for c in 0..components {
            for i in 0..size {
                names.push(format!("n{}", 1 + c * size + i));
                let branches: Vec<Value> = (0..size)
                    .map(|j| json!({"$ref": format!("#/$defs/n{}", 1 + c * size + j)}))
                    .collect();
                schemas.push(json!({"anyOf": branches}));
            }
        }
        let index: HashMap<String, u32> = names
            .iter()
            .enumerate()
            .map(|(at, name)| (name.clone(), at as u32))
            .collect();
        let definitions = schemas
            .iter_mut()
            .enumerate()
            .map(|(at, schema)| Definition {
                number: at as u32,
                edges: bundle::edges(schema, &mut |reference| {
                    reference
                        .strip_prefix("#/$defs/")
                        .and_then(|name| index.get(name))
                        .copied()
                }),
            })
            .collect();
        Bundle::assembled(names, schemas, definitions, 0, false)
    }

    // Dense components that pass the budget spend it: once one has, every
    // further component stands in at its references without enumerating its
    // paths again, so the unfolding adds at most the budget's copies.
    #[test]
    fn components_past_the_budget_spend_it_once() {
        let mut bundle = dense(200, 4);
        let before = bundle.definitions.len();
        let (stand_ins, charged) = unfold(&mut bundle, usize::MAX, &Control(false)).unwrap();
        let added = bundle.definitions.len() - before;
        assert!(
            added - stand_ins.len() <= COPIES,
            "{added} definitions added"
        );
        assert!(stand_ins.len() <= 200 * 4);
        assert!(charged > 0);
    }

    // The unfolding checks for cancellation before each component and while
    // it enumerates a component's paths.
    #[test]
    fn cancellation_interrupts_the_unfolding() {
        let mut bundle = dense(1, 8);
        assert!(unfold(&mut bundle, usize::MAX, &Control(true)).is_err());
        let mut bundle = dense(1, 8);
        assert!(unfold(&mut bundle, usize::MAX, &Control(false)).is_ok());
        // Once the first check passes, one component's enumeration checks
        // again after `CHECK_EVERY` steps.
        struct After(std::sync::atomic::AtomicUsize);
        impl WorkControl for After {
            fn is_cancelled(&self) -> bool {
                self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 0
            }
        }
        let mut bundle = dense(1, 24);
        let control = After(std::sync::atomic::AtomicUsize::new(0));
        assert!(unfold(&mut bundle, usize::MAX, &control).is_err());
    }
}
