# OpenBindings internal json

Stores exact JSON values and source evidence shared by the SDK. Applications
use the types re-exported by `openbindings`, including `JsonValue` and `JsonRef`.

`JsonValue::parse_prefix` and `parse_prefix_with_limits` admit the first exact
value from a byte buffer and return its exclusive end offset.
`JsonPrefixError::Incomplete` distinguishes an open buffer needing more bytes
from invalid input. Impossible prefixes return `Invalid` immediately. At EOF use
whole-text admission on an incomplete remainder. For files or batches whose
producers keep writing, geometric buffer growth before retrying keeps cumulative
work linear. Waiting for that growth can stall a request/reply peer; retrying on
every read reparses from the start and can make cumulative work quadratic.
For newline-delimited JSON or MCP over stdio, find the protocol delimiter and
parse the complete message once. The host owns separators and I/O. Prefix calls
validate only the value and constant lookahead.

This package is a delivery dependency, not a supported extension API. It is
unpublished and requires Rust 1.88. Start with the
[SDK guide](https://github.com/openbindings/sdk) for a working application example.
Implementation and dependency contracts are in the
[maintainer guide](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md).
