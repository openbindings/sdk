import assert from "node:assert/strict";

// The README uses top-level fenced blocks. Track every fence so a missing
// delimiter cannot make code inside another block look like the public example.
export function firstUseSnippet(markdown) {
  let offset = 0;
  let section;
  let firstUseSections = 0;
  let fence;
  const snippets = [];
  for (const [line] of markdown.matchAll(/[^\n]*(?:\n|$)/g)) {
    if (!line) continue;
    const text = line.replace(/\r?\n$/, "");
    const marker = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(text);
    if (fence) {
      if (
        marker &&
        marker[1][0] === fence.delimiter[0] &&
        marker[1].length >= fence.delimiter.length &&
        /^[ \t]*$/.test(marker[2])
      ) {
        if (section === "First useful result" && fence.language === "ts")
          snippets.push(markdown.slice(fence.start, offset));
        fence = undefined;
      }
    } else if (marker) {
      // Backticks are not allowed in a backtick fence's info string.
      if (marker[1][0] !== "`" || !marker[2].includes("`"))
        fence = {
          delimiter: marker[1],
          language: marker[2].trim().split(/\s+/)[0],
          start: offset + line.length,
        };
    } else {
      const heading = /^## (.*?)[ \t]*$/.exec(text);
      if (heading) {
        section = heading[1];
        if (section === "First useful result") firstUseSections++;
      }
    }
    offset += line.length;
  }
  assert.equal(fence, undefined, "README contains an unclosed fenced block");
  assert.equal(firstUseSections, 1, "README must retain one first-use section");
  assert.equal(
    snippets.length,
    1,
    "Exactly one first-use TypeScript block must remain executable",
  );
  return snippets[0];
}
