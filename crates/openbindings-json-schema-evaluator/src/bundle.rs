//! One schema resource for the JSON Schema library.
//!
//! The projection gives every schema a value contract reaches its own definition,
//! `<resource>#/$defs/n<number>`, in one projected resource per original
//! resource, and writes every subschema position as a `$ref` to its child's
//! definition. Definition numbers are unique across resources, so this module
//! bundles the definitions into the `$defs` of a single resource (JSON Schema
//! Core §9.3 describes bundling) and writes every reference as a plain `$ref`
//! within it. Inside one resource the library compiles each definition once,
//! whatever path reaches it.
//!
//! A `$dynamicRef` is resolved here rather than by the library: a definition
//! is specialized once per dynamic context it is reached in, the context
//! recording, for each dynamically looked-up anchor name, the outermost
//! resource entered so far that declares it (JSON Schema Core §8.2.3.2). The
//! bundle holds no `$dynamicAnchor`, so the library never resolves through the
//! dynamic scope.
//!
//! The walk that writes the references also records, for each definition, the
//! definitions its references reach, which [`crate::depth`] reads.
use serde_json::{Map, Value};
use std::collections::HashMap;

/// The bundle's resource URI. Library errors name keywords under it.
pub(crate) const URI: &str = "urn:openbindings:bundle";

/// A program as one resource, by definition until [`Bundle::document`]
/// assembles it.
pub(crate) struct Bundle {
    /// Each bundled definition's name in `$defs`.
    pub(crate) names: Vec<String>,
    /// Each bundled definition's schema.
    pub(crate) schemas: Vec<Value>,
    pub(crate) definitions: Vec<Definition>,
    pub(crate) entry: u32,
    /// The schema objects evaluation holds on reaching the entry definition:
    /// the root, the work marker's wrapper under markers, and the entry.
    pub(crate) entry_weight: u32,
    markers: bool,
}
/// One bundled definition: the projected definition it copies and what its
/// references reach.
pub(crate) struct Definition {
    pub(crate) number: u32,
    pub(crate) edges: Vec<Edge>,
}
/// A reference from one definition to another.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Edge {
    pub(crate) to: u32,
    /// The schema objects evaluation enters from the referring definition's
    /// root to the target's, counting the target.
    pub(crate) weight: u32,
    /// Whether the path passes a keyword that applies to a child instance.
    pub(crate) descends: bool,
    /// Whether every keyword on the path is one the library's
    /// `unevaluatedProperties` walk follows.
    pub(crate) walked: bool,
    /// What following the reference adds to an unevaluated keyword's
    /// annotation walk, which the library compiles by recursion:
    /// [`REFERENCE_WALK`], [`LOGIC_WALK`] more for each `allOf`, `anyOf` or
    /// `oneOf` on the path and [`CONDITIONAL_WALK`] more for each `if`,
    /// `then`, `else` or `dependentSchemas`, whose walks take more stack to
    /// compile.
    pub(crate) walk: u32,
}

/// Why bundling refused.
#[derive(Debug)]
pub(crate) enum Refusal {
    /// Dynamic specialization would grow the program by more than its budget.
    Budget(Growth),
    /// The projected program has an unexpected shape.
    Shape,
}
/// What dynamic specialization held when it passed its budget: every bundled
/// definition and dynamic context so far, as [`DEFINITION_BYTES`] and the rest
/// estimate them, against the projected definitions' own estimate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Growth {
    /// The bytes the program may grow by.
    pub(crate) budget: usize,
    /// The projected definitions' estimate.
    pub(crate) projected: usize,
    pub(crate) definitions: usize,
    pub(crate) definition_bytes: usize,
    pub(crate) contexts: usize,
    pub(crate) entries: usize,
    pub(crate) context_bytes: usize,
}

/// One projected resource, converted for the library.
pub(crate) struct Resource {
    pub(crate) uri: String,
    pub(crate) definitions: Map<String, Value>,
}

/// How to bundle.
pub(crate) struct Options {
    /// The bytes dynamic specialization may add to the projected program's
    /// estimate.
    pub(crate) program_bytes: usize,
    /// Put the SDK's work marker before every reference, so that each schema
    /// a reference enters charges evaluation work.
    pub(crate) markers: bool,
}
/// The custom keyword that charges one unit of evaluation work.
pub(crate) const MARKER: &str = "$obWork";

