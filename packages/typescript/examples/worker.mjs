// Serve this file beside the package's dist directory, or resolve its import
// with your bundler. The owner may terminate the worker to interrupt sync work.
import {
  initialize,
  assessDocument,
  parseDocument,
  parseJson,
} from "../dist/index.js";
const initialized = initialize();
self.onmessage = async ({ data }) => {
  const {
    id,
    document: bytes,
    operation = "run",
    side = "input",
    value,
  } = data;
  let document, context, prepared, input;
  try {
    await initialized;
    self.postMessage({ id, status: "started" });
    if (value === undefined) {
      self.postMessage({
        id,
        status: "assessed",
        assessment: assessDocument(new Uint8Array(bytes)),
      });
      return;
    }
    const parsed = parseDocument(new Uint8Array(bytes));
    if (parsed.status !== "parsed") {
      self.postMessage({ id, status: "input-error", error: parsed.error });
      return;
    }
    document = parsed.value;
    const parsedValue = parseJson(new Uint8Array(value));
    if (parsedValue.status !== "parsed") {
      self.postMessage({ id, status: "input-error", error: parsedValue.error });
      return;
    }
    input = parsedValue.value;
    context = document.contracts();
    prepared = context.prepare(operation, side);
    self.postMessage({
      id,
      status: "evaluated",
      result: prepared.validate(input),
    });
  } catch (error) {
    self.postMessage({
      id,
      status: "error",
      error: {
        code: error?.code ?? "worker-error",
        message: String(error?.message ?? error),
      },
    });
  } finally {
    input?.dispose();
    prepared?.dispose();
    context?.dispose();
    document?.dispose();
  }
};
