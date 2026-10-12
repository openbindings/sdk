# Binding invocation authority

`binding-invoker-0.1.json` is copied verbatim from
`openbindings/interfaces/binding-invoker/0.1.json` at interfaces revision
`3972eb977f419f792b92fd61d5afa5fba4946b04` (role version 0.1.0).

SHA-256: `8bad5b42057ff44ec412488b1ec754e5b2330af10a5e3dd3ab7d0625a373102a`.

The source prose is `binding-invoker/README.md` at the same revision. Both
documents define the carried-values and lifecycle behavior qualified here.

`../frame_correspondence.rs` is a deterministic SDK test harness. It checks
closed envelope tables against this pinned role and compares explicit wire
traces with the native lifecycle. These tests do not define a portable runtime
fixture standard or provide a production remote transport.
