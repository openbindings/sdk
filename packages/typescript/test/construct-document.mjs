// Fixture construction through the public field and part setters.
export function constructDocument(sdk, fields) {
  let document = sdk.Document.create();
  try {
    if (fields === null)
      document = replaceDocument(document, document.setOperations(null));
    for (const [key, value] of Object.entries(fields)) {
      if (value === undefined) continue;
      switch (key) {
        case "openbindings":
          document = replaceDocument(document, document.setOpenbindings(value));
          break;
        case "name":
          document = replaceDocument(document, document.setName(value));
          break;
        case "version":
          document = replaceDocument(document, document.setVersion(value));
          break;
        case "description":
          document = replaceDocument(document, document.setDescription(value));
          break;
        case "operations":
          document = replaceDocument(document, document.setOperations(value));
          break;
        case "sources":
          document = replaceDocument(document, document.setSources(value));
          break;
        case "bindings":
          document = replaceDocument(document, document.setBindings(value));
          break;
        case "dependencies":
          document = replaceDocument(document, document.setDependencies(value));
          break;
        case "additionalFields":
          for (const [name, exact] of Object.entries(value))
            document = replaceDocument(
              document,
              document.set(
                "/" + name.replaceAll("~", "~0").replaceAll("/", "~1"),
                exact,
              ),
            );
          break;
        default:
          document = replaceDocument(
            document,
            document.set(
              "/" + key.replaceAll("~", "~0").replaceAll("/", "~1"),
              value,
            ),
          );
      }
    }
    return document;
  } catch (error) {
    document.dispose();
    throw error;
  }
}

// Test fixture cursor: keep the new immutable snapshot and release its predecessor.
function replaceDocument(previous, next) {
  previous.dispose();
  return next;
}