/// Keywords whose subschemas apply to child instances: members, member names
/// and items.
pub(crate) fn descending(keyword: &str) -> bool {
    matches!(
        keyword,
        "properties"
            | "patternProperties"
            | "additionalProperties"
            | "propertyNames"
            | "items"
            | "prefixItems"
            | "contains"
            | "unevaluatedItems"
            | "unevaluatedProperties"
    )
}
/// What a reference adds to an annotation walk. With the weights below, each
/// level of a walk weighs at least the KiB of stack the library takes to
/// compile it in release builds on Linux, macOS and Windows
/// (`tests/walk_stack.rs`).
pub(crate) const REFERENCE_WALK: u32 = 4;
/// What each `allOf`, `anyOf` or `oneOf` on a reference's path adds to an
/// annotation walk.
pub(crate) const LOGIC_WALK: u32 = 5;
/// What each `if`, `then`, `else` or `dependentSchemas` on a reference's path
/// adds to an annotation walk.
pub(crate) const CONDITIONAL_WALK: u32 = 7;
/// What reaching another holder of an unevaluated keyword adds to an
/// annotation walk, whose own walk the library compiles there.
pub(crate) const HOLDER_WALK: u32 = 1;
fn keyword_walk(keyword: &str) -> u32 {
    match keyword {
        "allOf" | "anyOf" | "oneOf" => LOGIC_WALK,
        "if" | "then" | "else" | "dependentSchemas" => CONDITIONAL_WALK,
        _ => 0,
    }
}
// Keywords the library's `unevaluatedProperties` walk follows into subschemas.
fn walked(keyword: &str) -> bool {
    matches!(
        keyword,
        "allOf" | "anyOf" | "oneOf" | "if" | "then" | "else" | "dependentSchemas"
    )
}
/// Each subschema of an applicator keyword's value.
pub(crate) fn subschemas<'a>(
    keyword: &str,
    value: &'a mut Value,
) -> Box<dyn Iterator<Item = &'a mut Value> + 'a> {
    match (keyword, value) {
        ("properties" | "patternProperties" | "dependentSchemas", Value::Object(members)) => {
            Box::new(members.values_mut())
        }
        ("allOf" | "anyOf" | "oneOf" | "prefixItems", Value::Array(items)) => {
            Box::new(items.iter_mut())
        }
        (
            "additionalProperties"
            | "propertyNames"
            | "items"
            | "contains"
            | "unevaluatedItems"
            | "unevaluatedProperties"
            | "not"
            | "if"
            | "then"
            | "else",
            value,
        ) => Box::new(std::iter::once(value)),
        _ => Box::new(std::iter::empty()),
    }
}

// A projected definition's number, from its name `n<number>`.
fn number(name: &str) -> Option<u32> {
    name.strip_prefix('n')?.parse().ok()
}
// The resource and fragment of a projected reference.
fn split(reference: &str) -> Option<(&str, &str)> {
    reference.split_once('#')
}

/// Bundle `resources`, whose entry the projected reference `entry` names.
pub(crate) fn bundle(
    resources: Vec<Resource>,
    entry: &str,
    options: &Options,
) -> Result<Bundle, Refusal> {
    let dynamic = resources.iter().any(|resource| {
        resource
            .definitions
            .values()
            .any(|schema| schema.get("$dynamicAnchor").is_some())
    });
    let mut bundler = Bundler {
        indices: HashMap::new(),
        names: Vec::new(),
        definitions: Vec::new(),
        out: Vec::new(),
        markers: options.markers,
    };
    let entry = if dynamic {
        Specializer::new(&resources, options).run(&mut bundler, entry)?
    } else {
        bundler.merge(resources, entry)?
    };
    let Bundler {
        names,
        definitions,
        out,
        ..
    } = bundler;
    Ok(Bundle {
        names,
        schemas: out,
        definitions,
        entry,
        entry_weight: if options.markers { 3 } else { 2 },
        markers: options.markers,
    })
}
impl Bundle {
    /// The bundle as one schema resource.
    pub(crate) fn document(self) -> Value {
        let mut root = Map::new();
        root.insert("$id".into(), Value::String(URI.into()));
        root.insert(
            "$schema".into(),
            Value::String("https://json-schema.org/draft/2020-12/schema".into()),
        );
        let entry_reference = Value::String(format!("#/$defs/{}", self.names[self.entry as usize]));
        if self.markers {
            // Entering the entry charges a unit too, so a cap of zero decides nothing.
            root.insert(
                "allOf".into(),
                serde_json::json!([{ MARKER: 0 }, { "$ref": entry_reference }]),
            );
        } else {
            root.insert("$ref".into(), entry_reference);
        }
        let definitions: Map<String, Value> = self.names.into_iter().zip(self.schemas).collect();
        root.insert("$defs".into(), Value::Object(definitions));
        Value::Object(root)
    }
}
// Projected definitions keep their names (`n<number>`), specialized copies add
// their context (`n<number>-<context>`), and a definition a schema transform
// adds for one keyword of definition `n<number>` is `n<number>-p<index>`.
/// A bundled definition name read back: the projected definition number it
/// holds, and whether a schema transform added it.
pub(crate) fn parse_name(name: &str) -> Option<(u32, bool)> {
    let projected = name.strip_prefix('n')?;
    let (number, added) = match projected.split_once('-') {
        Some((number, suffix)) => (number, suffix.starts_with('p')),
        None => (projected, false),
    };
    Some((number.parse().ok()?, added))
}
/// The name of the definition a schema transform adds at `index` for a
/// keyword of projected definition `number`.
pub(crate) fn added_name(number: u32, index: u32) -> String {
    format!("n{number}-p{index}")
}
/// The edge of a reference that a definition's `allOf` item holds.
pub(crate) fn all_of_reference(to: u32) -> Edge {
    Path::ROOT.enter("allOf").edge(to, 0)
}

