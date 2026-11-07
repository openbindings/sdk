import { Document } from "../dist/index.js";
import { createWorkerView } from "./worker-view.mjs";

export const editorDocument = `{
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

// The component keeps its latest prepared snapshot, including one whose schema
// reaches a resource that was not supplied. Each value check can still return
// no verdict.
export class DocumentEditor {
  #document;
  update(text, value = { id: 7 }, explain = false) {
    const parsed = Document.parse(text);
    if (parsed.status !== "parsed") return parsed;
    let proof, operation, schema, input;
    try {
      const checked = parsed.value.validate();
      if (checked.status !== "conformant") return checked;
      proof = checked.document;
      const entries = proof.operations();
      let operations;
      try {
        operations = entries.map((entry) => ({ key: entry.key }));
      } finally {
        for (const entry of entries) entry.dispose();
      }
      operation = proof.resolveOperation("find");
      if (!operation) return { status: "operation-missing" };
      schema = operation.input();
      if (!schema) return { status: "no-schema" };
      const setup = schema.prepare();
      if (setup.status !== "ready") {
        if (
          setup.status === "refused" &&
          setup.failure.kind === "resolution" &&
          setup.failure.details.cause.kind === "resource-unavailable"
        )
          return { status: "resources-missing", evidence: setup.failure };
        return setup;
      }
      input = setup.schema;
      // Explaining reports why a value fails; the cheap check only whether.
      const result = explain ? input.explain(value) : input.evaluate(value);
      const previous = this.#document;
      this.#document = proof; // transfer the conformant owner after successful setup
      proof = undefined;
      previous?.dispose();
      return { status: "updated", operations, result };
    } finally {
      input?.dispose();
      schema?.dispose();
      operation?.dispose();
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
  // JSON quoting escapes newlines, tabs, quotes and backslashes; pointers retain ~0/~1.
  return `pointer ${JSON.stringify(location.pointer)}, line ${location.line}, UTF-8 byte column ${location.byteColumn}, byte offset ${location.byteOffset}`;
}

function schemaLocation(location) {
  if (!location) return "schema location unavailable";
  const source =
    location.sourceUri === null
      ? "document"
      : JSON.stringify(location.sourceUri);
  return `schema ${JSON.stringify(location.pointer)}, source ${source}`;
}

function valueLines(result) {
  switch (result.status) {
    case "satisfies":
      return ["Selected input satisfies the input schema."];
    case "fails":
      return [
        "Selected input does not satisfy the input schema.",
        ...result.problems.map(
          (problem) =>
            `Instance ${JSON.stringify(problem.instance)}; ${schemaLocation(problem.schema)}: ${problem.message}`,
        ),
        `Selected input diagnostics complete: ${result.problemsComplete}.`,
      ];
    case "input-error":
      return [
        `Selected input was not admitted (${result.error.code}) at ${JSON.stringify(result.error.instancePointer)}: ${result.error.message}`,
      ];
    case "no-verdict":
      return [
        `Input verdict unavailable (${result.detail.reason}, ${result.detail.code}); ${schemaLocation(result.detail.location)}: ${result.detail.message}`,
      ];
  }
}

export function formatEditorResult(result) {
  switch (result.status) {
    case "value-input-error":
      return `Selected input was not parsed (${result.error.code}): ${result.error.message}`;
    case "operational-error":
      return `SDK call failed (${result.code}): ${result.message}`;
    case "input-error":
      return `Document was not parsed (${result.error.code})${result.error.byteOffset === undefined ? "" : ` at UTF-8 byte offset ${result.error.byteOffset}`}: ${result.error.message}`;
    case "version-refused":
      return `Document version ${JSON.stringify(result.refusal.declared)} is unsupported; supported: ${JSON.stringify(result.refusal.supported)}.`;
    case "report":
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
    case "no-schema":
      return 'Document is conformant; operation "find" has no input schema.';
    case "refused":
      return `Document is conformant; input setup was refused (${result.failure.kind}); ${schemaLocation(result.failure.origin)}.`;
    case "resources-missing":
      return `Document is conformant; schema resources are missing. Previous prepared snapshot kept. One missing reference: ${result.evidence.details.cause.details.uri}. Prepare again with resources before replacement.`;
    case "updated":
      return [
        "Document is conformant; the input schema is ready.",
        `Operations: ${JSON.stringify(result.operations.map((operation) => operation.key))}.`,
        ...valueLines(result.result),
      ].join("\n");
  }
}

export async function mountEditor(root) {
  const form = root.querySelector("form"),
    source = root.querySelector("#source"),
    value = root.querySelector("#input"),
    disclosure = root.querySelector("#schema-details"),
    correction = root.querySelector('[data-action="correct"]'),
    locate = root.querySelector('[data-action="locate"]'),
    output = root.querySelector("output");
  source.value = editorDocument;
  let closed = false,
    lastSource,
    lastLocation;
  const view = createWorkerView(
    ({ result, source: checkedSource, arenas }) => {
      if (closed) return;
      output.textContent = formatEditorResult(result);
      output.dataset.pending = "false";
      output.dataset.arenas = String(arenas); // Test observation, not an RSS/heap claim.
      lastSource = checkedSource;
      lastLocation =
        result.status === "report"
          ? result.report.findings.find((f) => f.location)?.location
          : undefined;
      locate.disabled = !lastLocation;
    },
    new URL("./editor-worker.mjs", import.meta.url),
  );
  const render = async (event) => {
    event?.preventDefault();
    output.dataset.pending = "true";
    locate.disabled = true;
    try {
      await view.check(source.value, value.value, {
        explain: disclosure.checked,
      });
    } catch (error) {
      if (!closed && error.name !== "AbortError") {
        output.textContent = `SDK call failed: ${error.message}`;
        output.dataset.pending = "false";
      }
    }
  };
  const correct = () => {
    source.value = source.value.replace('"inputSchema":', '"input":');
    void render();
  };
  const focusLocation = () => {
    if (!lastLocation || source.value !== lastSource) return;
    // Convert against the identical source snapshot. Textarea selection is UTF-16;
    // SDK byteOffset is UTF-8. The source location is a point, not an invented span.
    const original = new TextEncoder().encode(lastSource);
    const offset = new TextDecoder("utf-8", { fatal: true }).decode(
      original.subarray(0, lastLocation.byteOffset),
    ).length;
    source.focus();
    source.setSelectionRange(offset, offset);
  };
  form.addEventListener("submit", render);
  source.addEventListener("input", render);
  value.addEventListener("input", render);
  disclosure.addEventListener("change", render);
  correction.addEventListener("click", correct);
  locate.addEventListener("click", focusLocation);
  await render();
  return () => {
    closed = true;
    form.removeEventListener("submit", render);
    source.removeEventListener("input", render);
    value.removeEventListener("input", render);
    disclosure.removeEventListener("change", render);
    correction.removeEventListener("click", correct);
    locate.removeEventListener("click", focusLocation);
    view.dispose(); // Rejects pending work and terminates the entire owned Wasm realm.
  };
}
