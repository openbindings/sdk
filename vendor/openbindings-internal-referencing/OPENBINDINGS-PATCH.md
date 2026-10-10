# Internal package identity fork

Based on referencing 0.58.6; upstream Rust source is byte-identical. Only package identity and internal dependency wiring change. This package isolates the SDK arbitrary-precision JSON feature graph from consumer serde_json. It is unpublished and not a supported SDK extension API.

Original licenses, UPSTREAM-Cargo.toml and UPSTREAM-VCS.json are retained. See the root DEPENDENCY-MAINTENANCE.md and tools/verify-dependency-isolation.py for the update procedure and source identity gate.
