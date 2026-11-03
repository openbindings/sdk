//! Bound expanded URI spelling before constructing any expanded identity index.
use crate::{Control, Failure, FailureReason, schema_index::SchemaIndex, uri};
use std::collections::HashMap;

/// Each expanded identifier can be retained in several independent indexes.
/// Keep this spelling budget below the evaluator's 64 MiB projection budget.
pub const DEFAULT_IDENTITY_BYTES: usize = 8 * 1024 * 1024;

pub fn exceeded() -> Failure {
    Failure::new(
        FailureReason::LimitExceeded,
        "schema-identity-bytes",
        "expanded schema identities exceed their configured byte budget",
    )
}
/// A conservative sum of resolved-identifier lengths, without resolving or
/// retaining those strings. Relative references may shrink after dot removal;
/// accounting deliberately does not depend on materializing that result.
pub fn measure(
    index: &SchemaIndex,
    retrieval: Option<&str>,
    maximum: usize,
    control: &dyn Control,
) -> Result<usize, Failure> {
    let mut used = 0usize;
    let mut charge = |bytes: usize| -> Result<(), Failure> {
        used = used
            .checked_add(bytes)
            .filter(|total| *total <= maximum)
            .ok_or_else(exceeded)?;
        Ok(())
    };
    let mut bases = HashMap::new();
    bases.insert(0, retrieval.map(str::len));
    if let Some(retrieval) = retrieval {
        charge(retrieval.len())?;
    }
    for (at, node) in index.nodes.iter().enumerate() {
        control.check()?;
        let value = index.value(node);
        if node.resource as usize == at + 1
            && let Some(id) = value.get("$id")
        {
            let parent = bases.get(&index.parent_resource(at)).copied().flatten();
            let length = match id.as_str().filter(|id| uri::valid(id)) {
                Some(id) if uri::absolute(id) => Some(id.len()),
                Some(id) => match parent {
                    Some(base) => Some(
                        base.checked_add(id.len())
                            .and_then(|bytes| bytes.checked_add(1))
                            .ok_or_else(exceeded)?,
                    ),
                    None => None,
                },
                None => None,
            };
            if let Some(length) = length {
                charge(length)?;
            }
            bases.insert(node.resource, length);
        }
        if node.dialect == Some(crate::dialect::Dialect::Draft7)
            && !node.dialect.unwrap().reference_object(value)
            && let Some(name) = value
                .get("$id")
                .and_then(|v| v.as_str())
                .and_then(|id| id.strip_prefix('#'))
            && crate::dialect::draft7_name(name)
        {
            charge(name.len())?;
        }
        for keyword in ["$anchor", "$dynamicAnchor", "$schema"] {
            if keyword == "$schema" {
                if node.depth != 0 && node.resource as usize != at + 1 {
                    continue;
                }
            } else if !node
                .dialect
                .is_some_and(|d| d.active(keyword) && !d.reference_object(value))
            {
                continue;
            }

            if let Some(text) = value.get(keyword).and_then(|value| value.as_str()) {
                charge(text.len())?;
            }
        }
    }
    Ok(used)
}
