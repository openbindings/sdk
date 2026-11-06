//! Keep chosen added bindings and what they reach.
//!
//! Lookups use sorted vectors so that no choice of keys can degrade them: each
//! search costs what the ledger charges for it, on every target.
use crate::{
    PullLimits, PullRequest, PullResult, PullValidationError as Failure, RelationshipError,
    budget::Attempt, validation,
};
use openbindings::{
    ConformantDocument, JsonRef, ReferenceReport, ReferenceResolution, WorkControl,
};

impl PullResult {
    /// Keep chosen added bindings, their added operations and declared `uses`,
    /// following schema references transitively to further additions. A reached
    /// added binding brings its own added operation and `uses`. References into
    /// an addition keep the whole entry; references through existing entries
    /// are followed too. Unreached additions, including those needed only by a
    /// proposal, are left out. Stale proposals are never applied.
    ///
    /// Keys must name bindings in the report's `added` entries. Repeated keys
    /// are harmless; an empty selection returns the original unchanged. The
    /// result first passes the same checks as [`Self::validate_for`], then the
    /// selected document is validated. The original document is immutable.
    /// Core edit, validation and reference inspection passes use their own
    /// limits; the relationship ledger bounds the stored graph and its walk.
    pub fn keep<B: AsRef<str>>(
        &self,
        request: &PullRequest,
        bindings: impl IntoIterator<Item = B>,
        limits: &PullLimits,
        control: &dyn WorkControl,
    ) -> Result<ConformantDocument, Failure> {
        let mut attempt = Attempt::new(*limits, control);
        attempt.request(request)?;
        let full = validation::validate(self, request, &mut attempt)?;
        let references = full
            .document()
            .references_with_resources(openbindings::ExternalSchemas::default(), &attempt);
        attempt.check()?;
        if !references.complete {
            return Err(Failure::LimitExceeded("reference-inspection"));
        }
        let raw = self.value().value();
        let additions = raw.get("additions").expect("admitted additions");
        let report = raw.get("report").expect("admitted report");
        let mut walk = Walk::new(additions, report, &references, &mut attempt)?;
        for binding in bindings {
            let at = walk
                .find("bindings", binding.as_ref(), &mut attempt)?
                .ok_or(RelationshipError::KeptBinding)?;
            walk.keep(at);
        }
        walk.run(&mut attempt)?;
        let selected = walk.selected(&mut attempt)?;
        if !selected.is_empty() && selected.iter().all(|kept| *kept) {
            // The full snapshot already has a conformance proof. Reuse it when
            // every addition was reached, preserving explicit empty maps too.
            attempt.check()?;
            return Ok(full);
        }
        validation::insert(request, additions, Some(&selected), &mut attempt)
    }
}

const SECTIONS: [&str; 3] = ["operations", "schemas", "bindings"];

// A sort runs once over text the size limits already bound, so it spends
// comparisons. Searches repeat per reached reference, so they spend key bytes.
fn sort_work(count: usize, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
    let levels = (usize::BITS - count.leading_zeros()) as usize;
    attempt.work(count.saturating_mul(2 * levels))
}
fn search_work(count: usize, key: usize, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
    let levels = (usize::BITS - count.leading_zeros()) as usize + 1;
    attempt.work(key.saturating_add(1).saturating_mul(levels))
}

#[derive(Clone, Copy)]
struct Entry<'a> {
    section: &'static str,
    key: &'a str,
    // Position in the additions' map order, which insertion selects by.
    at: usize,
    // A binding's entry in the report, which holds its operation and uses.
    report: Option<JsonRef<'a>>,
    kept: bool,
}
struct Edge<'a> {
    from: &'a str,
    to: &'a str,
}
enum Step<'a> {
    Entry(usize),
    Target(&'a str),
}

struct Walk<'a> {
    entries: Vec<Entry<'a>>,
    // Sorted by location, so every reference under a pointer is one range.
    edges: Vec<Edge<'a>>,
    // Each index holds the next edge at or after it not yet visited. Visited
    // ranges are skipped, so overlapping subtrees cost no second scan.
    next: Vec<usize>,
    pending: Vec<Step<'a>>,
}
impl<'a> Walk<'a> {
    fn new(
        additions: JsonRef<'a>,
        report: JsonRef<'a>,
        references: &'a ReferenceReport,
        attempt: &mut Attempt<'_>,
    ) -> Result<Self, Failure> {
        let entries = entries(additions, attempt)?;
        let edges = edges(references, attempt)?;
        attempt.work(edges.len())?;
        let mut next = attempt.vector(edges.len() + 1)?;
        next.extend(0..=edges.len());
        // Each entry and each edge is queued at most once.
        let pending = attempt.vector(entries.len() + edges.len())?;
        let mut walk = Self {
            entries,
            edges,
            next,
            pending,
        };
        for entry in report
            .get("added")
            .expect("added")
            .elements()
            .expect("array")
        {
            let key = entry
                .get("binding")
                .and_then(|v| v.as_str())
                .expect("binding");
            let at = walk
                .find("bindings", key, attempt)?
                .expect("checked binding");
            walk.entries[at].report = Some(entry);
        }
        Ok(walk)
    }

    fn find(
        &self,
        section: &str,
        key: &str,
        attempt: &mut Attempt<'_>,
    ) -> Result<Option<usize>, Failure> {
        search_work(self.entries.len(), key.len(), attempt)?;
        Ok(self
            .entries
            .binary_search_by(|entry| (entry.section, entry.key).cmp(&(section, key)))
            .ok())
    }

