import { readFile, readdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
export const repository = fileURLToPath(new URL("../../../", import.meta.url));
export const packageRoot = fileURLToPath(new URL("../", import.meta.url));
export const sha256 = (bytes) =>
  createHash("sha256").update(bytes).digest("hex");
export async function rustSourceIdentity() {
  // Source archives have no Git checkout. Enumerate actual compiler inputs,
  // including uncommitted files, without depending on a parent repository.
  const sources = [];
  async function visit(relative) {
    for (const entry of await readdir(path.join(repository, relative), {
      withFileTypes: true,
    })) {
      const name = path.posix.join(relative, entry.name);
      if (entry.isDirectory()) {
        if (!relative && !["crates", "vendor"].includes(entry.name)) continue;
        if (["target", ".git", "node_modules"].includes(entry.name)) continue;
        await visit(name);
      } else if (
        entry.isFile() &&
        // Nested locks from standalone dependency tests are not workspace
        // compiler inputs and may be ignored/untracked in a source checkout.
        (entry.name !== "Cargo.lock" || !relative) &&
        /^(crates\/|vendor\/|Cargo\.|rust-toolchain)/.test(name) &&
        /\.(rs|json|toml|lock|html)$/.test(name)
      )
        sources.push(name);
    }
  }
  await visit("");
  sources.sort();
  const files = {};
  for (const name of sources)
    files[name] = sha256(await readFile(path.join(repository, name)));
  return { files, sha256: sha256(JSON.stringify(files)) };
}
