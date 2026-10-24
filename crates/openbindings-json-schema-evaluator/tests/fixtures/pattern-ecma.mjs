// The ECMAScript oracle for the evaluator's reading of schema patterns: the
// dialect JSON Schema 2020-12 cites, ECMA-262 11th edition (ES2020) section
// 21.2.1 with Unicode semantics, recorded as V8's `new RegExp(pattern, "u")`
// over seeded generated patterns and strings. The checked-in results come from
// V8 12.4 (Node 22), whose `u`-mode syntax agrees with that edition on every
// row: an independent ES2020 pattern parser accepts exactly the patterns it
// accepts.
//
// Patterns combine literals, escapes, classes, groups, quantifiers, anchors and
// alternation, with some syntax errors, word boundaries, and some constructs
// the evaluator does not match (lookaround, backreferences, property escapes).
// Fixed rows then place negated and plain sets whose matches meet across the
// surrogate gap in several positions, with strings at the gap's edges, and
// word-boundary assertions beside word, non-word, multi-byte and astral
// characters. Where V8 departs from ECMA-262 (it matches `\B` between a
// surrogate pair's halves), a result is recorded as `-`, not an oracle.
// The fixture records each pattern, its strings and, for a pattern V8 accepts,
// whether each string matches. It spells every pattern and string as its UTF-16
// code units, so no generated text can read as a label the repository's label
// check rejects. `pattern_correctness.rs` checks the evaluator
// against it through `pattern` and `patternProperties`; the TypeScript suite's
// `pattern-ecma.test.mjs` generates the rows again and derives their results
// from the running V8 on every run, except where a later edition's syntax lets
// a newer engine accept a pattern the 11th edition refuses
// (`laterEditionSyntax`).
//
// Regenerate with: node crates/openbindings-json-schema-evaluator/tests/fixtures/pattern-ecma.mjs --write
import { readFileSync, writeFileSync } from "node:fs";

export const FIXTURE = new URL("./pattern-ecma.json", import.meta.url);
export const SPEC = { seed: 20261020, patterns: 1500, strings: 7 };

const LITERALS = ["a", "b", "c", "z", "A", "0", "9", "_", "-", " ", "é", "ß", "😀", " ", "\n", "\t", "/", "~", "&", "=", "!", ",", ":", "<", ">", "#", "%", '"', "'"];
const ESCAPES = [String.raw`\d`, String.raw`\D`, String.raw`\s`, String.raw`\S`, String.raw`\w`, String.raw`\W`, String.raw`\t`, String.raw`\n`, String.raw`\r`, String.raw`\v`, String.raw`\f`, String.raw`\0`, String.raw`\x41`, String.raw`\x7a`, String.raw`\u{1F600}`, String.raw`\u{61}`, String.raw`\ud83d`, String.raw`\ude00`, String.raw`😀`, String.raw`\u{d83d}`, String.raw`\cA`, String.raw`\cz`, String.raw`\.`, String.raw`\*`, String.raw`\+`, String.raw`\?`, String.raw`\(`, String.raw`\)`, String.raw`\[`, String.raw`\]`, String.raw`\{`, String.raw`\}`, String.raw`\|`, String.raw`\^`, String.raw`\$`, String.raw`\\`, String.raw`\/`];
const INVALID = [String.raw`\-`, String.raw`\a`, String.raw`\e`, String.raw`\c1`, String.raw`\x4`, String.raw`\u12`, String.raw`\u{110000}`, String.raw`\01`, String.raw`\8`, String.raw`\k<q>`, String.raw`\p{Nope}`, String.raw`\p`, "(?i:a)", "(?", "{", "}", "]", "{1}", "**", "[b-a]", String.raw`[\d-z]`, String.raw`[a-\w]`, String.raw`[\B]`, "(?<1>a)", "(?<a>x)(?<a>y)", "a{2,1}"];
const UNMATCHED = [String.raw`\b`, String.raw`\B`, "(?=a)", "(?!b)", "(?<=a)", "(?<!b)", String.raw`(a)\1`, String.raw`(?<n>a)\k<n>`, String.raw`\p{Lu}`, String.raw`\P{L}`, String.raw`\p{Script=Greek}`, String.raw`\p{gc=Nd}`];
const CLASS_ITEMS = [
  ["a", "b", "z", "0", "9", "é", "😀", "-", "_", "^", "[", "&&", "~~", "."],
  ["a-z", "A-Z", "0-9", "à-ÿ", " - ", "!--", "--/"],
  [String.raw`\d`, String.raw`\D`, String.raw`\s`, String.raw`\S`, String.raw`\w`, String.raw`\W`, String.raw`\b`, String.raw`\-`, String.raw`\]`, String.raw`\\`, String.raw`\x41`, String.raw`\u{1F600}`, String.raw`\ud83d`, String.raw`\cJ`],
  [String.raw`\ud800-\udfff`, String.raw`a-\ud83d`, String.raw`\u0000-@`, String.raw`\x00-\x7f`],
];
const QUANTIFIERS = ["*", "+", "?", "{2}", "{0,2}", "{1,}", "{3,5}", "{0}", "{01,2}"];
const ALPHABET = ["a", "b", "c", "z", "A", "Z", "0", "5", "9", "_", "-", " ", " ", " ", " ", "\n", "\r", "\t", "\u000b", "\u0000", "é", "ß", "ÿ", "😀", "𐐀", "﻿", "[", "]", "&", "~", "/", "\\", ".", "\b", "\u0001"];

