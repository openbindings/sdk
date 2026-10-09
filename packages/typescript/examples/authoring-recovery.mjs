// Initialize @openbindings/sdk for your host before calling this editor example.
import { authorDocument } from "@openbindings/sdk";
export function recoverAuthoringDraft() {
  const draft = {
    operations: { run: {} },
    additionalFields: { operations: {} },
  };
  const first = authorDocument(draft);
  if (first.status !== "authoring-error") {
    first.document.dispose();
    throw new Error("Expected the example's field collision.");
  }
  // An editor can focus this exact draft field; no message parsing or byte offset.
  if (
    first.error.code !== "field-collision" ||
    first.error.draftPointer !== "/additionalFields/operations"
  )
    throw new Error("Unexpected example diagnostic.");
  delete draft.additionalFields.operations;
  const retried = authorDocument(draft);
  if (retried.status !== "authored") throw new Error(retried.error.code);
  try {
    const checked = retried.document.validate();
    if (checked.status !== "validated")
      throw new Error("Conformance was not established.");
    // Caller owns the validated handle; the parsed source can now be disposed.
    return { focus: first.error.draftPointer, document: checked.document };
  } finally {
    retried.document.dispose();
  }
}
