# Internal dependency fork

Based on `referencing-0.58.6`; upstream license and notices are retained.
The distinct package name prevents unpatched transitive dependency resolution:
the sibling jsonschema fork depends on this package, so the SDK graph holds one
`referencing`.

One correction is the fork's own. Upstream `RegistryBuilder::add` collects its
pending resources into a new map on every call, and `extend` collects them once
more before calling `add` for each pair, so registering n resources one at a time
costs O(n^2). The builder now moves its pending map into the returned builder;
the map's value type is covariant in the builder's lifetime, so no rebuild is
needed and registration is linear. The SDK evaluator's `resource_registration`
test guards it through the jsonschema fork's re-exported `Registry`.

The repository maintenance contract is DEPENDENCY-MAINTENANCE.md. Original
upstream identity, manifest and lock are retained as UPSTREAM-VCS.json,
UPSTREAM-Cargo.toml and UPSTREAM-Cargo.lock; these names avoid Cargo-reserved
packaging metadata and a lock that standalone Cargo commands would rewrite. The
complete reproducible patch and archive checksum are in docs/dependency-patches
at the repository root. This package is an implementation detail, not a
supported SDK extension API.
