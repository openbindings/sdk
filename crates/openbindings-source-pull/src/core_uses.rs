//! Check dependencies visible in core schemas without interpreting kind data.
use crate::{
    PullRequest, PullResult, PullValidationError as Failure, RelationshipError, budget::Attempt,
};
use openbindings::{ConformantDocument, Document, Edit, JsonRef, ReferenceResolution};
use std::ops::Range;

fn label(pointer: &str, attempt: &mut Attempt<'_>) -> Result<Option<String>, Failure> {
    attempt.work(pointer.len().saturating_add(1))?;
    let mut parts = pointer.split('/').skip(1);
    let (Some(map), Some(key)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };
    if !matches!(map, "operations" | "schemas") {
        return Ok(None);
    }
    attempt.allocate(key.len().saturating_mul(4).saturating_add(32))?;
    Ok(Some(format!(
        "{map}/{}",
        key.replace("~1", "/").replace("~0", "~")
    )))
}

// Sorted vectors keep the per-entry sets small without reserving tree nodes for
// every label. A sort runs once over text the size limits already bound, so it
// spends comparisons. Searches repeat per reached edge, so they spend key bytes.
fn sort_work(count: usize, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
    let levels = (usize::BITS - count.leading_zeros()) as usize;
    attempt.work(count.saturating_mul(2 * levels))
}
fn search_work(count: usize, key: &str, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
    let levels = (usize::BITS - count.leading_zeros()) as usize + 1;
    attempt.work(key.len().saturating_add(1).saturating_mul(levels))
}
fn labels<'a>(value: JsonRef<'a>, attempt: &mut Attempt<'_>) -> Result<Vec<&'a str>, Failure> {
    let uses = value.get("uses");
    let mut result = attempt.vector(uses.and_then(|v| v.len()).unwrap_or(0) + 1)?;
    for name in uses.into_iter().flat_map(|v| v.elements().expect("uses")) {
        result.push(name.as_str().ok_or(RelationshipError::Identifier)?);
    }
    sort_work(result.len(), attempt)?;
    result.sort_unstable();
    Ok(result)
}
fn contains(
    labels: &[impl AsRef<str>],
    name: &str,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    search_work(labels.len(), name, attempt)?;
    Ok(labels
        .binary_search_by(|label| label.as_ref().cmp(name))
        .is_ok())
}
fn additions(value: JsonRef<'_>, attempt: &mut Attempt<'_>) -> Result<Vec<String>, Failure> {
    let count = ["operations", "schemas"]
        .into_iter()
        .filter_map(|map| value.get(map).and_then(|v| v.len()))
        .sum();
    let mut labels = attempt.vector(count)?;
    for map in ["operations", "schemas"] {
        for entry in value
            .get(map)
            .into_iter()
            .flat_map(|v| v.members().expect("map"))
        {
            let key = entry.name.as_str().ok_or(RelationshipError::Identifier)?;
            attempt.allocate(map.len().saturating_add(key.len()).saturating_add(1))?;
            labels.push(format!("{map}/{key}"));
        }
    }
    sort_work(labels.len(), attempt)?;
    labels.sort_unstable();
    Ok(labels)
}

