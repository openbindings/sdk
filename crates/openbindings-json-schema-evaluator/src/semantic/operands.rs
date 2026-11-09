//! Bounded optional readings of original numeric and cardinality predicates.
//! Cache policy never changes applicability: a deferred reading uses the
//! original operand at its normal evaluation site, under its normal allowance.
use super::*;
use openbindings_internal_json::numeric;

const TOKEN_BYTES: usize = 256;
const TOTAL_BYTES: usize = 65_536;
const RECORDS: usize = 1_024;

pub(super) const SIZES: [(&str, &str, JsonKind); 3] = [
    ("minLength", "maxLength", JsonKind::String),
    ("minItems", "maxItems", JsonKind::Array),
    ("minProperties", "maxProperties", JsonKind::Object),
];

pub(super) enum Reading<T> {
    Parsed(T),
    Invalid,
    Original,
}
pub(super) struct Number {
    pub keyword: &'static str,
    pub bytes: usize,
    pub reading: Reading<numeric::Bound>,
}
pub(super) struct Size {
    pub keyword: &'static str,
    pub kind: JsonKind,
    pub minimum: bool,
    pub reading: Reading<Option<u64>>,
}
pub(super) struct Prepared {
    pub numbers: Vec<Number>,
    pub sizes: Vec<Size>,
    pub contains: [Option<Reading<Option<u64>>>; 2],
}

#[derive(Default)]
pub(super) struct Preparation {
    pub records: HashMap<usize, Prepared>,
    bytes: usize,
}
impl Preparation {
    fn read<T>(
        &mut self,
        operand: JsonRef<'_>,
        control: &dyn openbindings_internal_schema::Control,
        parse: impl FnOnce(&str) -> Reading<T>,
    ) -> Result<Reading<T>, PreparationFailure> {
        let Some(text) = operand.number_text() else {
            // A marker is bounded by the record's thirteen predicate slots.
            // Do not inspect or copy a wrong-kind operand's payload.
            return Ok(Reading::Invalid);
        };
        if text.len() > TOKEN_BYTES || text.len() > TOTAL_BYTES - self.bytes {
            return Ok(Reading::Original);
        }
        // Choose the optional reading before charging. Actual exhaustion is
        // terminal; neither this method nor its caller catches it to defer.
        control.charge_work(text.len().saturating_mul(8).saturating_add(1))?;
        control.charge_allocation(text.len().saturating_mul(16).saturating_add(256))?;
        self.bytes += text.len();
        Ok(parse(text))
    }

    pub fn add(
        &mut self,
        node: usize,
        schema: Interpreted<'_>,
        control: &dyn openbindings_internal_schema::Control,
    ) -> Result<(), PreparationFailure> {
        if self.records.len() == RECORDS {
            return Ok(());
        }
        let numbers = numeric::KEYWORDS.map(|key| (key, schema.get(key)));
        let sizes: [_; 6] = std::array::from_fn(|index| {
            let (minimum, maximum, kind) = SIZES[index / 2];
            let key = if index % 2 == 0 { minimum } else { maximum };
            (key, kind, index % 2 == 0, schema.get(key))
        });
        let contains = ["minContains", "maxContains"]
            .map(|key| schema.get("contains").and_then(|_| schema.get(key)));
        let number_count = numbers.iter().filter(|(_, value)| value.is_some()).count();
        let size_count = sizes
            .iter()
            .filter(|(_, _, _, value)| value.is_some())
            .count();
        if number_count == 0 && size_count == 0 && contains.iter().all(Option::is_none) {
            return Ok(());
        }
        // Reserve the map record and both exactly-sized vectors before they
        // allocate. The map allowance covers growth/old+new buckets as in the
        // original preparation indexes; no authored numeric text is copied here.
        control.charge_allocation(
            (std::mem::size_of::<(usize, Prepared)>() + 64)
                .saturating_mul(8)
                .saturating_add(number_count * std::mem::size_of::<Number>())
                .saturating_add(size_count * std::mem::size_of::<Size>())
                .saturating_add(128),
        )?;
        let mut prepared = Prepared {
            numbers: Vec::with_capacity(number_count),
            sizes: Vec::with_capacity(size_count),
            contains: [None, None],
        };
        // Count copies remove both parsing and conversion from repeated calls.
        // Prefer them within the bounded cache; evaluation order is unchanged.
        for (keyword, kind, minimum, operand) in sizes {
            if let Some(operand) = operand {
                let reading = self.read(operand, control, |text| {
                    Reading::Parsed(Decimal::parse(text).nonnegative_integer_u64_saturating())
                })?;
                prepared.sizes.push(Size {
                    keyword,
                    kind,
                    minimum,
                    reading,
                });
            }
        }
        for (slot, operand) in prepared.contains.iter_mut().zip(contains) {
            if let Some(operand) = operand {
                *slot = Some(self.read(operand, control, |text| {
                    Reading::Parsed(Decimal::parse(text).nonnegative_integer_u64_saturating())
                })?);
            }
        }
        for (keyword, operand) in numbers {
            if let Some(operand) = operand {
                let bytes = operand.number_text().map_or(0, str::len);
                let reading = self.read(operand, control, |text| {
                    numeric::Bound::parse(keyword, text).map_or(Reading::Invalid, Reading::Parsed)
                })?;
                prepared.numbers.push(Number {
                    keyword,
                    bytes,
                    reading,
                });
            }
        }
        self.records.insert(node, prepared);
        Ok(())
    }
}
