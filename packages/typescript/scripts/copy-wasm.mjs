import { cp, mkdir, readFile, rm } from "node:fs/promises";
import { rustSourceIdentity, sha256 } from "./source-identity.mjs";
const info = JSON.parse(
  await readFile(
    new URL("../src/wasm/build-info.json", import.meta.url),
    "utf8",
  ),
);
if (info.rustSource.sha256 !== (await rustSourceIdentity()).sha256)
  throw new Error(
    "Generated Wasm is stale. Run npm run build:wasm before building this package.",
  );
for (const [name, hash] of Object.entries(info.artifacts))
  if (
    sha256(await readFile(new URL("../src/wasm/" + name, import.meta.url))) !==
    hash
  )
    throw new Error("Generated Wasm artifact changed: " + name);
await rm(new URL("../dist/wasm/", import.meta.url), {
  recursive: true,
  force: true,
});
await mkdir(new URL("../dist/wasm/", import.meta.url), { recursive: true });
await cp(
  new URL("../src/wasm/", import.meta.url),
  new URL("../dist/wasm/", import.meta.url),
  { recursive: true },
);
await cp(
  new URL("../src/wasm-module.d.ts", import.meta.url),
  new URL("../dist/wasm-module.d.ts", import.meta.url),
);
for (const name of ["LICENSE", "NOTICE"])
  await cp(
    new URL("../../../" + name, import.meta.url),
    new URL("../" + name, import.meta.url),
  );
