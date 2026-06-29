// FE-0005: every backend call goes through this typed wrapper — never `fetch()` a backend route
// directly. Centralizing it means auth headers, the `/api` prefix, error handling, and the response
// envelope are applied in exactly one place.
//
// The auth token comes from an injected provider (registered by the auth state singleton), NOT a
// direct import — that breaks the api <-> auth circular dependency.
//
// TODO(auth): the midian standard is Clerk for both auth and billing (STK-0005). Register a provider
// from your auth singleton that returns the current Clerk session token:
//   setAuthTokenProvider(() => clerk.session?.getToken() ?? null)

let getAuthToken: () => Promise<string | null> = async () => null;

export function setAuthTokenProvider(fn: () => Promise<string | null>): void {
	getAuthToken = fn;
}

const BASE = "/api";

/** The standard backend success envelope (BE-0002): the payload is under `data`. */
interface Envelope<T> {
	data: T;
	code: number;
	timestamp: string;
	count: number;
}

/** Call a backend route and return its unwrapped `data`. Throws on a non-2xx response. */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
	const token = await getAuthToken();
	const headers = new Headers(init.headers);
	headers.set("content-type", "application/json");
	if (token) headers.set("authorization", `Bearer ${token}`);

	const res = await fetch(`${BASE}${path}`, { ...init, headers });
	if (!res.ok) {
		throw new Error(`api ${path} failed: ${res.status}`);
	}
	const body = (await res.json()) as Envelope<T>;
	return body.data;
}
