//! Exact contains counts: concrete matches need no formula distribution.
use super::{FALSE, Formula, Graph, NoVerdict, TRUE, decision::limit};

#[derive(Default)]
pub(super) struct Matches {
    concrete: u64,
    // Entry n is the coherent formula for exactly n uncertain matches. Empty
    // means no uncertainty has occurred; concrete matches remain an offset.
    uncertain: Vec<Formula>,
}

impl Matches {
    pub(super) fn add(&mut self, result: Formula, graph: &mut Graph<'_>) -> Result<(), NoVerdict> {
        graph.tick()?;
        match result {
            FALSE => return Ok(()),
            TRUE => {
                self.concrete = self
                    .concrete
                    .checked_add(1)
                    .ok_or_else(|| limit("contains-count-limit"))?;
                return Ok(());
            }
            _ => {}
        }
        if self.uncertain.is_empty() {
            graph.charge(32)?;
            self.uncertain = vec![TRUE];
        }
        let not = graph.not(result)?;
        graph.charge(self.uncertain.len().saturating_add(1).saturating_mul(8))?;
        let mut next = vec![FALSE; self.uncertain.len() + 1];
        for (count, &formula) in self.uncertain.iter().enumerate() {
            graph.tick()?;
            let same = graph.and(formula, not)?;
            next[count] = graph.or(next[count], same)?;
            let more = graph.and(formula, result)?;
            next[count + 1] = graph.or(next[count + 1], more)?;
        }
        self.uncertain = next;
        Ok(())
    }

    pub(super) fn within(
        &self,
        minimum: u64,
        maximum: u64,
        graph: &mut Graph<'_>,
    ) -> Result<Formula, NoVerdict> {
        graph.tick()?;
        let range = minimum..=maximum;
        if self.uncertain.is_empty() {
            return Ok(if range.contains(&self.concrete) {
                TRUE
            } else {
                FALSE
            });
        }
        let mut valid = FALSE;
        for (count, &formula) in self.uncertain.iter().enumerate() {
            graph.tick()?;
            let total = self
                .concrete
                .checked_add(count as u64)
                .ok_or_else(|| limit("contains-count-limit"))?;
            if range.contains(&total) {
                valid = graph.or(valid, formula)?;
            }
        }
        Ok(valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Limits,
        semantic::{Plan, decision::Class},
    };
    use openbindings::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn concrete_count_needs_no_formula_allocation_and_keeps_exact_bounds() {
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(usize::MAX, Some(20_000), 0, &control);
        let mut matches = Matches::default();
        for _ in 0..4096 {
            matches.add(TRUE, &mut graph).unwrap();
            matches.add(FALSE, &mut graph).unwrap();
        }
        for (minimum, maximum, expected) in [
            (4096, 4096, TRUE),
            (4097, u64::MAX, FALSE),
            (0, 4095, FALSE),
            (0, u64::MAX, TRUE),
            (5, 4, FALSE),
        ] {
            assert_eq!(
                matches.within(minimum, maximum, &mut graph).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn promotion_keeps_shared_and_complementary_effects_correlated() {
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(1 << 20, None, 8, &control);
        let (_, a) = graph.variable(Class::Undefined).unwrap();
        let not_a = graph.not(a).unwrap();
        for values in [[TRUE, a, FALSE, not_a, TRUE], [a, TRUE, TRUE, not_a, FALSE]] {
            let mut matches = Matches::default();
            for result in values {
                matches.add(result, &mut graph).unwrap();
            }
            assert_eq!(matches.within(3, 3, &mut graph).unwrap(), TRUE);
            assert_eq!(matches.within(2, 2, &mut graph).unwrap(), FALSE);
        }
        let mut matches = Matches::default();
        for result in [TRUE, a, a, FALSE] {
            matches.add(result, &mut graph).unwrap();
        }
        assert_eq!(matches.within(2, 2, &mut graph).unwrap(), FALSE);
        assert_eq!(matches.within(3, 3, &mut graph).unwrap(), a);
        assert_eq!(matches.within(1, 1, &mut graph).unwrap(), not_a);
    }

    fn prepared(schema: &str) -> Plan {
        let document = Document::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
        ))
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
        let control = AtomicBool::new(false);
        let limits = PreparationLimits::default();
        let environment = document
            .operation("op")
            .unwrap()
            .input()
            .unwrap()
            .environment(&ExternalSchemas::default(), &limits, &control)
            .unwrap();
        openbindings_internal_schema::preparation::Attempt::run(
            limits.max_sdk_work_units,
            limits.max_sdk_reservation_bytes,
            &control,
            |attempt| Plan::new(environment, Limits::default(), limits, attempt),
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn original_engine_completes_concrete_contains_and_required_annotations() {
        let control = AtomicBool::new(false);
        let plan = prepared(r#"{"contains":{"type":"integer"},"unevaluatedItems":false}"#);
        for size in [128, 1024, 4096] {
            let text = format!("[{}]", vec!["1"; size].join(","));
            let value = JsonValue::parse(&text).unwrap();
            assert!(matches!(
                plan.evaluate(&value, Detail::Outcome, &control),
                ValueOutcome::Satisfies
            ));
            let late = JsonValue::parse(text.replace(']', ",\"late\"]")).unwrap();
            assert!(matches!(
                plan.evaluate(&late, Detail::Outcome, &control),
                ValueOutcome::Fails { .. }
            ));
        }
        for (schema, value, satisfies) in [
            (
                r#"{"contains":true,"minContains":0,"maxContains":0}"#,
                "[]",
                true,
            ),
            (
                r#"{"contains":true,"minContains":0,"maxContains":0}"#,
                "[1]",
                false,
            ),
            (
                r#"{"contains":{"type":"integer"},"minContains":2,"maxContains":2}"#,
                r#"[1,"x",2]"#,
                true,
            ),
            (
                r#"{"contains":{"type":"integer"},"minContains":2,"maxContains":2}"#,
                r#"[1,"x"]"#,
                false,
            ),
            (
                r#"{"contains":{"type":"integer"},"minContains":2,"maxContains":2}"#,
                "[1,2,3]",
                false,
            ),
        ] {
            let result = prepared(schema).evaluate(
                &JsonValue::parse(value).unwrap(),
                Detail::Outcome,
                &control,
            );
            assert!(
                if satisfies {
                    matches!(result, ValueOutcome::Satisfies)
                } else {
                    matches!(result, ValueOutcome::Fails { .. })
                },
                "{schema}: {value}: {result:?}"
            );
        }
    }
}
