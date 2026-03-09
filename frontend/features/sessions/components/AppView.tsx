"use client";

import { Button } from "@/components/ui/button";
import { readFileMux, writeFileMux } from "@/lib/mux-files";
import { cn } from "@/lib/utils";
import {
	AppWindow,
	Maximize2,
	Minimize2,
	RefreshCw,
	X,
} from "lucide-react";
import { useTheme } from "next-themes";
import {
	memo,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AppTab {
	id: string;
	filePath: string;
	title: string;
	content: string;
	pinned: boolean;
}

interface AppViewProps {
	workspacePath?: string | null;
	/** Tabs managed by parent */
	tabs: AppTab[];
	activeTabId: string | null;
	onSetActiveTab: (id: string) => void;
	onCloseTab: (id: string) => void;
	onUpdateTab: (id: string, patch: Partial<AppTab>) => void;
	className?: string;
	onExpand?: () => void;
	onCollapse?: () => void;
	isExpanded?: boolean;
}

// ---------------------------------------------------------------------------
// Apphost shim injected into srcdoc
// ---------------------------------------------------------------------------

function buildApphostShim(theme: string): string {
	return `<script>
(function() {
  var pending = {};
  var msgId = 0;

  function request(type, payload) {
    return new Promise(function(resolve, reject) {
      var id = ++msgId;
      pending[id] = { resolve: resolve, reject: reject };
      var msg = Object.assign({ source: "oqto-app", id: id, type: type }, payload);
      parent.postMessage(msg, "*");
    });
  }

  var themeCallbacks = [];
  var messageCallbacks = [];

  function applyThemeVars(vars) {
    if (!vars) return;
    var keys = Object.keys(vars);
    for (var i = 0; i < keys.length; i++) {
      document.documentElement.style.setProperty(keys[i], vars[keys[i]]);
    }
  }

  window.addEventListener("message", function(e) {
    if (!e.data || e.data.source !== "oqto-host") return;
    if (e.data.id && pending[e.data.id]) {
      var p = pending[e.data.id];
      delete pending[e.data.id];
      if (e.data.error) p.reject(new Error(e.data.error));
      else p.resolve(e.data.result);
      return;
    }
    if (e.data.type === "theme_change") {
      window.apphost.theme = e.data.theme;
      applyThemeVars(e.data.vars);
      for (var i = 0; i < themeCallbacks.length; i++) themeCallbacks[i](e.data.theme);
    }
    if (e.data.type === "state_update") {
      for (var j = 0; j < messageCallbacks.length; j++) messageCallbacks[j](e.data.data);
    }
  });

  window.apphost = {
    host: "oqto",
    theme: "${theme}",
    onThemeChange: function(cb) {
      themeCallbacks.push(cb);
      return function() { themeCallbacks = themeCallbacks.filter(function(c) { return c !== cb; }); };
    },
    send: function(data) {
      parent.postMessage({ source: "oqto-app", type: "app_message", data: data }, "*");
    },
    onMessage: function(cb) {
      messageCallbacks.push(cb);
      return function() { messageCallbacks = messageCallbacks.filter(function(c) { return c !== cb; }); };
    },
    readFile: function(path) { return request("read_file", { path: path }); },
    writeFile: function(path, data) { return request("write_file", { path: path, data: data }); },
    saveState: function(key, value) { return request("save_state", { key: key, value: value }); },
    loadState: function(key) { return request("load_state", { key: key }); },
  };

  parent.postMessage({ source: "oqto-app", type: "ready" }, "*");
})();
</script>
<style>
:root {
  --app-bg: #0f1210;
  --app-fg: #e0e4e1;
  --app-card: #181b1a;
  --app-card-fg: #e0e4e1;
  --app-primary: #3ba77c;
  --app-primary-fg: #ffffff;
  --app-muted: #232826;
  --app-muted-fg: #9ca89e;
  --app-border: #2a2f2c;
  --app-destructive: #e74c3c;
  --app-success: #3ba77c;
  --app-warning: #f39c12;
  --app-info: #3498db;
  --app-font: ui-sans-serif, system-ui, sans-serif;
  --app-radius: 0.5rem;
}
body {
  background: var(--app-bg);
  color: var(--app-fg);
  font-family: var(--app-font);
  margin: 0;
  padding: 0;
}
</style>`;
}

const LIGHT_THEME_VARS: Record<string, string> = {
	"--app-bg": "#f8faf9",
	"--app-fg": "#1a1f1c",
	"--app-card": "#ffffff",
	"--app-card-fg": "#1a1f1c",
	"--app-primary": "#3ba77c",
	"--app-primary-fg": "#ffffff",
	"--app-muted": "#f0f2f1",
	"--app-muted-fg": "#6b7c6e",
	"--app-border": "#d4dbd6",
	"--app-destructive": "#e74c3c",
	"--app-success": "#3ba77c",
	"--app-warning": "#f39c12",
	"--app-info": "#3498db",
};

const DARK_THEME_VARS: Record<string, string> = {
	"--app-bg": "#0f1210",
	"--app-fg": "#e0e4e1",
	"--app-card": "#181b1a",
	"--app-card-fg": "#e0e4e1",
	"--app-primary": "#3ba77c",
	"--app-primary-fg": "#ffffff",
	"--app-muted": "#232826",
	"--app-muted-fg": "#9ca89e",
	"--app-border": "#2a2f2c",
	"--app-destructive": "#e74c3c",
	"--app-success": "#3ba77c",
	"--app-warning": "#f39c12",
	"--app-info": "#3498db",
};

function injectApphost(html: string, theme: string): string {
	const shim = buildApphostShim(theme);
	if (html.includes("</head>")) {
		return html.replace("</head>", `${shim}</head>`);
	}
	if (html.includes("<html")) {
		return html.replace(/<html([^>]*)>/, `<html$1>${shim}`);
	}
	return `${shim}${html}`;
}

function sanitizePath(path: string): string | null {
	const normalized = path.replace(/\\/g, "/");
	if (normalized.startsWith("/") || normalized.includes("..")) return null;
	return normalized;
}

function titleFromPath(filePath: string): string {
	const name = filePath.split("/").pop() ?? filePath;
	return name.replace(/\.html?$/i, "");
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const AppView = memo(function AppView({
	workspacePath,
	tabs,
	activeTabId,
	onSetActiveTab,
	onCloseTab,
	onUpdateTab,
	className,
	onExpand,
	onCollapse,
	isExpanded,
}: AppViewProps) {
	const iframeRef = useRef<HTMLIFrameElement>(null);
	const { resolvedTheme } = useTheme();
	const theme = resolvedTheme === "dark" ? "dark" : "light";
	const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;
	const [loading, setLoading] = useState(false);

	// Load file content for a tab
	const loadTab = useCallback(
		async (tab: AppTab) => {
			if (!workspacePath) return;
			setLoading(true);
			try {
				const result = await readFileMux(workspacePath, tab.filePath);
				const text = new TextDecoder().decode(result.data);
				// Extract <title> if present
				const titleMatch = text.match(/<title>([^<]+)<\/title>/i);
				const title = titleMatch ? titleMatch[1].trim() : titleFromPath(tab.filePath);
				onUpdateTab(tab.id, { content: text, title });
			} catch (err) {
				const msg = err instanceof Error ? err.message : "Failed to load file";
				onUpdateTab(tab.id, {
					content: `<html><body style="color:#e74c3c;font-family:sans-serif;padding:2rem;"><h2>Error loading app</h2><p>${msg}</p><p><code>${tab.filePath}</code></p></body></html>`,
				});
			} finally {
				setLoading(false);
			}
		},
		[workspacePath, onUpdateTab],
	);

	// Load content when active tab changes or has no content
	useEffect(() => {
		if (activeTab && !activeTab.content) {
			void loadTab(activeTab);
		}
	}, [activeTab, loadTab]);

	// Handle postMessage from iframe
	useEffect(() => {
		const handler = async (e: MessageEvent) => {
			if (!e.data || e.data.source !== "oqto-app") return;
			const iframe = iframeRef.current;
			if (!iframe || e.source !== iframe.contentWindow) return;
			if (!workspacePath) return;

			const { type, id } = e.data;

			const respond = (result: unknown, error?: string) => {
				iframe.contentWindow?.postMessage(
					{ source: "oqto-host", id, result, error },
					"*",
				);
			};

			try {
				switch (type) {
					case "read_file": {
						const safePath = sanitizePath(e.data.path);
						if (!safePath) {
							respond(null, "Invalid path");
							return;
						}
						const file = await readFileMux(workspacePath, safePath);
						respond(new TextDecoder().decode(file.data));
						break;
					}
					case "write_file": {
						const safePath = sanitizePath(e.data.path);
						if (!safePath) {
							respond(null, "Invalid path");
							return;
						}
						const content =
							typeof e.data.data === "string"
								? e.data.data
								: JSON.stringify(e.data.data);
						const encoded = new TextEncoder().encode(content);
						await writeFileMux(
							workspacePath,
							safePath,
							encoded.buffer as ArrayBuffer,
							true,
						);
						respond(true);
						break;
					}
					case "save_state": {
						const key = String(e.data.key).replace(/[^a-zA-Z0-9_-]/g, "");
						if (!key) {
							respond(null, "Invalid state key");
							return;
						}
						const stateContent = new TextEncoder().encode(
							JSON.stringify(e.data.value),
						);
						await writeFileMux(
							workspacePath,
							`.oqto/app-state/${key}.json`,
							stateContent.buffer as ArrayBuffer,
							true,
						);
						respond(true);
						break;
					}
					case "load_state": {
						const key = String(e.data.key).replace(/[^a-zA-Z0-9_-]/g, "");
						if (!key) {
							respond(null);
							return;
						}
						try {
							const stateFile = await readFileMux(
								workspacePath,
								`.oqto/app-state/${key}.json`,
							);
							const value = JSON.parse(
								new TextDecoder().decode(stateFile.data),
							);
							respond(value);
						} catch {
							respond(null);
						}
						break;
					}
					case "app_message": {
						// Phase 2: route to agent session
						break;
					}
					case "ready": {
						// Send current theme vars
						const vars = theme === "dark" ? DARK_THEME_VARS : LIGHT_THEME_VARS;
						iframe.contentWindow?.postMessage(
							{ source: "oqto-host", type: "theme_change", theme, vars },
							"*",
						);
						break;
					}
				}
			} catch (err) {
				const msg = err instanceof Error ? err.message : "Unknown error";
				respond(null, msg);
			}
		};

		window.addEventListener("message", handler);
		return () => window.removeEventListener("message", handler);
	}, [workspacePath, theme]);

	// Push theme changes to iframe
	useEffect(() => {
		const iframe = iframeRef.current;
		if (!iframe?.contentWindow) return;
		const vars = theme === "dark" ? DARK_THEME_VARS : LIGHT_THEME_VARS;
		iframe.contentWindow.postMessage(
			{ source: "oqto-host", type: "theme_change", theme, vars },
			"*",
		);
	}, [theme]);

	const handleRefresh = useCallback(() => {
		if (activeTab) {
			onUpdateTab(activeTab.id, { content: "" });
			void loadTab(activeTab);
		}
	}, [activeTab, onUpdateTab, loadTab]);

	if (tabs.length === 0) {
		return (
			<div className={cn("flex items-center justify-center h-full", className)}>
				<div className="text-center space-y-2 text-muted-foreground">
					<AppWindow className="w-8 h-8 mx-auto opacity-50" />
					<p className="text-sm">No apps open</p>
					<p className="text-xs">
						Right-click an HTML file and select "Open as App"
					</p>
				</div>
			</div>
		);
	}

	const srcdoc = activeTab?.content
		? injectApphost(activeTab.content, theme)
		: undefined;

	return (
		<div className={cn("flex flex-col h-full", className)}>
			{/* Tab bar */}
			<div className="flex items-center gap-0.5 px-1 py-1 border-b border-border bg-muted/30 min-h-[36px]">
				<div className="flex-1 flex items-center gap-0.5 overflow-x-auto scrollbar-none [scrollbar-width:none] [-ms-overflow-style:none] [&::-webkit-scrollbar]:hidden">
					{tabs.map((tab) => (
						<button
							key={tab.id}
							type="button"
							onClick={() => onSetActiveTab(tab.id)}
							className={cn(
								"flex items-center gap-1.5 px-2.5 py-1 text-xs rounded transition-colors max-w-[160px] group",
								tab.id === activeTabId
									? "bg-background text-foreground shadow-sm"
									: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
							)}
						>
							<AppWindow className="w-3 h-3 flex-shrink-0" />
							<span className="truncate">{tab.title}</span>
							{!tab.pinned && (
								<button
									type="button"
									onClick={(e) => {
										e.stopPropagation();
										onCloseTab(tab.id);
									}}
									className="ml-auto flex-shrink-0 opacity-0 group-hover:opacity-100 p-0.5 hover:bg-muted rounded transition-opacity"
									aria-label="Close tab"
								>
									<X className="w-3 h-3" />
								</button>
							)}
						</button>
					))}
				</div>
				<div className="flex items-center gap-0.5 flex-shrink-0">
					<button
						type="button"
						onClick={handleRefresh}
						className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
						title="Refresh"
					>
						<RefreshCw className={cn("w-3.5 h-3.5", loading && "animate-spin")} />
					</button>
					{onExpand && !isExpanded && (
						<button
							type="button"
							onClick={onExpand}
							className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
							title="Expand"
						>
							<Maximize2 className="w-3.5 h-3.5" />
						</button>
					)}
					{onCollapse && isExpanded && (
						<button
							type="button"
							onClick={onCollapse}
							className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
							title="Collapse"
						>
							<Minimize2 className="w-3.5 h-3.5" />
						</button>
					)}
				</div>
			</div>

			{/* Iframe */}
			<div className="flex-1 min-h-0 relative">
				{loading && (
					<div className="absolute inset-0 flex items-center justify-center bg-background/50 z-10">
						<RefreshCw className="w-5 h-5 animate-spin text-muted-foreground" />
					</div>
				)}
				{srcdoc && (
					<iframe
						ref={iframeRef}
						key={activeTab?.id}
						srcDoc={srcdoc}
						sandbox="allow-scripts allow-forms allow-modals allow-popups"
						className="w-full h-full border-0"
						title={activeTab?.title ?? "App"}
					/>
				)}
			</div>
		</div>
	);
});
