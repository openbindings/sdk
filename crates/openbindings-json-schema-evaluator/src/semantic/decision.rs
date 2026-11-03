//! Exact Boolean reasoning over an explicitly constructed semantic model.
//! Constant bounds are universal proofs. Dependence additionally needs coherent
//! effect identities and an exact model; a wide interval is never a witness.
use openbindings::{NoVerdict, NoVerdictReason, WorkControl};
use std::collections::{HashMap, HashSet};

pub(super) type Formula = u32;
pub(super) const FALSE: Formula = 0;
pub(super) const TRUE: Formula = 1;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Class {
    /// An effect in a resource external content could address.
    Undefined,
    /// An original effect in the OBI's unnameable document resource.
    PrivateUndefined,
    External,
    Inability,
}
#[derive(Clone, Copy)]
struct Node {
    variable: u32,
    low: Formula,
    high: Formula,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Op {
    And,
    Or,
    Xor,
}

pub(super) struct Graph<'a> {
    nodes: Vec<Node>,
    classes: Vec<Class>,
    unique: HashMap<(u32, Formula, Formula), Formula>,
    applied: HashMap<(Op, Formula, Formula), Formula>,
    bytes: usize,
    maximum_bytes: usize,
    steps: usize,
    maximum_steps: Option<usize>,
    maximum_variables: usize,
    control: &'a dyn WorkControl,
}
pub(super) fn limit(resource: &'static str) -> NoVerdict {
    NoVerdict::new(
        NoVerdictReason::LimitExceeded,
        resource,
        "semantic analysis exceeded its configured resource allowance",
    )
}
impl<'a> Graph<'a> {
    pub(super) fn new(
        bytes: usize,
        steps: Option<usize>,
        variables: usize,
        control: &'a dyn WorkControl,
    ) -> Self {
        Self {
            nodes: Vec::new(),
            classes: Vec::new(),
            unique: HashMap::new(),
            applied: HashMap::new(),
            bytes: 0,
            maximum_bytes: bytes,
            steps: 0,
            maximum_steps: steps,
            maximum_variables: variables,
            control,
        }
    }
    pub(super) fn tick(&mut self) -> Result<(), NoVerdict> {
        self.control.check().map_err(NoVerdict::from)?;
        if self
            .maximum_steps
            .is_some_and(|maximum| self.steps >= maximum)
        {
            return Err(limit("evaluation-work-limit"));
        }
        self.steps = self.steps.saturating_add(1);
        Ok(())
    }
    pub(super) fn charge(&mut self, bytes: usize) -> Result<(), NoVerdict> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > self.maximum_bytes {
            return Err(limit("semantic-analysis-memory-limit"));
        }
        Ok(())
    }
    fn node(&self, formula: Formula) -> Node {
        self.nodes[(formula - 2) as usize]
    }
    fn make(&mut self, variable: u32, low: Formula, high: Formula) -> Result<Formula, NoVerdict> {
        if low == high {
            return Ok(low);
        }
        if let Some(&known) = self.unique.get(&(variable, low, high)) {
            return Ok(known);
        }
        self.charge(256)?;
        let id = u32::try_from(self.nodes.len())
            .ok()
            .and_then(|n| n.checked_add(2))
            .ok_or_else(|| limit("semantic-analysis-memory-limit"))?;
        self.nodes.push(Node {
            variable,
            low,
            high,
        });
        self.unique.insert((variable, low, high), id);
        Ok(id)
    }
    pub(super) fn variable(&mut self, class: Class) -> Result<(u32, Formula), NoVerdict> {
        self.tick()?;
        if self.classes.len() >= self.maximum_variables {
            return Err(limit("semantic-analysis-depth-limit"));
        }
        let variable = self.classes.len() as u32;
        self.classes.push(class);
        Ok((variable, self.make(variable, FALSE, TRUE)?))
    }
    pub(super) fn and(&mut self, a: Formula, b: Formula) -> Result<Formula, NoVerdict> {
        self.apply(Op::And, a, b)
    }
    pub(super) fn or(&mut self, a: Formula, b: Formula) -> Result<Formula, NoVerdict> {
        self.apply(Op::Or, a, b)
    }
    pub(super) fn not(&mut self, a: Formula) -> Result<Formula, NoVerdict> {
        self.apply(Op::Xor, a, TRUE)
    }
    pub(super) fn implies(&mut self, a: Formula, b: Formula) -> Result<Formula, NoVerdict> {
        let not = self.not(a)?;
        self.or(not, b)
    }
    pub(super) fn choose(
        &mut self,
        condition: Formula,
        yes: Formula,
        no: Formula,
    ) -> Result<Formula, NoVerdict> {
        let yes = self.and(condition, yes)?;
        let condition = self.not(condition)?;
        let no = self.and(condition, no)?;
        self.or(yes, no)
    }
    fn apply(&mut self, op: Op, mut a: Formula, mut b: Formula) -> Result<Formula, NoVerdict> {
        self.tick()?;
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        match op {
            Op::And if a == FALSE => return Ok(FALSE),
            Op::And if a == TRUE || a == b => return Ok(b),
            Op::Or if a == TRUE || b == TRUE => return Ok(TRUE),
            Op::Or if a == FALSE || a == b => return Ok(b),
            Op::Xor if a == b => return Ok(FALSE),
            Op::Xor if a == FALSE => return Ok(b),
            _ => {}
        }
        if let Some(&known) = self.applied.get(&(op, a, b)) {
            return Ok(known);
        }
        let variable = [a, b]
            .into_iter()
            .filter(|&n| n >= 2)
            .map(|n| self.node(n).variable)
            .min()
            .expect("nonterminal operand");
        let split = |id| {
            if id >= 2 && self.node(id).variable == variable {
                let node = self.node(id);
                (node.low, node.high)
            } else {
                (id, id)
            }
        };
        let (al, ah) = split(a);
        let (bl, bh) = split(b);
        let low = self.apply(op, al, bl)?;
        let high = self.apply(op, ah, bh)?;
        let result = self.make(variable, low, high)?;
        self.charge(128)?;
        self.applied.insert((op, a, b), result);
        Ok(result)
    }
    pub(super) fn exists_undefined(&mut self, root: Formula) -> Result<Formula, NoVerdict> {
        self.quantify(root, Class::Undefined, true)
    }
    #[cfg(test)]
    fn forall_unknown(&mut self, root: Formula) -> Result<Formula, NoVerdict> {
        let root = self.quantify(root, Class::Inability, false)?;
        self.quantify(root, Class::External, false)
    }
    /// A sufficient dependence witness: ∃addressable D ∀external ∃private D
    /// ∀inability. External content may refer to addressable original effects;
    /// an opaque local summary may also depend on private original effects.
    pub(super) fn witness(&mut self, root: Formula) -> Result<Formula, NoVerdict> {
        let root = self.quantify(root, Class::Inability, false)?;
        let root = self.quantify(root, Class::PrivateUndefined, true)?;
        let root = self.quantify(root, Class::External, false)?;
        self.exists_undefined(root)
    }
    fn quantify(
        &mut self,
        root: Formula,
        class: Class,
        existential: bool,
    ) -> Result<Formula, NoVerdict> {
        fn visit(
            graph: &mut Graph<'_>,
            root: Formula,
            class: Class,
            existential: bool,
            cache: &mut HashMap<Formula, Formula>,
        ) -> Result<Formula, NoVerdict> {
            graph.tick()?;
            if root < 2 {
                return Ok(root);
            }
            if let Some(&known) = cache.get(&root) {
                return Ok(known);
            }
            let node = graph.node(root);
            let low = visit(graph, node.low, class, existential, cache)?;
            let high = visit(graph, node.high, class, existential, cache)?;
            let result = if graph.classes[node.variable as usize] == class {
                if existential {
                    graph.or(low, high)?
                } else {
                    graph.and(low, high)?
                }
            } else {
                graph.make(node.variable, low, high)?
            };
            graph.charge(96)?;
            cache.insert(root, result);
            Ok(result)
        }
        visit(self, root, class, existential, &mut HashMap::new())
    }
    pub(super) fn support(&mut self, root: Formula) -> Result<Vec<u32>, NoVerdict> {
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        let mut variables = HashSet::new();
        while let Some(id) = pending.pop() {
            self.tick()?;
            if id < 2 || !seen.insert(id) {
                continue;
            }
            self.charge(128)?;
            let node = self.node(id);
            variables.insert(node.variable);
            pending.extend([node.low, node.high]);
        }
        let mut variables: Vec<_> = variables.into_iter().collect();
        variables.sort_unstable();
        Ok(variables)
    }
    #[cfg(test)]
    fn evaluate(&self, mut formula: Formula, values: &[bool]) -> bool {
        while formula >= 2 {
            let node = self.node(formula);
            formula = if values[node.variable as usize] {
                node.high
            } else {
                node.low
            };
        }
        formula == TRUE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    #[test]
    fn coherent_repetition_and_quantifier_order_match_concrete_completions() {
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(1 << 20, None, 32, &control);
        let (_, d) = graph.variable(Class::Undefined).unwrap();
        let (_, e) = graph.variable(Class::External).unwrap();
        let (_, c) = graph.variable(Class::Inability).unwrap();
        let not = graph.not(d).unwrap();
        assert_eq!(graph.or(d, not).unwrap(), TRUE);
        assert_eq!(graph.apply(Op::Xor, d, d).unwrap(), FALSE);
        for op in [Op::And, Op::Or, Op::Xor] {
            let formula = graph.apply(op, d, e).unwrap();
            let negated = graph.not(formula).unwrap();
            let yes = graph.exists_undefined(formula).unwrap();
            let no = graph.exists_undefined(negated).unwrap();
            for external in [false, true] {
                let expected: Vec<_> = [false, true]
                    .into_iter()
                    .map(|undefined| match op {
                        Op::And => undefined && external,
                        Op::Or => undefined || external,
                        Op::Xor => undefined ^ external,
                    })
                    .collect();
                assert_eq!(
                    graph.evaluate(yes, &[false, external, false]),
                    expected.contains(&true)
                );
                assert_eq!(
                    graph.evaluate(no, &[false, external, false]),
                    expected.contains(&false)
                );
            }
        }
        assert_eq!(graph.exists_undefined(e).unwrap(), e);
        assert_eq!(graph.exists_undefined(c).unwrap(), c);
    }
    #[test]
    fn unknown_content_can_correlate_with_undefined_choices() {
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(4 << 20, None, 32, &control);
        let (_, d) = graph.variable(Class::Undefined).unwrap();
        let (_, e) = graph.variable(Class::External).unwrap();
        // All two-variable Boolean functions; each external completion can be
        // any of the four functions of d, including identity and negation.
        for table in 0..16 {
            let mut formula = FALSE;
            for dv in [false, true] {
                for ev in [false, true] {
                    if table & (1 << (usize::from(dv) * 2 + usize::from(ev))) != 0 {
                        let dx = if dv { d } else { graph.not(d).unwrap() };
                        let ex = if ev { e } else { graph.not(e).unwrap() };
                        let term = graph.and(dx, ex).unwrap();
                        formula = graph.or(formula, term).unwrap();
                    }
                }
            }
            let all_yes = graph.forall_unknown(formula).unwrap();
            let yes = graph.exists_undefined(all_yes).unwrap() == TRUE;
            let not = graph.not(formula).unwrap();
            let all_no = graph.forall_unknown(not).unwrap();
            let no = graph.exists_undefined(all_no).unwrap() == TRUE;
            let always_dependent = (0..4).all(|completion| {
                let low = graph.evaluate(formula, &[false, completion & 1 != 0]);
                let high = graph.evaluate(formula, &[true, completion & 2 != 0]);
                low != high
            });
            assert_eq!(yes && no, always_dependent, "truth table {table}");
        }
    }
    #[test]
    fn private_effects_remain_independent_of_external_content_but_not_opaque_local_summaries() {
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(32 << 20, None, 32, &control);
        let (_, a) = graph.variable(Class::Undefined).unwrap();
        let (_, p) = graph.variable(Class::PrivateUndefined).unwrap();
        let (_, e) = graph.variable(Class::External).unwrap();
        let (_, i) = graph.variable(Class::Inability).unwrap();
        for table in 0..256 {
            let mut formula = FALSE;
            for av in [false, true] {
                for pv in [false, true] {
                    for ev in [false, true] {
                        let at = usize::from(av) * 4 + usize::from(pv) * 2 + usize::from(ev);
                        if table & (1 << at) != 0 {
                            let mut term = TRUE;
                            for (var, value) in [(a, av), (p, pv), (e, ev)] {
                                let chosen = if value { var } else { graph.not(var).unwrap() };
                                term = graph.and(term, chosen).unwrap();
                            }
                            formula = graph.or(formula, term).unwrap();
                        }
                    }
                }
            }
            let yes = graph.witness(formula).unwrap() == TRUE;
            let not = graph.not(formula).unwrap();
            let no = graph.witness(not).unwrap() == TRUE;
            let required = (0..4).all(|external_function| {
                let mut outcomes = [false; 2];
                for av in [false, true] {
                    for pv in [false, true] {
                        let ev = external_function & (1 << usize::from(av)) != 0;
                        outcomes[usize::from(graph.evaluate(formula, &[av, pv, ev, false]))] = true;
                    }
                }
                outcomes == [true, true]
            });
            assert_eq!(yes && no, required, "truth table {table}");
        }
        let opaque = graph.apply(Op::Xor, p, i).unwrap();
        assert_eq!(graph.witness(opaque).unwrap(), FALSE);
        let negated = graph.not(opaque).unwrap();
        assert_eq!(graph.witness(negated).unwrap(), FALSE);
    }
    #[test]
    fn exhausted_or_cancelled_analysis_never_produces_a_formula() {
        let cancelled = AtomicBool::new(true);
        assert_eq!(
            Graph::new(1024, None, 10, &cancelled)
                .variable(Class::Undefined)
                .unwrap_err()
                .reason,
            NoVerdictReason::Cancelled
        );
        let control = AtomicBool::new(false);
        for (bytes, steps, variables) in [(0, None, 10), (1024, Some(0), 10), (1024, None, 0)] {
            assert_eq!(
                Graph::new(bytes, steps, variables, &control)
                    .variable(Class::Undefined)
                    .unwrap_err()
                    .reason,
                NoVerdictReason::LimitExceeded
            );
        }
    }
}
