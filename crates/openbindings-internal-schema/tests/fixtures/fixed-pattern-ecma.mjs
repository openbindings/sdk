// The ECMAScript oracle for the fixed checks' five patterns: V8's
// `new RegExp(pattern, "u").test(string)` over a deterministic generated corpus.
//
// The corpus is generated from the specification below by a small seeded
// generator that `ecma_patterns.rs` repeats exactly, so the fixture records
// only that specification, a digest of the corpus and each pattern's results.
// The Rust test checks the shipped pattern engine against those results; the
// TypeScript suite's `fixed-pattern-ecma.test.mjs` regenerates them from V8.
//
// Regenerate with: node crates/openbindings-internal-schema/tests/fixtures/fixed-pattern-ecma.mjs --write
import { readFileSync, writeFileSync } from "node:fs";

export const FIXTURE = new URL("./fixed-pattern-ecma.json", import.meta.url);

export const SPEC = {
  seed: 20261019,
  size: 60000,
  // Every regular expression in the pinned document schema and the 2020-12
  // meta-schemas, in schema order: the SemVer `openbindings` value, the name
  // grammar, extension member names, `$id` and anchor names.
  patterns: [
    "^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)(?:-((?:0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*)(?:\\.(?:0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\\+([0-9a-zA-Z-]+(?:\\.[0-9a-zA-Z-]+)*))?$",
    "^[A-Za-z0-9_][A-Za-z0-9_.-]*$",
    "^x-",
    "^[^#]*#?$",
    "^[A-Za-z_][-A-Za-z0-9._]*$",
  ],
  // Pieces include line terminators, astral characters, the private-use
  // character the fixed-string presentation uses as its marker, unpaired
  // surrogates, and characters that case folding or Unicode classes relate to
  // ASCII ones.
  pieces: [
    "0",
    "1",
    "9",
    "00",
    "01",
    "10",
    "a",
    "Z",
    "z",
    "A",
    "x",
    "X",
    "x-",
    "-",
    "--",
    ".",
    "..",
    "+",
    "_",
    "#",
    "##",
    "\u00e9",
    "\u{1f600}",
    "\n",
    "\r",
    "\u2028",
    "\u2029",
    " ",
    "\t",
    "!",
    "1.2.3",
    "0.2.0",
    "-rc.1",
    "+build.5",
    ".0",
    "a.",
    "1a",
    "\ue000",
    "\ud800",
    "\udc00",
    "\ud83d",
    "\ude00",
    "d800;",
    "\u0000",
    "\u017f",
    "\u212a",
    "\u0130",
    "\u0663",
    "\u{1d7d8}",
    "\uff21",
    "\uff10",
    "-0",
    "0a",
    "a-",
    "-a",
    "++",
    "+-",
    "x-y",
    "$",
    "^",
    "\\",
    "/",
    "%",
    "~",
  ],
  versions: [
    "0.0.0",
    "1.2.3",
    "10.20.30",
    "1.1.2-prerelease+meta",
    "1.0.0-alpha",
    "1.0.0-alpha.beta",
    "1.0.0-alpha.1",
    "1.0.0-alpha0.valid",
    "1.0.0-alpha.0valid",
    "1.0.0-rc.1+build.1",
    "2.0.0-rc.1+build.123",
    "1.2.3-beta",
    "10.2.3-DEV-SNAPSHOT",
    "1.2.3-SNAPSHOT-123",
    "2.0.0+build.1848",
    "2.0.1-alpha.1227",
    "1.0.0-alpha+beta",
    "1.2.3----RC-SNAPSHOT.12.9.1--.12+788",
    "1.2.3----R-S.12.9.1--.12+meta",
    "1.0.0+0.build.1-rc.10000aaa-kk-0.1",
    "99999999999999999999999.999999999999999999.99999999999999999",
    "1.0.0-0A.is.legal",
    "0.2.0",
    "1.0.0-01",
    "01.1.1",
    "1.01.1",
    "1.1.01",
    "1.2",
    "1.2.3.DEV",
    "1.2-SNAPSHOT",
    "1.0.0-alpha_beta",
    "1.0.0-alpha..",
    "1.0.0-alpha..1",
    "1.0.0-alpha...1",
    "+invalid",
    "-invalid",
    "1.0.0-",
    "1.0.0+",
    "1.0.0-+",
    "1.0.0-a.",
    "1.0.0+a.",
  ],
  bases: ["x-", "op", "a_b", "A.b-c", "_", "9", "#frag", "id#", "a#b#"],
  decorations: ["\n", "#", "x-", "\ud800", "\ue000"],
  prerelease: ["0", "1", "01", "a", "alpha", "0a", "-", "a-b", "", "x"],
  build: ["0", "a", "01", "-", "", "b1"],
};

