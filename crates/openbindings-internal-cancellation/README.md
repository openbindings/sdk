# OpenBindings internal cancellation

Provides shared cancellation for asynchronous SDK companions. Applications
import `CancellationToken` from the companion they use. Clones share permanent
cancellation; use a fresh token for an independent retry.

This package is a delivery dependency, not a supported extension API. It is
unpublished and requires Rust 1.88. Start with the
[SDK guide](https://github.com/openbindings/sdk) for a working application example.
Implementation and dependency contracts are in the
[maintainer guide](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md).
