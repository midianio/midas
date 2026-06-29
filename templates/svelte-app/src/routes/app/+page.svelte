<script lang="ts">
	import { app } from "$lib/state/app.svelte";
	import { auth } from "$lib/state/auth.svelte";
	import { api } from "$lib/api";
	import { isMobile } from "$lib/utils";
	import Button from "$lib/components/ui/Button.svelte";

	let serverId = $state<string | null>(null);
	let error = $state<string | null>(null);

	// Calls the backend through the typed api<T>() wrapper (FE-0005). The bearer token comes from the
	// auth singleton via the provider it registered (FE-0005) — set once signed in.
	async function loadItems() {
		error = null;
		try {
			const items = await api<{ id: string }[]>("/items/items");
			serverId = items[0]?.id ?? "(none)";
		} catch (e) {
			error = e instanceof Error ? e.message : "request failed";
		}
	}
</script>

<main>
	<h1>{{NAME}} · app</h1>
	<p>Platform: {isMobile() ? "mobile" : "desktop"}</p>
	<p>Session: <code>{app.sessionId}</code></p>

	<p>
		Auth: {auth.signedIn ? `signed in as ${auth.userId}` : "signed out"}
		{#if auth.signedIn}
			<Button onclick={() => auth.signOut()}>sign out</Button>
		{:else}
			<Button onclick={() => auth.signIn("user_demo", "dev-token")}>dev sign in</Button>
		{/if}
	</p>

	<Button onclick={() => app.increment()}>count is {app.count}</Button>
	<Button onclick={loadItems}>load /items from the backend</Button>

	{#if serverId}<p>first item id: <code>{serverId}</code></p>{/if}
	{#if error}<p style="color: crimson">error: {error} (sign in + is app/api running?)</p>{/if}
</main>
