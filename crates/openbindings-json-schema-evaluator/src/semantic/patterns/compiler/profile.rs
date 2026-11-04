//! Private construction profile; caller options may tighten, never bypass it.
//! These are structural pressure scores, not operation or byte predictions.
use super::{inventory::Pressure, normalized};
mod budget;
pub(crate) use budget::Budget;
pub(super) use budget::Error as AdmissionError;

pub(super) const NORMALIZED_GROUPS: usize = crate::pattern::MAX_GROUP_DEPTH + 1;
pub(super) const AST_DEPTH: u32 = 250;
pub(super) const PREFIX_CALLS: usize = 64;
pub(super) const EQUALITY_HEIGHT: usize = 256;
pub(in super::super) const NFA_BYTES: usize = 32 * 1024 * 1024;
pub(super) const NFA_PATH: usize = 512;
pub(super) const NFA_DISPATCH: usize = 65_536;
pub(super) const NFA_HELPER_PAYLOAD: usize = 1_048_576;
pub(super) const NFA_EMPTY_TAIL: usize = 1_048_576;

pub(super) const AST_NODES: usize = 16_384;
pub(super) const CLASS_ITEMS: usize = 32_768;
pub(super) const RECONSTRUCTION_ENTRIES: usize = 262_144;
pub(super) const LITERAL_BYTE_REVISITS: usize = 1_048_576;
pub(super) const CANDIDATE_RANGE_SLOTS: usize = 33_554_432;

pub(super) fn ast_nodes(s: &normalized::Summary) -> Option<usize> {
    s.literals
        .checked_add(s.assertions)?
        .checked_add(s.classes)?
        .checked_add(s.groups.checked_mul(3)?)?
        .checked_add(s.repetitions)?
        .checked_add(s.alternatives.checked_mul(3)?)?
        .checked_add(2)
}
pub(super) fn admits_normalized(s: &normalized::Summary) -> bool {
    ast_nodes(s).is_some_and(|nodes| nodes <= AST_NODES) && s.class_items <= CLASS_ITEMS
}
pub(super) fn admits_translation(p: &Pressure) -> bool {
    p.reconstruction_entries <= RECONSTRUCTION_ENTRIES
        && p.literal_byte_revisits <= LITERAL_BYTE_REVISITS
        && p.candidate_range_slots <= CANDIDATE_RANGE_SLOTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_pressure_boundaries_are_inclusive_and_independent() {
        for (index, maximum) in [
            RECONSTRUCTION_ENTRIES,
            LITERAL_BYTE_REVISITS,
            CANDIDATE_RANGE_SLOTS,
        ]
        .into_iter()
        .enumerate()
        {
            for delta in [-1isize, 0, 1] {
                let mut fields = [0; 3];
                fields[index] = maximum.checked_add_signed(delta).unwrap();
                let pressure = Pressure {
                    reconstruction_entries: fields[0],
                    literal_byte_revisits: fields[1],
                    candidate_range_slots: fields[2],
                };
                assert_eq!(admits_translation(&pressure), delta <= 0);
            }
        }
    }
}
