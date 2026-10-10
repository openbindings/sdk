// Initialize @openbindings/sdk for the host before calling these functions.
import {
  authorDocument,
  ExactJson,
  parseDocument,
  type EditableDocumentDraft,
  type JsonInput,
} from "@openbindings/sdk";

/** Inspect declared bindings; this neither selects a provider nor invokes it. */
export function inspectBindings(text: string, operationName: string) {
  const parsed = parseDocument(text);
  if (parsed.status !== "parsed") return parsed;
  using document = parsed.value;
  const selected = document.resolveOperation(operationName);
  if (selected.status !== "found") return selected;
  using operation = selected.operation;
  const rows = [];
  for (const key of operation.bindings) {
    using binding = document.binding(key);
    if (!binding) throw new Error("An associated binding disappeared.");
    const metadata = binding.metadata;
    using source = document.source(metadata.source);
    rows.push({ binding: metadata, source: source?.metadata ?? null });
  }
  return { status: "inspected" as const, operation: operation.metadata, rows };
}

/** Application editing policy: add a tag to a primary operation, if present. */
export function addTag(
  draft: EditableDocumentDraft,
  primaryKey: string,
  tag: string,
): boolean {
  if (!Object.hasOwn(draft.operations, primaryKey)) return false;
  const operation = draft.operations[primaryKey];
  if (!operation) return false;
  const tags = (operation.tags ??= []);
  if (!tags.includes(tag)) tags.push(tag);
  return true;
}

/** Edit metadata while carrying every unrelated exact field through the draft. */
export function editMetadata(
  text: string,
  description: string,
  primaryKey: string,
  tag: string,
) {
  const parsed = parseDocument(text);
  if (parsed.status !== "parsed") return parsed;
  using original = parsed.value;
  const converted = original.toDraft();
  if (converted.status !== "drafted") return converted;
  using editing = converted.draft;
  editing.value.description = description;
  if (!addTag(editing.value, primaryKey, tag))
    return { status: "operation-missing" as const };
  const built = authorDocument(editing.value);
  if (built.status !== "authored") return built;
  using revised = built.document;
  const checked = revised.assess();
  if (checked.status !== "assessed") return checked;
  // Bytes and reports are ordinary values; no disposable owner escapes this call.
  // The caller decides whether a non-conformant edited document may be saved.
  return {
    status: "edited" as const,
    bytes: revised.originalBytes,
    report: checked.report,
  };
}

/** Display a bounded object prefix without decoding exact names or values. */
export function previewMembers(value: ExactJson, limit = 8) {
  if (!Number.isSafeInteger(limit) || limit < 0)
    throw new RangeError("limit must be a nonnegative safe integer");
  using members = value.members();
  if (!members) return undefined;
  const rows: { index: number; name: string; value: string }[] = [];
  if (limit === 0) return rows;
  for (using member of members) {
    using name = member.name;
    using child = member.value;
    rows.push({ index: member.index, name: name.text, value: child.text });
    if (rows.length === limit) break;
  }
  return rows;
}

/**
 * Replace one immediate member without converting any sibling value to JS.
 * This application recipe requires unique names throughout the input subtree
 * and member names representable as JS strings. It is not a JSON Patch editor:
 * absent members refuse, and formatting and member order may change.
 *
 * Borrows both inputs; the caller owns the result. Traversal visits every member,
 * and composition encodes/reparses the whole object, including unchanged values.
 */
export function replaceExactObjectMember(
  object: ExactJson,
  key: string,
  replacement: JsonInput,
): ExactJson {
  if (object.metadata.duplicateNames)
    throw new TypeError("This recipe requires unique member names.");
  using members = object.members();
  if (!members) throw new TypeError("Expected an exact object.");
  const entries: [string, JsonInput][] = [];
  const children: ExactJson[] = [];
  let found = false;
  try {
    for (using member of members) {
      using name = member.name;
      const decoded = name.toValue();
      if (decoded.status !== "converted" || typeof decoded.value !== "string")
        throw new TypeError(
          "Member name cannot be represented by this recipe.",
        );
      if (decoded.value === key) {
        entries.push([decoded.value, replacement]);
        found = true;
      } else {
        const child = member.value;
        children.push(child);
        entries.push([decoded.value, child]);
      }
    }
    if (!found) throw new TypeError("Replacement member is absent.");
    // An own __proto__ member stays data; object assignment would not ensure it.
    return ExactJson.from(Object.fromEntries(entries));
  } finally {
    for (const child of children) child.dispose();
  }
}

/** Application edit using the same scoped draft/build path as metadata edits. */
export function editExtensionMember(
  text: string,
  extensionKey: string,
  memberKey: string,
  replacement: JsonInput,
) {
  const parsed = parseDocument(text);
  if (parsed.status !== "parsed") return parsed;
  using original = parsed.value;
  const converted = original.toDraft();
  if (converted.status !== "drafted") return converted;
  using editing = converted.draft;
  const fields = editing.value.additionalFields;
  const leaf = fields?.[extensionKey];
  if (!fields || !(leaf instanceof ExactJson))
    return { status: "extension-missing" as const };
  using changed = replaceExactObjectMember(leaf, memberKey, replacement);
  fields[extensionKey] = changed;
  const built = authorDocument(editing.value);
  if (built.status !== "authored") return built;
  using revised = built.document;
  const checked = revised.assess();
  if (checked.status !== "assessed") return checked;
  return {
    status: "edited" as const,
    bytes: revised.originalBytes,
    report: checked.report,
  };
}