struct Bundler {
    // Bundled index by projected number (merge) or by name (specializer).
    indices: HashMap<u32, u32>,
    names: Vec<String>,
    definitions: Vec<Definition>,
    out: Vec<Value>,
    markers: bool,
}
impl Bundler {
    // The new definition's index; `None` past the most a `u32` numbers.
    fn add(&mut self, name: String, number: u32) -> Option<u32> {
        let index = u32::try_from(self.names.len()).ok()?;
        self.names.push(name);
        self.definitions.push(Definition {
            number,
            edges: Vec::new(),
        });
        self.out.push(Value::Bool(true));
        Some(index)
    }
    // Without a dynamic anchor every definition keeps its projected name.
    fn merge(&mut self, resources: Vec<Resource>, entry: &str) -> Result<u32, Refusal> {
        for resource in &resources {
            for name in resource.definitions.keys() {
                let number = number(name).ok_or(Refusal::Shape)?;
                let index = self.add(name.clone(), number).ok_or(Refusal::Shape)?;
                self.indices.insert(number, index);
            }
        }
        for resource in resources {
            for (name, mut schema) in resource.definitions {
                let from = self.indices[&number(&name).ok_or(Refusal::Shape)?];
                let indices = &self.indices;
                let mut edges = Vec::new();
                walk(
                    &mut schema,
                    Path::ROOT,
                    self.markers,
                    &mut |reference, _| {
                        // Definition names are unique across resources, so the
                        // reference keeps its fragment.
                        let hash = reference.find('#').ok_or(Refusal::Shape)?;
                        let target = reference[hash..]
                            .strip_prefix("#/$defs/")
                            .ok_or(Refusal::Shape)?;
                        let to = *indices
                            .get(&number(target).ok_or(Refusal::Shape)?)
                            .ok_or(Refusal::Shape)?;
                        reference.replace_range(..hash, "");
                        Ok(to)
                    },
                    &mut edges,
                )?;
                self.definitions[from as usize].edges = edges;
                self.out[from as usize] = schema;
            }
        }
        let (_, fragment) = split(entry).ok_or(Refusal::Shape)?;
        let target = fragment.strip_prefix("/$defs/").ok_or(Refusal::Shape)?;
        self.indices
            .get(&number(target).ok_or(Refusal::Shape)?)
            .copied()
            .ok_or(Refusal::Shape)
    }
}

// Where a walk stands below a definition's root.
#[derive(Clone, Copy)]
struct Path {
    // Schema objects entered below the root.
    depth: u32,
    descends: bool,
    walked: bool,
    // What the keywords entered add to an annotation walk.
    walk: u32,
}
impl Path {
    const ROOT: Path = Path {
        depth: 0,
        descends: false,
        walked: true,
        walk: 0,
    };
    fn enter(self, keyword: &str) -> Path {
        Path {
            depth: self.depth + 1,
            descends: self.descends || descending(keyword),
            walked: self.walked && walked(keyword),
            walk: self.walk + keyword_walk(keyword),
        }
    }
    fn edge(self, to: u32, extra: u32) -> Edge {
        Edge {
            to,
            weight: self.depth + 1 + extra,
            descends: self.descends,
            walked: self.walked,
            walk: self.walk + REFERENCE_WALK,
        }
    }
}

