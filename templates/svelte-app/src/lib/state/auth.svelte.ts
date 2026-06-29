// Auth state singleton (FE-0001). Owns the session and feeds tokens to the api layer through the
// injected provider (FE-0005) — `api.ts` never imports `auth`, so there's no circular dependency.
//
// TODO(auth): the midian standard is Clerk for both auth and billing (STK-0005). Wire
// @clerk/clerk-js (or svelte-clerk): initialize Clerk on load, drive `signedIn`/`userId` from its
// session, and have the token provider return `clerk.session?.getToken()`. This stub keeps the seam
// shape with no Clerk dependency or keys, so the app builds and runs as-is.

import { setAuthTokenProvider } from "$lib/api";

class AuthStore {
	signedIn = $state(false);
	userId = $state<string | null>(null);

	#token: string | null = null;

	constructor() {
		// Register how `api<T>()` obtains the bearer token. Replace the body with Clerk's getToken().
		setAuthTokenProvider(async () => this.#token);
	}

	/** DEV STUB sign-in — in a real app Clerk owns the session and mints the token. */
	signIn(userId: string, token: string): void {
		this.userId = userId;
		this.#token = token;
		this.signedIn = true;
	}

	signOut(): void {
		this.userId = null;
		this.#token = null;
		this.signedIn = false;
	}
}

export const auth = new AuthStore();
