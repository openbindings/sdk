// Exercise public proof acquisition and release the temporary owner after setup.
export function validatedContracts(document, options) {
  const result = document.validated();
  if (result.status !== "validated")
    throw new Error("Fixture document lacks conformance proof");
  try {
    return result.document.valueContracts(options);
  } finally {
    result.document.dispose();
  }
}
