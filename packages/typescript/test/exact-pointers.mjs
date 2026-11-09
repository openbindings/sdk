export function exactPointerCases(sdk) {
  const results = [];
  for (const depth of [8, 32, 64, 128]) {
    const text = `{"\\ud800":${'{"":'.repeat(depth - 1)}0${"}".repeat(depth)}`;
    const parsed = sdk.parseJson(text);
    if (parsed.status !== "parsed") throw Error(JSON.stringify(parsed));
    const value = parsed.value;
    const pointer = "/\ud800" + "/".repeat(depth - 1);
    let leaf, subtree, relativeLeaf;
    try {
      leaf = value.at(pointer);
      if (leaf?.text !== "0" || leaf.metadata.location.pointer !== pointer)
        throw Error(`depth ${depth}: original pointer changed`);
      // The unpaired ancestor is outside this retained root. Its relative path
      // consists only of empty scalar segments, and serialization stays exact.
      subtree = value.at("/\ud800");
      const relative = "/".repeat(depth - 1);
      relativeLeaf = subtree.at(relative);
      if (
        relativeLeaf?.text !== "0" ||
        subtree.locate(relative)?.pointer !== relative
      )
        throw Error(`depth ${depth}: relative pointer changed`);
      if (
        JSON.parse(JSON.stringify(leaf.metadata.location)).pointer !== pointer
      )
        throw Error(`depth ${depth}: pointer serialization changed`);
      if (subtree.locate("")?.pointer !== "")
        throw Error(`depth ${depth}: subtree root pointer changed`);
      results.push({ depth, status: "passed" });
    } finally {
      relativeLeaf?.dispose();
      subtree?.dispose();
      leaf?.dispose();
      value.dispose();
    }
  }
  return results;
}
