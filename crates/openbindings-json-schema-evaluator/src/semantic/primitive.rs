//! Primitive assertions read their original operands. No synthetic document,
//! URI context, string conversion or secondary compiler participates here.
use super::*;
use openbindings_internal_json::numeric;

fn shape() -> NoVerdict {
    no_verdict(
        NoVerdictReason::UnsupportedCapability,
        "semantic-predicate-schema",
        "a primitive operand does not have its vocabulary's required shape",
    )
}

impl Run<'_> {
    pub(super) fn number(&mut self, value: JsonRef<'_>) -> Result<Decimal, NoVerdict> {
        let text = value.number_text().ok_or_else(shape)?;
        self.graph
            .work(text.len().saturating_mul(8).saturating_add(1))?;
        self.graph
            .charge(text.len().saturating_mul(16).saturating_add(256))?;
        Ok(Decimal::parse(text))
    }
    fn typed(&mut self, name: JsonRef<'_>, value: JsonRef<'_>) -> Result<bool, NoVerdict> {
        Ok(match name.as_str().ok_or_else(shape)? {
            "null" => value.kind() == JsonKind::Null,
            "boolean" => value.kind() == JsonKind::Boolean,
            "object" => value.kind() == JsonKind::Object,
            "array" => value.kind() == JsonKind::Array,
            "string" => value.kind() == JsonKind::String,
            "number" => value.kind() == JsonKind::Number,
            "integer" => value.kind() == JsonKind::Number && self.number(value)?.is_integer(),
            _ => return Err(shape()),
        })
    }
    fn required(&mut self, names: JsonRef<'_>, value: JsonRef<'_>) -> Result<bool, NoVerdict> {
        let mut valid = true;
        for name in names.elements().ok_or_else(shape)? {
            self.graph.tick()?;
            if name.kind() != JsonKind::String {
                return Err(shape());
            }
            if value.kind() == JsonKind::Object && self.named(value, name)?.is_none() {
                valid = false;
            }
        }
        Ok(valid)
    }
    pub(super) fn primitives(
        &mut self,
        schema: &Interpreted<'_>,
        value: JsonRef<'_>,
    ) -> Result<bool, NoVerdict> {
        let mut valid = true;
        if let Some(types) = schema.get("type") {
            if let Some(names) = types.elements() {
                let mut any = false;
                for name in names {
                    self.graph.tick()?;
                    any |= self.typed(name, value)?;
                }
                valid &= any;
            } else {
                valid &= self.typed(types, value)?;
            }
        }
        if let Some(constant) = schema.get("const") {
            let actual = self.values.intern(value, &mut self.graph)?;
            valid &= actual == self.values.intern(constant, &mut self.graph)?;
        }
        if let Some(choices) = schema.get("enum") {
            let actual = self.values.intern(value, &mut self.graph)?;
            let mut any = false;
            for choice in choices.elements().ok_or_else(shape)? {
                self.graph.tick()?;
                any |= actual == self.values.intern(choice, &mut self.graph)?;
            }
            valid &= any;
        }
        for keyword in numeric::KEYWORDS {
            let Some(operand) = schema.get(keyword) else {
                continue;
            };
            let source = operand.number_text().ok_or_else(shape)?;
            self.graph
                .work(source.len().saturating_mul(8).saturating_add(1))?;
            self.graph
                .charge(source.len().saturating_mul(16).saturating_add(256))?;
            let bound = numeric::Bound::parse(keyword, source).map_err(|_| shape())?;
            if let Some(number) = value.number_text() {
                self.graph
                    .work(number.len().saturating_mul(8).saturating_add(1))?;
                self.graph
                    .charge(number.len().saturating_mul(16).saturating_add(256))?;
                if keyword == "multipleOf" && (source.len() > 20 || number.len() > 20) {
                    // The shared exact arithmetic admits at most 4096 digits
                    // per coefficient and a 40000-place exponent difference.
                    // Reserve bounded big-integer scratch before its modular
                    // exponentiation, independently of retained decimal text.
                    let digits = source.len().saturating_add(number.len()).min(8192);
                    self.graph
                        .charge(digits.saturating_mul(256).saturating_add(65536))?;
                    self.graph
                        .work(digits.saturating_mul(digits).saturating_mul(16))?;
                }
                valid &= bound
                    .admits(number)
                    .ok_or_else(|| decision::limit("multiple-of-arithmetic-limit"))?;
            }
        }
        let mut string_length = None;
        for (minimum, maximum, kind) in [
            ("minLength", "maxLength", JsonKind::String),
            ("minItems", "maxItems", JsonKind::Array),
            ("minProperties", "maxProperties", JsonKind::Object),
        ] {
            for keyword in [minimum, maximum] {
                let Some(operand) = schema.get(keyword) else {
                    continue;
                };
                let bound = self
                    .number(operand)?
                    .nonnegative_integer_u64_saturating()
                    .ok_or_else(shape)?;
                if value.kind() != kind {
                    continue;
                }
                let length = if kind == JsonKind::String {
                    if let Some(length) = string_length {
                        length
                    } else {
                        let mut length = 0u64;
                        let mut high = false;
                        for unit in value.as_string().expect("string").utf16_units() {
                            self.graph.tick()?;
                            if !(high && (0xdc00..=0xdfff).contains(&unit)) {
                                length += 1;
                            }
                            high = (0xd800..=0xdbff).contains(&unit);
                        }
                        string_length = Some(length);
                        length
                    }
                } else {
                    value.len().expect("container") as u64
                };
                valid &= if keyword == minimum {
                    length >= bound
                } else {
                    length <= bound
                };
            }
        }
        if let Some(unique) = schema.get("uniqueItems") {
            let unique = unique.as_bool().ok_or_else(shape)?;
            if unique && let Some(items) = value.elements() {
                let mut seen = HashSet::new();
                for item in items {
                    self.graph.tick()?;
                    let identity = self.values.intern(item, &mut self.graph)?;
                    self.graph.charge(64)?;
                    valid &= seen.insert(identity);
                }
            }
        }
        if let Some(names) = schema.get("required") {
            valid &= self.required(names, value)?;
        }
        for keyword in ["dependentRequired", "dependencies"] {
            let Some(dependencies) = schema.get(keyword) else {
                continue;
            };
            for dependency in dependencies.members().ok_or_else(shape)? {
                self.graph.tick()?;
                if keyword == "dependencies" && dependency.value.kind() != JsonKind::Array {
                    continue;
                }
                let applies = value.kind() == JsonKind::Object
                    && self.named(value, dependency.name)?.is_some();
                // Validate operand shape even when its triggering property is
                // absent. An array instance does not activate required checks.
                valid &= self.required(
                    dependency.value,
                    if applies { value } else { dependency.value },
                )?;
            }
        }
        Ok(valid)
    }
}