struct Edge {
    pointer: String,
    to: String,
    target: String,
}
struct Graph {
    edges: Vec<Edge>,
}
impl Graph {
    fn inspect(document: &Document, attempt: &mut Attempt<'_>) -> Result<Self, Failure> {
        let report =
            document.references_with_resources(openbindings::ExternalSchemas::default(), attempt);
        attempt.check()?;
        if !report.complete {
            return Err(Failure::LimitExceeded("reference-inspection"));
        }
        let mut edges = attempt.vector(report.references.len())?;
        for reference in report.references {
            let ReferenceResolution::Located { target, .. } = reference.resolution else {
                continue;
            };
            if target.source_uri.is_some() {
                continue;
            }
            let pointer = reference
                .location
                .pointer
                .as_str()
                .ok_or(RelationshipError::Identifier)?;
            let target = target
                .pointer
                .as_str()
                .ok_or(RelationshipError::Identifier)?;
            if let Some(to) = label(target, attempt)? {
                attempt.allocate(pointer.len().saturating_add(target.len()))?;
                edges.push(Edge {
                    pointer: pointer.to_owned(),
                    to,
                    target: target.to_owned(),
                });
            }
        }
        sort_work(edges.len(), attempt)?;
        edges.sort_unstable_by(|a, b| a.pointer.cmp(&b.pointer));
        Ok(Self { edges })
    }
    fn range(&self, root: &str, attempt: &mut Attempt<'_>) -> Result<Range<usize>, Failure> {
        if self.edges.is_empty() {
            attempt.check()?;
            return Ok(0..0);
        }
        search_work(self.edges.len(), root, attempt)?;
        search_work(self.edges.len(), root, attempt)?;
        attempt.allocate(root.len().saturating_add(1))?;
        let prefix = format!("{root}/");
        let start = self.edges.partition_point(|edge| edge.pointer < prefix);
        let end =
            start + self.edges[start..].partition_point(|edge| edge.pointer.starts_with(&prefix));
        Ok(start..end)
    }
}
// Origins in this proposal's slots come from the edited snapshot. All other
// origins come from the additions-only snapshot, even when another proposal in
// the same batch replaces them. Clip ranges before walking so discarded content
// costs no per-entry scan and a cycle back to an owned slot uses the proposal.
struct Overlay<'a> {
    graphs: [&'a Graph; 2],
    slots: [[Range<usize>; 2]; 2],
}
impl<'a> Overlay<'a> {
    fn new(
        graphs: [&'a Graph; 2],
        seeds: &[String],
        attempt: &mut Attempt<'_>,
    ) -> Result<Self, Failure> {
        let mut slots = [[0..0, 0..0], [0..0, 0..0]];
        for (graph, ranges) in graphs.iter().zip(&mut slots) {
            for (seed, range) in seeds.iter().zip(ranges.iter_mut()) {
                *range = graph.range(seed, attempt)?;
            }
            ranges.sort_unstable_by_key(|range| range.start);
        }
        Ok(Self { graphs, slots })
    }
    fn edge(&self, at: usize) -> &Edge {
        if at < self.graphs[0].edges.len() {
            &self.graphs[0].edges[at]
        } else {
            &self.graphs[1].edges[at - self.graphs[0].edges.len()]
        }
    }
}

// The successor links skip already visited ranges, including overlapping schema
// subtrees. Epochs reuse storage across entries without clearing a document-sized
// array for each entry. Every edge is queued at most once per entry.
struct Walk {
    seen: Vec<usize>,
    next: Vec<usize>,
    pending: Vec<usize>,
    epoch: usize,
}
impl Walk {
    fn new(count: usize, attempt: &mut Attempt<'_>) -> Result<Self, Failure> {
        let mut seen = attempt.vector(count + 1)?;
        let mut next = attempt.vector(count + 1)?;
        attempt.work(count.saturating_mul(2))?;
        seen.resize(count + 1, 0);
        next.resize(count + 1, 0);
        Ok(Self {
            seen,
            next,
            pending: attempt.vector(count)?,
            epoch: 0,
        })
    }
    fn unvisited(&mut self, mut at: usize, attempt: &mut Attempt<'_>) -> Result<usize, Failure> {
        let start = at;
        while self.seen[at] == self.epoch {
            attempt.work(1)?;
            at = self.next[at];
        }
        let mut previous = start;
        while previous != at {
            attempt.work(1)?;
            let next = self.next[previous];
            self.next[previous] = at;
            previous = next;
        }
        Ok(at)
    }
    fn enqueue(&mut self, range: Range<usize>, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        let mut at = self.unvisited(range.start, attempt)?;
        while at < range.end {
            attempt.work(1)?;
            self.pending.push(at);
            self.seen[at] = self.epoch;
            self.next[at] = at + 1;
            at = self.unvisited(at + 1, attempt)?;
        }
        Ok(())
    }
    fn reach(
        &mut self,
        view: &Overlay<'_>,
        root: &str,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        let mut original = view.graphs[0].range(root, attempt)?;
        for slot in &view.slots[0] {
            self.enqueue(original.start..original.end.min(slot.start), attempt)?;
            original.start = original.end.min(original.start.max(slot.end));
        }
        self.enqueue(original, attempt)?;
        let proposed = view.graphs[1].range(root, attempt)?;
        let offset = view.graphs[0].edges.len();
        for slot in &view.slots[1] {
            self.enqueue(
                offset + proposed.start.max(slot.start)..offset + proposed.end.min(slot.end),
                attempt,
            )?;
        }
        Ok(())
    }
    fn check(
        &mut self,
        graphs: [&Graph; 2],
        retained: &[&str],
        seeds: &[String],
        added: &[String],
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        self.epoch += 1;
        self.pending.clear();
        let view = Overlay::new(graphs, seeds, attempt)?;
        for name in retained {
            let (map, key) = name.split_once('/').expect("checked uses");
            let root = pointer(map, key, None, attempt)?;
            self.reach(&view, &root, attempt)?;
        }
        // The overlay already located each proposed seed. Queue those ranges
        // directly, without searching again or inspecting the replaced ranges.
        let offset = graphs[0].edges.len();
        for range in &view.slots[1] {
            self.enqueue(offset + range.start..offset + range.end, attempt)?;
        }
        while let Some(at) = self.pending.pop() {
            attempt.work(1)?;
            let edge = view.edge(at);
            if contains(added, &edge.to, attempt)? {
                if !contains(retained, &edge.to, attempt)? {
                    return Err(RelationshipError::UsesClosure.into());
                }
                // Every retained addition was seeded in full above.
            } else {
                self.reach(&view, &edge.target, attempt)?;
            }
        }
        Ok(())
    }
}
fn pointer(
    map: &str,
    key: &str,
    slot: Option<&str>,
    attempt: &mut Attempt<'_>,
) -> Result<String, Failure> {
    attempt.work(key.len().saturating_add(1))?;
    attempt.allocate(key.len().saturating_mul(8).saturating_add(64))?;
    let mut path = format!("/{map}/{}", key.replace('~', "~0").replace('/', "~1"));
    if let Some(slot) = slot {
        path.push('/');
        path.push_str(slot);
    }
    Ok(path)
}

struct Proposal<'a> {
    retained: Vec<&'a str>,
    seeds: Vec<String>,
    edits: Vec<Edit>,
    bytes: usize,
    batch: usize,
}
fn proposals<'a>(
    request: &PullRequest,
    report: JsonRef<'a>,
    attempt: &mut Attempt<'_>,
) -> Result<Vec<Proposal<'a>>, Failure> {
    let entries = report.get("existing");
    let mut proposals = attempt.vector(entries.and_then(|v| v.len()).unwrap_or(0))?;
    for entry in entries
        .into_iter()
        .flat_map(|v| v.elements().expect("existing"))
    {
        let Some(value) = entry.get("proposal") else {
            continue;
        };
        let mut proposal = Proposal {
            retained: labels(value, attempt)?,
            seeds: attempt.vector(2)?,
            edits: attempt.vector(2)?,
            bytes: 0,
            batch: 0,
        };
        if let Some(name) = entry.get("schema").and_then(|v| v.as_str()) {
            proposal.push(
                pointer("schemas", name, None, attempt)?,
                value.get("schema").expect("schema proposal"),
                attempt,
            )?;
        } else if let Some(name) = entry.get("binding").and_then(|v| v.as_str()) {
            let bindings = request
                .interface()
                .value()
                .get("bindings")
                .expect("bindings");
            search_work(bindings.len().expect("bindings"), name, attempt)?;
            let binding = bindings.get(name).expect("checked binding");
            let operation = binding
                .get("operation")
                .and_then(|v| v.as_str())
                .expect("operation");
            for slot in ["input", "output"] {
                if let Some(schema) = value.get(slot) {
                    proposal.push(
                        pointer("operations", operation, Some(slot), attempt)?,
                        schema,
                        attempt,
                    )?;
                }
            }
        } else {
            continue;
        }
        proposals.push(proposal);
    }
    Ok(proposals)
}
impl Proposal<'_> {
    fn push(
        &mut self,
        path: String,
        value: JsonRef<'_>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        attempt.allocate(path.len())?;
        self.bytes = self.bytes.saturating_add(value.text().len());
        self.edits.push(Edit::set_at(&path, value.to_owned()));
        self.seeds.push(path);
        Ok(())
    }
}

