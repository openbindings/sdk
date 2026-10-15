// Initialize @openbindings/sdk for your host before calling this editor example.
import { Document, EditError } from "@openbindings/sdk";
export function recoverAuthoringDraft() {
  const operation = { additionalFields: { input: {} } };
  const document = Document.create();
  try {
    let first;
    try {
      document.setOperation("run", operation);
    } catch (error) {
      if (!(error instanceof EditError)) throw error;
      first = error;
    }
    if (
      first?.code !== "field-collision" ||
      first.draftPointer !== "/operations/run/additionalFields/input"
    )
      throw new Error("Expected the example's field collision.");
    // Focus the caller's typed part field; no message parsing or byte offset.
    delete operation.additionalFields.input;
    document.setOperation("run", operation);
    const checked = document.validate();
    if (checked.status !== "conformant")
      throw new Error("Conformance was not established.");
    return { focus: first.draftPointer, document: checked.document };
  } finally {
    document.dispose();
  }
}
