# TypeScript module implementation design

The current facade keeps Rust as its single semantic engine. Replace the 3,103-line
internal bucket with modules owned by their actual responsibilities. This is code
organization, not a second schema implementation or a new public role API.

- `runtime`: explicit host/Wasm initialization and readiness; initialization errors
  use shared diagnostics. No semantic module is initialized by importing it.
- `diagnostics`: source coordinates, parse failures, thrown SDK/edit/encoding errors
  and decoding of Rust report carriers. Authored exact JSON never uses report-key
  conversion.
- `ownership`: the private raw-handle table, disposal, finalization and exception-safe
  adoption. It holds no domain policy and makes no domain proof from a JS class.
- `json`: exact owners/cursors, admitted ordinary conversion, input limits and atomic
  edit value encoding. The edit envelope is a host codec mechanism; document edits
  and proofs remain in the document module and Rust.
- `schema`: original schemas, external schema sets, reference reports, preparation,
  evaluation outcomes and limits. Depends on JSON/ownership/runtime/diagnostics.
- `document`: drafts, conformance, immutable canonical entries and edits. It imports
  schema capabilities; schemas do not depend on document facade owners.
- `http-discovery`: actual optional acquisition/publication implementation, consuming
  the document facade. Core entry imports no discovery implementation.
- `index`/`testing`: explicit public exports and optional ownership observation.

The implementation graph is acyclic apart from erased type references if needed.
Private cross-module helpers may be exported between source files but never from
package entry points; generated public declarations cannot import Wasm bindings.
Retain language-appropriate disposal and proof-authority checks. Existing exports
are kept only where they are already the current designed API, not for compatibility.
No internal.ts compatibility barrel remains.

Validation: compiler/type consumers; all current Node semantic/ownership/error
checks; emitted-reference graph; installed npm archive; actual Chromium/WebKit
module loading. Browser fixtures serve the generated module graph, not a frozen
internal.js name. No semantic fixture or expected outcome changes are justified by
moving a declaration. Build and package source hashes delimit evidence.
