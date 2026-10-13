# CRUD portability observations

These observations come from the private 3.0/3.1 experiments. They are inputs to
fresh binding specifications, not published binding requirements. Old drafts were
excluded. Neither generic roles nor the core specification change in this slice.

## Observable value correspondence

A caller must distinguish no output from one JSON null value. This experiment
returns no value for a verified empty success selected by a content-less response
declaration, and one exact value for a supported JSON response. Unexpected bytes
in the first case and missing JSON in the second fail after dispatch. No retry or
rollback follows from those failures.

OpenAPI response selection is exact status, then status range, then default.
However, treating omitted `content` as requiring empty delivery is an adapter
policy, not an upstream guarantee that every such server response has no body.
For example, `200` JSON plus a content-less `default` selects that default for
201; JSON bytes on that 201 fail in this experiment. The previous adapter refused
that whole operation, so this is newly admitted behavior, not a changed successful
mapping for a previously admitted operation. An unmatched success retains the
earlier JSON fallback. These choices must be agreed if a future kind adopts them;
they are not automatically promoted to that future kind.

HTTP 204/205 require empty delivery regardless of the selected source declaration.
The 205 rule both admits correct empty responses and rejects illegal payloads that
the prior experiment could return as JSON. We retain the private r1 labels for
this HTTP correction and capability extension; we do not claim strict behavioral
compatibility or a new published binding standard. The old freeze remains a
historical experiment; the package guide and changelog record the changed behavior.

## Edition meaning

The 3.0 and 3.1 kinds continue to identify different source interpretations. For
DELETE, the client ignores 3.0 body declarations and retains 3.1 declarations.
The paired consumer checks required, absent and null bodies and actual transmitted
bytes. Future specifications can defer to their applicable OpenAPI edition rather
than restating those rules or forcing a common interpretation.

## Implementation choices

The five-method profile, application/json-only PATCH, conservative operation
exclusions, retained-byte charges and compiled response bitset are implementation
choices. They do not constrain another implementation's algorithms, caches,
internal representation or supported feature set. Resource limits remain SDK
policies. The client handles protocol interpretation; the adapter determines
OpenBindings value correspondence. No new generic role operation was needed.
