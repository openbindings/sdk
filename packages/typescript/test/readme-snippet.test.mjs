import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  browserBootstrapSnippet,
  firstUseSnippet,
} from "../scripts/readme-snippet.mjs";

const original = readFileSync(new URL("../README.md", import.meta.url), "utf8");
// Normalize the mutation fixture, never the code compiled by test:package.
const readme = original.replaceAll("\r\n", "\n");
const source = firstUseSnippet(readme);

test("README source is captured verbatim with LF or CRLF", () => {
  assert(original.includes(firstUseSnippet(original)));
  assert(source.startsWith("import { parseDocument, type JsonInput }"));
  assert(source.endsWith('console.log(checkInput("seven"));\n'));
  assert.equal(
    firstUseSnippet(readme.replaceAll("\n", "\r\n")),
    source.replaceAll("\n", "\r\n"),
  );
});

test("README guard rejects missing, duplicate, and unclosed example fences", () => {
  for (const changed of [
    readme.replace("```ts\n", ""),
    readme.replace(
      "## Browser resource management",
      "```ts\nconst extra = true;\n```\n\n## Browser resource management",
    ),
    readme.replace(
      "## Browser resource management",
      "```ts\nconst unclosed = true;\n\n## Browser resource management",
    ),
    readme.replace(
      'console.log(checkInput("seven"));\n```',
      'console.log(checkInput("seven"));',
    ),
  ])
    assert.throws(() => firstUseSnippet(changed));
});

test("browser bootstrap remains a single verbatim executable README block", () => {
  const bootstrap = browserBootstrapSnippet(original);
  assert(original.includes(bootstrap));
  assert(bootstrap.endsWith('await import("./app.js");\n'));
  assert.throws(() =>
    browserBootstrapSnippet(
      readme.replace(
        "## Initialization",
        "```js\nawait import('./other.js');\n```\n\n## Initialization",
      ),
    ),
  );
});

test("README guard rejects an example swallowed by the preceding shell fence", () => {
  const changed = readme.replace(
    "examples/first-use-node.mjs\n```",
    "examples/first-use-node.mjs",
  );
  assert.notEqual(changed, readme);
  assert.throws(() => firstUseSnippet(changed));
});

test("fence length, delimiter and info strings follow top-level Markdown rules", () => {
  const markdown =
    "## First useful result\r\n\r\n~~~~ts\r\nconst x = 1;\r\n```\r\n~~~\r\n~~~~~\r\n## Initialization\r\n";
  assert.equal(firstUseSnippet(markdown), "const x = 1;\r\n```\r\n~~~\r\n");
  assert.throws(
    () => firstUseSnippet(markdown.replace("~~~~~\r\n", "~~~~~invalid\r\n")),
    /unclosed/,
  );
  assert.throws(() =>
    firstUseSnippet("## First useful result\n```ts`invalid\nx\n```\n"),
  );
});
