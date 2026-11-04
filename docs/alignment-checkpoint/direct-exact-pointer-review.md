# Independent direct exact-pointer correction

**PASS for the bounded pointer correction.** No correctness, ownership or allocation blocker was found. This does not qualify Explain witness propagation/materialization or the separate VM construction certificate.

Reviewed `raw.rs::exact_pointer_from`, the private `Pointer::from_escaped_unpaired_units` constructor, new native controls and the actual TypeScript/browser probe. Production source and linked library identities are in `source-hashes.json`; production was not edited.

The scalar path remains unchanged. A failed scalar path means an unpaired name occurs between the selected root and node. The new exact fallback measures escaped units without allocation, allocates one final vector, and fills segments from the end while preserving each name's forward unit order. Array indices are written backward in decimal, including zero and multiple digits. Name escaping expands only tilde/slash. The constructor accepts the already-established unpaired property, avoiding speculative String::from_utf16 allocation. A separator between segments prevents unrelated surrogate units in different names from accidentally pairing. Both path walks stop at the selected root; an unpaired ancestor outside a retained subtree does not force the Units representation. No control, owner, cache or semantic resource identity is introduced.

Independent checks against the actual rebuilt library:

* The **unchanged** initial allocator witness now requests exactly the final pointer size: **18 / 66 / 130 / 258 bytes** at depths 8 / 32 / 64 / 128, versus prior **323 / 1,547 / 3,179 / 6,443**. All measured storage releases on drop. These are cumulative requested-byte measurements, not process-memory claims.
* **236 exact input documents**, including every pair of 14 difficult names and deterministic mixed trees, produce **16,506 independently constructed pointer checks** across **4,327 retained subtree roots**. Expected paths are assembled independently from exact member units and array positions. Every pointer resolves to the same actual node, its scalar/Units representation matches whether its relative path contains an unpaired unit, and JSON serialization/readback preserves it.
* Inputs cover empty names, slash/tilde escaping, literal escape-looking names, NUL/newline, scalar non-ASCII, astral pairs, lone high/low and reversed surrogate units, arrays and multi-digit indices, root/empty pointers, and unpaired ancestors outside retained roots.

The first roundtrip-driver build selected a serde_json artifact from a different feature graph. Its trait-mismatch log is preserved as a driver error; the corrected command derives the matching dependency from the linked internal-json fingerprint. No input, expected pointer or production code changed to obtain the pass.

The helper still assumes the supplied root is an ancestor in the same arena, as it did before. Callers creating diagnostics must establish that invariant and reserve final pointer bytes plus their own origin/URI/message/copy ownership. This correction removes the temporary ancestor/name-buffer amplification; it does not itself grant an unbounded diagnostic allocation allowance. Measuring before materializing remains necessary.

Coordinator native/minimum-Rust and real host gates are separate broader evidence. The durable independent artifacts here are the source review, unchanged allocation witness and exact mixed/subtree roundtrip matrix.
