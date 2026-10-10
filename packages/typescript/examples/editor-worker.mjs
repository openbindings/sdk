// A single component owns this Worker, its current proved snapshot and Wasm realm.
// Relative package URLs run directly when the installed examples directory is served.
import { initialize, parseJson, liveStorageOwners } from "../dist/index.js";
import { DocumentEditor } from "./editor.mjs";
const editor = new DocumentEditor();
self.onmessage = async ({ data }) => {
  const { id, document: source, value, includeSchemaDetails = false } = data;
  let input,
    initialized = false;
  try {
    await initialize();
    initialized = true;
    const parsed = parseJson(value);
    let result;
    if (parsed.status !== "parsed")
      result = { status: "value-input-error", error: parsed.error };
    else {
      input = parsed.value;
      result = editor.update(source, input, includeSchemaDetails);
    }
    input?.dispose();
    input = undefined;
    self.postMessage({ id, result, source, arenas: liveStorageOwners() });
  } catch (error) {
    // Operational exceptions stay separate from semantic outcomes.
    self.postMessage({
      id,
      result: {
        status: "operational-error",
        code: error.code ?? error.name,
        message: String(error.message),
      },
      source,
      arenas: initialized ? liveStorageOwners() : undefined,
    });
  } finally {
    input?.dispose();
  }
};
