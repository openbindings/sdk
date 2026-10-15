// The Worker owns every Wasm handle. Only input bytes and plain results cross realms.
import {
  initialize,
  validateDocument,
  Document,
  parseJson,
} from "@openbindings/sdk";
self.onmessage = async ({ data }) => {
  const {
    id,
    document: bytes,
    operation = "run",
    direction = "input",
    value,
  } = data;
  let document, context, prepared, input;
  try {
    await initialize(); // retryable if a previous attempt failed
    if (data.kind === "initialize") {
      self.postMessage({ id, status: "ready" });
      return;
    }
    self.postMessage({ id, status: "started" });
    if (value === undefined) {
      self.postMessage({
        id,
        status: "report",
        assessment: validateDocument(new Uint8Array(bytes)),
      });
      return;
    }
    const parsed = Document.parse(new Uint8Array(bytes));
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
    context = document.valueContracts();
    const setup = context.prepare(operation, direction);
    if (setup.status !== "ready") {
      self.postMessage({ id, status: "preparation", preparation: setup });
      return;
    }
    prepared = setup.valueContract;
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
        location: error?.location,
      },
    });
  } finally {
    input?.dispose();
    prepared?.dispose();
    context?.dispose();
    document?.dispose();
  }
};
