// Shared client utilities. Two seams the standard pins here:
//
// - FE-0010: `generateId()` is the **one** place `crypto.randomUUID` is allowed; every other module
//   calls this, so ID generation has a single source instead of inline `crypto.randomUUID()` calls.
// - FE-0012: platform detection lives here (and in `state/screen.svelte.ts`), not ad-hoc
//   `navigator.userAgent` sniffing scattered across components.

/** A new random id (UUIDv4), with a fallback for insecure contexts / old runtimes. */
export function generateId(): string {
	if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
		return crypto.randomUUID();
	}
	return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
		const r = (Math.random() * 16) | 0;
		const v = c === "x" ? r : (r & 0x3) | 0x8;
		return v.toString(16);
	});
}

/** True on mobile user agents. The only sanctioned `navigator.userAgent` read (FE-0012). */
export function isMobile(): boolean {
	if (typeof navigator === "undefined") return false;
	return /iPhone|iPad|iPod|Android/i.test(navigator.userAgent);
}