// Point every reference below a schema at its bundled definition and record its
// edge. `resolve` writes a projected reference (and whether it is a
// `$dynamicRef`) as the bundled reference, in place, and gives its target's
// index.
type Resolve<'a> = dyn FnMut(&mut String, bool) -> Result<u32, Refusal> + 'a;
fn walk(
    value: &mut Value,
    path: Path,
    markers: bool,
    resolve: &mut Resolve<'_>,
    edges: &mut Vec<Edge>,
) -> Result<(), Refusal> {
    let Value::Object(schema) = value else {
        return Ok(());
    };
    schema.remove("$dynamicAnchor");
    for (keyword, child) in schema.iter_mut() {
        let inner = path.enter(keyword);
        for subschema in subschemas(keyword, child) {
            walk(subschema, inner, markers, resolve, edges)?;
        }
    }
    let mark = markers;
    let mut wrappers = Vec::new();
    if let Some(Value::String(reference)) = schema.get_mut("$ref") {
        let to = resolve(reference, false)?;
        if mark {
            edges.push(path.edge(to, 1));
            if let Some(Value::String(reference)) = schema.remove("$ref") {
                wrappers.push(reference);
            }
        } else {
            edges.push(path.edge(to, 0));
        }
    }
    if let Some(Value::String(mut reference)) = schema.remove("$dynamicRef") {
        let to = resolve(&mut reference, true)?;
        edges.push(path.edge(to, 1));
        wrappers.push(reference);
    }
    if !wrappers.is_empty() {
        let all_of = match schema
            .entry("allOf")
            .or_insert_with(|| Value::Array(Vec::new()))
        {
            Value::Array(all_of) => all_of,
            _ => unreachable!("a projected allOf is an array"),
        };
        for reference in wrappers {
            if mark {
                all_of.push(serde_json::json!({ MARKER: 0 }));
            }
            all_of.push(serde_json::json!({ "$ref": reference }));
        }
    }
    Ok(())
}

/// The edges of a bundled schema, whose references `resolve` maps to their
/// targets' indices.
pub(crate) fn edges(schema: &mut Value, resolve: &mut dyn FnMut(&str) -> Option<u32>) -> Vec<Edge> {
    let mut edges = Vec::new();
    let _ = walk(
        schema,
        Path::ROOT,
        false,
        &mut |reference, _| resolve(reference).ok_or(Refusal::Shape),
        &mut edges,
    );
    edges
}

