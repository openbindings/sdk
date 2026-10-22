import { rm } from "node:fs/promises";
// tsc only adds to its output directory: start each build from an empty one, so
// no file of an earlier build, such as a removed module, reaches the package.
await rm(new URL("../dist/", import.meta.url), {
  recursive: true,
  force: true,
});
