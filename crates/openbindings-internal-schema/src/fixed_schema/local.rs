//! Charged read extent of the fixed, localized standard meta-schema checks.
//! Their immediate object/Boolean schema children and the explicitly audited
//! reference-string operands have type-only checks.
//! The complete localized-resource snapshot and read-shape controls bind this
//! table to those validators. This table does not apply to actual custom metas.
use crate::{Control, Failure, FailureReason, JsonKind, JsonRef, dialect::Dialect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Children {
    Direct,
    Map,
    Array,
}

fn children(keyword: &str, value: JsonRef<'_>, dialect: Dialect) -> Option<Children> {
    use Children::{Array, Direct, Map};
    Some(match keyword {
        "definitions" | "properties" | "patternProperties" | "dependencies" => Map,
        "$defs" | "dependentSchemas" if dialect != Dialect::Draft7 => Map,
        "items" if dialect != Dialect::Draft202012 && value.kind() == JsonKind::Array => Array,
        "allOf" | "anyOf" | "oneOf" => Array,
        "prefixItems" if dialect == Dialect::Draft202012 => Array,
        "additionalItems" if dialect != Dialect::Draft202012 => Direct,
        "additionalProperties"
        | "propertyNames"
        | "items"
        | "contains"
        | "not"
        | "if"
        | "then"
        | "else" => Direct,
        "unevaluatedProperties" | "unevaluatedItems" | "contentSchema"
            if dialect != Dialect::Draft7 =>
        {
            Direct
        }
        _ => return None,
    })
}

// The complete fixed graph and non-asserting format policy are guarded in
// tests. 2020-12 also constrains the deprecated $recursiveRef string. Draft 7's
// reference-object shortcut reads as_str metadata (and rejects lone surrogates).
fn reference_string(keyword: &str, dialect: Dialect) -> bool {
    match keyword {
        "$ref" => true,
        "$recursiveRef" => dialect != Dialect::Draft7,
        "$dynamicRef" => dialect == Dialect::Draft202012,
        _ => false,
    }
}

fn invalid_extent() -> Failure {
    Failure::new(
        FailureReason::EvaluatorFailure,
        "fixed-meta-local-extent",
        "fixed syntax child extents are not disjoint within their original parent",
    )
}

fn subtract(bytes: usize, child: usize) -> Result<usize, Failure> {
    bytes
        .checked_sub(child)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(invalid_extent)
}

fn cut(bytes: &mut usize, child: JsonRef<'_>, control: &dyn Control) -> Result<(), Failure> {
    control.charge_work(1)?;
    // Wrong kinds, and especially dependencies' string arrays in ALL editions,
    // retain their complete extent. Each cut is one disjoint immediate token.
    if matches!(child.kind(), JsonKind::Object | JsonKind::Boolean) {
        *bytes = subtract(*bytes, child.text().len())?;
    }
    Ok(())
}

fn extent(value: JsonRef<'_>, dialect: Dialect, control: &dyn Control) -> Result<usize, Failure> {
    control.check()?;
    let mut bytes = value.text().len();
    for field in value.members().into_iter().flatten() {
        let work = field
            .name
            .text()
            .len()
            .checked_add(1)
            .ok_or_else(invalid_extent)?;
        control.charge_work(work)?;
        let Some(keyword) = field.name.as_str() else {
            continue;
        };
        if reference_string(keyword, dialect) && field.value.kind() == JsonKind::String {
            control.charge_work(1)?;
            bytes = subtract(bytes, field.value.text().len())?;
            continue;
        }
        match children(keyword, field.value, dialect) {
            Some(Children::Direct) => cut(&mut bytes, field.value, control)?,
            Some(Children::Map) => {
                for child in field.value.members().into_iter().flatten() {
                    cut(&mut bytes, child.value, control)?;
                }
            }
            Some(Children::Array) => {
                for child in field.value.elements().into_iter().flatten() {
                    cut(&mut bytes, child, control)?;
                }
            }
            None => {}
        }
    }
    Ok(bytes)
}

fn reserve_bytes(bytes: usize, control: &dyn Control) -> Result<(), Failure> {
    let allocation = bytes.checked_mul(64).and_then(|n| n.checked_add(4096));
    let Some(allocation) = allocation else {
        control.allocation_limit_reached();
        return Err(Failure::new(
            FailureReason::LimitExceeded,
            "preparation-sdk-reservation-bytes",
            "fixed syntax checking exceeded its allocation allowance",
        ));
    };
    // The larger allocation expression already proves this one cannot overflow.
    let work = bytes * 64 + 1;
    control.charge_allocation(allocation)?;
    control.charge_work(work)
}

pub(super) fn reserve(
    value: JsonRef<'_>,
    dialect: Dialect,
    control: &dyn Control,
) -> Result<(), Failure> {
    reserve_bytes(extent(value, dialect, control)?, control)
}

#[cfg(test)]
mod tests;
