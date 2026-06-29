# Feature gating — plan gating + usage metering

Two mechanisms, two seams, one rule: gating is structural, never hand-rolled per handler. This is
`BE-0006`. Canonical code: `src/modules/billing/` (plan) + `src/modules/usage/` (metering).

The principle: the "check entitlement → do work → record usage" dance is the kind of thing that gets
copy-pasted, drifts, and silently breaks (a forgotten increment, an increment-before-failure double
charge). Encode the ordering in the types so the compiler keeps it honest. *Origin: extracted during
midian's Go→Rust port.*

## Plan gating — `src/modules/billing/`

A paid/free decision from the billing provider, behind a cache and an extractor.

- `Billing::is_paid(user_id) -> bool` — query the provider for the user's subscription
  *(midian: Clerk Billing — `GET /v1/users/{id}/billing/subscription`, bearer `CLERK_SECRET_KEY`)*;
  any active subscription item on a non-free plan = paid. 5-minute cache.
- `Billing::invalidate(user_id)` — called from the subscription webhook so a just-upgraded user isn't
  stuck "free" until the cache expires.
- `BE-0006` `[check]` **`RequirePlan` extractor** (sibling to `RequireAuth`) for whole-route gating:
  verifies the token, requires paid, else **402 `{"error":"plan_required","requiredPlan":"<tier>"}`**;
  yields `user_id` so the handler needn't also take `RequireAuth`:
  ```rust
  async fn create_thing(State(st): State<AppState>, plan: RequirePlan, …) -> Result<Response, AppError> {
      // plan.user_id is the verified, paid user
  }
  ```
  Apply it to the project's premium route groups *(midian: `/journey/*`, `/desk/*`, `/desk/images/*`)*.

`Billing` lives in `AppState`, built in `main` from the provider secret.

### Failure policy — distinguish misconfig from outage

`[review]` The cache makes the failure modes tractable; treat them differently:

- **Missing provider secret → fail CLOSED** (treat everyone as free) + a loud `error!`. A misconfig
  must never silently grant everyone paid, which would disable all gating.
- **Transient provider error** (secret present, API down) → serve **last-known-good** from cache. Only
  a *never-seen* user during an outage fails **open** to paid, so a blip doesn't 402 a paying customer
  out. Bounded and uncached → self-heals when the provider recovers.

## Usage metering — `src/modules/usage/`

A `Feature` enum maps each metered action to its `counter_name`, `free_limit`, and bonus column;
`usage_counters(user_id, counter_name, count)` is updated atomically via `ON DUPLICATE KEY UPDATE`.
*(midian features + free limits: wander 7 · enrich 3 · transcript 2 · pin 10 · insight 10, plus
per-user referral bonuses — the current instance, not the convention.)*

`BE-0006` `[check]` **The guard pattern makes the ordering structural** — `Pass` is `#[must_use]`, so
you can't forget or misorder the increment:

```rust
// free users: checked + blocked at the limit; paid users: unlimited, no count
let pass = usage::guard(pool, &st.billing, &user, Feature::Wander).await?;  // 402 if over
// ... do the gated work (AI call, DB write) — only runs if the gate passed ...
pass.commit(pool, &user).await?;   // increments ONCE, only for a metered free use, AFTER success
```

- `guard` → `Pass::Unlimited` (paid) | `Pass::Metered(feature)` (free, under limit) |
  **`Err` 402 `{"error":"usage_limit_exceeded","feature","used","limit"}`** (free, over).
- `commit` increments for `Metered`, no-ops for `Unlimited`. On work failure, **don't commit** → no
  charge for failed work.
- `check_usage` returns the full `UsageInfo` (used / limit / remaining / exceeded / bonus) — also the
  shape an entitlements endpoint (`GET /usage`) returns, so the frontend can show quota without
  provoking a 402.

## 402 bodies — typed on `AppError`

`[check]` These are the one sanctioned bespoke error shape (the generic envelope is `{status,code}`):

- `AppError::UsageLimit { feature, used, limit }` → `{"error":"usage_limit_exceeded", …}`
- `AppError::PlanRequired { required_plan }` → `{"error":"plan_required","requiredPlan": …}`

Both are 402, emitted at the top of `into_response` before the generic mapping.
