import test from "node:test";
import assert from "node:assert/strict";
import { cp, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import ts from "typescript";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checker = path.join(root, "scripts/check-api-reference.mjs");
// Mutate only temporary emitted declarations. Verify definitions, nested fields
// and inherited contracts are all protected by the actual exported-graph gate.
for (const [owner, member, expected, insertion] of [
  ["NoVerdict", undefined, "NoVerdict"],
  ["SourceLocation", "byteColumn", "SourceLocation.byteColumn"],
  ["Managed", "dispose", "ExactJson.dispose"],
  [
    "SourceLocation",
    undefined,
    "SourceLocation.undocumentedMethod",
    "undocumentedMethod(): void;",
  ],
]) {
  test(`reference check rejects missing ${expected} documentation`, async () => {
    const temporary = await mkdtemp(
      path.join(tmpdir(), "sdk-reference-control-"),
    );
    try {
      await cp(path.join(root, "dist"), path.join(temporary, "dist"), {
        recursive: true,
      });
      const filename = path.join(temporary, "dist/internal.d.ts");
      let source = await readFile(filename, "utf8");
      const ast = ts.createSourceFile(
        filename,
        source,
        ts.ScriptTarget.Latest,
        true,
      );
      const declaration = ast.statements.find(
        (node) => node.name?.text === owner,
      );
      if (insertion) {
        const end = declaration.end - 1;
        assert.equal(source[end], "}");
        source = source.slice(0, end) + insertion + source.slice(end);
      } else {
        const node = member
          ? declaration.members.find((node) => node.name?.text === member)
          : declaration;
        const documentation = ts
          .getJSDocCommentsAndTags(node)
          .filter(ts.isJSDoc);
        assert.ok(
          documentation.length > 0,
          "control requires existing real documentation",
        );
        for (const comment of documentation.reverse())
          source = source.slice(0, comment.pos) + source.slice(comment.end);
      }
      await writeFile(filename, source);
      const result = spawnSync(process.execPath, [checker], {
        encoding: "utf8",
        env: { ...process.env, OPENBINDINGS_REFERENCE_PACKAGE_ROOT: temporary },
      });
      assert.equal(result.status, 1, result.stderr || result.stdout);
      assert.ok(
        JSON.parse(result.stdout).failures.some((failure) =>
          failure.startsWith(expected + ":"),
        ),
        result.stdout,
      );
    } finally {
      await rm(temporary, { recursive: true, force: true });
    }
  });
}