fn assign_batches(
    proposals: &mut [Proposal<'_>],
    attempt: &mut Attempt<'_>,
) -> Result<usize, Failure> {
    // Each slot records its next free round. A proposal occupies one round for
    // both slots, so input/output overlaps cannot reintroduce an edit collision.
    let mut slots = attempt.vector(proposals.len().saturating_mul(2))?;
    for proposal in proposals.iter() {
        for path in &proposal.seeds {
            attempt.allocate(path.len())?;
            slots.push(path.clone());
        }
    }
    sort_work(slots.len(), attempt)?;
    slots.sort_unstable();
    slots.dedup();
    let mut next = attempt.vector(slots.len())?;
    next.resize(slots.len(), 0usize);
    let mut count = 0;
    for proposal in proposals {
        let mut positions = [0; 2];
        for (position, path) in positions.iter_mut().zip(&proposal.seeds) {
            search_work(slots.len(), path, attempt)?;
            *position = slots.binary_search(path).expect("indexed slot");
            proposal.batch = proposal.batch.max(next[*position]);
        }
        for position in &positions[..proposal.seeds.len()] {
            next[*position] = proposal.batch + 1;
        }
        count = count.max(proposal.batch + 1);
    }
    Ok(count)
}

fn check_added(
    report: JsonRef<'_>,
    base: &Graph,
    added: &[String],
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    let empty = Graph { edges: Vec::new() };
    let mut walk = Walk::new(base.edges.len(), attempt)?;
    for entry in report
        .get("added")
        .expect("added")
        .elements()
        .expect("added array")
    {
        let mut retained = labels(entry, attempt)?;
        let operation = entry
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or(RelationshipError::Identifier)?;
        attempt.allocate(operation.len().saturating_add(11))?;
        let own = format!("operations/{operation}");
        search_work(retained.len(), &own, attempt)?;
        if let Err(at) = retained.binary_search(&own.as_str()) {
            attempt.work(retained.len())?;
            retained.insert(at, &own);
        }
        walk.check([base, &empty], &retained, &[], added, attempt)?;
    }
    Ok(())
}

fn check_batch(
    batch: Vec<Proposal<'_>>,
    repeated: bool,
    document: &Document,
    base: &Graph,
    added: &[String],
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    let mut edits = attempt.vector(batch.len().saturating_mul(2))?;
    let mut bytes = document.as_bytes().len();
    for proposal in &batch {
        bytes = bytes.saturating_add(proposal.bytes);
        for (edit, path) in proposal.edits.iter().zip(&proposal.seeds) {
            attempt.allocate(path.len())?;
            edits.push(edit.clone());
        }
    }
    sort_work(edits.len(), attempt)?;
    // The first batch is one of a pull's fixed core passes. Only colliding
    // proposals add rounds, so a result can multiply them: those spend the ledger
    // for the edit and the inspection that follows it.
    if repeated {
        attempt.work(bytes.saturating_mul(2))?;
    }
    let proposed = document.edit(&edits).map_err(Failure::Edit)?;
    attempt.check()?;
    let changed = Graph::inspect(&proposed, attempt)?;
    let mut walk = Walk::new(
        base.edges.len().saturating_add(changed.edges.len()),
        attempt,
    )?;
    for proposal in batch {
        walk.check(
            [base, &changed],
            &proposal.retained,
            &proposal.seeds,
            added,
            attempt,
        )?;
    }
    Ok(())
}

fn check_proposals(
    mut proposals: Vec<Proposal<'_>>,
    document: &Document,
    base: &Graph,
    added: &[String],
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    let count = assign_batches(&mut proposals, attempt)?;
    let mut batches = attempt.vector(count)?;
    for _ in 0..count {
        batches.push(Vec::new());
    }
    for proposal in proposals {
        attempt.allocate(std::mem::size_of::<Proposal<'_>>().saturating_mul(4))?;
        batches[proposal.batch].push(proposal);
    }
    for (round, batch) in batches.into_iter().enumerate() {
        check_batch(batch, round > 0, document, base, added, attempt)?;
    }
    Ok(())
}

pub(crate) fn check(
    request: &PullRequest,
    result: &PullResult,
    document: &ConformantDocument,
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    let raw = result.value().value();
    let added = additions(raw.get("additions").expect("additions"), attempt)?;
    let report = raw.get("report").expect("report");
    let base = Graph::inspect(document.document(), attempt)?;
    check_added(report, &base, &added, attempt)?;
    let proposals = proposals(request, report, attempt)?;
    check_proposals(proposals, document.document(), &base, &added, attempt)?;
    attempt.check()
}
