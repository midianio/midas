# Frontend Conventions

Portable conventions for a SvelteKit 2 / Svelte 5 (runes) frontend that also ships as a PWA and a
Capacitor native app. **Extracted** from the midian `app/web` frontend (live in production) and
**refined** where the status quo was incidental rather than intentional. Strip the midian-specifics
(Bible/TipTap/Clerk) and keep the patterns.

Canonical examples live in `midian/app/web/src/`. Entries marked **[refinement]** improve on current
code — they are the recommended target, with the escape hatch noted.

## Stack

SvelteKit 2 · Svelte 5 **runes** (`runes: true`) · TypeScript strict · Vite 6 · **Bun** (package
manager + scripts) · Tailwind 4 (`@tailwindcss/vite`, CSS-variable theming) · `bits-ui` +
shadcn-svelte primitives · TipTap 3 (rich text) · Clerk (`svelte-clerk`) · Capacitor 8 (iOS/Android)
· PostHog (`posthog-js`) · TanStack Query present but **not** the primary state mechanism (see State).

## Project structure

```
src/
├── app.html / app.css / app.d.ts        # shell, global Tailwind + CSS vars, ambient types
├── hooks.server.ts                       # error handling, crawler/OG injection
├── service-worker.ts                     # PWA
├── lib/
│   ├── state/      *.svelte.ts           # global state singletons (the spine — see State)
│   ├── components/
│   │   ├── ui/                           # design-system primitives (shadcn/bits-ui), re-exported via index.ts
│   │   ├── shared/                       # domain-agnostic chrome (nav, modals, panes)
│   │   └── <domain>/                     # feature components, grouped by domain
│   ├── nav/                              # pane navigation: url-codec, open-target, routes
│   ├── types/                            # hand-authored types AND the generated API client
│   ├── utils.ts + utils/                 # cn(), generateId(), platform detection, helpers
│   └── config/                           # constants
└── routes/
    ├── (public)/                         # SSR'd marketing/auth/legal
    └── app/                              # the SPA shell — ssr=false; content via panes, not routes
```

Imports always go through the `$lib` alias: `import { notes } from "$lib/state/notes.svelte"`.
**Singletons are imported as instances, never destructured** (destructuring breaks runes reactivity).

## State — class-based runes singletons (`FE-0001`)

The core pattern. One class per domain, reactive fields via `$state`, a **single exported instance**
at module bottom. Files are `src/lib/state/<domain>.svelte.ts`. Canonical: `state/notes.svelte.ts`,
`state/auth.svelte.ts`, `state/panes.svelte.ts`.

`[check]` only proves `src/lib/state` exists — a cheap structural proxy, not evidence any file in it
follows the pattern below. Whether a given file is actually a class-based singleton with one exported
instance (not a factory, not a bag of exported functions) is a `[review]` judgment, delegated to the
review agent (`standards/review-agent-prompt.md`).

```ts
// src/lib/state/notes.svelte.ts
class NotesState {
  loading = $state(false);
  error: string | null = $state(null);
  notes: Note[] = $state([]);

  // derived state is computed, not hand-maintained — see [refinement] below
  visible = $derived(this.notes.filter((n) => !this.activeTagFilters.length
    || n.tags?.some((t) => this.activeTagFilters.includes(t))));

  async fetchRecentNotes() {
    const { data } = await api<Note[]>({ endpoint: `/notes/notes/all` });
    this.notes = data ?? [];
  }
}
export const notes = new NotesState();   // one instance per client
```

Rules:

- **`$state` for source-of-truth, `$derived` for anything computable.** [refinement] Current code
  computes filtered/grouped views imperatively and stores the result in another `$state` field, which
  can desync. Prefer `$derived`/`$derived.by` so computed views can't drift from their inputs.
  *Escape:* `$state` for a computed view is acceptable only when the computation is genuinely
  expensive and memoized deliberately — comment why.
