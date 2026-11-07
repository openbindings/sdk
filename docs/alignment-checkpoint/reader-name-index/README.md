# Ordered pattern-name index

This checkpoint changes the Reader's private uniqueness/membership index from a
linear vector to an ordered set. Capture numbering and source-order normalization
remain separate. The added permanent tests cover escaped duplicate identity,
forward and numeric references, Unicode names, duplicate-before-unsupported
precedence, and 1,024 names with long common prefixes.

Current and minimum Rust preserve all 46 name-specific and 1,186 retained full
Reading results, with complete measured cleanup. Node 22.23, Chromium 156 and
WebKit 27.2 preserve those same 1,232 results using the recorded actual Wasm
artifact. The 1,189 no-name cases request zero isolated name-index storage.
Independent review reran the native probes and added 59 controls, including
10,000 names, 512-character prefixes and exact Unicode identity distinctions.
The attached summaries identify the inputs and artifacts; raw receipts remain in
the migration coordinator's `pattern-reader-name-work` evidence directory.

The instrumented prototype reduces its widest tested name/reference case from
1,048,576 to 35,314 string comparisons. This is a scoped representation correction,
not a universal timing ratio or a full Reader resource certificate. No new public
API or quota is introduced. The [compiler resource-policy recommendation](../compiler-resource-policy.md)
explicitly supersedes the earlier full dependency accounting promise; its broader
implementation and qualification remain open.