// Mulberry32: a small seeded generator, so the rows are reproducible anywhere.
function generator(seed) {
  let state = seed >>> 0;
  const next = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const below = (n) => Math.floor(next() * n);
  return { next, below, pick: (items) => items[below(items.length)] };
}

// Sets whose matched scalar values meet across the surrogate gap (U+D7FF and
// U+E000), negated and not, with strings at its edges: the regex crate's
// negation of such a class adds a range across the gap, so the evaluator must
// never write one negated.
const GAP_SETS = [
  String.raw`[^\S]`, String.raw`[^\D]`, String.raw`[^\W]`, String.raw`[^\s]`, String.raw`[^\d]`, String.raw`[^\w]`,
  String.raw`[^\u0000-￿]`, String.raw`[^퟿]`, String.raw`[^]`, String.raw`[^퟿]`,
  String.raw`[^퟿-]`, String.raw`[^\u{10000}-\u{10ffff}]`, String.raw`[^\S\d]`, String.raw`[^\D\s]`,
  String.raw`[^a\S]`, String.raw`[^\u0000-퟿]`, String.raw`[^-\u{10ffff}]`, String.raw`[^\ud800-\udfff]`,
  String.raw`[\S]`, String.raw`[\D]`, String.raw`[\W]`, String.raw`\S`, String.raw`\D`, String.raw`\W`, ".", "[^]", "[]",
];
const GAP_SHAPES = [(set) => set, (set) => `^${set}$`, (set) => `^${set}+$`, (set) => `^${set}{2}$`, (set) => `a${set}*b`];
const GAP_STRINGS = ["", "퟿", "", "퟿", "a", "1", " ", "￿", "\u{10000}", "\u{10ffff}", "11", "a퟿b", "ab"];

// Word-boundary assertions beside word, non-word, multi-byte and astral
// characters, alone and in alternations: an ASCII word boundary never falls
// inside a multi-byte character, but the regex crate's non-word boundary can.
const BOUNDARY_ATOMS = ["a", "_", "1", " ", "-", "é", "中", "😀", ".", "[^a]", String.raw`\W`, String.raw`\w`];
const BOUNDARY_SHAPES = [
  (b) => b, (b, x) => `${x}${b}`, (b, x) => `${b}${x}`, (b, x) => `${x}|${b}`, (b, x) => `${b}|${x}`,
  (b, x) => `${x}${b}${x}`, (b) => `^.*$|${b}`, (b) => `^a${b}`, (b) => `${b}é|${b}😀`,
];
const BOUNDARY_STRINGS = ["", "a", "aéa", "a中a", "a😀a", "abc", "é", "中", "😀", "a b", "_é_", "1-1", "éa", "a😀", "😀😀"];
// V8 tests positions between a surrogate pair's halves, where ECMA-262's
// Unicode mode has no position: `\B` holds there, so its result for a pattern
// holding `\B` on a string with an astral character is not an oracle.
const v8Trusted = (pattern, string) => !(pattern.includes(String.raw`\B`) && /[\u{10000}-\u{10FFFF}]/u.test(string));