// mulberry32: a 32-bit generator both implementations reproduce bit for bit.
function generator(seed) {
  let state = seed | 0;
  const next = () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return (t ^ (t >>> 14)) >>> 0;
  };
  return (n) => next() % n;
}

// Strings are sequences of UTF-16 code units; an edit can split a surrogate pair.
export function corpus(spec = SPEC) {
  const below = generator(spec.seed);
  const pick = (list) => list[below(list.length)];
  const edit = (text) => {
    let units = text;
    for (let count = 1 + below(3); count > 0; count--) {
      const operation = below(3);
      const at = below(units.length + 1);
      const piece = pick(spec.pieces);
      const last = Math.min(at, units.length - 1);
      if (operation === 0) {
        units = units.slice(0, at) + piece + units.slice(at);
      } else if (units.length > 0 && operation === 1) {
        units = units.slice(0, last) + units.slice(last + 1);
      } else if (units.length > 0) {
        units = units.slice(0, last) + piece + units.slice(last + 1);
      }
    }
    return units;
  };
  const strings = [];
  const seen = new Set();
  const add = (text) => {
    if (!seen.has(text)) {
      seen.add(text);
      strings.push(text);
    }
  };
  for (const text of [...spec.versions, ...spec.pieces, ""]) {
    add(text);
    for (const decoration of spec.decorations) {
      add(text + decoration);
      add(decoration + text);
    }
  }
  while (strings.length < spec.size) {
    const kind = below(4);
    if (kind === 0) {
      let text = "";
      for (let count = below(13); count > 0; count--) text += pick(spec.pieces);
      add(text);
    } else if (kind === 1) {
      add(edit(pick(spec.versions)));
    } else if (kind === 2) {
      const base = pick(spec.bases);
      add(below(10) < 8 ? edit(base) : base);
    } else {
      const number = () => {
        const any = below(1000);
        return String([0, 1, 9, 10, any][below(5)]);
      };
      const core = [number(), number(), number()].join(".");
      const join = (list, count) =>
        Array.from({ length: count }, () => pick(list)).join(".");
      const pre = join(spec.prerelease, below(5));
      const build = join(spec.build, below(4));
      let text = core;
      if (pre !== "" || below(5) === 0) text += "-" + pre;
      if (build !== "" || below(5) === 0) text += "+" + build;
      add(below(10) < 7 ? text : edit(text));
    }
  }
  return strings;
}

// FNV-1a over each string's length (four bytes) and code units (two bytes
// each), little-endian.
export function digest(strings) {
  const prime = 0x100000001b3n;
  const mask = (1n << 64n) - 1n;
  let hash = 0xcbf29ce484222325n;
  const byte = (value) => {
    hash = ((hash ^ BigInt(value & 0xff)) * prime) & mask;
  };
  for (const text of strings) {
    for (const shift of [0, 8, 16, 24]) byte(text.length >>> shift);
    for (let i = 0; i < text.length; i++) {
      const unit = text.charCodeAt(i);
      byte(unit);
      byte(unit >>> 8);
    }
  }
  return hash.toString(16).padStart(16, "0");
}

// One hexadecimal bit set per pattern, string i at bit i % 8 of byte i / 8.
export function matches(spec, strings) {
  return spec.patterns.map((pattern) => {
    const regex = new RegExp(pattern, "u");
    const bytes = new Uint8Array(Math.ceil(strings.length / 8));
    strings.forEach((text, i) => {
      if (regex.test(text)) bytes[i >> 3] |= 1 << (i & 7);
    });
    return Buffer.from(bytes).toString("hex");
  });
}

export function fixture() {
  const strings = corpus(SPEC);
  return {
    oracle: 'new RegExp(pattern, "u").test(string)',
    node: process.version,
    v8: process.versions.v8,
    ...SPEC,
    digest: digest(strings),
    matches: matches(SPEC, strings),
  };
}

export function checkedIn() {
  return JSON.parse(readFileSync(FIXTURE, "utf8"));
}

if (process.argv[2] === "--write") {
  writeFileSync(FIXTURE, JSON.stringify(fixture(), null, 2) + "\n");
}
