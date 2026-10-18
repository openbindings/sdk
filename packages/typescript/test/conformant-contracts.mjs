// Exercise public proof acquisition and operation selection, then release the
// temporary owners after setup. A plain Document is validated; a
// ConformantDocument is used as given and stays caller-owned. Only the
// returned preparation result (and its ready PreparedValueContract) is transferred.
function prepare(document, operation, field, options) {
  let proof, view, contract;
  try {
    if (typeof document.validate === "function") {
      const result = document.validate();
      if (result.status !== "conformant")
        throw new Error("Fixture document lacks conformance proof");
      proof = result.document;
    }
    view = (proof ?? document).resolveOperation(operation);
    if (!view) throw new Error(`Fixture lacks operation ${operation}`);
    contract = field === "input" ? view.input() : view.output();
    if (!contract)
      throw new Error(`Fixture operation ${operation} declares no ${field}`);
    return contract.prepare(options);
  } finally {
    contract?.dispose();
    view?.dispose();
    proof?.dispose();
  }
}

/** Prepare the selected operation's input contract with the given options. */
export function prepareInput(document, operation, options) {
  return prepare(document, operation, "input", options);
}

/** Prepare the selected operation's output contract with the given options. */
export function prepareOutput(document, operation, options) {
  return prepare(document, operation, "output", options);
}
