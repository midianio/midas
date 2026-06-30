// FE-0001: global state is a class-based **runes singleton per domain**, living in
// `lib/state/<domain>.svelte.ts` and exported as a single instance. Components import the instance
// and read/mutate its `$state` fields directly; cross-singleton talk is a direct method call, never
// a global DOM event (FE-0008).
//
// `midas touch state <name>` scaffolds another one of these.

import { generateId } from "$lib/utils";

class AppStore {
	/** A counter, to show reactive `$state` end-to-end. */
	count = $state(0);

	/** A per-session id, minted through the one sanctioned generator (FE-0010). */
	readonly sessionId = generateId();

	increment(): void {
		this.count += 1;
	}
}

export const app = new AppStore();
