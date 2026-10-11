import assert from "node:assert/strict";

// The README uses top-level fenced blocks. Track every fence so a missing
// delimiter cannot make code inside another block look like the public example.
export function firstUseSnippet(markdown) {
  return sectionSnippet(markdown, "First useful result", "ts");
}

export function browserBootstrapSnippet(markdown) {
  return sectionSnippet(markdown, "Browser resource management", "js");
}

function sectionSnippet(markdown, title, language) {
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
      // These executable examples contain no Markdown headings. A missing close
      // must not swallow the next section and borrow a later block's delimiter.
      if (section === title && fence.language === language)
        assert(
          !/^## /.test(text),
          "README has an unclosed example before a section heading",
        );
      if (
        marker &&
        marker[1][0] === fence.delimiter[0] &&
        marker[1].length >= fence.delimiter.length &&
        /^[ \t]*$/.test(marker[2])
      ) {
        if (section === title && fence.language === language)
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
        if (section === title) firstUseSections++;
      }
    }
    offset += line.length;
  }
  assert.equal(fence, undefined, "README contains an unclosed fenced block");
  assert.equal(firstUseSections, 1, `README must retain one ${title} section`);
  assert.equal(
    snippets.length,
    1,
    `Exactly one ${title} ${language} block must remain executable`,
  );
  return snippets[0];
}
