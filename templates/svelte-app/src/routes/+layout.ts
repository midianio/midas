// Static SPA shell (adapter-static): prerender the shell, render everything on the client. Pages
// fetch a live backend through the `api<T>()` wrapper at runtime, not at build time. When the app
// grows SSR'd marketing/auth/legal routes, move those under a `(public)/` group with `ssr = true`.
export const prerender = true;
export const ssr = false;
