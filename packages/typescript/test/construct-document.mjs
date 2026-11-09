// Test fixture assembly uses exact JSON editing. Missing version/operations are
// supplied explicitly here; the SDK never constructs missing namespace parents.
export function constructDocument(sdk, fields) {
  let document = sdk.Document.create();
  const replace = (next) => {
    document.dispose();
    document = next;
  };
  try {
    replace(document.set("", fields));
    const root = document.value();
    let version, operations;
    try {
      version = root.get("openbindings");
      operations = root.get("operations");
    } finally {
      root.dispose();
    }
    try {
      if (!version) replace(document.set("/openbindings", "0.2.0"));
      if (!operations) replace(document.set("/operations", {}));
    } finally {
      version?.dispose();
      operations?.dispose();
    }
    return document;
  } catch (error) {
    document.dispose();
    throw error;
  }
}
