import { initialize, authorDocument } from "@openbindings/sdk";
// The component keeps only its latest immutable snapshot. Diagnostics are plain data.
export class DocumentEditor {
  #document;
  update(draft) {
    const authored = authorDocument(draft);
    if (authored.status !== "authored") return authored;
    let proof;
    try {
      const checked = authored.document.validate();
      if (checked.status !== "validated") {
        authored.document.dispose();
        return checked;
      }
      proof = checked.document;
      const previous = this.#document;
      this.#document = authored.document;
      previous?.dispose();
      return { status: "updated", operations: proof.operations };
    } catch (error) {
      authored.document.dispose();
      throw error;
    } finally {
      proof?.dispose();
    }
  }
  snapshot() {
    return this.#document?.retain();
  } // caller owns this snapshot
  dispose() {
    this.#document?.dispose();
    this.#document = undefined;
  }
}
export async function mountEditor(root) {
  await initialize();
  const editor = new DocumentEditor();
  const form = root.querySelector("form"),
    output = root.querySelector("output");
  const render = (event) => {
    event?.preventDefault();
    const name = root.querySelector("input").value;
    try {
      output.textContent = JSON.stringify(
        editor.update({
          operations: { [name]: { input: { type: "integer" } } },
        }),
        null,
        2,
      );
    } catch (error) {
      output.textContent = error.message;
    }
  };
  form.addEventListener("submit", render);
  render();
  return () => {
    form.removeEventListener("submit", render);
    editor.dispose();
  };
}
