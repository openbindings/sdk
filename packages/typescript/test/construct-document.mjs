// Fixture construction through the public field and part setters.
export function constructDocument(sdk, fields) {
  const document = sdk.Document.create();
  try {
    if (fields === null) document.setOperations(null);
    for (const [key, value] of Object.entries(fields)) {
      if (value === undefined) continue;
      switch (key) {
        case "openbindings":
          document.setOpenbindings(value);
          break;
        case "name":
          document.setName(value);
          break;
        case "version":
          document.setVersion(value);
          break;
        case "description":
          document.setDescription(value);
          break;
        case "operations":
          document.setOperations(value);
          break;
        case "sources":
          document.setSources(value);
          break;
        case "bindings":
          document.setBindings(value);
          break;
        case "dependencies":
          document.setDependencies(value);
          break;
        case "additionalFields":
          for (const [name, exact] of Object.entries(value))
            document.set(
              "/" + name.replaceAll("~", "~0").replaceAll("/", "~1"),
              exact,
            );
          break;
        default:
          document.set(
            "/" + key.replaceAll("~", "~0").replaceAll("/", "~1"),
            value,
          );
      }
    }
    return document;
  } catch (error) {
    document.dispose();
    throw error;
  }
}