export function rows(spec = SPEC) {
  const random = generator(spec.seed);
  const set = () => {
    let items = "";
    for (let i = random.below(5); i > 0; i--) {
      const r = random.next();
      items += random.pick(CLASS_ITEMS[r < 0.35 ? 0 : r < 0.6 ? 1 : r < 0.9 ? 2 : 3]);
    }
    return `[${random.next() < 0.3 ? "^" : ""}${items}]`;
  };
  const atom = (depth) => {
    const r = random.next();
    if (r < 0.35) return random.pick(LITERALS);
    if (r < 0.55) return random.pick(ESCAPES);
    if (r < 0.7) return set();
    if (r < 0.75) return ".";
    if (r < 0.9 && depth < 3) {
      const open = random.pick(["(", "(?:", `(?<g${random.below(10)}>`]);
      return `${open}${alternation(depth + 1)})`;
    }
    return random.pick(LITERALS);
  };
  const quantifier = () =>
    random.next() < 0.55
      ? ""
      : random.pick(QUANTIFIERS) + (random.next() < 0.2 ? "?" : "");
  const sequence = (depth) => {
    let out = "";
    for (let i = random.below(5); i > 0; i--) {
      const r = random.next();
      if (r < 0.05) out += random.pick(INVALID);
      else if (r < 0.09) out += random.pick(UNMATCHED);
      else if (r < 0.14) out += random.pick(["^", "$"]);
      else out += atom(depth) + quantifier();
    }
    return out;
  };
  const alternation = (depth) => {
    const count = random.next() < 0.75 ? 1 : 2 + random.below(2);
    return Array.from({ length: count }, () => sequence(depth)).join("|");
  };
  const seen = new Set();
  const out = [];
  for (let i = 0; i < spec.patterns; i++) {
    let pattern = alternation(0);
    if (random.next() < 0.3) pattern = `^${pattern}`;
    if (random.next() < 0.3) pattern = `${pattern}$`;
    const strings = [""];
    for (let s = 0; s < spec.strings; s++) {
      let text = "";
      for (let c = 1 + random.below(6); c > 0; c--) text += random.pick(ALPHABET);
      strings.push(text);
    }
    if (seen.has(pattern)) continue;
    seen.add(pattern);
    out.push({ pattern, strings });
  }
  for (const set of GAP_SETS) {
    for (const shape of GAP_SHAPES) {
      const pattern = shape(set);
      if (seen.has(pattern)) continue;
      seen.add(pattern);
      out.push({ pattern, strings: GAP_STRINGS });
    }
  }
  for (const boundary of [String.raw`\b`, String.raw`\B`]) {
    for (const shape of BOUNDARY_SHAPES) {
      for (const atom of BOUNDARY_ATOMS) {
        const pattern = shape(boundary, atom);
        if (seen.has(pattern)) continue;
        seen.add(pattern);
        out.push({ pattern, strings: BOUNDARY_STRINGS });
      }
    }
  }
  return out;
}

const units = (text) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));

// Each row's results: `null` for a pattern V8 refuses, otherwise one `1` or
// `0` per string, or `-` where V8's result is not an ECMA-262 oracle.
export function fixture(spec = SPEC) {
  return {
    node: process.version,
    v8: process.versions.v8,
    spec,
    rows: rows(spec).map(({ pattern, strings }) => {
      const row = { pattern: units(pattern), strings: strings.map(units) };
      let regex;
      try {
        regex = new RegExp(pattern, "u");
      } catch {
        return { ...row, matches: null };
      }
      const matches = strings
        .map((s) => (v8Trusted(pattern, s) ? (regex.test(s) ? "1" : "0") : "-"))
        .join("");
      return { ...row, matches };
    }),
  };
}

// The pattern syntax ECMA-262 added after the 11th edition, which a newer
// engine accepts though the 11th edition, and so the evaluator, refuses it.
// The 16th edition (ES2025) added both classes: modifiers, as in `(?i:a)` and
// `(?i-m:a)` (section 22.2.1, the `RegularExpressionModifiers` forms of
// `Atom`), and duplicate named groups in alternatives that cannot both
// participate (section 22.2.1.1, the `MightBothParticipate` early error, where
// the 11th edition's section 21.2.1.1 refuses any repeated group name). V8
// accepts both from 12.5 on, so Node 24 does and Node 22 does not. Returns the
// classes a pattern's text uses outside escapes and character classes; a
// repeated name counts even where the later edition still refuses it.
export function laterEditionSyntax(pattern) {
  const classes = new Set();
  const names = new Set();
  let inClass = false;
  for (let i = 0; i < pattern.length; i++) {
    const c = pattern[i];
    if (c === "\\") i++;
    else if (inClass) inClass = c !== "]";
    else if (c === "[") inClass = true;
    else if (c === "(" && pattern[i + 1] === "?") {
      const rest = pattern.slice(i + 2);
      if (/^[ims]*-?[ims]*:/.test(rest) && !rest.startsWith(":"))
        classes.add("modifiers");
      const name = /^<([^=!>][^>]*)>/.exec(rest);
      if (name) {
        if (names.has(name[1])) classes.add("duplicate named groups");
        names.add(name[1]);
      }
    }
  }
  return [...classes];
}

export function checkedIn() {
  return JSON.parse(readFileSync(FIXTURE, "utf8"));
}

if (process.argv.includes("--write")) {
  const generated = fixture();
  writeFileSync(FIXTURE, `${JSON.stringify(generated)}\n`);
  const refused = generated.rows.filter((row) => row.matches === null).length;
  console.log(`${generated.rows.length} patterns (${refused} refused by V8) from Node ${process.version}, V8 ${process.versions.v8}`);
}