    // The addition a pointer lands in, at whatever depth inside the entry.
    fn addition(&self, pointer: &str, attempt: &mut Attempt<'_>) -> Result<Option<usize>, Failure> {
        let mut parts = pointer.split('/').skip(1);
        let (Some(section), Some(key)) = (parts.next(), parts.next()) else {
            return Ok(None);
        };
        if !key.contains('~') {
            return self.find(section, key, attempt);
        }
        attempt.allocate(key.len().saturating_mul(2))?;
        let key = key.replace("~1", "/").replace("~0", "~");
        self.find(section, &key, attempt)
    }

    fn keep(&mut self, at: usize) {
        if !self.entries[at].kept {
            self.entries[at].kept = true;
            self.pending.push(Step::Entry(at));
        }
    }

    fn keep_named(
        &mut self,
        section: &str,
        key: &str,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        let at = self.find(section, key, attempt)?.expect("checked addition");
        self.keep(at);
        Ok(())
    }

    // Binding data is opaque, so a binding's needs are its operation and uses.
    fn needs(&mut self, report: JsonRef<'a>, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        if report.get("reused").is_none() {
            let operation = report
                .get("operation")
                .and_then(|v| v.as_str())
                .expect("operation");
            self.keep_named("operations", operation, attempt)?;
        }
        for label in report
            .get("uses")
            .into_iter()
            .flat_map(|v| v.elements().expect("uses"))
        {
            let (section, key) = label
                .as_str()
                .and_then(|label| label.split_once('/'))
                .expect("checked uses");
            self.keep_named(section, key, attempt)?;
        }
        Ok(())
    }

    fn unvisited(&mut self, mut at: usize, attempt: &mut Attempt<'_>) -> Result<usize, Failure> {
        let start = at;
        while self.next[at] != at {
            attempt.work(1)?;
            at = self.next[at];
        }
        let mut step = start;
        while step != at {
            attempt.work(1)?;
            step = std::mem::replace(&mut self.next[step], at);
        }
        Ok(at)
    }

    // Queue the target of every unvisited reference located under a pointer.
    fn reach(&mut self, root: &str, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        search_work(self.edges.len(), root.len(), attempt)?;
        search_work(self.edges.len(), root.len(), attempt)?;
        attempt.allocate(root.len().saturating_add(1))?;
        let prefix = format!("{root}/");
        let start = self
            .edges
            .partition_point(|edge| edge.from < prefix.as_str());
        let end = start
            + self.edges[start..].partition_point(|edge| edge.from.starts_with(prefix.as_str()));
        let mut at = self.unvisited(start, attempt)?;
        while at < end {
            attempt.work(1)?;
            self.pending.push(Step::Target(self.edges[at].to));
            self.next[at] = at + 1;
            at = self.unvisited(at + 1, attempt)?;
        }
        Ok(())
    }

    fn run(&mut self, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        while let Some(step) = self.pending.pop() {
            attempt.work(1)?;
            match step {
                Step::Entry(at) => self.expand(at, attempt)?,
                // A reference into an addition keeps the whole entry. A
                // reference into an existing entry follows only that subtree.
                Step::Target(pointer) => match self.addition(pointer, attempt)? {
                    Some(at) => self.keep(at),
                    None => self.reach(pointer, attempt)?,
                },
            }
        }
        Ok(())
    }

    fn expand(&mut self, at: usize, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        let entry = self.entries[at];
        if let Some(report) = entry.report {
            return self.needs(report, attempt);
        }
        attempt.allocate(entry.key.len().saturating_mul(8).saturating_add(64))?;
        let root = format!(
            "/{}/{}",
            entry.section,
            entry.key.replace('~', "~0").replace('/', "~1")
        );
        self.reach(&root, attempt)
    }

    fn selected(&self, attempt: &mut Attempt<'_>) -> Result<Vec<bool>, Failure> {
        attempt.work(self.entries.len())?;
        let mut selected = attempt.vector(self.entries.len())?;
        selected.resize(self.entries.len(), false);
        for entry in &self.entries {
            selected[entry.at] = entry.kept;
        }
        Ok(selected)
    }
}

fn entries<'a>(
    additions: JsonRef<'a>,
    attempt: &mut Attempt<'_>,
) -> Result<Vec<Entry<'a>>, Failure> {
    let count = SECTIONS
        .into_iter()
        .filter_map(|section| additions.get(section).and_then(|map| map.len()))
        .sum();
    let mut entries = attempt.vector(count)?;
    for section in SECTIONS {
        for member in additions
            .get(section)
            .into_iter()
            .flat_map(|map| map.members().expect("map"))
        {
            let at = entries.len();
            entries.push(Entry {
                section,
                key: member.name.as_str().expect("checked key"),
                at,
                report: None,
                kept: false,
            });
        }
    }
    sort_work(entries.len(), attempt)?;
    entries.sort_unstable_by_key(|entry| (entry.section, entry.key));
    Ok(entries)
}

fn edges<'a>(
    report: &'a ReferenceReport,
    attempt: &mut Attempt<'_>,
) -> Result<Vec<Edge<'a>>, Failure> {
    let mut edges = attempt.vector(report.references.len())?;
    for reference in &report.references {
        attempt.work(1)?;
        let ReferenceResolution::Located { target, .. } = &reference.resolution else {
            continue;
        };
        if target.source_uri.is_some() {
            continue;
        }
        let to = target
            .pointer
            .as_str()
            .ok_or(RelationshipError::Identifier)?;
        if !matches!(to.split('/').nth(1), Some("operations" | "schemas")) {
            continue;
        }
        let from = reference
            .location
            .pointer
            .as_str()
            .ok_or(RelationshipError::Identifier)?;
        edges.push(Edge { from, to });
    }
    sort_work(edges.len(), attempt)?;
    edges.sort_unstable_by_key(|edge| edge.from);
    Ok(edges)
}
