import { parseDocument } from "../dist/index.js";
import { createWorkerView } from "./worker-view.mjs";

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
  update(text, value = { id: 7 }, includeSchemaDetails = false) {
    const parsed = parseDocument(text);
    if (parsed.status !== "parsed") return parsed;
    let proof, context, input;
    try {
      const checked = parsed.value.validate();
      if (checked.status !== "validated") return checked;
      proof = checked.document;
      const operations = proof.operations;
      context = proof.contracts({ includeSchemaDetails });
      const setup = context.prepare("find", "input");
      if (setup.status !== "ready") return setup;
      input = setup.contract;
      const result = input.validate(value);
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
  // JSON quoting escapes newlines, tabs, quotes and backslashes; pointers retain ~0/~1.
  return `pointer ${JSON.stringify(location.pointer)}, line ${location.line}, UTF-8 byte column ${location.byteColumn}, byte offset ${location.byteOffset}`;
}

function schemaLocation(location) {
  if (!location) return "schema location unavailable";
  return `schema ${JSON.stringify(location.pointer)}, resource ${JSON.stringify(location.resource)}`;
}

// This example displays details only after the user explicitly requests them.
// Tokens/names are displayed as text, never interpreted as HTML or rounded numbers.
function detailLine(detail) {
  if (!detail) return "";
  switch (detail.kind) {
    case "required":
      return ` Missing member: ${JSON.stringify(detail.member)}.`;
    case "numeric-bound":
    case "size-bound":
      return ` Exact bound: ${detail.bound}.`;
    case "enum":
      return ` Exact allowed values: ${detail.choices.join(", ")}.`;
    case "type":
      return ` Expected types: ${detail.expected.join(", ")}.`;
    case "truncated":
      return " Requested schema detail was omitted for the diagnostic budget.";
  }
}

function valueLines(result) {
  switch (result.outcome) {
    case "satisfies":
      return ["Selected input satisfies the input contract."];
    case "fails":
      return [
        "Selected input does not satisfy the input contract.",
        ...result.problems.map(
          (problem) =>
            `Instance ${JSON.stringify(problem.instancePointer)}; ${schemaLocation(problem.schemaLocation)}: ${problem.message}${detailLine(problem.details)}`,
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
  const form = root.querySelector("form"),
    source = root.querySelector("#source"),
    value = root.querySelector("#input"),
    disclosure = root.querySelector("#schema-details"),
    correction = root.querySelector('[data-action="correct"]'),
    locate = root.querySelector('[data-action="locate"]'),
    output = root.querySelector("output");
  source.value = exampleDraft;
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
        result.status === "assessed"
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
        includeSchemaDetails: disclosure.checked,
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
