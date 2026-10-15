// Initialize @openbindings/sdk for the host before calling these functions.
import {
  ExactJson,
  Document,
  SdkError,
  type JsonInput,
} from "@openbindings/sdk";

/** Inspect declared bindings; this neither selects a provider nor invokes it. */
export function inspectBindings(text: string, operationName: string) {
  const parsed = Document.parse(text);
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

const pointer = (name: string) =>
  name.replaceAll("~", "~0").replaceAll("/", "~1");
/** Application editing policy: add a tag to a primary operation, if present. */
export function addTag(
  document: Document,
  primaryKey: string,
  tag: string,
): boolean {
  const operation = document.operations.find((row) => row.key === primaryKey);
  if (!operation) return false;
  document.set(`/operations/${pointer(primaryKey)}/tags`, [
    ...new Set([...(operation.tags ?? []), tag]),
  ]);
  return true;
}
function edited(document: Document) {
  const checked = document.validate();
  if (checked.status === "version-refused") return checked;
  if (checked.status === "conformant") checked.document.dispose();
  return {
    status: "edited" as const,
    bytes: document.toBytes(),
    report: checked.report,
  };
}
/** Edit two fields atomically while preserving every unrelated byte. */
export function editMetadata(
  text: string,
  description: string,
  primaryKey: string,
  tag: string,
) {
  const opened = Document.parse(text);
  if (opened.status !== "parsed") return opened;
  using document = opened.value;
  try {
    const operation = document.operations.find((row) => row.key === primaryKey);
    if (!operation) return { status: "operation-missing" as const };
    document.edit([
      { op: "set", pointer: "/description", value: description },
      {
        op: "set",
        pointer: `/operations/${pointer(primaryKey)}/tags`,
        value: [...new Set([...(operation.tags ?? []), tag])],
      },
    ]);
    // Bytes and reports are detached. Application policy decides whether to save.
    return edited(document);
  } catch (error) {
    if (error instanceof SdkError)
      return { status: "edit-error" as const, error };
    throw error;
  }
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
    using name = member.name();
    using child = member.value();
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
      using name = member.name();
      const decoded = name.toValue();
      if (decoded.status !== "converted" || typeof decoded.value !== "string")
        throw new TypeError(
          "Member name cannot be represented by this recipe.",
        );
      if (decoded.value === key) {
        entries.push([decoded.value, replacement]);
        found = true;
      } else {
        const child = member.value();
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

/** Edit an extension member in place, keeping unrelated exact bytes. */
export function editExtensionMember(
  text: string,
  extensionKey: string,
  memberKey: string,
  replacement: JsonInput,
) {
  const opened = Document.parse(text);
  if (opened.status !== "parsed") return opened;
  using document = opened.value;
  using root = document.value();
  using extension = root.get(extensionKey);
  if (!extension) return { status: "extension-missing" as const };
  using member = extension.get(memberKey);
  if (!member) throw new TypeError("Replacement member is absent.");
  document.set(`/${pointer(extensionKey)}/${pointer(memberKey)}`, replacement);
  return edited(document);
}
