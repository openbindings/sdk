import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

// Declarations and the build receipt are not executable runtime artifacts.
export async function discoverRuntimeArtifacts(directory) {
  const files = [];
  async function visit(relative) {
    for (const entry of await readdir(path.join(directory, relative), {
      withFileTypes: true,
    })) {
      const name = path.posix.join(relative, entry.name);
      if (entry.isDirectory()) await visit(name);
      else if (entry.isFile() && /\.(js|wasm)$/.test(name)) files.push(name);
    }
  }
  await visit("");
  return files.sort();
}

export async function runtimeFiles(packageRoot) {
  const dist = path.join(packageRoot, "dist");
  const info = JSON.parse(
    await readFile(path.join(dist, "wasm/build-info.json"), "utf8"),
  );
  return [
    ...(await readdir(dist)).filter((name) => name.endsWith(".js")),
    ...Object.keys(info.artifacts).map((name) => "wasm/" + name),
  ].sort();
}
