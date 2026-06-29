import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		// Dev proxy to the backend service (app/api). Every call goes through the typed `api<T>()`
		// wrapper (FE-0005), which prefixes `/api`.
		proxy: {
			"/api": {
				target: process.env.BACKEND_URL || "http://localhost:8080",
				changeOrigin: true,
				rewrite: (path) => path.replace(/^\/api/, ""),
			},
		},
	},
});
