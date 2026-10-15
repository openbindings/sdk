// Exercise public proof acquisition and release the temporary owner after setup.
export function conformantContracts(document, options) {
  if (typeof document.valueContracts === "function")
    return document.valueContracts(options);
  const result = document.validate();
  if (result.status !== "conformant")
    throw new Error("Fixture document lacks conformance proof");
  try {
    return result.document.valueContracts(options);
  } finally {
    result.document.dispose();
  }
}
