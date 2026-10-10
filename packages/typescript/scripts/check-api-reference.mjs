/** Check the actual exported declaration graph and optionally render its reference. */
import ts from "typescript";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root =
  process.env.OPENBINDINGS_REFERENCE_PACKAGE_ROOT ??
  path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = process.argv[2];
const entries = ["index.d.ts", "http-discovery.d.ts", "wasm-module.d.ts"];
const program = ts.createProgram(
  entries.map((entry) => path.join(root, "dist", entry)),
  {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.NodeNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
    skipLibCheck: true,
    lib: ["lib.es2022.d.ts", "lib.dom.d.ts", "lib.esnext.disposable.d.ts"],
  },
);
const checker = program.getTypeChecker();
const rows = [],
  failures = [],
  seen = new Set();
const own = (node) =>
  node?.getSourceFile().fileName.startsWith(path.join(root, "dist") + path.sep);
const isInternal = (node) =>
  ts.getJSDocTags(node).some((tag) => tag.tagName.text === "internal");
const privateMember = (node) =>
  node.modifiers?.some((modifier) =>
    [ts.SyntaxKind.PrivateKeyword, ts.SyntaxKind.ProtectedKeyword].includes(
      modifier.kind,
    ),
  );
function record(name, declaration, symbol) {
  if (!declaration || !own(declaration) || isInternal(declaration)) return;
  const key = `${declaration.getSourceFile().fileName}:${declaration.pos}:${name}`;
  if (seen.has(key)) return;
  seen.add(key);
  const direct = ts
    .getJSDocCommentsAndTags(declaration)
    .filter(ts.isJSDoc)
    .map((doc) =>
      typeof doc.comment === "string"
        ? doc.comment
        : (doc.comment?.map((part) => part.text).join("") ?? ""),
    )
    .join("\n");
  const documentation =
    direct ||
    (symbol
      ? ts.displayPartsToString(symbol.getDocumentationComment(checker))
      : "");
  const source = declaration.getSourceFile();
  const line =
    source.getLineAndCharacterOfPosition(declaration.getStart()).line + 1;
  rows.push({
    name,
    source: path.relative(root, source.fileName),
    line,
    documentation,
    signature: declaration
      .getText(source)
      .replace(/\/\*\*[\s\S]*?\*\//g, "")
      .trim(),
  });
  if (documentation.trim().length < 20)
    failures.push(`${name}: missing or empty reference contract (${line})`);
}
function fields(node, scope) {
  if (ts.isPropertySignature(node))
    record(
      `${scope}.${node.name.getText()}`,
      node,
      checker.getSymbolAtLocation(node.name),
    );
  ts.forEachChild(node, (child) => fields(child, scope));
}
function declarationMembers(declaration, name) {
  if (ts.isClassDeclaration(declaration)) {
    const type = checker.getTypeAtLocation(declaration);
    for (const property of checker.getPropertiesOfType(type)) {
      for (const member of property.declarations ?? []) {
        if (
          privateMember(member) ||
          !own(member) ||
          (member.name && ts.isPrivateIdentifier(member.name))
        )
          continue;
        record(`${name}.${property.name}`, member, property);
        if (member.type) fields(member.type, `${name}.${property.name}`);
      }
    }
    // Constructors/static members are not properties of the instance type.
    for (const member of declaration.members) {
      if (privateMember(member) || isInternal(member)) continue;
      if (
        ts.isConstructorDeclaration(member) ||
        member.modifiers?.some((m) => m.kind === ts.SyntaxKind.StaticKeyword)
      ) {
        record(
          `${name}.${ts.isConstructorDeclaration(member) ? "constructor" : member.name?.getText()}`,
          member,
          member.name && checker.getSymbolAtLocation(member.name),
        );
        for (const param of member.parameters ?? [])
          if (param.type) fields(param.type, `${name}.options`);
      }
    }
  } else fields(declaration, name);
}
const exportsByEntry = {};
const targets = new Map();
for (const entry of entries) {
  const source = program.getSourceFile(path.join(root, "dist", entry));
  if (!source)
    throw new Error(`Build declarations first: dist/${entry} is missing`);
  const module = checker.getSymbolAtLocation(source);
  if (!module) throw new Error(`Missing module exports: ${entry}`);
  exportsByEntry[entry] = checker.getExportsOfModule(module).map((exported) => {
    const symbol =
      exported.flags & ts.SymbolFlags.Alias
        ? checker.getAliasedSymbol(exported)
        : exported;
    targets.set(symbol, exported.name);
    return exported.name;
  });
}
for (const [symbol, name] of targets) {
  const declarations = symbol.declarations ?? [];
  for (const declaration of declarations) {
    record(name, declaration, symbol);
    declarationMembers(declaration, name);
  }
}
// Follow declaration-local aliases that carry public payloads but are not named
// exports (for example discovery body branches). Never descend into private
// class state, implementation-only Wasm types or standard-library definitions.
const visitedTypes = new Set();
function followTypes(node) {
  if (
    privateMember(node) ||
    isInternal(node) ||
    (node.name && ts.isPrivateIdentifier(node.name))
  )
    return;
  if (ts.isTypeReferenceNode(node)) {
    let symbol = checker.getSymbolAtLocation(node.typeName);
    if (symbol?.flags & ts.SymbolFlags.Alias)
      symbol = checker.getAliasedSymbol(symbol);
    for (const declaration of symbol?.declarations ?? []) {
      if (
        !own(declaration) ||
        visitedTypes.has(declaration) ||
        !(
          ts.isTypeAliasDeclaration(declaration) ||
          ts.isInterfaceDeclaration(declaration)
        )
      )
        continue;
      visitedTypes.add(declaration);
      if (!targets.has(symbol)) {
        record(declaration.name.text, declaration, symbol);
        fields(declaration, declaration.name.text);
      }
      ts.forEachChild(declaration, followTypes);
    }
  }
  ts.forEachChild(node, followTypes);
}
for (const symbol of targets.keys())
  for (const declaration of symbol.declarations ?? []) followTypes(declaration);
const result = {
  exportsByEntry,
  uniqueExports: targets.size,
  checkedEntries: rows.length,
  failures,
  rows,
};
if (output) {
  await mkdir(output, { recursive: true });
  await writeFile(
    path.join(output, "typescript-reference.json"),
    JSON.stringify(result, null, 2) + "\n",
  );
  const escape = (text) =>
    text.replace(
      /[&<>"']/g,
      (c) =>
        ({
          "&": "&amp;",
          "<": "&lt;",
          ">": "&gt;",
          '"': "&quot;",
          "'": "&#39;",
        })[c],
    );
  await writeFile(
    path.join(output, "typescript-reference.html"),
    `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>OpenBindings TypeScript API reference</title><style>body{font:16px/1.6 system-ui;max-width:76rem;margin:2rem auto;padding:0 1rem;color:#182532}nav{columns:3;column-width:15rem}article{border-top:1px solid #ccd5dc;padding:1.5rem 0}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#f3f5f7;padding:1rem}a{color:#075f95}p{max-width:75ch}</style><h1>OpenBindings TypeScript API reference</h1><p>Generated from emitted public declarations. Contracts are inherited across public owners. Presence checks prevent omissions; compiled examples and review assess quality.</p><nav>${rows.map((row, i) => `<div><a href="#item-${i}">${escape(row.name)}</a></div>`).join("")}</nav>${rows.map((row, i) => `<article id="item-${i}"><h2>${escape(row.name)}</h2><p>${escape(row.documentation)}</p><details><summary>Declaration — ${escape(row.source)}:${row.line}</summary><pre>${escape(row.signature)}</pre></details></article>`).join("")}</html>`,
  );
}
console.log(
  JSON.stringify(
    { uniqueExports: targets.size, checkedEntries: rows.length, failures },
    null,
    2,
  ),
);
if (failures.length) process.exitCode = 1;
