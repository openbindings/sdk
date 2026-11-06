//! The SDK's keywords and the library's copy of each projected resource.
//! Literal values stay in the exact flat representation: only the shallow
//! projected schema structure crosses the dependency's recursive serde boundary.
use crate::compiled::pattern::{Allowance, Given, Memory, Unsupported};
use crate::compiled::{Limits, no_verdict};
use jsonschema::json::{JsonNumber, Node};
use jsonschema::{Keyword, ValidationError, ValidationOptions};
use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson, View};
use openbindings_internal_json::numeric;
use openbindings_internal_schema::{Definition, Field, Schema};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

/// Each string `pattern` keyword's source, by its definition's number.
/// Preparation reads a pattern from the projection rather than from a copy:
/// the library never meets the source, as reading writes its matcher's index
/// in its place, so the converted schema holds `null` there until then.
pub(crate) type Sources<'a> = std::collections::HashMap<u32, Cow<'a, str>>;

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
            crate::compiled::pass::arithmetic();
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
/// The bytes a run matcher's place among a value schema's matchers takes,
/// as 64-bit hosts lay a matcher out. A 32-bit host takes less, and the
/// pattern memory counts this on every host, so no outcome depends on it.
const MATCHER: usize = 104;
/// The bytes a value schema holds for a run matcher: its place among the
/// schema's matchers and the heap bytes it holds.
pub(crate) fn run_matcher_bytes(matcher: &crate::compiled::pattern::Simple) -> usize {
    MATCHER + matcher.heap_bytes()
}
/// How the SDK's `pattern` keyword matches one pattern.
// An explicit tag preserves the counted slot layout when Simple is shared.
#[repr(C)]
enum Matcher {
    Simple(Arc<crate::compiled::pattern::Simple>),
    // Completed under the shared preparation control before publication.
    Regex {
        form: Arc<str>,
        compiled: OnceLock<Compiled>,
    },
}
enum Compiled {
    Regex(regex::Regex),
    Refused(Unsupported),
}
/// Completed private matchers sharing the unchanged construction allowance.
pub(crate) struct Patterns {
    matchers: Box<[Matcher]>,
    memory: Memory,
}
impl Patterns {
    pub(super) fn complete(&self, control: &dyn WorkControl) -> Result<(), NoVerdict> {
        control.check()?;
        self.share(&[], &mut |_| unreachable!("no property names"), control);
        control.check()?;
        for matcher in &self.matchers {
            if let Matcher::Regex { compiled, .. } = matcher {
                match compiled.get() {
                    Some(Compiled::Regex(_)) => {}
                    Some(Compiled::Refused(reason)) => {
                        return Err(crate::compiled::unsupported_pattern(*reason, "pattern"));
                    }
                    None => return Err(crate::compiled::program_failure()),
                }
            }
        }
        Ok(())
    }
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
        control: &dyn WorkControl,
    ) -> Vec<Result<usize, Unsupported>> {
        self.share(names, together, control)
    }
    fn share(
        &self,
        names: &[&str],
        together: &mut dyn FnMut(Allowance) -> bool,
        control: &dyn WorkControl,
    ) -> Vec<Result<usize, Unsupported>> {
        enum Owner {
            Name(usize),
            Matcher(usize),
        }
        let first = self.memory.first();
        let mut given = vec![Ok(first.compiled); names.len()];
        let (mut forms, mut owners) = (Vec::new(), Vec::new());
        for (at, name) in names.iter().enumerate() {
            if crate::compiled::pattern::checked_compile(name, first, control).is_none() {
                forms.push(*name);
                owners.push(Owner::Name(at));
            }
        }
        for (at, matcher) in self.matchers.iter().enumerate() {
            if let Matcher::Regex { form, compiled } = matcher
                && compiled.get().is_none()
            {
                match crate::compiled::pattern::checked_compile(form, first, control) {
                    // Construction completes this immutable matcher once.
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
        let shares = self.memory.share(&forms, names_sharing, together, control);
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
        given
    }
    // An unexpected refused matcher stops the pass; it never publishes a Boolean.
    fn is_match(&self, index: usize, text: &str) -> bool {
        match &self.matchers[index] {
            Matcher::Simple(simple) => simple.is_match(text),
            Matcher::Regex { compiled, .. } => {
                match compiled
                    .get()
                    .expect("matcher completed before publication")
                {
                    Compiled::Regex(regex) => regex.is_match(text),
                    Compiled::Refused(unsupported) => {
                        crate::compiled::pass::unmatched(*unsupported);
                        true
                    }
                }
            }
        }
    }
}
// A simple keyword owns its shared matcher directly, so it does not keep the
// construction table alive. Regex keywords retain the completed table as before.
enum Pattern {
    Simple(Arc<crate::compiled::pattern::Simple>),
    Regex(Arc<Patterns>, usize),
}
impl<'i> Keyword<'i, FlatJson> for Pattern {
    fn is_valid(&self, instance: View<'i>) -> bool {
        match instance.as_string() {
            Some(text) => match self {
                Self::Simple(simple) => simple.is_match(&text),
                Self::Regex(patterns, index) => patterns.is_match(*index, &text),
            },
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
        "$dynamicAnchor" => Role::Read,
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
    pub(crate) fn definitions<'a>(
        &mut self,
        definitions: Vec<Definition<'a>>,
        literals: &[JsonValue],
        limits: &Limits,
        sources: &mut Sources<'a>,
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
                        let value = match field {
                            Field::Copied(value) if key == "pattern" => {
                                let (value, source) = pattern_source(value, limits)?;
                                if let Some(source) = source {
                                    let number = definition_number(&definition.name)
                                        .ok_or_else(crate::compiled::program_failure)?;
                                    sources.insert(number, source);
                                }
                                value
                            }
                            field => self.field(key, role, field, literals, limits)?,
                        };
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
            (Role::Read, Field::Copied(value)) => read(value, limits)?,
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
            _ => return Err(crate::compiled::program_failure()),
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
    pub(crate) fn has_pattern(&self, form: &str) -> bool {
        self.indices.contains_key(form)
    }
    /// The value a schema's `pattern` keyword holds for `reading`: the index
    /// of its matcher, which the SDK's keyword finds by it. Patterns with the
    /// same form share one matcher.
    pub(crate) fn pattern(&mut self, reading: crate::compiled::pattern::Reading) -> String {
        let key = reading.form.as_ref();
        let index = match self.indices.get(key) {
            Some(&index) => index,
            None => {
                let key = reading.form;
                let matcher = match reading.simple {
                    Some(simple) => Matcher::Simple(simple),
                    None => Matcher::Regex {
                        form: key.clone(),
                        compiled: OnceLock::new(),
                    },
                };
                let index = self.patterns.len();
                self.patterns.push(matcher);
                self.indices.insert(key, index);
                index
            }
        };
        index.to_string()
    }
    /// The SDK's keywords for this value schema, and the `pattern` keyword's
    /// matchers, which compile within the value schema's `memory`.
    pub(crate) fn keywords(self, memory: Memory) -> Keywords {
        // Only the matchers outlive preparation; their index by form does not.
        drop(self.indices);
        let matched = !self.patterns.is_empty();
        let patterns = Arc::new(Patterns {
            matchers: crate::compiled::pattern::exact(self.patterns),
            memory,
        });
        let held = |choices: bool| {
            self.literals
                .iter()
                .any(|literal| matches!(literal, Literal::Choices(_)) == choices)
        };
        let literals = [held(false), held(true)];
        Keywords {
            patterns,
            matched,
            numbers: Arc::new(self.numbers),
            numeric: self.numeric,
            literals: Arc::new(self.literals),
            held: literals,
        }
    }
}
/// The SDK's keywords a value schema's validators compile, which share its
/// literals, numeric operands and `pattern` matchers.
pub(crate) struct Keywords {
    patterns: Arc<Patterns>,
    // Whether the value schema holds a `pattern` matcher.
    matched: bool,
    numbers: Arc<numeric::Operands>,
    numeric: [bool; numeric::KEYWORDS.len()],
    literals: Arc<Vec<Literal>>,
    // Whether the value schema holds a `const` and an `enum` literal.
    held: [bool; 2],
}
impl Keywords {
    /// The value schema's `pattern` matchers.
    pub(crate) fn patterns(&self) -> &Arc<Patterns> {
        &self.patterns
    }
    /// The library's options with the SDK's keywords. Only the keywords the
    /// bundle can hold are registered: those `registered` names, and the
    /// literal, numeric and `pattern` keywords the schemas reached from this value schema
    /// hold. The library compiles a keyword the bundle does not hold the same
    /// whether it is registered or not: it never meets it.
    pub(crate) fn register<'o, R>(
        &self,
        mut options: ValidationOptions<'o, R, FlatJson>,
    ) -> ValidationOptions<'o, R, FlatJson> {
        if self.matched {
            let keyword = self.patterns.clone();
            options = options.with_keyword("pattern", move |_, value, _| {
                let index = value
                    .as_str()
                    .and_then(|key| key.parse::<usize>().ok())
                    .filter(|&index| index < keyword.matchers.len())
                    .ok_or_else(|| ValidationError::custom("invalid internal pattern reference"))?;
                let pattern = match &keyword.matchers[index] {
                    Matcher::Simple(simple) => Pattern::Simple(simple.clone()),
                    Matcher::Regex { .. } => Pattern::Regex(keyword.clone(), index),
                };
                Ok(Box::new(pattern))
            });
        }
        for (keyword, _) in numeric::KEYWORDS
            .into_iter()
            .zip(self.numeric)
            .filter(|&(_, held)| held)
        {
            let numbers = self.numbers.clone();
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
        let keywords = [("const", self.held[0]), ("enum", self.held[1])];
        for (keyword, _) in keywords.into_iter().filter(|&(_, held)| held) {
            let literals = self.literals.clone();
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
        options
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
// A definition's number, which its name `n<number>` gives.
fn definition_number(name: &str) -> Option<u32> {
    name.strip_prefix('n').and_then(|n| n.parse().ok())
}
// A `pattern` keyword's value for the library, as `read` reads it, except
// that a string is `null`, its source borrowed rather than copied.
fn pattern_source<'a>(
    value: JsonRef<'a>,
    limits: &Limits,
) -> Result<(Value, Option<Cow<'a, str>>), NoVerdict> {
    // A string holds no nesting for the depth limit to count, so `read` gives
    // exactly its decoded text.
    let source = match value.as_str() {
        Some(source) => Cow::Borrowed(source),
        None => match read(value, limits)? {
            Value::String(source) => Cow::Owned(source),
            other => return Ok((other, None)),
        },
    };
    Ok((Value::Null, Some(source)))
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_keywords_release_the_construction_table() {
        let (validator, table) = {
            let crate::pattern::Reading::Linear(reading) =
                crate::pattern::read("^ab$", 32, 1024).unwrap()
            else {
                panic!("simple reading expected")
            };
            let matched = run_matcher_bytes(reading.simple.as_ref().unwrap());
            let mut operands = Operands::default();
            let key = operands.pattern(reading.into());
            let keywords = operands.keywords(Memory::new(64 * 1024 * 1024, 0, matched));
            keywords
                .patterns()
                .complete(&std::sync::atomic::AtomicBool::new(false))
                .unwrap();
            let table = Arc::downgrade(keywords.patterns());
            let options = keywords.register(jsonschema::options_for::<FlatJson>());
            (
                options.build(&serde_json::json!({"pattern": key})).unwrap(),
                table,
            )
        };
        assert!(table.upgrade().is_none());
        for (text, expected) in [(r#""ab""#, true), (r#""abc""#, false)] {
            let value = JsonValue::parse(text).unwrap();
            assert_eq!(validator.is_valid(backend::view(&value)), expected);
        }
    }

    // A run matcher's place is counted as 64-bit hosts lay it out; no host
    // takes more.
    #[test]
    fn a_matcher_takes_the_place_counted_for_it() {
        assert!(size_of::<super::Matcher>() <= super::MATCHER);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<super::Matcher>(), super::MATCHER);
    }
}
