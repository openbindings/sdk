// Initialize @openbindings/sdk for your host before calling this editor example.
import { Document, EditError } from "@openbindings/sdk";
export function recoverEncoding() {
  const operation = { input: { minimum: NaN } };
  const document = Document.create();
  try {
    let first;
    try {
      const unexpected = document.set("/operations/run", operation);
      unexpected.dispose();
    } catch (error) {
      if (!(error instanceof EditError)) throw error;
      first = error;
    }
    if (
      first?.code !== "non-finite-number" ||
      first.inputPointer !== "/0/value/input/minimum"
    )
      throw new Error("Expected the example's non-finite JSON input.");
    // Focus the caller's JSON input; no message parsing or invented byte offset.
    operation.input.minimum = 0;
    const changed = document.set("/operations/run", operation);
    try {
      const checked = changed.validate();
      if (checked.status !== "conformant")
        throw new Error("Conformance was not established.");
      return { focus: first.inputPointer, document: checked.document };
    } finally {
      changed.dispose();
    }
  } finally {
    document.dispose();
  }
}
