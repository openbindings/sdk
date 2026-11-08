# Document to typed result

The complete program is printed in [the walkthrough](../../docs/document-to-typed-result.md).
This external crate uses only public SDK APIs. From this directory run
`cargo run --locked` or `cargo test --locked`. Its path dependencies select the
adjacent unpublished SDK checkout. The snippet verifier checks the guide against
`src/main.rs`, runs the program and tests its failure boundaries.
