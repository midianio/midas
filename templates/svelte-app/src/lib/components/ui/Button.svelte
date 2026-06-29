<script lang="ts">
	// Button — a UI component (FE-0011). Props in, events out; behavior lives in state/ (FE-0009),
	// not here. This is the shape `midas add component <name> --ui` scaffolds.
	import type { Snippet } from "svelte";

	interface Props {
		onclick?: () => void;
		disabled?: boolean;
		class?: string;
		children: Snippet;
	}

	let { onclick, disabled = false, class: className = "", children }: Props = $props();
</script>

<button class={className} {disabled} {onclick}>
	{@render children()}
</button>
