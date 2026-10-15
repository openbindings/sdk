## Changed

- Atomic document batches now accept typed operations, dependencies, sources and
  bindings maps alongside pointer edits. Each typed part serializes once into a
  bounded splice buffer. Namespace setters use that same engine.
- Removal planning indexes each parent's children once per batch.
- TypeScript editing helpers have runtime privacy. Wasm proofs own immutable
  `Conformant<Document>` revisions; substituted editor receivers cannot mutate them.
- The Catalog and synthesis-only examples commit construction in one batch and
  preserve main's exact document member order.
