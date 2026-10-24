//! The SDK's keywords and the library's copy of each projected resource.
//! Literal values stay in the exact flat representation: only the shallow
//! projected schema structure crosses the dependency's recursive serde boundary.
use crate::pattern::{Allowance, Given, Memory, Unsupported};
use crate::{Limits, no_verdict};
use jsonschema::json::{JsonNumber, Node};
use jsonschema::paths::Location;
use jsonschema::{Keyword, ValidationError, ValidationOptions};
use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson, View};
use openbindings_internal_json::numeric;
use openbindings_internal_schema::{Definition, Field, Schema};
use serde_json::{Map, Value};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

#[derive(Default)]
pub(crate) struct Operands {
    // Each compiled `const` and `enum` keyword's literal, by the id it holds.
    literals: Vec<Literal>,
    numbers: numeric::Operands,
    // Which numeric keywords, by their place in `numeric::KEYWORDS`, hold an
    // operand, so the validator registers only the SDK keywords the value
    // contract holds.
    numeric: [bool; numeric::KEYWORDS.len()],
    // Each distinct `pattern` matcher, in the order preparation reads them; a
    // `pattern` keyword holds its index.
    patterns: Vec<Matcher>,
    // Each matcher's index by its form, or by the reason the evaluator does
    // not match its pattern, while preparing. A form is kept once, shared with
    // its matcher.
    indices: std::collections::HashMap<Arc<str>, usize>,
}
// A `const` value, or an `enum` keyword's choices. Both are views of the
// projection's separated literals.
#[derive(Clone)]
enum Literal {
    Value(JsonValue),
    Choices(Arc<[JsonValue]>),
}
struct Constraint(Literal);
impl<'i> Keyword<'i, FlatJson> for Constraint {
    fn is_valid(&self, instance: View<'i>) -> bool {
        match &self.0 {
            Literal::Value(value) => backend::equals(instance, backend::view(value)),
            Literal::Choices(choices) => choices
                .iter()
                .any(|choice| backend::equals(instance, backend::view(choice))),
        }
    }
    fn validate(&self, instance: View<'i>) -> Result<(), ValidationError<'i>> {
        if self.is_valid(instance) {
            Ok(())
        } else {
            Err(ValidationError::custom(
                "value differs from the required exact literal",
            ))
        }
    }
}
/// A numeric keyword compared exactly with its original operand. Division the
/// exact arithmetic cannot decide stops the pass with an arithmetic stop.
struct ExactBound(numeric::Bound);
impl<'i> Keyword<'i, FlatJson> for ExactBound {
    fn is_valid(&self, instance: View<'i>) -> bool {
        let Some(number) = instance.as_number() else {
            return true;
        };
        self.0.admits(&number.as_str()).unwrap_or_else(|| {
            crate::pass::arithmetic();
            false
        })
    }
    fn validate(&self, instance: View<'i>) -> Result<(), ValidationError<'i>> {
        if <Self as Keyword<'i, FlatJson>>::is_valid(self, instance) {
            Ok(())
        } else {
            Err(ValidationError::custom(format!(
                "exact numeric {} constraint failed",
                self.0.keyword()
            )))
        }
    }
}
/// The keyword a stand-in for a resource that was not supplied compiles to. A
/// pass that reaches it gives no verdict.
struct Missing(u32);
impl<'i> Keyword<'i, FlatJson> for Missing {
    fn is_valid(&self, _: View<'i>) -> bool {
        crate::pass::missing(self.0);
        true
    }
    fn validate(&self, _: View<'i>) -> Result<(), ValidationError<'i>> {
        crate::pass::missing(self.0);
        Ok(())
    }
}
/// How the SDK's `pattern` keyword matches one pattern.
enum Matcher {
    Simple(crate::pattern::Simple),
    // Compiled on first use, so preparation does not pay for patterns no
    // value reaches.
    Regex {
        form: Arc<str>,
        compiled: OnceLock<Compiled>,
    },
    Unmatched(Unsupported),
}
enum Compiled {
    Regex(regex::Regex),
    Refused(Unsupported),
}
/// A value contract's `pattern` matchers and its pattern memory. Each compiled
/// matcher first compiles within its first allowance. When a pattern is found
/// that does not fit its own, at preparation for a `patternProperties` name or
/// when a string first reaches a `pattern` matcher, every compiled pattern of
/// the value contract not yet compiled is compiled within its first allowance, and
/// those that do not fit share what the first allowances leave. That happens
/// once, so whether a pattern compiles does not depend on the order strings
/// reach the value contract's patterns.
pub(crate) struct Patterns {
    matchers: Vec<Matcher>,
    memory: Memory,
    // Whether the memory has been shared.
    shared: Mutex<bool>,
    // Each `pattern` keyword's location in the bundle, by the site index the
    // keyword holds: a pass that stops at a pattern names its keyword by it.
    sites: Mutex<Vec<Location>>,
}
impl Patterns {
    /// Share the pattern memory among the `names`, distinct
    /// `patternProperties` name forms that the library compiles, and every
    /// matcher, because one of the names does not fit its first allowance.
    /// `together` compiles every name within one allowance and answers
    /// whether they all fit. Each name's compiled size, or why it does not
    /// fit.
    pub(crate) fn share_with(
        &self,
        names: &[&str],
        together: &mut dyn FnMut(Allowance) -> bool,
    ) -> Vec<Result<usize, Unsupported>> {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        self.share(names, together, &mut shared)
    }
    fn share(
        &self,
        names: &[&str],
        together: &mut dyn FnMut(Allowance) -> bool,
        shared: &mut bool,
    ) -> Vec<Result<usize, Unsupported>> {
        enum Owner {
            Name(usize),
            Matcher(usize),
        }
        let first = self.memory.first();
        let mut given = vec![Ok(first.compiled); names.len()];
        let (mut forms, mut owners) = (Vec::new(), Vec::new());
        for (at, name) in names.iter().enumerate() {
            if crate::pattern::compile(name, first).is_none() {
                forms.push(*name);
                owners.push(Owner::Name(at));
            }
        }
        for (at, matcher) in self.matchers.iter().enumerate() {
            if let Matcher::Regex { form, compiled } = matcher
                && compiled.get().is_none()
            {
                match crate::pattern::compile(form, first) {
                    // A string reaching it meanwhile compiles it the same way.
                    Some(regex) => drop(compiled.set(Compiled::Regex(regex))),
                    None => {
                        forms.push(form);
                        owners.push(Owner::Matcher(at));
                    }
                }
            }
        }
        let names_sharing = owners
            .iter()
            .filter(|owner| matches!(owner, Owner::Name(_)))
            .count();
        let shares = self.memory.share(&forms, names_sharing, together);
        for (owner, share) in owners.into_iter().zip(shares) {
            match owner {
                Owner::Name(at) => {
                    given[at] = match share {
                        Given::Compiled { compiled, .. } => Ok(compiled),
                        Given::Refused(unsupported) => Err(unsupported),
                    };
                }
                Owner::Matcher(at) => {
                    let Matcher::Regex { compiled, .. } = &self.matchers[at] else {
                        unreachable!("only a regex matcher compiles")
                    };
                    let outcome = match share {
                        Given::Compiled {
                            regex: Some(regex), ..
                        } => Compiled::Regex(regex),
                        Given::Compiled { regex: None, .. } => {
                            unreachable!("a matcher keeps its compiled form")
                        }
                        Given::Refused(unsupported) => Compiled::Refused(unsupported),
                    };
                    drop(compiled.set(outcome));
                }
            }
        }
        *shared = true;
        given
    }
    /// The bundle location of the `pattern` keyword at `site`.
    pub(crate) fn site(&self, site: u32) -> Option<Location> {
        let sites = self.sites.lock().unwrap_or_else(PoisonError::into_inner);
        sites.get(site as usize).cloned()
    }
    // Whether `text` matches the matcher at `index`; a pattern the evaluator
    // does not match stops the pass, naming the keyword at `site` that holds it.
    fn is_match(&self, index: usize, text: &str, site: u32) -> bool {
        match &self.matchers[index] {
            Matcher::Simple(simple) => simple.is_match(text),
            Matcher::Regex { compiled, .. } => {
                match compiled.get().unwrap_or_else(|| self.compile(index)) {
                    Compiled::Regex(regex) => regex.is_match(text),
                    Compiled::Refused(unsupported) => {
                        crate::pass::unmatched(*unsupported, site);
                        true
                    }
                }
            }
            Matcher::Unmatched(unsupported) => {
                crate::pass::unmatched(*unsupported, site);
                true
            }
        }
    }
    // The first use of a compiled matcher, within its first allowance or, when
    // it does not fit that, within what sharing the pattern memory gives it.
    #[cold]
    fn compile(&self, index: usize) -> &Compiled {
        let Matcher::Regex { form, compiled } = &self.matchers[index] else {
            unreachable!("only a regex matcher compiles")
        };
        if let Some(regex) = crate::pattern::compile(form, self.memory.first()) {
            return compiled.get_or_init(|| Compiled::Regex(regex));
        }
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if !*shared {
            self.share(&[], &mut |_| unreachable!("no name shares"), &mut shared);
        }
        compiled.get().expect("sharing compiles every matcher")
    }
}
// The forms by which patterns the evaluator does not match share a matcher; no
// linear form holds a control character.
const BACKTRACKING: &str = "\u{1}backtracking";
const PROPERTY_ESCAPE: &str = "\u{1}property-escape";
const NON_WORD_BOUNDARY: &str = "\u{1}non-word-boundary";
const SIZE: &str = "\u{1}size";
/// The SDK's `pattern` keyword, which matches with ECMA-262 semantics through
/// its pattern's reading: the value contract's matchers, its matcher's index, and
/// its own site. A string that reaches a pattern the evaluator does not match
/// ends the pass with no verdict naming the keyword's location.
struct Pattern(Arc<Patterns>, usize, u32);
impl<'i> Keyword<'i, FlatJson> for Pattern {
    fn is_valid(&self, instance: View<'i>) -> bool {
        match instance.as_string() {
            Some(text) => self.0.is_match(self.1, &text, self.2),
            None => true,
        }
    }
    fn validate(&self, instance: View<'i>) -> Result<(), ValidationError<'i>> {
        if self.is_valid(instance) {
            Ok(())
        } else {
            Err(ValidationError::custom(
                "string does not match the schema pattern",
            ))
        }
    }
}
/// The SDK's marker before each reference when evaluation work is capped:
/// entering it charges a unit, and once the allowance is spent it fails, so
/// the reference is not entered.
struct Work;
impl<'i> Keyword<'i, FlatJson> for Work {
    fn is_valid(&self, _: View<'i>) -> bool {
        openbindings_internal_json::work::charge(1)
    }
    fn validate(&self, instance: View<'i>) -> Result<(), ValidationError<'i>> {
        if self.is_valid(instance) {
            Ok(())
        } else {
            Err(ValidationError::custom("evaluation work limit reached"))
        }
    }
}
/// How the compiled validator receives one projected keyword.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// An exact numeric operand handled by the SDK's numeric keywords.
    Number,
    /// A size operand normalized for the library.
    Size,
    /// An exact literal handled by the SDK's literal keywords.
    Const,
    /// Exact literal choices handled by the SDK's literal keywords.
    Enum,
    /// A value the library reads as it is.
    Read,
    /// A `$dynamicAnchor`, also read as it is: references can then resolve
    /// through the dynamic scope.
    DynamicAnchor,
}
/// The role of a projected keyword, or `None` for keywords the validator never
/// compiles: annotations and unknown keywords. No library error names them.
pub(crate) fn role(keyword: &str) -> Option<Role> {
    Some(match keyword {
        "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "multipleOf" => {
            Role::Number
        }
        "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties" | "maxProperties"
        | "minContains" | "maxContains" => Role::Size,
        "const" => Role::Const,
        "enum" => Role::Enum,
        "$dynamicAnchor" => Role::DynamicAnchor,
        "$ref"
        | "$dynamicRef"
        | "type"
        | "pattern"
        | "uniqueItems"
        | "required"
        | "dependentRequired"
        | "properties"
        | "patternProperties"
        | "additionalProperties"
        | "propertyNames"
        | "dependentSchemas"
        | "unevaluatedProperties"
        | "items"
        | "prefixItems"
        | "contains"
        | "unevaluatedItems"
        | "not"
        | "if"
        | "then"
        | "else"
        | "allOf"
        | "anyOf"
        | "oneOf" => Role::Read,
        _ => return None,
    })
}
impl Operands {
    /// The library's copy of a projected resource's text, which the
    /// `conversion-check` feature still compares with [`Self::definitions`]. Its `const` and `enum`
    /// values are indices into `literals`, the literals the projection
    /// separates, which the compiled keywords share rather than copy.
    #[cfg(feature = "conversion-check")]
    pub(crate) fn resource(
        &mut self,
        resource: &JsonValue,
        literals: &[JsonValue],
        limits: &Limits,
        control: &dyn WorkControl,
    ) -> Result<Value, NoVerdict> {
        let mut root = Map::new();
        for member in resource
            .view()
            .members()
            .expect("projected resource is an object")
        {
            control.check()?;
            let key = member.name.as_str().expect("projected member name");
            if key == "$defs" {
                let mut defs = Map::new();
                for member in member.value.members().expect("projected definitions map") {
                    control.check()?;
                    let value = match member.value.kind() {
                        JsonKind::Boolean => Value::Bool(member.value.as_bool().unwrap()),
                        _ => {
                            let mut schema = Map::new();
                            for field in member.value.members().expect("admitted schema object") {
                                let key = field.name.as_str().expect("admitted scalar name");
                                let value = match role(key) {
                                    Some(Role::Number) => self.number(key, field.value.text()),
                                    // No finite JSON collection can reach u64::MAX elements.
                                    // Preserve contains annotations by keeping its native keyword;
                                    // normalize every size operand without exponent expansion.
                                    Some(Role::Size) => Value::from(
                                        openbindings_internal_json::decimal::Decimal::parse(
                                            field.value.text(),
                                        )
                                        .nonnegative_integer_u64_saturating()
                                        .expect("admitted size operand"),
                                    ),
                                    Some(Role::Const) => {
                                        let value = separated(field.value, literals)?.clone();
                                        let id = self.literals.len();
                                        self.literals.push(Literal::Value(value));
                                        Value::from(id)
                                    }
                                    Some(Role::Enum) => {
                                        let choices: Vec<_> = separated(field.value, literals)?
                                            .view()
                                            .elements()
                                            .expect("admitted enum array")
                                            .map(|v| v.to_owned())
                                            .collect();
                                        let id = self.literals.len();
                                        self.literals.push(Literal::Choices(choices.into()));
                                        Value::Array(vec![Value::from(id)])
                                    }
                                    Some(Role::Read) => read(field.value, limits)?,
                                    Some(Role::DynamicAnchor) => read(field.value, limits)?,
                                    // Annotations and unknown-keyword data do not affect this
                                    // validity-only API. Their original bytes stay in the SDK.
                                    None => continue,
                                };
                                schema.insert(key.into(), value);
                            }
                            Value::Object(schema)
                        }
                    };
                    defs.insert(member.name.as_str().unwrap().into(), value);
                }
                root.insert(key.into(), Value::Object(defs));
            } else {
                root.insert(key.into(), read(member.value, limits)?);
            }
        }
        Ok(Value::Object(root))
    }
    /// The library's copy of a projected resource's definitions, as
    /// [`Self::resource`] gives its `$defs` from the program's text: each
    /// schema object's keywords in name order, each once, and the same
    /// operands and literals in the same order.
    pub(crate) fn definitions(
        &mut self,
        definitions: Vec<Definition<'_>>,
        literals: &[JsonValue],
        limits: &Limits,
        control: &dyn WorkControl,
    ) -> Result<Map<String, Value>, NoVerdict> {
        let mut defs = Map::new();
        for definition in definitions {
            control.check()?;
            let value = match definition.schema {
                Schema::Boolean(value) => Value::Bool(value),
                Schema::Object(mut fields) => {
                    // In the order the program's text holds them, which a map
                    // that keeps its insertion order keeps.
                    fields.sort_by(|a, b| a.0.cmp(b.0));
                    let mut fields = fields.into_iter().peekable();
                    let mut schema = Map::new();
                    while let Some((key, field)) = fields.next() {
                        // The text holds the last of a repeated keyword.
                        if fields.peek().is_some_and(|next| next.0 == key) {
                            continue;
                        }
                        let Some(role) = role(key) else {
                            continue;
                        };
                        let value = self.field(key, role, field, literals, limits)?;
                        schema.insert(key.into(), value);
                    }
                    Value::Object(schema)
                }
            };
            defs.insert(definition.name, value);
        }
        Ok(defs)
    }
    // One projected keyword's value for the library, as `resource` reads it
    // from the text the program writes for it.
    fn field(
        &mut self,
        key: &str,
        role: Role,
        field: Field<'_>,
        literals: &[JsonValue],
        limits: &Limits,
    ) -> Result<Value, NoVerdict> {
        // The nesting the program's text gives a subschema's reference, and
        // the reference itself.
        let reference = |address: String| {
            let mut object = Map::new();
            object.insert("$ref".into(), Value::String(address));
            Value::Object(object)
        };
        let nested = |depth: usize, value: Value| {
            if depth > limits.compile_json_depth {
                Err(compile_json_depth())
            } else {
                Ok(value)
            }
        };
        Ok(match (role, field) {
            (Role::Number, Field::Copied(value)) => self.number(key, value.text()),
            (Role::Size, Field::Copied(value)) => Value::from(
                openbindings_internal_json::decimal::Decimal::parse(value.text())
                    .nonnegative_integer_u64_saturating()
                    .expect("admitted size operand"),
            ),
            (Role::Const, Field::Literal(index)) => {
                let value = literal(index, literals)?.clone();
                let id = self.literals.len();
                self.literals.push(Literal::Value(value));
                Value::from(id)
            }
            (Role::Enum, Field::Literal(index)) => {
                let choices: Vec<_> = literal(index, literals)?
                    .view()
                    .elements()
                    .expect("admitted enum array")
                    .map(|v| v.to_owned())
                    .collect();
                let id = self.literals.len();
                self.literals.push(Literal::Choices(choices.into()));
                Value::Array(vec![Value::from(id)])
            }
            (Role::Read | Role::DynamicAnchor, Field::Copied(value)) => read(value, limits)?,
            (Role::Read, Field::Reference(uri)) => Value::String(uri),
            (Role::Read, Field::Single(address)) => nested(1, reference(address))?,
            (Role::Read, Field::Map(mut children)) => {
                // In the order the program's text holds them: by name, each
                // once, the last of a repeated one. A map that keeps its
                // insertion order, as serde_json's `preserve_order` makes
                // it, then holds them as the text's parse did.
                children.sort_by(|a, b| a.0.cmp(b.0));
                let mut children = children.into_iter().peekable();
                let mut object = Map::new();
                while let Some((name, address)) = children.next() {
                    if children.peek().is_some_and(|next| next.0 == name) {
                        continue;
                    }
                    object.insert(name.into(), reference(address));
                }
                nested(if object.is_empty() { 1 } else { 2 }, Value::Object(object))?
            }
            (Role::Read, Field::List(children)) => nested(
                if children.is_empty() { 1 } else { 2 },
                Value::Array(children.into_iter().map(reference).collect()),
            )?,
            // A projection writes each keyword in one shape.
            _ => return Err(crate::program_failure()),
        })
    }
    // A numeric keyword's operand: its handle.
    fn number(&mut self, keyword: &str, token: &str) -> Value {
        if let Some(at) = numeric::KEYWORDS.iter().position(|k| *k == keyword) {
            self.numeric[at] = true;
        }
        self.numbers.insert(token)
    }
    /// Whether two conversions separated the same literals.
    #[cfg(feature = "conversion-check")]
    pub(crate) fn same_literals(&self, other: &Self) -> bool {
        let texts = |operands: &Self| -> Vec<Vec<String>> {
            operands
                .literals
                .iter()
                .map(|literal| match literal {
                    Literal::Value(value) => vec![value.text().to_owned()],
                    Literal::Choices(choices) => {
                        choices.iter().map(|c| c.text().to_owned()).collect()
                    }
                })
                .collect()
        };
        texts(self) == texts(other)
    }
    /// Whether a `pattern` keyword with this form already has a matcher, so
    /// another adds nothing.
    pub(crate) fn has_pattern(&self, form: &str) -> bool {
        self.indices.contains_key(form)
    }
    /// The value a schema's `pattern` keyword holds for `reading`: the index
    /// of its matcher, which the SDK's keyword finds by it. Patterns with the
    /// same form share one matcher.
    pub(crate) fn pattern(&mut self, reading: crate::pattern::Reading) -> String {
        use crate::pattern::{Reading, Unsupported};
        let key = match &reading {
            Reading::Linear(linear) => linear.form.as_str(),
            Reading::Unsupported(unsupported) => match unsupported {
                Unsupported::Backtracking => BACKTRACKING,
                Unsupported::PropertyEscape => PROPERTY_ESCAPE,
                Unsupported::NonWordBoundary => NON_WORD_BOUNDARY,
                // Reading finds no shortfall: a pattern meets one only when it
                // is compiled.
                Unsupported::Size | Unsupported::Memory(_) => SIZE,
            },
        };
        let index = match self.indices.get(key) {
            Some(&index) => index,
            None => {
                let key: Arc<str> = Arc::from(key);
                let matcher = match reading {
                    Reading::Linear(linear) => match linear.simple {
                        Some(simple) => Matcher::Simple(simple),
                        None => Matcher::Regex {
                            form: key.clone(),
                            compiled: OnceLock::new(),
                        },
                    },
                    Reading::Unsupported(unsupported) => Matcher::Unmatched(unsupported),
                };
                let index = self.patterns.len();
                self.patterns.push(matcher);
                self.indices.insert(key, index);
                index
            }
        };
        index.to_string()
    }
    /// The library's options with the SDK's keywords, and the `pattern`
    /// keyword's matchers, which compile within the value contract's `memory`.
    /// Only the keywords the bundle can hold are registered: the work marker
    /// with `markers`, the missing keyword with `missing`, and the literal,
    /// numeric and `pattern` keywords this value contract's schemas hold. The
    /// library compiles a keyword the bundle does not hold the same whether
    /// it is registered or not: it never meets it.
    pub(crate) fn apply<R>(
        self,
        mut options: ValidationOptions<'_, R, FlatJson>,
        memory: Memory,
        markers: bool,
        missing: bool,
    ) -> (ValidationOptions<'_, R, FlatJson>, Arc<Patterns>) {
        if markers {
            options = options.with_keyword(crate::bundle::MARKER, |_, _, _| Ok(Box::new(Work)));
        }
        if missing {
            options = options.with_keyword(crate::missing::KEYWORD, |_, value, _| {
                let at = value
                    .as_u64()
                    .and_then(|at| u32::try_from(at).ok())
                    .ok_or_else(|| ValidationError::custom("invalid internal stand-in index"))?;
                Ok(Box::new(Missing(at)))
            });
        }
        // Only the matchers outlive preparation; their index by form does not.
        drop(self.indices);
        let matched = !self.patterns.is_empty();
        let patterns = Arc::new(Patterns {
            matchers: self.patterns,
            memory,
            shared: Mutex::new(false),
            sites: Mutex::new(Vec::new()),
        });
        let keyword = patterns.clone();
        if matched {
            options = options.with_keyword("pattern", move |_, value, at| {
                let index = value
                    .as_str()
                    .and_then(|key| key.parse::<usize>().ok())
                    .filter(|&index| index < keyword.matchers.len())
                    .ok_or_else(|| ValidationError::custom("invalid internal pattern reference"))?;
                // The library compiles each keyword as it builds; a contract built
                // again records its keywords again.
                let mut sites = keyword.sites.lock().unwrap_or_else(PoisonError::into_inner);
                let site = u32::try_from(sites.len())
                    .map_err(|_| ValidationError::custom("too many internal pattern sites"))?;
                sites.push(at);
                drop(sites);
                Ok(Box::new(Pattern(keyword.clone(), index, site)))
            });
        }
        let numbers = Arc::new(self.numbers);
        for (keyword, _) in numeric::KEYWORDS
            .into_iter()
            .zip(self.numeric)
            .filter(|&(_, held)| held)
        {
            let numbers = numbers.clone();
            options = options.with_keyword(keyword, move |_, value, _| {
                if value.as_number().is_none() {
                    return Err(ValidationError::custom("numeric keyword requires a number"));
                }
                let bound = numbers
                    .bound(keyword, value)
                    .map_err(ValidationError::custom)?;
                Ok(Box::new(ExactBound(bound)))
            });
        }
        let held = |choices: bool| {
            self.literals
                .iter()
                .any(|literal| matches!(literal, Literal::Choices(_)) == choices)
        };
        let keywords = [("const", held(false)), ("enum", held(true))];
        let literals = Arc::new(self.literals);
        for (keyword, _) in keywords.into_iter().filter(|&(_, held)| held) {
            let literals = literals.clone();
            options = options.with_keyword(keyword, move |_, value, _| {
                let id = if keyword == "enum" {
                    value
                        .as_array()
                        .and_then(|v| v.first())
                        .and_then(Value::as_u64)
                } else {
                    value.as_u64()
                };
                let literal = id
                    .and_then(|id| literals.get(id as usize))
                    .ok_or_else(|| ValidationError::custom("invalid internal literal reference"))?;
                Ok(Box::new(Constraint(literal.clone())))
            });
        }
        (options, patterns)
    }
}
/// The separated literal at `index`.
fn literal(index: usize, literals: &[JsonValue]) -> Result<&JsonValue, NoVerdict> {
    literals.get(index).ok_or_else(missing_literal)
}
fn missing_literal() -> NoVerdict {
    no_verdict(
        NoVerdictReason::EvaluatorFailure,
        "program-literal",
        "a projected literal is missing",
    )
}
fn compile_json_depth() -> NoVerdict {
    no_verdict(
        NoVerdictReason::LimitExceeded,
        "compile-json-depth",
        "projected schema structure exceeds the dependency compilation-depth admission limit",
    )
}
/// The separated literal a projected `const` or `enum` names by its index.
#[cfg(feature = "conversion-check")]
pub(crate) fn separated<'a>(
    index: JsonRef<'_>,
    literals: &'a [JsonValue],
) -> Result<&'a JsonValue, NoVerdict> {
    index
        .number_text()
        .and_then(|index| index.parse::<usize>().ok())
        .and_then(|index| literals.get(index))
        .ok_or_else(missing_literal)
}
fn read(value: JsonRef<'_>, limits: &Limits) -> Result<Value, NoVerdict> {
    if !backend::depth_at_most(&value.to_owned(), limits.compile_json_depth) {
        return Err(compile_json_depth());
    }
    if let Some(value) = direct(value) {
        return Ok(value);
    }
    let mut decoder = serde_json::Deserializer::from_slice(value.text().as_bytes());
    decoder.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut decoder).map_err(|_| {
        no_verdict(
            NoVerdictReason::EvaluatorFailure,
            "program-decoding",
            "the projected schema could not be decoded",
        )
    })
}
// The value read straight from the arena; `None` when it holds a number or a
// string with an unpaired unit, which the text decoder reads instead.
fn direct(value: JsonRef<'_>) -> Option<Value> {
    Some(match value.kind() {
        JsonKind::Null => Value::Null,
        JsonKind::Boolean => Value::Bool(value.as_bool()?),
        JsonKind::String => Value::String(value.as_str()?.to_owned()),
        JsonKind::Number => return None,
        JsonKind::Array => Value::Array(value.elements()?.map(direct).collect::<Option<_>>()?),
        JsonKind::Object => {
            let mut object = Map::new();
            for member in value.members()? {
                object.insert(member.name.as_str()?.to_owned(), direct(member.value)?);
            }
            Value::Object(object)
        }
    })
}
