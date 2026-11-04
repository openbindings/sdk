//! Primitive assertions use prepared readings or their exact original operands. No synthetic document,
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
        if value.number_text().is_none() {
            return Err(shape());
        }
        value_predicates::number(value, &mut self.graph)
    }
    fn numeric_operand(
        &mut self,
        schema: &Interpreted<'_>,
        keyword: &str,
        prepared: Option<&operands::Number>,
        value: JsonRef<'_>,
    ) -> Result<bool, NoVerdict> {
        use operands::Reading;
        if matches!(prepared.map(|p| &p.reading), Some(Reading::Invalid)) {
            return Err(shape());
        }
        let original = if matches!(prepared.map(|p| &p.reading), Some(Reading::Parsed(_))) {
            None
        } else {
            Some(
                schema
                    .get(keyword)
                    .and_then(|v| v.number_text())
                    .ok_or_else(shape)?,
            )
        };
        let bytes = prepared.map_or_else(|| original.expect("original operand").len(), |p| p.bytes);
        self.graph.work(bytes.saturating_mul(8).saturating_add(1))?;
        let parsed;
        let bound = if let Some(Reading::Parsed(bound)) = prepared.map(|p| &p.reading) {
            bound
        } else {
            self.graph
                .charge(bytes.saturating_mul(16).saturating_add(256))?;
            parsed = numeric::Bound::parse(keyword, original.expect("original operand"))
                .map_err(|_| shape())?;
            &parsed
        };
        let Some(number) = value.number_text() else {
            return Ok(true);
        };
        self.graph
            .work(number.len().saturating_mul(8).saturating_add(1))?;
        if let Ok(integer) = number.parse::<i64>()
            && let Some(valid) = bound.admits_integer(integer)
        {
            return Ok(valid);
        }
        // General decimal comparison may allocate magnitude scratch even with
        // a prepared bound. Only the proven integer branch above skips it.
        if matches!(prepared.map(|p| &p.reading), Some(Reading::Parsed(_))) {
            self.graph
                .charge(bytes.saturating_mul(16).saturating_add(256))?;
        }
        self.graph
            .charge(number.len().saturating_mul(16).saturating_add(256))?;
        if keyword == "multipleOf" && (bytes > 20 || number.len() > 20) {
            // The exact arithmetic admits at most 4096 coefficient digits and
            // a 40000-place exponent difference. Reserve its modular scratch.
            let digits = bytes.saturating_add(number.len()).min(8192);
            self.graph
                .charge(digits.saturating_mul(256).saturating_add(65536))?;
            self.graph
                .work(digits.saturating_mul(digits).saturating_mul(16))?;
        }
        bound
            .admits(number)
            .ok_or_else(|| decision::limit("multiple-of-arithmetic-limit"))
    }
    pub(super) fn count_operand(
        &mut self,
        schema: &Interpreted<'_>,
        keyword: &str,
        reading: Option<&operands::Reading<Option<u64>>>,
    ) -> Result<Option<u64>, NoVerdict> {
        match reading {
            Some(operands::Reading::Parsed(count)) => Ok(*count),
            Some(operands::Reading::Invalid) => Err(shape()),
            Some(operands::Reading::Original) | None => Ok(self
                .number(schema.get(keyword).ok_or_else(shape)?)?
                .nonnegative_integer_u64_saturating()),
        }
    }
    fn size_bound(
        &mut self,
        bound: u64,
        kind: JsonKind,
        minimum: bool,
        value: JsonRef<'_>,
        string_length: &mut Option<u64>,
    ) -> Result<bool, NoVerdict> {
        if value.kind() != kind {
            return Ok(true);
        }
        let length = if kind == JsonKind::String {
            if let Some(length) = *string_length {
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
                *string_length = Some(length);
                length
            }
        } else {
            value.len().expect("container") as u64
        };
        Ok(if minimum {
            length >= bound
        } else {
            length <= bound
        })
    }
    fn typed(&mut self, name: JsonRef<'_>, value: JsonRef<'_>) -> Result<bool, NoVerdict> {
        Ok(match name.as_str().ok_or_else(shape)? {
            "null" => value.kind() == JsonKind::Null,
            "boolean" => value.kind() == JsonKind::Boolean,
            "object" => value.kind() == JsonKind::Object,
            "array" => value.kind() == JsonKind::Array,
            "string" => value.kind() == JsonKind::String,
            "number" => value.kind() == JsonKind::Number,
            "integer" => {
                value.kind() == JsonKind::Number
                    && value_predicates::integer(value, &mut self.graph)?
            }
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
        node: usize,
        schema: &Interpreted<'_>,
        value: JsonRef<'_>,
    ) -> Result<Option<&'static str>, NoVerdict> {
        let mut failed = None;
        let mut record = |valid: bool, keyword: &'static str| {
            if !valid && failed.is_none() {
                failed = Some(keyword);
            }
        };
        if let Some(types) = schema.get("type") {
            if let Some(names) = types.elements() {
                let mut any = false;
                for name in names {
                    self.graph.tick()?;
                    any |= self.typed(name, value)?;
                }
                record(any, "type");
            } else {
                record(self.typed(types, value)?, "type");
            }
        }
        if let Some(constant) = schema.get("const") {
            record(
                value_predicates::literal_equal(value, constant, &mut self.graph)?,
                "const",
            );
        }
        if let Some(choices) = schema.get("enum") {
            let mut any = false;
            for choice in choices.elements().ok_or_else(shape)? {
                self.graph.tick()?;
                any |= value_predicates::literal_equal(value, choice, &mut self.graph)?;
            }
            record(any, "enum");
        }
        let prepared = self.plan.operands.get(&node);
        if let Some(prepared) = prepared {
            for number in &prepared.numbers {
                record(
                    self.numeric_operand(schema, number.keyword, Some(number), value)?,
                    number.keyword,
                );
            }
        } else {
            // A missing record is deferred preparation, never absent predicates.
            for keyword in numeric::KEYWORDS {
                if schema.get(keyword).is_some() {
                    record(self.numeric_operand(schema, keyword, None, value)?, keyword);
                }
            }
        }
        let mut string_length = None;
        if let Some(prepared) = prepared {
            for size in &prepared.sizes {
                let bound = self
                    .count_operand(schema, size.keyword, Some(&size.reading))?
                    .ok_or_else(shape)?;
                record(
                    self.size_bound(bound, size.kind, size.minimum, value, &mut string_length)?,
                    size.keyword,
                );
            }
        } else {
            for (minimum, maximum, kind) in operands::SIZES {
                for keyword in [minimum, maximum] {
                    if schema.get(keyword).is_some() {
                        let bound = self
                            .count_operand(schema, keyword, None)?
                            .ok_or_else(shape)?;
                        record(
                            self.size_bound(
                                bound,
                                kind,
                                keyword == minimum,
                                value,
                                &mut string_length,
                            )?,
                            keyword,
                        );
                    }
                }
            }
        }
        if let Some(unique) = schema.get("uniqueItems") {
            let unique = unique.as_bool().ok_or_else(shape)?;
            if unique && value.kind() == JsonKind::Array {
                record(
                    value_predicates::unique(value, &self.plan.value_hash, &mut self.graph)?,
                    "uniqueItems",
                );
            }
        }
        if let Some(names) = schema.get("required") {
            record(self.required(names, value)?, "required");
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
                record(
                    self.required(
                        dependency.value,
                        if applies { value } else { dependency.value },
                    )?,
                    keyword,
                );
            }
        }
        Ok(failed)
    }
}
