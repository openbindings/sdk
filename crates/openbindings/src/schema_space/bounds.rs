//! Compositional validity bounds. Strict and positive-only encodings stay separate.
mod oneof;
use super::*;
use crate::contracts::ProgramOrigin;
use openbindings_internal_json::backend::{decoded_string_size, pointer_size};
use std::{
    fmt::{self, Write},
    sync::Arc,
};

const TEXT_LIMIT: usize = 64 * 1024 * 1024;
const NODE_LIMIT: usize = 100_000;
const EDGE_LIMIT: usize = 200_000;
const HOLE_LIMIT: usize = 100_000;
const NAMESPACE: &str = "https://sdk-bounds.openbindings.invalid/";
const UNAVAILABLE_CODE: &str = "resource-unavailable";
const UNAVAILABLE_MESSAGE: &str = "validity could depend on a resource that was not supplied";

#[derive(Clone, Copy)]
struct Admission {
    text: usize,
    nodes: usize,
    edges: usize,
    holes: usize,
    generated_nodes: usize,
    generated_edges: usize,
}
impl Default for Admission {
    fn default() -> Self {
        Self {
            text: TEXT_LIMIT,
            nodes: NODE_LIMIT,
            edges: EDGE_LIMIT,
            holes: HOLE_LIMIT,
            generated_nodes: 100_000,
            generated_edges: 200_000,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Influence {
    Positive,
    OneOf,
    Excluded,
}
#[derive(Clone, Copy)]
struct Edge {
    from: usize,
    to: usize,
    influence: Influence,
}
#[derive(Default)]
struct Plan {
    reach: Reach,
    dependencies: Vec<Edge>,
    holes: HashSet<usize>,
    first_hole: Option<usize>,
    edge_count: usize,
    // Numeric namespace segments only: original resolved identities remain borrowed.
    occupied_namespaces: HashSet<usize>,
    namespace: usize,
    dependent: HashSet<usize>,
    oneofs: HashSet<usize>,
}
#[derive(Clone, Copy)]
enum Children {
    Map,
    List,
    Single,
}
#[derive(Clone, Copy)]
struct Policy {
    shape: Children,
    inplace: bool,
    influence: Influence,
}
// This is the only partial influence policy. $defs/annotations are not edges.
fn policy(keyword: &str, has_if: bool) -> Option<Policy> {
    use Children::*;
    use Influence::*;
    let (shape, inplace, influence) = match keyword {
        "properties" | "patternProperties" => (Map, false, Positive),
        "dependentSchemas" => (Map, true, Positive),
        "allOf" | "anyOf" => (List, true, Positive),
        "prefixItems" => (List, false, Positive),
        "additionalProperties" | "propertyNames" | "items" => (Single, false, Positive),
        "oneOf" => (List, true, OneOf),
        "not" => (Single, true, Excluded),
        "if" | "then" | "else" if has_if => (Single, true, Excluded),
        "contains" => (Single, false, Excluded),
        _ => return None,
    };
    Some(Policy {
        shape,
        inplace,
        influence,
    })
}
fn limit(code: &str, message: &str) -> NoVerdict {
    NoVerdict::new(NoVerdictReason::LimitExceeded, code, message)
}
fn text_limit() -> NoVerdict {
    limit(
        "partial-program-byte-limit",
        "paired projection text exceeds 64 MiB",
    )
}
fn add_bytes(total: &mut usize, size: usize, cap: usize) -> Result<(), NoVerdict> {
    *total = total
        .checked_add(size)
        .filter(|n| *n <= cap)
        .ok_or_else(text_limit)?;
    Ok(())
}
impl Plan {
    fn edge(&mut self, cap: Admission) -> Result<(), NoVerdict> {
        if self.edge_count >= cap.edges {
            return Err(limit(
                "schema-edge-limit",
                "partial preparation exceeds 200,000 dependency edges",
            ));
        }
        self.edge_count += 1;
        Ok(())
    }
    fn child(
        &mut self,
        from: usize,
        to: usize,
        policy: Policy,
        queue: &mut VecDeque<usize>,
        cap: Admission,
    ) -> Result<(), NoVerdict> {
        self.edge(cap)?;
        self.dependencies.push(Edge {
            from,
            to,
            influence: policy.influence,
        });
        if policy.inplace {
            self.reach.edges.entry(from).or_default().push(to);
        }
        queue.push_back(to);
        Ok(())
    }
    fn qualify(&mut self, control: &WorkControl) -> Result<(), NoVerdict> {
        let mut reverse: HashMap<usize, Vec<&Edge>> = HashMap::new();
        for edge in &self.dependencies {
            reverse.entry(edge.to).or_default().push(edge);
        }
        let mut dependent = self.holes.clone();
        let mut queue: VecDeque<_> = self.holes.iter().copied().collect();
        while let Some(id) = queue.pop_front() {
            control.check()?;
            for edge in reverse.get(&id).into_iter().flatten() {
                if edge.influence == Influence::Excluded {
                    return Err(NoVerdict::new(
                        NoVerdictReason::ConservativePreparation,
                        "partial-nonpositive-influence",
                        "an unavailable reference influences an unqualified applicator",
                    ));
                }
                if edge.influence == Influence::OneOf {
                    self.oneofs.insert(edge.from);
                }
                if dependent.insert(edge.from) {
                    queue.push_back(edge.from);
                }
            }
        }
        self.dependent = dependent;
        Ok(())
    }
}
impl SchemaSpace {
    pub(crate) fn bounds(
        &self,
        entry: usize,
        control: &WorkControl,
    ) -> Result<EvaluationBounds, NoVerdict> {
        self.bounds_admitted(entry, control, Admission::default())
    }
    // Scratch guard is separate from retained text. Resolver errors allocate
    // original locations; measure before calling the unchanged strict resolver.
    fn bounds_scratch(&self, id: usize, reference: &str) -> Result<(), NoVerdict> {
        let node = &self.nodes[id];
        let base = self.resources[node.resource].base.as_deref().unwrap_or("");
        if reference.len().saturating_add(base.len()) > TEXT_LIMIT
            || pointer_size(node.value.view(), TEXT_LIMIT).is_none()
            || self.sources[node.source]
                .uri
                .as_ref()
                .is_some_and(|s| s.len() > TEXT_LIMIT)
        {
            return Err(limit(
                "partial-scratch-limit",
                "reference inputs or full source pointer exceed the 64 MiB scratch guard",
            ));
        }
        Ok(())
    }
    fn partial_reach(
        &self,
        entry: usize,
        control: &WorkControl,
        cap: Admission,
    ) -> Result<Plan, NoVerdict> {
        let mut out = Plan::default();
        let mut queue = VecDeque::from([entry]);
        while let Some(id) = queue.pop_front() {
            control.check()?;
            if out.reach.nodes.contains(&id) {
                continue;
            }
            if out.reach.nodes.len() >= cap.nodes {
                return Err(limit(
                    "schema-node-limit",
                    "preparation reached more than 100,000 schema nodes",
                ));
            }
            out.reach.nodes.insert(id);
            self.bounds_scratch(id, "")?;
            self.check_known_node(id)?;
            let node = &self.nodes[id];
            out.reach.resources.insert(node.resource);
            let has_if = node.value.get("if").is_some();
            for member in node.value.view().members().into_iter().flatten() {
                let keyword = member.name.as_str().unwrap_or("");
                if matches!(
                    keyword,
                    "unevaluatedProperties" | "unevaluatedItems" | "$dynamicRef" | "$dynamicAnchor"
                ) {
                    return Err(NoVerdict::new(
                        NoVerdictReason::ConservativePreparation,
                        "partial-annotation-or-dynamic",
                        "partial bounds do not support evaluated unevaluated or dynamic keywords",
                    ));
                }
                if keyword == "$ref" {
                    let reference = member.value.as_str().expect("known meta-schema check");
                    self.bounds_scratch(id, reference)?;
                    let mut occupied_suffix = None;
                    let resolved = self.resolve_with_missing(id, reference, &mut |name| {
                        occupied_suffix = private_namespace(name);
                    });
                    match resolved {
                        Ok(target) => {
                            out.child(
                                id,
                                target.node,
                                Policy {
                                    shape: Children::Single,
                                    inplace: true,
                                    influence: Influence::Positive,
                                },
                                &mut queue,
                                cap,
                            )?;
                            out.reach.targets.insert((id, "$ref".into()), target);
                        }
                        Err(detail)
                            if detail.reason == NoVerdictReason::ResourceUnavailable
                                && detail.code == UNAVAILABLE_CODE =>
                        {
                            // The missing carrier cannot hide independently invalid fragment grammar.
                            let fragment =
                                uri::decode_fragment(uri::fragment(reference).unwrap_or(""))
                                    .ok_or_else(|| {
                                        NoVerdict::new(
                                            NoVerdictReason::ConservativePreparation,
                                            "fragment-encoding",
                                            "reference fragment is not UTF-8",
                                        )
                                    })?;
                            if !valid_unknown_fragment(&fragment) {
                                return Err(NoVerdict::new(
                                    NoVerdictReason::ConservativePreparation,
                                    "invalid-reference-fragment",
                                    "missing-resource reference has invalid pointer or anchor syntax",
                                ));
                            }
                            if out.holes.len() >= cap.holes {
                                return Err(limit(
                                    "schema-hole-limit",
                                    "partial preparation exceeds 100,000 missing-reference edges",
                                ));
                            }
                            out.edge(cap)?;
                            if let Some(suffix) = occupied_suffix {
                                out.occupied_namespaces.insert(suffix);
                            }
                            out.holes.insert(id);
                            out.first_hole.get_or_insert(id);
                        }
                        Err(detail) => return Err(detail),
                    }
                    continue;
                }
                let Some(policy) = policy(keyword, has_if) else {
                    continue;
                };
                let mut child = |value: JsonRef<'_>| -> Result<(), NoVerdict> {
                    let target = self.sources[node.source]
                        .positions
                        .get(&node_id(&value.to_owned()))
                        .copied()
                        .ok_or_else(|| {
                            NoVerdict::new(
                                NoVerdictReason::ConservativePreparation,
                                "projection-target",
                                "an evaluated child is outside the schema index",
                            )
                        })?;
                    out.child(id, target, policy, &mut queue, cap)
                };
                // The strict and partial walks borrow the same child-selection
                // primitive; no wide child list is collected before edge admission.
                for value in keyword_children(keyword, member.value) {
                    child(value)?;
                }
            }
        }
        // At most one occupied namespace per hole, so a free namespace exists by
        // holes.len(). No source-controlled text is copied to choose it.
        while out.occupied_namespaces.contains(&out.namespace) {
            out.namespace += 1;
        }
        self.check_cycles(&out.reach, control)?;
        out.qualify(control)?;
        Ok(out)
    }
    fn location_bytes(&self, id: usize) -> Result<usize, NoVerdict> {
        let node = &self.nodes[id];
        let source = &self.sources[node.source];
        let full = pointer_size(node.value.view(), TEXT_LIMIT).ok_or_else(text_limit)?;
        let prefix = pointer_size(source.value.view(), TEXT_LIMIT).ok_or_else(text_limit)?;
        Ok(full
            .saturating_sub(prefix)
            .saturating_add(source.uri.as_ref().map_or(0, String::len)))
    }
    fn bounds_admitted(
        &self,
        entry: usize,
        control: &WorkControl,
        cap: Admission,
    ) -> Result<EvaluationBounds, NoVerdict> {
        let plan = self.partial_reach(entry, control, cap)?;
        if !plan.oneofs.is_empty() {
            return self.oneof_bounds(entry, control, cap, &plan);
        }
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut nodes: Vec<_> = plan.reach.nodes.iter().copied().collect();
        nodes.sort_unstable();
        let mut total = 0;
        // Shared source map, copied only once after the entire text plan passes.
        for &id in &nodes {
            control.check()?;
            let mut count = Sink::count(cap.text);
            address(&mut count, plan.namespace, self.nodes[id].resource, id)
                .map_err(|_| text_limit())?;
            add_bytes(&mut total, count.size, cap.text)?;
            add_bytes(&mut total, self.location_bytes(id)?, cap.text)?;
            groups.entry(self.nodes[id].resource).or_default().push(id);
        }
        if let Some(id) = plan.first_hole {
            add_bytes(&mut total, self.location_bytes(id)?, cap.text)?;
            add_bytes(
                &mut total,
                "/$ref".len() + UNAVAILABLE_CODE.len() + UNAVAILABLE_MESSAGE.len(),
                cap.text,
            )?;
        }
        let mut sizes = [Vec::new(), Vec::new()];
        for (pass, lower) in [true, false].into_iter().enumerate() {
            let mut entry_count = Sink::count(cap.text);
            resource_uri(&mut entry_count, plan.namespace, self.nodes[entry].resource)
                .map_err(|_| text_limit())?;
            add_bytes(&mut total, entry_count.size, cap.text)?;
            for (&resource, ids) in &groups {
                let mut count = Sink::count(cap.text - total);
                self.write_resource(&mut count, resource, ids, entry, &plan, control)
                    .map_err(|_| control.check().err().unwrap_or_else(text_limit))?;
                add_bytes(&mut total, count.size + count.decoded, cap.text)?;
                sizes[pass].push((count.size, count.decoded));
                let mut uri_count = Sink::count(cap.text);
                resource_uri(&mut uri_count, plan.namespace, resource).map_err(|_| text_limit())?;
                add_bytes(&mut total, uri_count.size, cap.text)?;
            }
            if !plan.holes.is_empty() {
                let text = if lower { "false" } else { "true" };
                add_bytes(
                    &mut total,
                    text.len() + hole_uri_size(plan.namespace),
                    cap.text,
                )?;
            }
        }
        control.check()?;
        let mut locations = BTreeMap::new();
        for id in nodes {
            control.check()?;
            let mut uri = String::new();
            address(&mut uri, plan.namespace, self.nodes[id].resource, id).expect("String writer");
            locations.insert(uri, ProgramOrigin::Prefix(self.location(id)));
        }
        let locations = Arc::new(locations);
        let unavailable = plan.first_hole.map(|id| {
            let mut location = self.location(id);
            location.pointer.push_str("/$ref");
            NoVerdict::new(
                NoVerdictReason::ResourceUnavailable,
                UNAVAILABLE_CODE,
                UNAVAILABLE_MESSAGE,
            )
            .located(location)
        });
        let mut programs = Vec::with_capacity(2);
        for (pass, lower) in [true, false].into_iter().enumerate() {
            let mut resources =
                Vec::with_capacity(groups.len() + usize::from(!plan.holes.is_empty()));
            for ((&resource, ids), &(size, decoded)) in groups.iter().zip(&sizes[pass]) {
                control.check()?;
                let mut sink = Sink::output(size, decoded);
                self.write_resource(&mut sink, resource, ids, entry, &plan, control)
                    .map_err(|_| control.check().err().unwrap_or_else(text_limit))?;
                let mut uri = String::new();
                resource_uri(&mut uri, plan.namespace, resource).expect("String writer");
                let document = JsonValue::parse(sink.text.expect("output sink")).map_err(|_| {
                    limit(
                        "program-json-limit",
                        "projected resource exceeds JSON admission",
                    )
                })?;
                resources.push(SchemaResource { uri, document });
            }
            if !plan.holes.is_empty() {
                resources.push(SchemaResource {
                    uri: format!("{NAMESPACE}{}/hole", plan.namespace),
                    document: JsonValue::parse(if lower { "false" } else { "true" })
                        .expect("boolean JSON"),
                });
            }
            let mut entry_uri = String::new();
            resource_uri(&mut entry_uri, plan.namespace, self.nodes[entry].resource)
                .expect("String writer");
            programs.push(EvaluationProgram {
                entry_uri,
                resources,
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
    fn write_resource(
        &self,
        sink: &mut Sink,
        resource: usize,
        ids: &[usize],
        entry: usize,
        plan: &Plan,
        control: &WorkControl,
    ) -> fmt::Result {
        sink.write_str("{\"$id\":\"")?;
        resource_uri(sink, plan.namespace, resource)?;
        sink.write_str("\",\"$schema\":")?;
        quote(sink, DIALECT)?;
        sink.write_str(",\"$defs\":{")?;
        for (index, &id) in ids.iter().enumerate() {
            if control.is_cancelled() {
                return Err(fmt::Error);
            }
            if index != 0 {
                sink.write_char(',')?;
            }
            write!(sink, "\"n{id}\":")?;
            self.write_node(sink, id, plan)?;
        }
        sink.write_char('}')?;
        if resource == self.nodes[entry].resource {
            sink.write_str(",\"$ref\":\"")?;
            address(sink, plan.namespace, resource, entry)?;
            sink.write_char('"')?;
        }
        sink.write_char('}')
    }
    fn write_child(
        &self,
        sink: &mut Sink,
        namespace: usize,
        source: usize,
        value: JsonRef<'_>,
    ) -> fmt::Result {
        let target = self.sources[source].positions[&node_id(&value.to_owned())];
        sink.write_str("{\"$ref\":\"")?;
        address(sink, namespace, self.nodes[target].resource, target)?;
        sink.write_str("\"}")
    }
    fn write_node(&self, sink: &mut Sink, id: usize, plan: &Plan) -> fmt::Result {
        let node = &self.nodes[id];
        if node.value.kind() == JsonKind::Boolean {
            return sink.write_str(node.value.text());
        }
        sink.write_char('{')?;
        let mut first = true;
        let has_if = node.value.get("if").is_some();
        for member in node.value.view().members().expect("checked object") {
            let keyword = member.name.as_str().expect("checked Unicode");
            if matches!(
                keyword,
                "$id"
                    | "$schema"
                    | "$defs"
                    | "definitions"
                    | "dependencies"
                    | "contentSchema"
                    | "$anchor"
            ) || (matches!(keyword, "then" | "else") && !has_if)
            {
                continue;
            }
            if !first {
                sink.write_char(',')?;
            }
            first = false;
            quote(sink, keyword)?;
            sink.write_char(':')?;
            if keyword == "$ref" {
                sink.write_char('"')?;
                if plan.holes.contains(&id) {
                    write!(sink, "{NAMESPACE}{}/hole", plan.namespace)?;
                } else {
                    let target = &plan.reach.targets[&(id, "$ref".into())];
                    address(
                        sink,
                        plan.namespace,
                        self.nodes[target.node].resource,
                        target.node,
                    )?;
                }
                sink.write_char('"')?;
            } else if let Some(policy) = policy(keyword, has_if) {
                match policy.shape {
                    Children::Map => {
                        sink.write_char('{')?;
                        for (i, m) in member.value.members().expect("checked map").enumerate() {
                            if i != 0 {
                                sink.write_char(',')?;
                            }
                            quote(sink, m.name.as_str().expect("checked Unicode"))?;
                            sink.write_char(':')?;
                            self.write_child(sink, plan.namespace, node.source, m.value)?;
                        }
                        sink.write_char('}')?;
                    }
                    Children::List => {
                        sink.write_char('[')?;
                        for (i, v) in member.value.elements().expect("checked list").enumerate() {
                            if i != 0 {
                                sink.write_char(',')?;
                            }
                            self.write_child(sink, plan.namespace, node.source, v)?;
                        }
                        sink.write_char(']')?;
                    }
                    Children::Single => {
                        self.write_child(sink, plan.namespace, node.source, member.value)?
                    }
                }
            } else {
                sink.raw(member.value)?;
            }
        }
        sink.write_char('}')
    }
}
fn private_namespace(name: &str) -> Option<usize> {
    let (suffix, _) = name.strip_prefix(NAMESPACE)?.split_once('/')?;
    if suffix.len() > 1 && suffix.starts_with('0') {
        return None;
    }
    if !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}
fn hole_uri_size(suffix: usize) -> usize {
    NAMESPACE.len()
        + "/hole".len()
        + if suffix == 0 {
            1
        } else {
            suffix.ilog10() as usize + 1
        }
}
fn valid_unknown_fragment(fragment: &str) -> bool {
    if fragment.is_empty() {
        return true;
    }
    if !fragment.starts_with('/') {
        return crate::schema_index::plain_name(fragment);
    }
    let mut bytes = fragment.bytes();
    while let Some(b) = bytes.next() {
        if b == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
            return false;
        }
    }
    true
}
fn resource_uri(sink: &mut impl Write, namespace: usize, resource: usize) -> fmt::Result {
    write!(sink, "{NAMESPACE}{namespace}/r{resource}")
}
fn address(sink: &mut impl Write, namespace: usize, resource: usize, node: usize) -> fmt::Result {
    resource_uri(sink, namespace, resource)?;
    write!(sink, "#/$defs/n{node}")
}
fn quote(sink: &mut Sink, text: &str) -> fmt::Result {
    // The parser owns a complete decoded string if any emitted character is
    // escaped. Count the actual output spelling, not the original token spelling.
    if text.bytes().any(|b| b < b' ' || matches!(b, b'"' | b'\\')) {
        sink.add_decoded(text.len())?;
    }
    sink.write_char('"')?;
    for c in text.chars() {
        match c {
            '"' => sink.write_str("\\\"")?,
            '\\' => sink.write_str("\\\\")?,
            '\n' => sink.write_str("\\n")?,
            '\r' => sink.write_str("\\r")?,
            '\t' => sink.write_str("\\t")?,
            '\u{8}' => sink.write_str("\\b")?,
            '\u{c}' => sink.write_str("\\f")?,
            c if c < ' ' => write!(sink, "\\u{:04x}", c as u32)?,
            _ => sink.write_char(c)?,
        }
    }
    sink.write_char('"')
}
// Count both arena source and separately owned decoded strings without copying.
// Allocate the serialized buffer only after the complete paired plan is admitted.
struct Sink {
    text: Option<String>,
    size: usize,
    decoded: usize,
    cap: usize,
}
impl Sink {
    fn count(cap: usize) -> Self {
        Self {
            text: None,
            size: 0,
            decoded: 0,
            cap,
        }
    }
    fn output(size: usize, decoded: usize) -> Self {
        Self {
            text: Some(String::with_capacity(size)),
            size: 0,
            decoded: 0,
            cap: size + decoded,
        }
    }
    fn add_decoded(&mut self, bytes: usize) -> fmt::Result {
        self.decoded = self
            .decoded
            .checked_add(bytes)
            .filter(|n| *n <= self.cap - self.size)
            .ok_or(fmt::Error)?;
        Ok(())
    }
    fn raw(&mut self, value: JsonRef<'_>) -> fmt::Result {
        let remaining = (self.cap - self.size - self.decoded)
            .checked_sub(value.text().len())
            .ok_or(fmt::Error)?;
        // Unchanged subtrees retain identical encoded tokens. Borrow the existing
        // flat parser's decoded storage lengths, including nested member names.
        let decoded = decoded_string_size(value, remaining).ok_or(fmt::Error)?;
        self.add_decoded(decoded)?;
        self.write_str(value.text())
    }
}
impl Write for Sink {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let size = self
            .size
            .checked_add(value.len())
            .filter(|n| *n <= self.cap - self.decoded)
            .ok_or(fmt::Error)?;
        if let Some(text) = &mut self.text {
            text.push_str(value);
        }
        self.size = size;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn space(schema: &str) -> (SchemaSpace, usize) {
        let document = ParsedDocument::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
        ))
        .unwrap();
        let space = SchemaSpace::new(document, ResourceSet::default());
        let entry = space
            .document_node(
                &space
                    .document
                    .value()
                    .at("/operations/op/input")
                    .unwrap()
                    .to_owned(),
            )
            .unwrap();
        (space, entry)
    }
    pub(super) fn text_size(bounds: &EvaluationBounds) -> usize {
        let mut size = 0;
        for program in [&bounds.lower, &bounds.upper] {
            size += program.entry_uri.len();
            for resource in &program.resources {
                size += resource.uri.len() + resource.document.text().len();
                size += decoded_string_size(resource.document.view(), usize::MAX).unwrap();
            }
        }
        assert!(Arc::ptr_eq(
            &bounds.lower.locations,
            &bounds.upper.locations
        ));
        for (generated, origin) in bounds.lower.locations.iter() {
            size += generated.len();
            if let ProgramOrigin::Prefix(at) | ProgramOrigin::Exact(at) = origin {
                size += at.pointer.len() + at.resource.as_ref().map_or(0, String::len);
            }
        }
        if let Some(e) = &bounds.unavailable {
            size += e.code.len() + e.message.len();
            if let Some(at) = &e.location {
                size += at.pointer.len() + at.resource.as_ref().map_or(0, String::len);
            }
        }
        size
    }
    #[test]
    fn exact_combined_text_admission_includes_wrappers_entries_maps_and_evidence() {
        for schema in [
            r#"{"properties":{"quote\"/tilde~雪":{"$ref":"https://absent.invalid/U"}},"description":"line\nline"}"#,
            r#"{"$ref":"https://absent.invalid/U","const":{"nested\n":["\u0061","\uD83D\uDE00"]},"description":"escaped\""}"#,
            r#"{"\u0070roperties":{"\u0061":{"$ref":"https://absent.invalid/U"},"x\"\\\n":true},"x\u002dopaque":{"\u006b":["raw雪","\uD83D\uDE00",{"q\"":"a\/b"}]}}"#,
            r#"{"type":"number","const":0.290000000000000000001}"#,
            "true",
        ] {
            let (space, entry) = space(schema);
            let bounds = space.bounds(entry, &WorkControl::new()).unwrap();
            let size = text_size(&bounds);
            let cap = Admission {
                text: size,
                ..Admission::default()
            };
            assert_eq!(
                text_size(
                    &space
                        .bounds_admitted(entry, &WorkControl::new(), cap)
                        .unwrap()
                ),
                size
            );
            let refused = space
                .bounds_admitted(
                    entry,
                    &WorkControl::new(),
                    Admission {
                        text: size - 1,
                        ..cap
                    },
                )
                .unwrap_err();
            assert_eq!(refused.code, "partial-program-byte-limit");
            if schema == "true" {
                assert!(bounds.unavailable.is_none());
            }
        }
    }
    #[test]
    fn decoded_projection_storage_uses_actual_emitted_spelling_before_copying() {
        for (source, decoded) in [
            (r#""raw雪""#, 0),
            (r#""\u0061""#, 1),
            (r#""a\/b""#, 3),
            (r#""\uD83D\uDE00""#, 4),
            (r#"{"k\n":["x\"",{"\u0061":"z\\"}]}"#, 7),
        ] {
            let value = JsonValue::parse(source).unwrap();
            let total = source.len() + decoded;
            let mut count = Sink::count(total);
            count.raw(value.view()).unwrap();
            assert_eq!((count.size, count.decoded), (source.len(), decoded));
            assert!(count.text.is_none());
            let mut refused = Sink::count(total - 1);
            assert!(refused.raw(value.view()).is_err());
            assert_eq!((refused.size, refused.decoded), (0, 0));
            assert!(refused.text.is_none());
            let mut output = Sink::output(count.size, count.decoded);
            output.raw(value.view()).unwrap();
            let parsed = JsonValue::parse(output.text.unwrap()).unwrap();
            assert_eq!(decoded_string_size(parsed.view(), decoded), Some(decoded));
            if let Some(text) = value.view().as_str() {
                let mut rewritten = Sink::count(1024);
                quote(&mut rewritten, text).unwrap();
                let mut output = Sink::output(rewritten.size, rewritten.decoded);
                quote(&mut output, text).unwrap();
                let parsed = JsonValue::parse(output.text.unwrap()).unwrap();
                assert_eq!(
                    decoded_string_size(parsed.view(), usize::MAX),
                    Some(rewritten.decoded)
                );
                // Rewriting unnecessary Unicode/slash escapes does not retain a
                // decoded copy, while copying the original raw token above does.
                assert_eq!(rewritten.decoded, 0);
            }
        }
        for text in ["quote\"", "back\\slash", "\n\t\r\u{8}\u{c}\0雪"] {
            let mut count = Sink::count(1024);
            quote(&mut count, text).unwrap();
            assert_eq!(count.decoded, text.len());
            let mut output = Sink::output(count.size, count.decoded);
            quote(&mut output, text).unwrap();
            let parsed = JsonValue::parse(output.text.unwrap()).unwrap();
            assert_eq!(
                decoded_string_size(parsed.view(), usize::MAX),
                Some(text.len())
            );
            let mut refused = Sink::count(count.size + count.decoded - 1);
            assert!(quote(&mut refused, text).is_err());
            assert!(refused.text.is_none());
        }
    }
    #[test]
    #[ignore = "real 64 MiB admission boundary; run explicitly under an external memory ceiling"]
    fn escaped_text_real_64_mib_boundary_and_frozen_twenty_mib_witness() {
        let witness = |n: usize| {
            space(&format!(
                r#"{{"description":"{}\"","properties":{{"x":{{"$ref":"https://sol-review.invalid/U"}}}}}}"#,
                "a".repeat(n)
            ))
        };
        let (small, entry) = witness(0);
        let base = text_size(&small.bounds(entry, &WorkControl::new()).unwrap());
        assert_eq!(base, 1273); // Frozen independent review witness.
        let largest = (TEXT_LIMIT - base) / 4;
        for (n, admitted) in [
            (largest, true),
            (largest + 1, false),
            (20 * 1024 * 1024, false),
        ] {
            let (space, entry) = witness(n);
            let expected = base + 4 * n;
            let result = space.bounds(entry, &WorkControl::new());
            if admitted {
                let actual = text_size(&result.unwrap());
                assert_eq!(actual, expected);
                assert!(actual <= TEXT_LIMIT);
                println!("repeat={n}; retained_utf8={actual}; admitted");
            } else {
                assert!(expected > TEXT_LIMIT);
                assert_eq!(result.unwrap_err().code, "partial-program-byte-limit");
                println!("repeat={n}; retained_utf8_would_be={expected}; refused");
            }
        }
    }
    #[test]
    fn graph_admission_precedes_each_append_including_shared_and_hole_edges() {
        assert_eq!(
            (NODE_LIMIT, EDGE_LIMIT, HOLE_LIMIT, TEXT_LIMIT),
            (100_000, 200_000, 100_000, 64 * 1024 * 1024)
        );
        let (space, entry) = space(
            r##"{"$id":"https://review.invalid/root","$defs":{"shared":{"$ref":"https://absent.invalid/U"}},"allOf":[{"$ref":"#/$defs/shared"},{"$ref":"#/$defs/shared"}]}"##,
        );
        let control = WorkControl::new();
        let plan = space
            .partial_reach(entry, &control, Admission::default())
            .unwrap();
        assert_eq!(plan.reach.nodes.len(), 4);
        assert_eq!(plan.edge_count, 5);
        assert_eq!(plan.holes.len(), 1);
        for (cap, code) in [
            (
                Admission {
                    nodes: 3,
                    ..Admission::default()
                },
                "schema-node-limit",
            ),
            (
                Admission {
                    edges: 4,
                    ..Admission::default()
                },
                "schema-edge-limit",
            ),
            (
                Admission {
                    holes: 0,
                    ..Admission::default()
                },
                "schema-hole-limit",
            ),
        ] {
            assert_eq!(
                space
                    .bounds_admitted(entry, &control, cap)
                    .unwrap_err()
                    .code,
                code
            );
        }
        assert!(
            space
                .bounds_admitted(
                    entry,
                    &control,
                    Admission {
                        nodes: 4,
                        edges: 5,
                        holes: 1,
                        ..Admission::default()
                    }
                )
                .is_ok()
        );
    }
    #[test]
    fn generated_resources_never_use_an_original_missing_carrier_identity() {
        let refs = (0..12)
            .map(|i| {
                let uri = if i % 2 == 0 {
                    format!("{NAMESPACE}{i}/r0#missing")
                } else {
                    format!("{i}/hole#/$defs/unknown")
                };
                format!(r#"{{"$ref":"{uri}"}}"#)
            })
            .collect::<Vec<_>>()
            .join(",");
        let schema = format!(r#"{{"$id":"{NAMESPACE}base","anyOf":[true,{refs}]}}"#);
        let (space, entry) = space(&schema);
        let bounds = space.bounds(entry, &WorkControl::new()).unwrap();
        for program in [&bounds.lower, &bounds.upper] {
            for resource in &program.resources {
                for i in 0..12 {
                    assert!(!resource.uri.starts_with(&format!("{NAMESPACE}{i}/")));
                }
            }
            let constant = program
                .resources
                .iter()
                .find(|r| r.document.kind() == JsonKind::Boolean)
                .unwrap();
            assert_eq!(constant.uri, format!("{NAMESPACE}12/hole"));
            assert!(program.original_location(&constant.uri).is_none());
        }
        let size = text_size(&bounds);
        assert!(
            space
                .bounds_admitted(
                    entry,
                    &WorkControl::new(),
                    Admission {
                        text: size,
                        ..Admission::default()
                    }
                )
                .is_ok()
        );
        assert!(
            space
                .bounds_admitted(
                    entry,
                    &WorkControl::new(),
                    Admission {
                        text: size - 1,
                        ..Admission::default()
                    }
                )
                .is_err()
        );
    }
    #[test]
    fn count_sink_and_separate_scratch_guard_refuse_before_copying() {
        let mut count = Sink::count(3);
        assert!(count.write_str("four").is_err());
        assert_eq!(count.size, 0);
        assert!(count.text.is_none());
        let (space, entry) = space("true");
        assert_eq!(
            space
                .bounds_scratch(entry, &"x".repeat(TEXT_LIMIT + 1))
                .unwrap_err()
                .code,
            "partial-scratch-limit"
        );
    }
    #[test]
    fn wide_shared_graph_is_finite_and_keeps_all_influence_paths() {
        let refs = vec![r##"{"$ref":"#/$defs/shared"}"##; 2000].join(",");
        let schema = format!(
            r##"{{"$id":"https://review.invalid/root","$defs":{{"shared":{{"$ref":"https://absent.invalid/U"}}}},"anyOf":[true,{refs}]}}"##
        );
        let (space, entry) = space(&schema);
        let plan = space
            .partial_reach(entry, &WorkControl::new(), Admission::default())
            .unwrap();
        assert_eq!(plan.holes.len(), 1);
        assert_eq!(plan.reach.nodes.len(), 2003);
        assert_eq!(plan.edge_count, 4002);
        assert!(space.bounds(entry, &WorkControl::new()).is_ok());
    }
}
