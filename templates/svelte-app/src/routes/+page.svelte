<script lang="ts">
	import { app } from "$lib/state/app.svelte";
	import { api } from "$lib/api";
	import { isMobile } from "$lib/utils";

	let serverId = $state<string | null>(null);
	let error = $state<string | null>(null);

	// Demonstrates the api<T>() seam (FE-0005): hits the backend's /hello through the wrapper.
	async function loadHello() {
		error = null;
		try {
			const data = await api<{ id: string }>("/hello");
			serverId = data.id;
		} catch (e) {
			error = e instanceof Error ? e.message : "request failed";
		}
	}
</script>

<main>
	<h1>{{NAME}}</h1>
	<p>A midian app, scaffolded by <code>midas new --profile app</code>.</p>

	<p>Session id: <code>{app.sessionId}</code></p>
	<p>Platform: {isMobile() ? "mobile" : "desktop"}</p>

	<button onclick={() => app.increment()}>count is {app.count}</button>
	<button onclick={loadHello}>load /hello from the backend</button>

	{#if serverId}<p>server id: <code>{serverId}</code></p>{/if}
	{#if error}<p style="color: crimson">error: {error} (is app/api running?)</p>{/if}
</main>
