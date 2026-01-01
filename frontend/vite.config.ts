import fs from "node:fs";
import path from "node:path";
import react from "@vitejs/plugin-react";
import { type Plugin, defineConfig, loadEnv } from "vite";

const ghosttyWasmSource = path.resolve(
	__dirname,
	"node_modules",
	"ghostty-web",
	"ghostty-vt.wasm",
);
const ghosttyWasmTarget = path.resolve(__dirname, "public", "ghostty-vt.wasm");

function copyGhosttyWasm(): Plugin {
	const copy = (warn: (message: string) => void) => {
		if (!fs.existsSync(ghosttyWasmSource)) {
			warn(
				"ghostty-vt.wasm not found in node_modules; run install to enable the terminal.",
			);
			return;
		}
		fs.mkdirSync(path.dirname(ghosttyWasmTarget), { recursive: true });
		fs.copyFileSync(ghosttyWasmSource, ghosttyWasmTarget);
	};

	return {
		name: "copy-ghostty-wasm",
		buildStart() {
			copy(this.warn);
		},
		configureServer() {
			copy((message) => console.warn(message));
		},
	};
}

export default defineConfig(({ mode }) => {
	const env = loadEnv(mode, process.cwd(), "");
	const caddyUrl = env.VITE_CADDY_BASE_URL || "http://localhost";
	const controlPlaneUrl = env.VITE_CONTROL_PLANE_URL || "http://localhost:8080";
	const opencodeUrl = env.VITE_OPENCODE_BASE_URL || "http://localhost:41820";
	const fileserverUrl = env.VITE_FILE_SERVER_URL || "http://localhost:41821";

	return {
		plugins: [react(), copyGhosttyWasm()],
		resolve: {
			alias: {
				"@": path.resolve(__dirname, "./"),
			},
		},
		server: {
			host: true,
			port: 3000,
			allowedHosts: ["archlinux"],
			proxy: {
				"^/c/[^/]+/api": {
					target: caddyUrl,
					changeOrigin: true,
				},
				"^/c/[^/]+/files": {
					target: caddyUrl,
					changeOrigin: true,
				},
				"^/c/[^/]+/term": {
					target: caddyUrl,
					changeOrigin: true,
					ws: true,
				},
				"/api/opencode": {
					target: opencodeUrl,
					changeOrigin: true,
					rewrite: (pathValue) => pathValue.replace(/^\/api\/opencode/, ""),
				},
				"/api/files": {
					target: fileserverUrl,
					changeOrigin: true,
					rewrite: (pathValue) => pathValue.replace(/^\/api\/files/, ""),
				},
				"/api/models-dev": {
					target: "https://models.dev",
					changeOrigin: true,
					rewrite: (pathValue) =>
						pathValue.replace(/^\/api\/models-dev/, ""),
				},
				"/api": {
					target: controlPlaneUrl,
					changeOrigin: true,
					rewrite: (pathValue) => pathValue.replace(/^\/api/, ""),
				},
			},
		},
	};
});
