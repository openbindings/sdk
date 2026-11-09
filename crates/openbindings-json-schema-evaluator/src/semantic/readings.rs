//! Complete active-keyword presence at an admitted original schema position.
//! A missing certificate always reads the original; present operands do too.

macro_rules! keywords {
    ($($name:ident = $bit:literal => $keyword:literal),+ $(,)?) => {
        $(const $name: u64 = 1 << $bit;)+
        fn bit(keyword: &str) -> Option<u64> {
            match keyword {
                $($keyword => Some($name),)+
                _ => None,
            }
        }
    };
}
keywords! {
    TYPE = 0 => "type",
    CONST = 1 => "const",
    ENUM = 2 => "enum",
    MINIMUM = 3 => "minimum",
    MAXIMUM = 4 => "maximum",
    EXCLUSIVE_MINIMUM = 5 => "exclusiveMinimum",
    EXCLUSIVE_MAXIMUM = 6 => "exclusiveMaximum",
    MULTIPLE_OF = 7 => "multipleOf",
    MIN_LENGTH = 8 => "minLength",
    MAX_LENGTH = 9 => "maxLength",
    MIN_ITEMS = 10 => "minItems",
    MAX_ITEMS = 11 => "maxItems",
    MIN_PROPERTIES = 12 => "minProperties",
    MAX_PROPERTIES = 13 => "maxProperties",
    UNIQUE_ITEMS = 14 => "uniqueItems",
    REQUIRED = 15 => "required",
    DEPENDENT_REQUIRED = 16 => "dependentRequired",
    DEPENDENCIES = 17 => "dependencies",
    FORMAT = 18 => "format",
    PATTERN = 19 => "pattern",
    REF = 20 => "$ref",
    DYNAMIC_REF = 21 => "$dynamicRef",
    RECURSIVE_REF = 22 => "$recursiveRef",
    ALL_OF = 23 => "allOf",
    ANY_OF = 24 => "anyOf",
    ONE_OF = 25 => "oneOf",
    NOT = 26 => "not",
    IF = 27 => "if",
    THEN = 28 => "then",
    ELSE = 29 => "else",
    DEPENDENT_SCHEMAS = 30 => "dependentSchemas",
    PROPERTIES = 31 => "properties",
    PATTERN_PROPERTIES = 32 => "patternProperties",
    ADDITIONAL_PROPERTIES = 33 => "additionalProperties",
    PROPERTY_NAMES = 34 => "propertyNames",
    UNEVALUATED_PROPERTIES = 35 => "unevaluatedProperties",
    ITEMS = 36 => "items",
    PREFIX_ITEMS = 37 => "prefixItems",
    ADDITIONAL_ITEMS = 38 => "additionalItems",
    CONTAINS = 39 => "contains",
    MIN_CONTAINS = 40 => "minContains",
    MAX_CONTAINS = 41 => "maxContains",
    UNEVALUATED_ITEMS = 42 => "unevaluatedItems",
}
const ORIGINAL: u64 = 1 << 63;
const LEAF: u64 = 1 << 62;
const REFERENCED_TARGET: u64 = 1 << 43;
const SHARED_TARGET: u64 = 1 << 44;
const LOGICAL: u64 = ALL_OF | ANY_OF | ONE_OF | NOT | IF | THEN | ELSE;
const OBJECT: u64 = DEPENDENT_SCHEMAS
    | DEPENDENCIES
    | PROPERTIES
    | PATTERN_PROPERTIES
    | ADDITIONAL_PROPERTIES
    | PROPERTY_NAMES
    | UNEVALUATED_PROPERTIES;
const ARRAY: u64 = ITEMS
    | PREFIX_ITEMS
    | ADDITIONAL_ITEMS
    | CONTAINS
    | MIN_CONTAINS
    | MAX_CONTAINS
    | UNEVALUATED_ITEMS;

#[derive(Clone, Copy)]
pub(super) struct Fields(u64);
impl Fields {
    pub(super) const ORIGINAL: Self = Self(ORIGINAL);
    pub(super) const EMPTY: Self = Self(0);

    /// Only call while accumulating a separate, not-yet-published certificate.
    pub(super) fn include(&mut self, keyword: &str) {
        if let Some(bit) = bit(keyword) {
            self.0 |= bit;
        }
    }
    pub(super) fn certify_leaf(&mut self) {
        self.0 |= LEAF;
    }
    pub(super) fn is_leaf(self) -> bool {
        self.0 & (ORIGINAL | LEAF) == LEAF
    }
    pub(super) fn note_reference_target(&mut self) {
        if self.is_leaf() {
            if self.0 & REFERENCED_TARGET != 0 {
                self.0 |= SHARED_TARGET;
            }
            self.0 |= REFERENCED_TARGET;
        }
    }
    pub(super) fn is_shared_leaf(self) -> bool {
        self.is_leaf() && self.0 & SHARED_TARGET != 0
    }
    pub(super) fn may_read(self, keyword: &str) -> bool {
        self.0 & ORIGINAL != 0 || bit(keyword).is_none_or(|bit| self.0 & bit != 0)
    }
    pub(super) fn logical(self) -> bool {
        self.0 & (ORIGINAL | LOGICAL) != 0
    }
    pub(super) fn object(self) -> bool {
        self.0 & (ORIGINAL | OBJECT) != 0
    }
    pub(super) fn array(self) -> bool {
        self.0 & (ORIGINAL | ARRAY) != 0
    }
}
