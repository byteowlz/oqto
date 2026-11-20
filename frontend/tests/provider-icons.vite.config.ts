import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/** Headless synthetic fixture only; no backend, private runtime assets or app startup. */
export default defineConfig({
	plugins: [react()],
	resolve: { alias: { "@": path.resolve(import.meta.dirname, "..") } },
	server: { host: "127.0.0.1", port: 18739, strictPort: true },
});
