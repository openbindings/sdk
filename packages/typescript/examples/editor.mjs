import { initialize, parseDocument } from "@openbindings/sdk";

export const exampleDraft = `{
  "openbindings": "0.2.0",
  "operations": {
    "lookup": {
      "description": "Find an item",
      "aliases": ["find"],
      "inputSchema": {
        "type": "object",
        "properties": { "id": { "type": "integer", "minimum": 1 } },
        "required": ["id"]
      }
    }
  }
}`;

// The component keeps its latest usable immutable snapshot. Results are plain data.
export class DocumentEditor {
  #document;
  update(text) {
    const parsed = parseDocument(text);
    if (parsed.status !== "parsed") return parsed;
    let proof, context, input;
    try {
      const checked = parsed.value.validate();
      if (checked.status !== "validated") return checked;
      proof = checked.document;
      const operations = proof.operations;
      context = proof.contracts();
      const setup = context.prepare("find", "input");
      if (setup.status !== "ready") return setup;
      input = setup.contract;
      const result = input.validate({ id: 7 });
      const previous = this.#document;
      this.#document = proof; // transfer the validated owner after successful setup
      proof = undefined;
      previous?.dispose();
      return { status: "updated", operations, result };
    } finally {
      input?.dispose();
      context?.dispose();
      proof?.dispose();
      parsed.value.dispose();
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

function sourceLocation(location) {
  if (!location) return "location unavailable";
  // JSON quoting keeps control characters visible; pointers retain their ~0/~1 escapes.
  return `pointer ${JSON.stringify(location.pointer)}, line ${location.line}, UTF-8 byte column ${location.byteColumn}, byte offset ${location.byteOffset}`;
}

function schemaLocation(location) {
  if (!location) return "schema location unavailable";
  return `schema ${JSON.stringify(location.pointer)}, resource ${JSON.stringify(location.resource)}`;
}

function valueLines(result) {
  switch (result.outcome) {
    case "satisfies":
      return ["Example input { id: 7 } satisfies the input contract."];
    case "mismatch":
      return [
        "Example input does not satisfy the input contract.",
        ...result.problems.map(
          (problem) =>
            `Instance ${JSON.stringify(problem.instancePointer)}; ${schemaLocation(problem.schemaLocation)}: ${problem.message}`,
        ),
        `Selected input diagnostics complete: ${result.problemsComplete}.`,
      ];
    case "input-error":
      return [
        `Example input was not admitted (${result.error.code}) at ${JSON.stringify(result.error.instancePointer)}: ${result.error.message}`,
      ];
    case "no-verdict":
      return [
        `Input verdict unavailable (${result.detail.reason}, ${result.detail.code}); ${schemaLocation(result.detail.location)}: ${result.detail.message}`,
      ];
  }
}

export function formatEditorResult(result) {
  switch (result.status) {
    case "input-error":
      return `Document was not parsed (${result.error.code})${result.error.byteOffset === undefined ? "" : ` at UTF-8 byte offset ${result.error.byteOffset}`}: ${result.error.message}`;
    case "version-refused":
      return `Document version ${JSON.stringify(result.refusal.declared)} is unsupported; supported: ${JSON.stringify(result.refusal.supported)}.`;
    case "assessed":
      return [
        `Document conformance: ${result.report.conclusion}.`,
        ...result.report.findings.map(
          (finding) =>
            `${finding.rule}/${finding.code} (${finding.status}); ${sourceLocation(finding.location)}: ${finding.message}`,
        ),
        ...(result.report.findingsTruncated
          ? [
              "More findings were omitted; the report's rule evidence remains available.",
            ]
          : []),
      ].join("\n");
    case "operation-missing":
      return 'Document is conformant; operation "find" is missing.';
    case "operation-ambiguous":
      return `Document is conformant; operation "find" is ambiguous: ${JSON.stringify(result.candidates)}.`;
    case "no-contract":
      return 'Document is conformant; operation "find" has no input contract.';
    case "no-verdict":
      return `Document is conformant; input setup was refused (${result.detail.reason}, ${result.detail.code}); ${schemaLocation(result.detail.location)}: ${result.detail.message}`;
    case "updated":
      return [
        "Document is conformant; the input contract is ready.",
        `Operations: ${JSON.stringify(result.operations.map((operation) => operation.key))}.`,
        ...valueLines(result.result),
      ].join("\n");
  }
}

export async function mountEditor(root) {
  await initialize();
  const editor = new DocumentEditor();
  const form = root.querySelector("form"),
    source = root.querySelector("textarea"),
    correction = root.querySelector('[data-action="correct"]'),
    output = root.querySelector("output");
  source.value = exampleDraft;
  const render = (event) => {
    event?.preventDefault();
    try {
      // Never interpret diagnostics or caller-controlled pointers as HTML.
      output.textContent = formatEditorResult(editor.update(source.value));
    } catch (error) {
      output.textContent = `SDK call failed: ${error.message}`;
    }
  };
  const correct = () => {
    // Correct this known fixture's original source, without parsing message text.
    source.value = source.value.replace('"inputSchema":', '"input":');
    render();
  };
  form.addEventListener("submit", render);
  correction.addEventListener("click", correct);
  render();
  return () => {
    form.removeEventListener("submit", render);
    correction.removeEventListener("click", correct);
    editor.dispose();
  };
}
