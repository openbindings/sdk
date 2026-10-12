# Invocation authority

`binding-invoker-0.1.json` is copied verbatim from
`openbindings/interfaces/binding-invoker/0.1.json` at interfaces revision
`8b25ddccb477db31d7a507fe07227e08f544679d` (role version 0.1.0).

SHA-256: `8bad5b42057ff44ec412488b1ec754e5b2330af10a5e3dd3ab7d0625a373102a`.

The source prose is `binding-invoker/README.md` at the same revision. Both
documents define the carried-values and lifecycle behavior qualified here.

`../frame_correspondence.rs` is a deterministic SDK test harness. It checks
closed envelope tables against this pinned role and compares explicit wire
traces with the native lifecycle. These tests do not define a portable runtime
fixture standard or provide a production remote transport.

`operation-invoker-0.1.json` is copied verbatim from
`openbindings/interfaces/operation-invoker/0.1.json` at the same revision.
SHA-256: `964ba89ef4d39500b7ab309b7164c1fc636fe824bb2ab9833e4a8e4ad9edf2ef`.
Its source prose is `operation-invoker/README.md`.
