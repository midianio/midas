# Authorization — the access seam

How the backend decides who can read / edit / manage an owned resource. **One central choke point**
every resource handler goes through, so authorization is consistent and impossible to forget. This is
`BE-0005`. Canonical code: `src/modules/access/mod.rs`.

The principle: authorization is a single seam, not a clause scattered into every query. Pushing
`user_id` into every `WHERE` makes the policy un-auditable and easy to forget on a new endpoint, and
tends to muddle status codes (a missing row and a forbidden row look the same to the caller). One seam
fixes all three. *Origin: centralized during midian's Go→Rust port — see
[`../playbooks/go-to-rust.md`](../playbooks/go-to-rust.md) for the at-parity migration of an existing
service.*

## The seam

```rust
// src/modules/access/mod.rs
pub enum Role { None, Viewer, Editor, Owner }          // derive Ord: None < Viewer < Editor < Owner
impl Role { fn can_view/can_edit/can_manage(self) -> bool }

pub enum ResourceKind { Note, Desk, Chat }             // → the owner table for that kind

/// Effective role of `user_id` on a resource. Owner short-circuits; missing/none → None.
pub async fn role_for(pool, user_id, kind, id) -> Result<Role, AppError>;

/// Enforce a minimum role, mapping failure to the correct HTTP status.
pub async fn require(pool, user_id, kind, id, min: Role) -> Result<Role, AppError>;
```

`ResourceKind` enumerates the project's owned resource kinds *(midian: `Note`, `Desk`, `Chat` →
`notes` / `desks` / `conversations`)*.

## Owner-only today, grants-ready by construction

`role_for` reads `SELECT user_id FROM <table> WHERE id = ?`; owner → `Owner`, else `None`. When a
sharing model lands, the `else` arm becomes `MAX(public grant, user grant)` over one indexed query on
a `resource_grants` table — **and no call site changes.** That stable boundary is the entire reason to
route through the seam now rather than inline the owner check. *(midian: the sharing feature is
specified in `docs/resource-permissions-plan.md`.)*

## Status-code policy — enforced in one place (`require`)

| Situation                                                     | Status  | Where                                                  |
| ------------------------------------------------------------- | ------- | ------------------------------------------------------ |
| Missing / invalid token                                       | **401** | `RequireAuth` extractor (upstream)                     |
| Authenticated, resource missing **or** no access              | **404** | `require` — `!role.can_view()`. Never leaks existence. |
| Visible but below the needed role (e.g. viewer asked to edit) | **403** | `require` — `role < min`                               |

`[review]` Reads pass `Role::Viewer`, edits `Role::Editor`, manage ops (share / delete / role-change)
`Role::Owner`. Returning 404 (not 403) for "missing or no access" is deliberate — it refuses to
confirm a resource exists to someone who can't see it.

## Handler pattern

```rust
// single-resource read
access::require(pool, &auth.user_id, ResourceKind::Note, &id, Role::Viewer).await?;
let note = service::get_note_by_id(pool, &id).await?.ok_or(AppError::NotFound)?;

// edit / delete
access::require(pool, &auth.user_id, ResourceKind::Desk, &id, Role::Editor).await?;   // 403 if viewer
access::require(pool, &auth.user_id, ResourceKind::Desk, &id, Role::Owner).await?;     // delete
```

Rules:

- `[check]` **Service fetchers take `id` only** (`get_note_by_id`), no `user_id` filter — access is
  gated centrally first. The `WHERE user_id = ?` clause is *out* of the query; a handler that re-adds
  it is the violation.
- `[review]` **List endpoints don't use `role_for`** (it's per-resource). They stay an owner-scoped
  query; when grants land they become `owner rows UNION granted rows` — one set-based query, no N+1.
- `[review]` **Globally-shared content is exempt.** Content keyed by a natural key rather than an owner
  never goes through `access` *(midian: passage/relation/chunk/strong/theological insights — keyed by
  ref/strong/chunk, shared, not owned)*.
