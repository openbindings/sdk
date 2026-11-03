//! Syntax-only ECMA-262 edition-11 regular expressions without flags, including
//! Annex B legacy syntax. This is the SDK's regex-format acceptance policy;
//! OBI pattern/patternProperties retain their separate mandatory Unicode mode.
//! Input is exact UTF-16 and the parser uses an explicit group stack.
use super::{Graph, NoVerdict};
use openbindings::JsonStringRef;
use std::collections::HashSet;

enum Error {
    Syntax,
    Stopped(NoVerdict),
}
impl From<NoVerdict> for Error {
    fn from(value: NoVerdict) -> Self {
        Self::Stopped(value)
    }
}
type Parsed<T> = Result<T, Error>;
#[derive(Clone, Copy)]
enum Atom {
    Unit(u16),
    Set,
}

pub(super) fn valid(value: JsonStringRef<'_>, graph: &mut Graph<'_>) -> Result<bool, NoVerdict> {
    let units: Vec<_> = value.utf16_units().collect();
    let named = has_named_capture(&units, graph)?;
    let parser = Parser {
        units,
        at: 0,
        graph,
        named,
        names: HashSet::new(),
        references: Vec::new(),
        groups: Vec::new(),
        quantifiable: false,
    };
    match parser.parse() {
        Ok(()) => Ok(true),
        Err(Error::Syntax) => Ok(false),
        Err(Error::Stopped(detail)) => Err(detail),
    }
}
fn has_named_capture(units: &[u16], graph: &mut Graph<'_>) -> Result<bool, NoVerdict> {
    let mut at = 0;
    let mut class = false;
    while at < units.len() {
        graph.tick()?;
        match units[at] {
            92 => {
                at += 2;
                continue;
            }
            91 if !class => class = true,
            93 if class => class = false,
            40 if !class
                && units.get(at + 1) == Some(&63)
                && units.get(at + 2) == Some(&60)
                && !matches!(units.get(at + 3), Some(61 | 33)) =>
            {
                return Ok(true);
            }
            _ => {}
        }
        at += 1;
    }
    Ok(false)
}
struct Parser<'a, 'b> {
    units: Vec<u16>,
    at: usize,
    graph: &'a mut Graph<'b>,
    named: bool,
    names: HashSet<Vec<u16>>,
    references: Vec<Vec<u16>>,
    groups: Vec<bool>,
    quantifiable: bool,
}
impl Parser<'_, '_> {
    fn peek(&self) -> Option<u16> {
        self.units.get(self.at).copied()
    }
    fn eat(&mut self, unit: u16) -> bool {
        if self.peek() == Some(unit) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn next(&mut self) -> Parsed<u16> {
        self.graph.tick()?;
        let unit = self.peek().ok_or(Error::Syntax)?;
        self.at += 1;
        Ok(unit)
    }
    fn parse(mut self) -> Parsed<()> {
        while self.at < self.units.len() {
            match self.next()? {
                40 => {
                    let mut quantifiable = true;
                    if self.eat(63) {
                        match self.next()? {
                            58 | 61 | 33 => {}
                            60 => {
                                if matches!(self.peek(), Some(61 | 33)) {
                                    self.at += 1;
                                    quantifiable = false;
                                } else {
                                    let name = self.name()?;
                                    self.graph
                                        .charge(name.len().saturating_mul(2).saturating_add(64))?;
                                    if !self.names.insert(name) {
                                        return Err(Error::Syntax);
                                    }
                                }
                            }
                            _ => return Err(Error::Syntax),
                        }
                    }
                    self.graph.charge(16)?;
                    self.groups.push(quantifiable);
                    self.quantifiable = false;
                }
                41 => self.quantifiable = self.groups.pop().ok_or(Error::Syntax)?,
                124 | 94 | 36 => self.quantifiable = false,
                42 | 43 | 63 => self.quantify()?,
                123 => {
                    if self.braces()? {
                        self.quantify()?;
                    } else {
                        self.quantifiable = true;
                    }
                }
                91 => {
                    self.class()?;
                    self.quantifiable = true;
                }
                92 => match self.peek() {
                    Some(98 | 66) => {
                        self.at += 1;
                        self.quantifiable = false;
                    }
                    Some(107) if self.named => {
                        self.at += 1;
                        if !self.eat(60) {
                            return Err(Error::Syntax);
                        }
                        let name = self.name()?;
                        self.graph.charge(32)?;
                        self.references.push(name);
                        self.quantifiable = true;
                    }
                    _ => {
                        self.escape(false)?;
                        self.quantifiable = true;
                    }
                },
                _ => self.quantifiable = true,
            }
        }
        if !self.groups.is_empty() {
            return Err(Error::Syntax);
        }
        for name in &self.references {
            self.graph.work(name.len().saturating_add(1))?;
            if !self.names.contains(name) {
                return Err(Error::Syntax);
            }
        }
        Ok(())
    }
    fn quantify(&mut self) -> Parsed<()> {
        if !self.quantifiable {
            return Err(Error::Syntax);
        }
        self.eat(63); // optional lazy modifier
        self.quantifiable = false;
        Ok(())
    }
    // Malformed brace syntax is literal text in Annex B. Only a complete
    // decimal quantifier establishes quantifier placement and ordering rules.
    fn braces(&mut self) -> Parsed<bool> {
        let start = self.at;
        while self.peek().is_some_and(|u| (48..=57).contains(&u)) {
            self.next()?;
        }
        let lower_end = self.at;
        if start == lower_end {
            return Ok(false);
        }
        if self.eat(125) {
            return Ok(true);
        }
        if !self.eat(44) {
            self.at = start;
            return Ok(false);
        }
        let upper_start = self.at;
        while self.peek().is_some_and(|u| (48..=57).contains(&u)) {
            self.next()?;
        }
        let upper_end = self.at;
        if !self.eat(125) {
            self.at = start;
            return Ok(false);
        }
        if upper_start != upper_end {
            let lower = trimmed(&self.units[start..lower_end]);
            let upper = trimmed(&self.units[upper_start..upper_end]);
            if lower.len() > upper.len() || (lower.len() == upper.len() && lower > upper) {
                return Err(Error::Syntax);
            }
        }
        Ok(true)
    }
    fn class(&mut self) -> Parsed<()> {
        self.eat(94);
        loop {
            if self.eat(93) {
                return Ok(());
            }
            let left = self.class_atom()?;
            if self.peek() == Some(45) && self.units.get(self.at + 1) != Some(&93) {
                self.at += 1;
                let right = self.class_atom()?;
                // Legacy class escapes at either endpoint turn the hyphen
                // into a literal; ordinary endpoints must still be ordered.
                if let (Atom::Unit(left), Atom::Unit(right)) = (left, right)
                    && left > right
                {
                    return Err(Error::Syntax);
                }
            }
        }
    }
    fn class_atom(&mut self) -> Parsed<Atom> {
        match self.next()? {
            92 => self.escape(true),
            unit => Ok(Atom::Unit(unit)),
        }
    }
    fn escape(&mut self, class: bool) -> Parsed<Atom> {
        let escaped = self.next()?;
        Ok(match escaped {
            100 | 68 | 115 | 83 | 119 | 87 => Atom::Set,
            98 if class => Atom::Unit(8),
            102 => Atom::Unit(12),
            110 => Atom::Unit(10),
            114 => Atom::Unit(13),
            116 => Atom::Unit(9),
            118 => Atom::Unit(11),
            99 => {
                if let Some(unit) = self.peek()
                    && ((65..=90).contains(&unit)
                        || (97..=122).contains(&unit)
                        || (class && ((48..=57).contains(&unit) || unit == 95)))
                {
                    self.at += 1;
                    Atom::Unit(unit % 32)
                } else {
                    // Invalid legacy control escapes retain the backslash as
                    // a literal, leaving c for the next atom (and range check).
                    self.at -= 1;
                    Atom::Unit(92)
                }
            }
            120 | 117 => {
                let start = self.at;
                match self.hex(if escaped == 120 { 2 } else { 4 }) {
                    Ok(unit) => Atom::Unit(unit),
                    Err(Error::Syntax) => {
                        self.at = start;
                        Atom::Unit(escaped)
                    }
                    Err(error) => return Err(error),
                }
            }
            48..=55 => {
                let mut value = escaped - 48;
                for _ in 0..if escaped <= 51 { 2 } else { 1 } {
                    if let Some(next) = self.peek().filter(|u| (48..=55).contains(u)) {
                        self.at += 1;
                        value = value * 8 + next - 48;
                    } else {
                        break;
                    }
                }
                Atom::Unit(value)
            }
            107 if self.named => return Err(Error::Syntax),
            unit => Atom::Unit(unit),
        })
    }
    fn hex(&mut self, count: usize) -> Parsed<u16> {
        let mut value = 0u16;
        for _ in 0..count {
            let digit = hex(self.next()?).ok_or(Error::Syntax)?;
            value = value * 16 + digit;
        }
        Ok(value)
    }
    fn name(&mut self) -> Parsed<Vec<u16>> {
        let mut name = Vec::new();
        loop {
            if self.eat(62) {
                return if name.is_empty() {
                    Err(Error::Syntax)
                } else {
                    Ok(name)
                };
            }
            let point = self.name_point()?;
            let character = char::from_u32(point).ok_or(Error::Syntax)?;
            let admitted = if name.is_empty() {
                matches!(character, '$' | '_') || crate::pattern::identifier::start(character)
            } else {
                matches!(character, '$' | '\u{200c}' | '\u{200d}')
                    || crate::pattern::identifier::part(character)
            };
            if !admitted {
                return Err(Error::Syntax);
            }
            self.graph.charge(8)?;
            let mut buffer = [0; 2];
            name.extend_from_slice(character.encode_utf16(&mut buffer));
        }
    }
    fn name_point(&mut self) -> Parsed<u32> {
        let unit = self.next()?;
        if unit != 92 {
            if (0xd800..=0xdbff).contains(&unit) {
                return pair(unit, self.next()?);
            }
            return Ok(u32::from(unit));
        }
        if !self.eat(117) {
            return Err(Error::Syntax);
        }
        if !self.eat(123) {
            let first = self.hex(4)?;
            if (0xd800..=0xdbff).contains(&first) {
                if !self.eat(92) || !self.eat(117) {
                    return Err(Error::Syntax);
                }
                return pair(first, self.hex(4)?);
            }
            return Ok(u32::from(first));
        }
        let mut value = 0u32;
        let mut digits = 0;
        loop {
            let next = self.next()?;
            if next == 125 && digits > 0 {
                return Ok(value);
            }
            let digit = u32::from(hex(next).ok_or(Error::Syntax)?);
            value = value
                .checked_mul(16)
                .and_then(|v| v.checked_add(digit))
                .filter(|v| *v <= 0x10ffff)
                .ok_or(Error::Syntax)?;
            digits += 1;
        }
    }
}
fn hex(unit: u16) -> Option<u16> {
    match unit {
        48..=57 => Some(unit - 48),
        65..=70 => Some(unit - 65 + 10),
        97..=102 => Some(unit - 97 + 10),
        _ => None,
    }
}
fn trimmed(mut digits: &[u16]) -> &[u16] {
    while digits.first() == Some(&48) {
        digits = &digits[1..];
    }
    digits
}
fn pair(first: u16, second: u16) -> Parsed<u32> {
    if !(0xdc00..=0xdfff).contains(&second) {
        return Err(Error::Syntax);
    }
    Ok(0x10000 + ((u32::from(first) - 0xd800) << 10) + u32::from(second) - 0xdc00)
}
