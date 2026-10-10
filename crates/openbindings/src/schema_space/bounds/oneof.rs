//! Dual-polarity extension, selected only when a oneOf depends on a hole.
//! Definition identity is (original node, polarity), never a reference path.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Polarity {
    Lower,
    Upper,
}
impl Polarity {
    fn suffix(self) -> char {
        if self == Self::Lower { 'l' } else { 'u' }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol {
    Variant(usize, Polarity),
    Body(usize, Polarity),
    UpperOne(usize),
    Summary(usize),
    Hole(Polarity),
}
fn symbol_name(out: &mut impl Write, symbol: Symbol) -> fmt::Result {
    match symbol {
        Symbol::Variant(id, p) => write!(out, "n{id}{}", p.suffix()),
        Symbol::Body(id, p) => write!(out, "b{id}{}", p.suffix()),
        Symbol::UpperOne(id) => write!(out, "p{id}"),
        Symbol::Summary(id) => write!(out, "s{id}"),
        Symbol::Hole(p) => write!(out, "hole{}", p.suffix()),
    }
}
fn dual_uri(out: &mut impl Write, namespace: usize) -> fmt::Result {
    write!(out, "{NAMESPACE}{namespace}/dual")
}
fn symbol_uri(out: &mut impl Write, namespace: usize, symbol: Symbol) -> fmt::Result {
    dual_uri(out, namespace)?;
    out.write_str("#/$defs/")?;
    symbol_name(out, symbol)
}
fn variant(plan: &Plan, id: usize, polarity: Polarity) -> Symbol {
    // A closed node is exact and needs only one definition per registry.
    Symbol::Variant(
        id,
        if plan.dependent.contains(&id) {
            polarity
        } else {
            Polarity::Upper
        },
    )
}
fn generated_limit(nodes: bool) -> NoVerdict {
    if nodes {
        limit(
            "partial-generated-node-limit",
            "paired oneOf projection exceeds 100,000 emitted schema-node occurrences",
        )
    } else {
        limit(
            "partial-generated-edge-limit",
            "paired oneOf projection exceeds 200,000 emitted reference/applicator-edge occurrences",
        )
    }
}
fn skip(keyword: &str, has_if: bool, body: bool) -> bool {
    matches!(
        keyword,
        "$id" | "$schema" | "$defs" | "definitions" | "dependencies" | "contentSchema" | "$anchor"
    ) || (matches!(keyword, "then" | "else") && !has_if)
        || (body && keyword == "oneOf")
}

// One traversal emits actual syntax in both modes. Each schema object/boolean is
// one node; each $ref and each applicator-to-child selection is one edge. $defs
// containment and opaque literal/annotation JSON are not evaluation edges/nodes.
// We retain two separate, independently closed copies, so every event charges 2.
// Local inline glue is a tree. In-place reference edges between definitions are
// collected and cycle-checked before any projected text or origin strings copy.
struct Emitter<'a> {
    space: &'a SchemaSpace,
    plan: &'a Plan,
    control: &'a WorkControl,
    cap: Admission,
    sink: Sink,
    nodes: usize,
    edges: usize,
    error: Option<NoVerdict>,
    discovering: bool,
    symbols: BTreeSet<Symbol>,
    queue: VecDeque<Symbol>,
    graph: BTreeMap<Symbol, Vec<Symbol>>,
    current: Option<Symbol>,
}
impl<'a> Emitter<'a> {
    fn new(
        space: &'a SchemaSpace,
        plan: &'a Plan,
        control: &'a WorkControl,
        cap: Admission,
        sink: Sink,
        discovering: bool,
    ) -> Self {
        Self {
            space,
            plan,
            control,
            cap,
            sink,
            nodes: 0,
            edges: 0,
            error: None,
            discovering,
            symbols: BTreeSet::new(),
            queue: VecDeque::new(),
            graph: BTreeMap::new(),
            current: None,
        }
    }
    fn stop(&mut self, error: NoVerdict) -> fmt::Result {
        self.error = Some(error);
        Err(fmt::Error)
    }
    fn check(&mut self) -> fmt::Result {
        if let Err(error) = self.control.check() {
            return self.stop(error);
        }
        Ok(())
    }
    fn node(&mut self) -> fmt::Result {
        self.check()?;
        let Some(next) = self
            .nodes
            .checked_add(2)
            .filter(|n| *n <= self.cap.generated_nodes)
        else {
            return self.stop(generated_limit(true));
        };
        self.nodes = next;
        Ok(())
    }
    fn edge(&mut self) -> fmt::Result {
        self.check()?;
        let Some(next) = self
            .edges
            .checked_add(2)
            .filter(|n| *n <= self.cap.generated_edges)
        else {
            return self.stop(generated_limit(false));
        };
        self.edges = next;
        Ok(())
    }
    fn schedule(&mut self, symbol: Symbol) -> fmt::Result {
        if self.discovering && !self.symbols.contains(&symbol) {
            // Reserve the two emitted definition nodes plus two container roots
            // before retaining numeric graph metadata. The exact emitter also
            // counts inline nodes and refuses before every occurrence.
            if self
                .symbols
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(2))
                .and_then(|n| n.checked_add(2))
                .is_none_or(|n| n > self.cap.generated_nodes)
            {
                return self.stop(generated_limit(true));
            }
            self.symbols.insert(symbol);
            self.queue.push_back(symbol);
        }
        Ok(())
    }
    fn ref_value(&mut self, target: Symbol, inplace: bool) -> fmt::Result {
        self.edge()?;
        self.schedule(target)?;
        if self.discovering
            && inplace
            && let Some(from) = self.current
        {
            self.graph.entry(from).or_default().push(target);
        }
        self.sink.write_char('"')?;
        symbol_uri(&mut self.sink, self.plan.namespace, target)?;
        self.sink.write_char('"')
    }
    fn reference(&mut self, target: Symbol, inplace: bool) -> fmt::Result {
        self.node()?;
        self.sink.write_str("{\"$ref\":")?;
        self.ref_value(target, inplace)?;
        self.sink.write_char('}')
    }
    fn child_ref(&mut self, target: Symbol, inplace: bool) -> fmt::Result {
        self.edge()?;
        self.reference(target, inplace)
    }
    fn child(
        &mut self,
        source: usize,
        value: JsonRef<'_>,
        polarity: Polarity,
        inplace: bool,
    ) -> fmt::Result {
        let target = self.space.sources[source].positions[&node_id(&value.to_owned())];
        self.child_ref(variant(self.plan, target, polarity), inplace)
    }
    fn branch_refs(&mut self, id: usize, polarity: Polarity) -> fmt::Result {
        let node = &self.space.nodes[id];
        let children = node.value.get("oneOf").expect("transformed oneOf");
        for (i, value) in children.elements().expect("checked list").enumerate() {
            if i != 0 {
                self.sink.write_char(',')?;
            }
            self.child(node.source, value, polarity, true)?;
        }
        Ok(())
    }
    fn ordinary(&mut self, id: usize, polarity: Polarity, body: bool) -> fmt::Result {
        self.node()?;
        let node = &self.space.nodes[id];
        if node.value.kind() == JsonKind::Boolean {
            return self.sink.write_str(node.value.text());
        }
        self.sink.write_char('{')?;
        let has_if = node.value.get("if").is_some();
        let mut first = true;
        for member in node.value.view().members().expect("checked object") {
            self.check()?;
            let keyword = member.name.as_str().expect("checked Unicode");
            if skip(keyword, has_if, body) {
                continue;
            }
            if !first {
                self.sink.write_char(',')?;
            }
            first = false;
            quote(&mut self.sink, keyword)?;
            self.sink.write_char(':')?;
            if keyword == "$ref" {
                let target = if self.plan.holes.contains(&id) {
                    Symbol::Hole(polarity)
                } else {
                    variant(
                        self.plan,
                        self.plan.reach.targets[&(id, "$ref".into())].node,
                        polarity,
                    )
                };
                self.ref_value(target, true)?;
            } else if let Some(policy) = policy(keyword, has_if) {
                match policy.shape {
                    Children::Map => {
                        self.sink.write_char('{')?;
                        for (i, member) in member.value.members().expect("checked map").enumerate()
                        {
                            if i != 0 {
                                self.sink.write_char(',')?;
                            }
                            quote(&mut self.sink, member.name.as_str().expect("Unicode"))?;
                            self.sink.write_char(':')?;
                            self.child(node.source, member.value, polarity, policy.inplace)?;
                        }
                        self.sink.write_char('}')?;
                    }
                    Children::List => {
                        self.sink.write_char('[')?;
                        for (i, value) in member.value.elements().expect("checked list").enumerate()
                        {
                            if i != 0 {
                                self.sink.write_char(',')?;
                            }
                            self.child(node.source, value, polarity, policy.inplace)?;
                        }
                        self.sink.write_char(']')?;
                    }
                    Children::Single => {
                        self.child(node.source, member.value, polarity, policy.inplace)?
                    }
                }
            } else {
                self.sink.raw(member.value)?;
            }
        }
        self.sink.write_char('}')
    }
    fn definition(&mut self, symbol: Symbol) -> fmt::Result {
        self.current = Some(symbol);
        match symbol {
            Symbol::Hole(p) => {
                self.node()?;
                self.sink.write_str(if p == Polarity::Lower {
                    "false"
                } else {
                    "true"
                })
            }
            Symbol::Body(id, p) => self.ordinary(id, p, true),
            Symbol::Variant(id, p) if !self.plan.oneofs.contains(&id) => {
                self.ordinary(id, p, false)
            }
            Symbol::Variant(id, p) => {
                self.node()?;
                self.sink.write_str("{\"allOf\":[")?;
                self.child_ref(Symbol::Body(id, p), true)?;
                self.sink.write_char(',')?;
                if p == Polarity::Upper {
                    self.child_ref(Symbol::Summary(id), true)?;
                } else {
                    // L_one = anyOf(L_i) AND oneOf(U_i).
                    self.edge()?;
                    self.node()?;
                    self.sink.write_str("{\"anyOf\":[")?;
                    self.branch_refs(id, Polarity::Lower)?;
                    self.sink.write_str("]},")?;
                    self.edge()?;
                    self.node()?;
                    self.sink.write_str("{\"oneOf\":[")?;
                    self.branch_refs(id, Polarity::Upper)?;
                    self.sink.write_str("]}")?;
                }
                self.sink.write_str("]}")
            }
            Symbol::UpperOne(id) => {
                // U_one = anyOf(U_i) AND oneOf(L_i..., not(anyOf(L_i))).
                self.node()?;
                self.sink.write_str("{\"allOf\":[")?;
                self.edge()?;
                self.node()?;
                self.sink.write_str("{\"anyOf\":[")?;
                self.branch_refs(id, Polarity::Upper)?;
                self.sink.write_str("]},")?;
                self.edge()?;
                self.node()?;
                self.sink.write_str("{\"oneOf\":[")?;
                self.branch_refs(id, Polarity::Lower)?;
                self.sink.write_char(',')?;
                self.edge()?;
                self.node()?;
                self.sink.write_str("{\"not\":")?;
                self.edge()?;
                self.node()?;
                self.sink.write_str("{\"anyOf\":[")?;
                self.branch_refs(id, Polarity::Lower)?;
                self.sink.write_str("]}}]}]}")
            }
            Symbol::Summary(id) => {
                self.node()?;
                self.sink.write_str("{\"oneOf\":[")?;
                self.child_ref(Symbol::UpperOne(id), true)?;
                self.sink.write_str("]}")
            }
        }
    }
    fn header(&mut self) -> fmt::Result {
        self.node()?;
        self.sink.write_str("{\"$id\":\"")?;
        dual_uri(&mut self.sink, self.plan.namespace)?;
        self.sink.write_str("\",\"$schema\":")?;
        quote(&mut self.sink, DIALECT)?;
        self.sink.write_str(",\"$defs\":{")
    }
    fn named(&mut self, symbol: Symbol, first: bool) -> fmt::Result {
        if !first {
            self.sink.write_char(',')?;
        }
        self.sink.write_char('"')?;
        symbol_name(&mut self.sink, symbol)?;
        self.sink.write_str("\":")?;
        self.definition(symbol)
    }
    fn footer(&mut self, entry: Symbol) -> fmt::Result {
        self.current = None;
        self.sink.write_str("},\"$ref\":")?;
        self.ref_value(entry, true)?;
        self.sink.write_char('}')
    }
    fn discover(&mut self, entry: usize) -> fmt::Result {
        for p in [Polarity::Lower, Polarity::Upper] {
            self.schedule(variant(self.plan, entry, p))?;
            self.schedule(Symbol::Hole(p))?;
        }
        self.header()?;
        let mut first = true;
        while let Some(symbol) = self.queue.pop_front() {
            self.named(symbol, first)?;
            first = false;
        }
        self.footer(variant(self.plan, entry, Polarity::Lower))
    }
    fn write(&mut self, symbols: &BTreeSet<Symbol>, entry: Symbol) -> fmt::Result {
        self.header()?;
        for (i, &symbol) in symbols.iter().enumerate() {
            self.named(symbol, i == 0)?;
        }
        self.footer(entry)
    }
    fn failure(&mut self) -> NoVerdict {
        self.error.take().unwrap_or_else(text_limit)
    }
    fn check_cycles(&self) -> Result<(), NoVerdict> {
        let mut colors = BTreeMap::new();
        for &root in &self.symbols {
            if colors.get(&root) == Some(&2) {
                continue;
            }
            let mut stack = vec![(root, false)];
            while let Some((id, leaving)) = stack.pop() {
                self.control.check()?;
                if leaving {
                    colors.insert(id, 2);
                    continue;
                }
                match colors.get(&id) {
                    Some(1) => {
                        return Err(NoVerdict::new(
                            NoVerdictReason::EvaluatorFailure,
                            "partial-generated-cycle",
                            "generated bounds contain an in-place cycle",
                        ));
                    }
                    Some(2) => continue,
                    _ => {}
                }
                colors.insert(id, 1);
                stack.push((id, true));
                for &target in self.graph.get(&id).into_iter().flatten() {
                    stack.push((target, false));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum OriginKind {
    Prefix,
    Exact,
    Synthetic,
}
fn pointer_keyword(out: &mut impl Write, keyword: Option<&str>) -> fmt::Result {
    if let Some(keyword) = keyword {
        out.write_char('/')?;
        for c in keyword.chars() {
            match c {
                '~' => out.write_str("~0")?,
                '/' => out.write_str("~1")?,
                _ => out.write_char(c)?,
            }
        }
    }
    Ok(())
}
impl SchemaSpace {
    fn each_dual_origin(
        &self,
        plan: &Plan,
        symbols: &BTreeSet<Symbol>,
        control: &WorkControl,
        mut visit: impl FnMut(
            Option<Symbol>,
            Option<&str>,
            Option<usize>,
            OriginKind,
        ) -> Result<(), NoVerdict>,
    ) -> Result<(), NoVerdict> {
        visit(None, None, None, OriginKind::Synthetic)?; // container/root glue
        for &symbol in symbols {
            control.check()?;
            match symbol {
                Symbol::Body(id, Polarity::Upper) | Symbol::Variant(id, Polarity::Upper)
                    if matches!(symbol, Symbol::Body(..)) || !plan.oneofs.contains(&id) =>
                {
                    visit(Some(symbol), None, Some(id), OriginKind::Exact)?;
                    let node = &self.nodes[id];
                    let has_if = node.value.get("if").is_some();
                    for member in node.value.view().members().into_iter().flatten() {
                        control.check()?;
                        let key = member.name.as_str().expect("checked Unicode");
                        if skip(key, has_if, matches!(symbol, Symbol::Body(..))) {
                            continue;
                        }
                        // Exact applicator boundaries block rewritten child-ref
                        // descendants. Opaque/preserved payloads retain prefixes.
                        let kind = if policy(key, has_if).is_some() || key == "$ref" {
                            OriginKind::Exact
                        } else {
                            OriginKind::Prefix
                        };
                        visit(Some(symbol), Some(key), Some(id), kind)?;
                    }
                }
                Symbol::Summary(id) => {
                    visit(Some(symbol), None, None, OriginKind::Synthetic)?;
                    visit(Some(symbol), Some("oneOf"), Some(id), OriginKind::Exact)?;
                }
                _ => visit(Some(symbol), None, None, OriginKind::Synthetic)?,
            }
        }
        Ok(())
    }
    pub(super) fn oneof_bounds(
        &self,
        entry: usize,
        control: &WorkControl,
        cap: Admission,
        plan: &Plan,
    ) -> Result<EvaluationBounds, NoVerdict> {
        let mut count = Emitter::new(self, plan, control, cap, Sink::count(cap.text), true);
        if count.discover(entry).is_err() {
            return Err(count.failure());
        }
        count.check_cycles()?;
        let mut total = 0;
        let mut uri_count = Sink::count(cap.text);
        dual_uri(&mut uri_count, plan.namespace).map_err(|_| text_limit())?;
        // Same closure, different one-character entry polarity. No arena sharing:
        // count both serialized/decoded copies and both URI/entry copies.
        for _ in 0..2 {
            add_bytes(&mut total, count.sink.size, cap.text)?;
            add_bytes(&mut total, count.sink.decoded, cap.text)?;
            add_bytes(&mut total, uri_count.size, cap.text)?;
            add_bytes(&mut total, uri_count.size, cap.text)?;
        }
        self.each_dual_origin(
            plan,
            &count.symbols,
            control,
            |symbol, keyword, original, _| {
                let mut key = Sink::count(cap.text);
                if let Some(symbol) = symbol {
                    symbol_uri(&mut key, plan.namespace, symbol)
                } else {
                    dual_uri(&mut key, plan.namespace)
                }
                .map_err(|_| text_limit())?;
                pointer_keyword(&mut key, keyword).map_err(|_| text_limit())?;
                add_bytes(&mut total, key.size, cap.text)?;
                if let Some(id) = original {
                    add_bytes(&mut total, self.location_bytes(id)?, cap.text)?;
                    let mut suffix = Sink::count(cap.text);
                    pointer_keyword(&mut suffix, keyword).map_err(|_| text_limit())?;
                    add_bytes(&mut total, suffix.size, cap.text)?;
                }
                Ok(())
            },
        )?;
        if let Some(id) = plan.first_hole {
            add_bytes(&mut total, self.location_bytes(id)?, cap.text)?;
            add_bytes(
                &mut total,
                "/$ref".len() + UNAVAILABLE_CODE.len() + UNAVAILABLE_MESSAGE.len(),
                cap.text,
            )?;
        }
        control.check()?;
        let mut locations = BTreeMap::new();
        self.each_dual_origin(
            plan,
            &count.symbols,
            control,
            |symbol, keyword, original, kind| {
                let mut key = String::new();
                if let Some(symbol) = symbol {
                    symbol_uri(&mut key, plan.namespace, symbol)
                } else {
                    dual_uri(&mut key, plan.namespace)
                }
                .expect("String writer");
                pointer_keyword(&mut key, keyword).expect("String writer");
                let origin = match original {
                    Some(id) => {
                        let mut at = self.location(id);
                        pointer_keyword(&mut at.pointer, keyword).expect("String writer");
                        match kind {
                            OriginKind::Prefix => ProgramOrigin::Prefix(at),
                            OriginKind::Exact => ProgramOrigin::Exact(at),
                            OriginKind::Synthetic => unreachable!(),
                        }
                    }
                    None => ProgramOrigin::Synthetic,
                };
                locations.insert(key, origin);
                Ok(())
            },
        )?;
        let locations = Arc::new(locations);
        let unavailable = plan.first_hole.map(|id| {
            let mut at = self.location(id);
            at.pointer.push_str("/$ref");
            NoVerdict::new(
                NoVerdictReason::ResourceUnavailable,
                UNAVAILABLE_CODE,
                UNAVAILABLE_MESSAGE,
            )
            .located(at)
        });
        let mut programs = Vec::with_capacity(2);
        for polarity in [Polarity::Lower, Polarity::Upper] {
            control.check()?;
            let mut output = Emitter::new(
                self,
                plan,
                control,
                cap,
                Sink::output(count.sink.size, count.sink.decoded),
                false,
            );
            if output
                .write(&count.symbols, variant(plan, entry, polarity))
                .is_err()
            {
                return Err(output.failure());
            }
            debug_assert_eq!(
                (
                    output.nodes,
                    output.edges,
                    output.sink.size,
                    output.sink.decoded
                ),
                (
                    count.nodes,
                    count.edges,
                    count.sink.size,
                    count.sink.decoded
                )
            );
            let document =
                JsonValue::parse(output.sink.text.expect("output sink")).map_err(|_| {
                    limit(
                        "program-json-limit",
                        "projected resource exceeds JSON admission",
                    )
                })?;
            let mut uri = String::new();
            dual_uri(&mut uri, plan.namespace).expect("String writer");
            programs.push(EvaluationProgram {
                entry_uri: uri.clone(),
                resources: vec![SchemaResource { uri, document }],
                locations: locations.clone(),
            });
        }
        let upper = programs.pop().expect("upper");
        let lower = programs.pop().expect("lower");
        Ok(EvaluationBounds {
            lower,
            upper,
            unavailable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{space, text_size};
    use super::*;
    // Independent inspection of actual emitted schema occurrences, including
    // definitions and opaque values. This does not call emitter event methods.
    fn occurrences(schema: JsonRef<'_>) -> (usize, usize) {
        let mut nodes = 1;
        let mut edges = usize::from(schema.get("$ref").is_some());
        for member in schema.members().into_iter().flatten() {
            let key = member.name.as_str().unwrap();
            let children: Vec<_> = match key {
                "$defs" | "properties" | "patternProperties" | "dependentSchemas" => member
                    .value
                    .members()
                    .into_iter()
                    .flatten()
                    .map(|m| m.value)
                    .collect(),
                "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
                    member.value.elements().into_iter().flatten().collect()
                }
                "not"
                | "if"
                | "then"
                | "else"
                | "items"
                | "additionalProperties"
                | "propertyNames"
                | "contains" => vec![member.value],
                _ => Vec::new(),
            };
            if key != "$defs" {
                edges += children.len();
            }
            for child in children {
                let (n, e) = occurrences(child);
                nodes += n;
                edges += e;
            }
        }
        (nodes, edges)
    }
    #[test]
    fn aggregate_emitted_occurrences_and_retained_text_have_exact_boundaries() {
        let schemas = [
            r#"{"oneOf":[true,{"$ref":"https://bounds.invalid/U"}]}"#,
            r#"{"oneOf":[{"properties":{"quote\"/~雪":{"$ref":"https://bounds.invalid/U"}},"x-opaque":{"\u006b":["\u0061","escaped\"",{"\n":true}]}},{"oneOf":[false,{"$ref":"https://bounds.invalid/V"}]}],"description":"\u0061"}"#,
            r##"{"$id":"https://bounds.invalid/root","oneOf":[{"properties":{"next":{"$ref":"#"},"x":{"$ref":"https://bounds.invalid/U"}}},false]}"##,
        ];
        for schema in schemas {
            let (space, entry) = space(schema);
            let control = WorkControl::new();
            let bounds = space.bounds(entry, &control).unwrap();
            let (mut nodes, mut edges) = (0, 0);
            for program in [&bounds.lower, &bounds.upper] {
                for resource in &program.resources {
                    let (n, e) = occurrences(resource.document.view());
                    nodes += n;
                    edges += e;
                }
            }
            let text = text_size(&bounds);
            let cap = Admission {
                text,
                generated_nodes: nodes,
                generated_edges: edges,
                ..Admission::default()
            };
            assert_eq!(
                text_size(&space.bounds_admitted(entry, &control, cap).unwrap()),
                text
            );
            for (cap, expected) in [
                (
                    Admission {
                        text: text - 1,
                        ..cap
                    },
                    "partial-program-byte-limit",
                ),
                (
                    Admission {
                        generated_nodes: nodes - 1,
                        ..cap
                    },
                    "partial-generated-node-limit",
                ),
                (
                    Admission {
                        generated_edges: edges - 1,
                        ..cap
                    },
                    "partial-generated-edge-limit",
                ),
            ] {
                assert_eq!(
                    space
                        .bounds_admitted(entry, &control, cap)
                        .unwrap_err()
                        .code,
                    expected
                );
            }
            let plan = space.partial_reach(entry, &control, cap).unwrap();
            let mut counted =
                Emitter::new(&space, &plan, &control, cap, Sink::count(cap.text), true);
            counted.discover(entry).unwrap();
            assert_eq!((counted.nodes, counted.edges), (nodes, edges));
            assert!(counted.sink.text.is_none());
        }
        let (space, entry) = space(r#"{"anyOf":[true,{"$ref":"https://bounds.invalid/U"}]}"#);
        // Existing positive-only plans pay no new generated allowance.
        assert!(
            space
                .bounds_admitted(
                    entry,
                    &WorkControl::new(),
                    Admission {
                        generated_nodes: 0,
                        generated_edges: 0,
                        ..Admission::default()
                    }
                )
                .is_ok()
        );
        let cap = Admission::default();
        assert_eq!(
            (cap.generated_nodes, cap.generated_edges, cap.text),
            (100_000, 200_000, 64 * 1024 * 1024)
        );
    }
    #[test]
    fn exact_summary_and_synthetic_barriers_stop_descendant_inheritance() {
        let (space, entry) = space(
            r#"{"oneOf":[true,true,{"$ref":"https://bounds.invalid/U"}],"title":"original"}"#,
        );
        let bounds = space.bounds(entry, &WorkControl::new()).unwrap();
        let p = &bounds.upper;
        let (uri, at) = p
            .locations
            .iter()
            .find_map(|(uri, origin)| match origin {
                ProgramOrigin::Exact(at) if at.pointer.ends_with("/oneOf") => Some((uri, at)),
                _ => None,
            })
            .unwrap();
        assert_eq!(p.original_location(uri), Some(at.clone()));
        assert!(p.original_location(&(uri.to_owned() + "/0")).is_none());
        assert!(p.original_location(&(uri.to_owned() + "/0/$ref")).is_none());
        assert_eq!(
            p.original_location_bounded(uri, at.pointer.len() - 1),
            Err(LocationBudgetExceeded)
        );
        assert_eq!(
            p.original_location_bounded(uri, at.pointer.len()),
            Ok(Some(at.clone()))
        );
        for (uri, origin) in p.locations.iter() {
            if matches!(origin, ProgramOrigin::Synthetic) {
                assert!(p.original_location(uri).is_none());
                assert!(
                    p.original_location(&(uri.to_owned() + "/invented"))
                        .is_none()
                );
            }
        }
    }
    #[test]
    fn identity_closure_cancellation_and_generated_cycle_invariant() {
        let (space, entry) = space(
            r#"{"oneOf":[{"$ref":"https://sdk-bounds.openbindings.invalid/0/dual"},{"$ref":"https://sdk-bounds.openbindings.invalid/1/dual#/$defs/holeu"}]}"#,
        );
        let control = WorkControl::new();
        let plan = space
            .partial_reach(entry, &control, Admission::default())
            .unwrap();
        assert_eq!(plan.namespace, 2);
        let bounds = space.bounds(entry, &control).unwrap();
        for program in [&bounds.lower, &bounds.upper] {
            assert_eq!(program.resources.len(), 1);
            assert_eq!(program.resources[0].uri, format!("{NAMESPACE}2/dual"));
            assert_eq!(
                program.resources[0]
                    .document
                    .at("/$defs/holel")
                    .unwrap()
                    .as_bool(),
                Some(false)
            );
            assert_eq!(
                program.resources[0]
                    .document
                    .at("/$defs/holeu")
                    .unwrap()
                    .as_bool(),
                Some(true)
            );
        }
        let mut emitter = Emitter::new(
            &space,
            &plan,
            &control,
            Admission::default(),
            Sink::count(TEXT_LIMIT),
            true,
        );
        emitter.discover(entry).unwrap();
        emitter.check_cycles().unwrap();
        let symbol = *emitter.symbols.first().unwrap();
        emitter.graph.entry(symbol).or_default().push(symbol);
        assert_eq!(
            emitter.check_cycles().unwrap_err().code,
            "partial-generated-cycle"
        );
        emitter.nodes = usize::MAX;
        emitter.cap.generated_nodes = usize::MAX;
        assert!(emitter.node().is_err());
        assert_eq!(emitter.failure().code, "partial-generated-node-limit");
        emitter.edges = usize::MAX;
        emitter.cap.generated_edges = usize::MAX;
        assert!(emitter.edge().is_err());
        assert_eq!(emitter.failure().code, "partial-generated-edge-limit");
        control.cancel();
        assert!(emitter.header().is_err());
        assert_eq!(emitter.failure().reason, NoVerdictReason::Cancelled);
        assert_eq!(
            space.bounds(entry, &control).unwrap_err().reason,
            NoVerdictReason::Cancelled
        );
    }
}
