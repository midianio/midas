import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: [vitePreprocess()],
	kit: {
		// One codebase → web / PWA / native via the static adapter (FE-0004). The Capacitor build
		// switch (`CAPACITOR_BUILD=1`) is added when the app grows a native target.
		adapter: adapter({ fallback: "200.html" }),
	},
	compilerOptions: { runes: true },
};

export default config;
