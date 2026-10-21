# OpenBindings internal interface records

Provides the records that the source inspector, interface synthesizer and binding
invoker interfaces define identically: an advisory supported kind, a kind support
verdict, and the by-value binding with its structural diagnostic. Applications
import them from the companion they use; every companion hands out the same
types, so one provider serving several jobs builds one supported-kind list, and
an inspected target's binding is the binding an invoker receives.

This package is a delivery dependency, not a supported extension API. It is
unpublished and requires Rust 1.88. Start with the
[SDK guide](https://github.com/openbindings/sdk) for a working application example.
Implementation and dependency contracts are in the
[maintainer guide](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md).
