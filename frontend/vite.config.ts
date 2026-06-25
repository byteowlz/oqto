import fs from "node:fs";
import path from "node:path";
import react from "@vitejs/plugin-react";
import { type Plugin, type ProxyOptions, defineConfig, loadEnv } from "vite";

// Log transient upstream proxy failures (ECONNREFUSED/ECONNRESET while the
// backend is down or restarting) as a concise warning, instead of letting the
// unhandled http-proxy 'error' event surface as a stack trace / tear down the
// Vite dev server. oqto-sp28.
function proxyErrorLogger(label: string): ProxyOptions["configure"] {
	return (proxy) => {
		proxy.on("error", (err) => {
			const code = (err as NodeJS.ErrnoException).code ?? err.message;
			console.warn(
				`[vite proxy ${label}] upstream ${code} - backend down or restarting`,
			);
		});
	};
}

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
		try {
			fs.copyFileSync(ghosttyWasmSource, ghosttyWasmTarget);
		} catch (err) {
			// copyFileSync uses copy_file_range/reflink, which some filesystems
			// (CoW/overlay/virtiofs) reject with EPERM/ENOSYS/EXDEV even when
			// permissions are fine. Fall back to a plain read+write copy so the
			// dev server can start. oqto-y0zy.
			const code = (err as NodeJS.ErrnoException).code;
			if (code === "EPERM" || code === "ENOSYS" || code === "EXDEV") {
				fs.writeFileSync(ghosttyWasmTarget, fs.readFileSync(ghosttyWasmSource));
			} else {
				throw err;
			}
		}
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
	const caddyUrl = env.VITE_CADDY_BASE_URL || "http://127.0.0.1";
	const controlPlaneUrl = env.VITE_CONTROL_PLANE_URL || "http://127.0.0.1:8080";
	const fileserverUrl = env.VITE_FILE_SERVER_URL || "http://127.0.0.1:41821";

	return {
		plugins: [react(), copyGhosttyWasm()],
		resolve: {
			alias: {
				"@": path.resolve(__dirname, "./"),
			},
		},
		build: {
			rollupOptions: {
				input: {
					main: path.resolve(__dirname, "index.html"),
					// Standalone mini-app workbench (open /workbench.html in dev).
					workbench: path.resolve(__dirname, "workbench.html"),
				},
			},
		},
		optimizeDeps: {
			include: [
				"react",
				"react-dom",
				"react-router-dom",
				"@tanstack/react-query",
				"@tanstack/react-virtual",
				"lucide-react",
				"cmdk",
				"react-markdown",
				"remark-gfm",
				"react-syntax-highlighter",
				"recharts",
			],
			exclude: ["ghostty-web"],
		},
		server: {
			headers: {
				"Permissions-Policy": "geolocation=(), microphone=(self), camera=()",
			},
			host: true,
			port: 3000,
			allowedHosts: true,
			proxy: {
				"^/c/[^/]+/api": {
					target: caddyUrl,
					changeOrigin: true,
					configure: proxyErrorLogger("c/api"),
				},
				"^/c/[^/]+/files": {
					target: caddyUrl,
					changeOrigin: true,
					configure: proxyErrorLogger("c/files"),
				},
				"^/c/[^/]+/term": {
					target: caddyUrl,
					changeOrigin: true,
					ws: true,
					configure: proxyErrorLogger("c/term"),
				},
				"/api/files": {
					target: fileserverUrl,
					changeOrigin: true,
					rewrite: (pathValue) => pathValue.replace(/^\/api\/files/, ""),
					configure: proxyErrorLogger("api/files"),
				},
				"/api/models-dev": {
					target: "https://models.dev",
					changeOrigin: true,
					rewrite: (pathValue) => pathValue.replace(/^\/api\/models-dev/, ""),
					configure: proxyErrorLogger("api/models-dev"),
				},
				"/api": {
					target: controlPlaneUrl,
					changeOrigin: true,
					ws: true,
					configure: proxyErrorLogger("api"),
				},
			},
		},
	};
});
