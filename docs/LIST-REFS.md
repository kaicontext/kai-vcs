# List-refs contract — P0 draft

Derived from SPEC §§4.7 and 9.2. These transport-neutral JSON payloads list one
explicit ref namespace. They use the existing 1 MiB UTF-8 JSON bound, reject
unknown/duplicate fields, and require schema version 1. Request/repository IDs
are 32 lowercase hexadecimal digits; generations are canonical unsigned decimal
strings through u64::MAX. Optional fields may be omitted or null; encoding omits
absent fields. This is an experimental contract, not an authenticated server.

## Request

Required fields: `schema_version`, `request_id`, `repository_id`, `namespace`,
`limit`. Namespace is `branches`, `tags`, `shelves` or `remotes`; the page limit
is an integer 1–1000. Optional `after` is an exclusive ref-name cursor in that
namespace. Optional `generation` pins the repository snapshot. Any request with
`after` must include `generation`. A first request may also pin a known generation.
Names follow the native NFC grammar, with strict unsigned UTF-8 byte ordering.

## Reply

Required fields: `schema_version`, echoed `request_id`, echoed `repository_id`,
echoed `namespace`, `generation`, `refs`, and `has_more`. Optional `after` must
equal the request's cursor. A requested generation must equal the reply's.
Every page comes from one immutable repository snapshot. If the server cannot
serve the requested generation, it must reject the request; it must not silently
substitute the latest generation. Snapshot retention and authorization are future
server responsibilities.

Each ref is `{name, generation, targets}`. Ref generations are positive decimal
u64 strings. `targets` is an array of canonical native IDs, strictly digest-sorted:
zero means a deletion tombstone, one means a target, two or more mean explicit
branch alternatives. Only `branches` allows multiple targets. No head is selected
or discarded. Tombstones preserve their generation for future expected-old checks.
Object target types and available closure require separate object verification.

Entries must be strictly name-sorted, in the requested namespace and strictly
after the cursor. Their count cannot exceed `limit`. `has_more: true` requires a
nonempty page so the cursor advances; a page may be shorter than `limit` to respect
the byte bound. An empty final page is valid. An entry that cannot fit within the
byte limit must produce an error, never a truncated list of alternatives.

The continuation helper carries forward the exact namespace, repository and
snapshot generation, advances `after` to the last entry, and requires a request ID different from the current page
request ID. A terminal page produces no continuation. Clients must use this
pinned continuation instead of rebuilding an unpinned request. A client should
bound total pages/refs and session duration independently; each page is bounded,
not the lifetime of a caller-controlled pagination loop.

Request IDs and generation claims do not authenticate a peer. Authorization may
restrict the visible refs; `has_more` refers only to that authorized snapshot.
A server must keep visibility stable during a listing or reject the continuation.
Selecting `shelves` or `remotes` is not permission to expose private state. This
module performs no storage access, filtering, network I/O, or ref mutation.

The structural schema is `list-refs.schema.json`. Cross-field and cursor rules
are normative here and checked by Rust and an independent Python corpus checker.

Try the pagination contract example with
`cargo run --locked --offline --example list_refs`. It also demonstrates rejection
of a changed-generation reply.