// Dynamic specialization. A context assigns each looked-up anchor name the
// resource that declares it outermost on the evaluation path, if any. Only the
// names some `$dynamicRef` looks up take part, and a context holds only the
// names it assigns, sorted, so contexts take space in proportion to what they
// record. The definitions and contexts specialization produces are charged
// what the SDK holds for them, and together may pass the projected
// definitions' own charge by at most the program budget.
type Context = std::rc::Rc<[(u32, u32)]>;
struct Specializer<'a> {
    resources: &'a [Resource],
    by_uri: HashMap<&'a str, u32>,
    // Each resource's dynamic anchors whose names are looked up: (name id,
    // definition name).
    anchors: Vec<Vec<(u32, &'a str)>>,
    names: HashMap<&'a str, u32>,
    contexts: HashMap<Context, u32>,
    context_list: Vec<Context>,
    // Bundled index by (resource, definition name, context).
    output: HashMap<(u32, &'a str, u32), u32>,
    queue: Vec<(u32, u32, &'a str, u32)>,
    // Each projected definition's charge, by (resource, definition name).
    charges: HashMap<(u32, &'a str), usize>,
    growth: Growth,
}

/// What the SDK holds for each bundled definition besides its schema's
/// content: the schema's slot, its name, its references and its index entries.
const DEFINITION_BYTES: usize = 160;
/// What a JSON object's map takes before its members: serde_json allocates the
/// first node of its map for eleven members.
const OBJECT_BYTES: usize = 640;
/// What each object member takes besides its name's bytes and its value's
/// content: the name's string and the value's slot.
const MEMBER_BYTES: usize = 56;
/// What each array element takes besides its content: its slot.
const ELEMENT_BYTES: usize = 32;
/// What each dynamic context takes besides its entries: its shared list and
/// its index entries.
const CONTEXT_BYTES: usize = 64;
/// What each dynamic-scope entry of a context takes.
const ENTRY_BYTES: usize = 8;
// The memory the SDK holds for `value`'s content, beyond the slot holding
// `value`: a counting allocator measures ordinary projected definitions at most
// a sixth below this estimate.
fn content(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        Value::Array(items) => items.iter().map(|item| ELEMENT_BYTES + content(item)).sum(),
        Value::Object(members) => {
            OBJECT_BYTES
                + members
                    .iter()
                    .map(|(name, member)| MEMBER_BYTES + name.len() + content(member))
                    .sum::<usize>()
        }
        _ => 0,
    }
}
// What a bundled definition holding `schema` is charged.
fn definition_charge(schema: &Value) -> usize {
    DEFINITION_BYTES.saturating_add(content(schema))
}
// The anchor names `value` looks up through `$dynamicRef` fragments.
fn looked_up<'v>(value: &'v Value, out: &mut std::collections::HashSet<&'v str>) {
    match value {
        Value::Object(members) => {
            if let Some(Value::String(reference)) = members.get("$dynamicRef")
                && let Some((_, fragment)) = split(reference)
                && !fragment.starts_with('/')
            {
                out.insert(fragment);
            }
            members.values().for_each(|member| looked_up(member, out));
        }
        Value::Array(items) => items.iter().for_each(|item| looked_up(item, out)),
        _ => {}
    }
}
impl<'a> Specializer<'a> {
    fn new(resources: &'a [Resource], options: &Options) -> Self {
        let mut wanted = std::collections::HashSet::new();
        for resource in resources {
            for schema in resource.definitions.values() {
                looked_up(schema, &mut wanted);
            }
        }
        let mut names = HashMap::new();
        let mut anchors = Vec::with_capacity(resources.len());
        let mut by_uri = HashMap::with_capacity(resources.len());
        let projected = resources
            .iter()
            .flat_map(|resource| resource.definitions.values())
            .fold(0usize, |total, schema| {
                total.saturating_add(definition_charge(schema))
            });
        for (index, resource) in resources.iter().enumerate() {
            by_uri.insert(resource.uri.as_str(), index as u32);
            let mut declared = Vec::new();
            for (definition, schema) in &resource.definitions {
                if let Some(Value::String(anchor)) = schema.get("$dynamicAnchor") {
                    if !wanted.contains(anchor.as_str()) {
                        continue;
                    }
                    let next = names.len() as u32;
                    let id = *names.entry(anchor.as_str()).or_insert(next);
                    declared.push((id, definition.as_str()));
                }
            }
            declared.sort_unstable();
            anchors.push(declared);
        }
        Self {
            resources,
            by_uri,
            anchors,
            names,
            contexts: HashMap::new(),
            context_list: Vec::new(),
            output: HashMap::new(),
            queue: Vec::new(),
            charges: HashMap::new(),
            growth: Growth {
                budget: options.program_bytes,
                projected,
                definitions: 0,
                definition_bytes: 0,
                contexts: 0,
                entries: 0,
                context_bytes: 0,
            },
        }
    }
    // Whether what specialization holds passes the projected definitions'
    // charge by more than the budget. The projected program is admitted
    // whatever its size; specialized copies and contexts beyond it are bounded.
    fn within_budget(&self) -> Result<(), Refusal> {
        let growth = self.growth;
        let held = growth.definition_bytes.saturating_add(growth.context_bytes);
        if held > growth.projected.saturating_add(growth.budget) {
            return Err(Refusal::Budget(growth));
        }
        Ok(())
    }
    fn intern(&mut self, context: Vec<(u32, u32)>) -> Result<u32, Refusal> {
        if let Some(&id) = self.contexts.get(context.as_slice()) {
            return Ok(id);
        }
        let entries = context.len();
        self.growth.contexts += 1;
        self.growth.entries = self.growth.entries.saturating_add(entries);
        self.growth.context_bytes = self
            .growth
            .context_bytes
            .saturating_add(CONTEXT_BYTES.saturating_add(entries.saturating_mul(ENTRY_BYTES)));
        self.within_budget()?;
        let context: Context = context.into();
        let id =
            u32::try_from(self.context_list.len()).map_err(|_| Refusal::Budget(self.growth))?;
        self.context_list.push(context.clone());
        self.contexts.insert(context, id);
        Ok(id)
    }
    // The resource `context` assigns `name`, if any.
    fn assigned(&self, context: u32, name: u32) -> Option<u32> {
        let entries = &self.context_list[context as usize];
        entries
            .binary_search_by_key(&name, |&(assigned, _)| assigned)
            .ok()
            .map(|at| entries[at].1)
    }
    // The context after entering `resource`: each name it declares that no
    // resource entered earlier declares now resolves to it.
    fn enter(&mut self, context: u32, resource: u32) -> Result<u32, Refusal> {
        let declared = &self.anchors[resource as usize];
        if declared
            .iter()
            .all(|&(name, _)| self.assigned(context, name).is_some())
        {
            return Ok(context);
        }
        let current = &self.context_list[context as usize];
        let mut next: Vec<(u32, u32)> = current.to_vec();
        for &(name, _) in declared {
            if let Err(at) = next.binary_search_by_key(&name, |&(assigned, _)| assigned) {
                next.insert(at, (name, resource));
            }
        }
        self.intern(next)
    }
    fn index(
        &mut self,
        bundler: &mut Bundler,
        resource: u32,
        definition: &'a str,
        context: u32,
    ) -> Result<u32, Refusal> {
        if let Some(&index) = self.output.get(&(resource, definition, context)) {
            return Ok(index);
        }
        let charge = match self.charges.get(&(resource, definition)) {
            Some(&charge) => charge,
            None => {
                let charge =
                    definition_charge(&self.resources[resource as usize].definitions[definition]);
                self.charges.insert((resource, definition), charge);
                charge
            }
        };
        self.growth.definitions += 1;
        self.growth.definition_bytes = self.growth.definition_bytes.saturating_add(charge);
        self.within_budget()?;
        let number = number(definition).ok_or(Refusal::Shape)?;
        let index = bundler
            .add(format!("n{number}-{context}"), number)
            .ok_or(Refusal::Budget(self.growth))?;
        self.output.insert((resource, definition, context), index);
        self.queue.push((index, resource, definition, context));
        Ok(index)
    }
    // The bundled definition a reference from `from` in `context` reaches.
    fn target(
        &mut self,
        bundler: &mut Bundler,
        reference: &str,
        from: u32,
        context: u32,
        dynamic: bool,
    ) -> Result<u32, Refusal> {
        let (uri, fragment) = split(reference).ok_or(Refusal::Shape)?;
        let mut resource = if uri.is_empty() {
            from
        } else {
            *self.by_uri.get(uri).ok_or(Refusal::Shape)?
        };
        let definition: &'a str = if let Some(definition) = fragment.strip_prefix("/$defs/") {
            let (name, _) = self.resources[resource as usize]
                .definitions
                .get_key_value(definition)
                .ok_or(Refusal::Shape)?;
            name.as_str()
        } else if dynamic {
            // The projection writes `<resource>#<name>` only where the
            // reference resolves to a dynamic anchor of that name.
            let name = *self.names.get(fragment).ok_or(Refusal::Shape)?;
            if let Some(outer) = self.assigned(context, name) {
                resource = outer;
            }
            self.anchors[resource as usize]
                .iter()
                .find(|&&(declared, _)| declared == name)
                .map(|&(_, definition)| definition)
                .ok_or(Refusal::Shape)?
        } else {
            return Err(Refusal::Shape);
        };
        let entered = if resource == from {
            context
        } else {
            self.enter(context, resource)?
        };
        self.index(bundler, resource, definition, entered)
    }
    fn run(mut self, bundler: &mut Bundler, entry: &str) -> Result<u32, Refusal> {
        let start = self.intern(Vec::new())?;
        let (uri, _) = split(entry).ok_or(Refusal::Shape)?;
        let entry_resource = *self.by_uri.get(uri).ok_or(Refusal::Shape)?;
        let start = self.enter(start, entry_resource)?;
        let entry = self.target(bundler, entry, entry_resource, start, false)?;
        while let Some((index, resource, definition, context)) = self.queue.pop() {
            let mut schema = self.resources[resource as usize].definitions[definition].clone();
            let mut edges = Vec::new();
            let markers = bundler.markers;
            walk(
                &mut schema,
                Path::ROOT,
                markers,
                &mut |reference, dynamic| {
                    let to = self.target(bundler, reference, resource, context, dynamic)?;
                    *reference = format!("#/$defs/{}", bundler.names[to as usize]);
                    Ok(to)
                },
                &mut edges,
            )?;
            bundler.definitions[index as usize].edges = edges;
            bundler.out[index as usize] = schema;
        }
        Ok(entry)
    }
}