- **Reactive collections use the Svelte wrappers** — `SvelteSet`/`SvelteMap`/`SvelteDate`, never a
  plain `Set`/`Map` in `$state` (mutations on a plain collection aren't reactive).
- **Cross-singleton communication is a direct method call**, not a global event. [refinement] Current
  code dispatches `window.dispatchEvent(new CustomEvent("refresh-notes"))` to nudge other state. That
  is untyped, untraceable, and invisible to the type checker. Prefer importing the other singleton
  and calling its method, or exposing a `$derived` the other side reads. *Escape:* a DOM event is
  acceptable only to reach genuinely decoupled, non-state listeners (e.g. an analytics shim) — never
  between two state singletons.
- **No business logic in components.** Components read singleton fields and call singleton methods;
  fetching, mutation, and orchestration live in `state/`. (Mirrors the backend's "handlers are thin"
  rule.)
- **Async init is explicit and idempotent.** Side-effecting setup (`auth.sync(clerk)`,
  `journey.init()`) is a method called from the app shell, guarded against double-run — never done in
  the constructor (constructors run at import, before the app is ready).
- **Background/streaming work is owned by a per-task class + manager singleton**, surfaced via a
  progress UI — not run inline in a component. (SSE jobs must survive component unmount.)

## API client — one typed wrapper, generated types

All backend calls go through a single typed `api<T>()` wrapper. Canonical: `state/api.svelte.ts`.
Never `fetch()` a backend route directly from a component or another state file.

```ts
const { data, error } = await api<Note[]>({ endpoint: `/notes/notes/all` });
if (error) { /* handle */ }
```

- **Response envelope mirrors the backend exactly:** `{ data, code, timestamp, count, pageInfo? }`
  on success; `{ status, code }` on error. `api<T>()` unwraps to a discriminated
  `{ data, error, code, … }` so call sites branch on `error`, never parse the envelope themselves.
- **Auth token via injected provider, not an import.** `api` calls a `getAuthToken()` provider that
  `auth.svelte.ts` registers via `setAuthTokenProvider()` — this breaks the `api ↔ auth` circular
  import. Token attaches as `Authorization: Bearer …`. Requests send `credentials: "include"` (cookie
  fallback for iOS Safari / Capacitor where header auth is flaky).
- **402 is centrally handled** — `api` intercepts `Payment Required`, lazy-loads the upsell modal, and
  surfaces the typed usage/plan body. Handlers don't each re-implement the paywall.
- **Types are generated from the backend OpenAPI, not hand-written.** [refinement] Current code
  hand-authors request/response types, which silently drift from the Rust contract (the exact
  weak-area the backend's utoipa→OpenAPI→TS pipeline exists to close). Target: `midas gen types`
  produces `$lib/types/api.ts` from `openapi.json`; domain types extend the generated ones.
  *Escape:* hand-written types are fine for client-only shapes the backend never sees (UI view-models,
  local form state). `midas check`'s mechanical half of `FE-0006` (`artifact-hash`) verifies both
  `openapi.json` and the generated types file are committed (tracked, not gitignored) — a gitignored
  source means the pair's freshness can't be verified at all, the concrete failure this rule exists to
  catch.

## Navigation — the static-SPA pane system

Content navigation is **pane state, not routing.** The `/app/*` routes render a fixed shell; what's
*shown* is a stack of typed pane entries. This is what makes the app shippable as a static SPA for
PWA/Capacitor (no server round-trip per view). Canonical: `state/panes.svelte.ts`,
`nav/url-codec.ts`, `nav/open-target.ts`, `components/pane-registry.ts`.

- **A pane is a typed entry:** `{ type: PaneEntryType, params, label }`. The `type` union is the
  closed set of views; `params` is per-view payload.
- **Panes are addressed by id** (`"center" | "right"` today) so layout is data, not routes.
- **Open a pane via the `openTarget(entry)` seam** — one function, so deep-link/back/forward and
  pane-stack mutation stay consistent. Don't `goto()` to navigate content.
- **The pane stack serializes to the URL** via `nav/url-codec.ts` (`entryToUrl` / `urlToEntry`) so
  links and back/forward work without server routes. The catch-all `/app/[...nav]` route decodes it.
- **Pane components are lazy-loaded through a registry** (`pane-registry.ts`): `type → () =>
  import(...)`, cached after first load. Adding a view = add a `PaneEntryType` + a registry entry +
  the component. (`midas touch pane` should stamp all three.)
- `/app/*` is `ssr = false`; marketing/auth/legal under `(public)/` stay SSR'd for SEO/OG.

## Components & design system

- **PascalCase `.svelte` files**, grouped by domain under `components/<domain>/`; domain-agnostic
  chrome under `components/shared/`.
- **Primitives live in `components/ui/`** and are re-exported through an `index.ts` per primitive
  (`export { Root as Button, buttonVariants, type ButtonProps }`). Consumers import from the barrel,
  not the raw file.
- **Variants via `tailwind-variants` (`tv`)** in a `<script module>` block; never ad-hoc conditional
  class strings. Merge incoming `class` with **`cn()`** (`clsx` + `tailwind-merge`) so callers can
  override.
- **Runes API in components:** `$props()` (with `$bindable()` for two-way refs/controlled inputs),
  `$derived` for computed view values, snippet children (`children: Snippet` + `{@render children?.()}`).
- **Styling is Tailwind + CSS variables.** Theme tokens are CSS vars in `app.css`; component styles
  are utility classes. Scoped `<style>` only for what utilities can't express; avoid `:global`.

## Platform & build (PWA + Capacitor)

- **One codebase, adapter switch by env.** Default build uses the Bun/SSR adapter; setting
  `CAPACITOR_BUILD=1` switches `svelte.config.js` to `@sveltejs/adapter-static` (fallback
  `index.html`) → static SPA in `./build/` that Capacitor wraps. Native rebuilds go through the
  `cap:build:*` scripts (which set the flag) — never hand-run `vite build` for a native build.
- **Platform detection is centralized** in `utils.ts` (`isIOS`, `isAndroid`, `isAppShell`,
  `isIOSPWA`, `isStandalonePWA`) and the reactive `state/screen.svelte.ts` — feature-detect through
  these, don't sniff `navigator.userAgent` ad-hoc.
- **The shell owns viewport/safe-areas.** `screen.svelte.ts` pins an `--app-height` CSS var across
  keyboard transitions and the iOS PWA viewport shortfall; layout uses `env(safe-area-inset-*)`.
  `contentInset: "never"` (edge-to-edge) — CSS owns insets, not the native shell.
- **IDs via `generateId()`**, never `crypto.randomUUID()` directly. The wrapper falls back to
  `crypto.getRandomValues` where `randomUUID` is absent (non-HTTPS / older iOS Safari / Capacitor
  webview). A raw `crypto.randomUUID()` throws or is undefined in those contexts — this has bitten the
  app before. *Hard rule.*
- **App version is CalVer** — `YYYY.MM.DD+<short-git-hash>` injected as `__APP_VERSION__` at build
  (Vite). OTA update gating keys off it.

## Quality gates

`bun run check` (svelte-check, strict TS) · `bun run lint` · the generated API types are regenerated
and committed (drift guard, same loop as the backend's `.sqlx`/OpenAPI) · build succeeds for **both**
adapters (default + `CAPACITOR_BUILD=1`). A component holding fetch/mutation logic, a plain
`Set`/`Map` in `$state`, a direct `fetch()` to a backend route, or a raw `crypto.randomUUID()` are
review-blocking.

## Reuse

This doc is project-agnostic minus the midian-specific view types (Bible/journey/desk) and TipTap.
The behavioral pieces — `api<T>()` wrapper, the pane system, `screen`/platform detection,
`generateId()` — are candidates for the shared `@midian/*` packages (copy the shape, depend on the
mechanism; see `SPEC.md §3`).
