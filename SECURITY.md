# Security policy

Report sensitive SDK issues privately to `hello@openbindings.com`, or contact
[Matthew Clevenger](https://github.com/clevengermatt). These are the reporting
contacts published by the [OpenBindings project](https://github.com/openbindings/project/blob/main/SECURITY.md).
Do not put credentials, private documents, or sensitive reproductions in a public
issue. Coordinate disclosure with the maintainers.

Include the package version or source commit, Rust/Node/browser runtime, affected
API, impact, and the smallest synthetic reproduction available. For resource
issues, include the configured limits and distinguish input size, returned output,
live retained data, and process memory. A correct validation verdict does not
rule out a resource-handling issue.

The Rust-backed packages are currently unpublished prereleases. There is no
published stable support branch or response-time SLA. Legacy Go and TypeScript
packages have separate histories; identify which implementation is affected.
For nonsensitive bugs, use this repository's issue tracker.
