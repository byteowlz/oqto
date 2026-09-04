"use client";

import {
	VISUAL_RUNTIME_MODE_DEFAULT,
	prepareVisualRuntimeDocument,
} from "@/features/sessions/visual-runtime";
import {
	decideAppPermissions,
	fetchAppPresentation,
	getAppPermissions,
	listAppCandidates,
	listAppInstances,
	publishApp,
} from "@/lib/api/apps";
import { readFileMux, writeFileMux } from "@/lib/mux-files";
import { cn } from "@/lib/utils";
import type { AppCandidateSummary } from "@/src/generated/AppCandidateSummary";
import type { AppInstanceSummary } from "@/src/generated/AppInstanceSummary";
import type { AppPermissionRequest } from "@/src/generated/AppPermissionRequest";
import type { AppPresentationDocument } from "@/src/generated/AppPresentationDocument";
import { AppWindow, Maximize2, Minimize2, RefreshCw, X } from "lucide-react";
import { useTheme } from "next-themes";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AppPermissionDialog } from "./AppPermissionDialog";
import { RuntimeOqtoAppFrame } from "./RuntimeOqtoAppFrame";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface LegacyHtmlAppTab {
	kind: "legacy-html";
	id: string;
	filePath: string;
	title: string;
	content: string;
	pinned: boolean;
}

export interface OqtoAppTab {
	kind: "oqto-app";
	id: string;
	appId: string;
	instanceId: string;
	installationId: string;
	definitionId: string;
	title: string;
	/** Self-contained inlined document fetched over the authenticated API. */
	html: string;
	pinned: boolean;
}

export type AppTab = LegacyHtmlAppTab | OqtoAppTab;

interface AppViewProps {
	workspacePath?: string | null;
	/** Tabs managed by parent */
	tabs: AppTab[];
	activeTabId: string | null;
	onSetActiveTab: (id: string | null) => void;
	onCloseTab: (id: string) => void;
	onUpdateTab: (id: string, patch: Partial<AppTab>) => void;
	onOpenOqtoApp: (
		instance: AppInstanceSummary,
		presentation: AppPresentationDocument,
	) => void;
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

export interface AppCatalogEntry {
	appId: string;
	candidate?: AppCandidateSummary;
	instance?: AppInstanceSummary;
}

export function buildAppCatalogEntries(
	candidates: AppCandidateSummary[],
	instances: AppInstanceSummary[],
): AppCatalogEntry[] {
	const instancesByApp = new Map(
		instances.map((instance) => [instance.app_id, instance]),
	);
	const discovered = candidates.map((candidate) => ({
		appId: candidate.app_id,
		candidate,
		instance: instancesByApp.get(candidate.app_id),
	}));
	const discoveredIds = new Set(
		candidates.map((candidate) => candidate.app_id),
	);
	const installedOnly = instances
		.filter((instance) => !discoveredIds.has(instance.app_id))
		.map((instance) => ({
			appId: instance.app_id,
			instance,
		}));
	return [...discovered, ...installedOnly];
}

function RuntimeAppCatalog({
	workspacePath,
	onOpen,
}: {
	workspacePath: string;
	onOpen: (
		instance: AppInstanceSummary,
		presentation: AppPresentationDocument,
	) => void;
}) {
	const { t } = useTranslation();
	const [candidates, setCandidates] = useState<AppCandidateSummary[]>([]);
	const [instances, setInstances] = useState<AppInstanceSummary[]>([]);
	const [loading, setLoading] = useState(true);
	const [busyAppId, setBusyAppId] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [permissionRequest, setPermissionRequest] =
		useState<AppPermissionRequest | null>(null);
	const [permissionError, setPermissionError] = useState<string | null>(null);

	const load = useCallback(async () => {
		setLoading(true);
		setError(null);
		try {
			const [candidateList, instanceList] = await Promise.all([
				listAppCandidates(workspacePath),
				listAppInstances(workspacePath),
			]);
			setCandidates(candidateList.candidates);
			setInstances(instanceList.instances);
		} catch (loadError) {
			setError(
				loadError instanceof Error ? loadError.message : t("apps.loadFailed"),
			);
		} finally {
			setLoading(false);
		}
	}, [workspacePath, t]);

	// useeffect-guardrail: allow: runtime App discovery must follow the selected
	// authenticated work directory and is cancelled by React on unmount.
	useEffect(() => {
		void load();
	}, [load]);

	const openInstance = useCallback(
		async (instance: AppInstanceSummary) => {
			setBusyAppId(instance.app_id);
			setError(null);
			try {
				if (instance.status === "awaiting_permission") {
					const status = await getAppPermissions(
						workspacePath,
						instance.instance_id,
					);
					setPermissionError(null);
					setPermissionRequest(status.request);
					return;
				}
				const presentation = await fetchAppPresentation(
					workspacePath,
					instance.instance_id,
				);
				onOpen(instance, presentation);
			} catch (openError) {
				setError(
					openError instanceof Error ? openError.message : t("apps.openFailed"),
				);
			} finally {
				setBusyAppId(null);
			}
		},
		[workspacePath, onOpen, t],
	);

	const publishCandidate = useCallback(
		async (candidate: AppCandidateSummary) => {
			setBusyAppId(candidate.app_id);
			setError(null);
			try {
				const result = await publishApp(workspacePath, candidate.app_id);
				if (
					result.status === "awaiting_permission" &&
					result.permission_request
				) {
					setPermissionError(null);
					setPermissionRequest(result.permission_request);
					await load();
					return;
				}
				if (result.status !== "instance_ready" || !result.instance) {
					throw new Error(result.rejection?.message ?? t("apps.publishFailed"));
				}
				const presentation = await fetchAppPresentation(
					workspacePath,
					result.instance.instance_id,
				);
				onOpen(result.instance, presentation);
				await load();
			} catch (publishError) {
				setError(
					publishError instanceof Error
						? publishError.message
						: t("apps.publishFailed"),
				);
			} finally {
				setBusyAppId(null);
			}
		},
		[workspacePath, onOpen, load, t],
	);

	const decidePermission = useCallback(
		async (decision: "allow" | "deny") => {
			if (!permissionRequest) return;
			setBusyAppId(permissionRequest.app_id);
			setPermissionError(null);
			try {
				const status = await decideAppPermissions(
					workspacePath,
					permissionRequest.instance_id,
					decision,
					permissionRequest.content_digest,
				);
				setPermissionRequest(null);
				await load();
				if (decision === "allow") {
					const refreshed = (
						await listAppInstances(workspacePath)
					).instances.find(
						(instance) => instance.instance_id === status.request.instance_id,
					);
					if (!refreshed) throw new Error(t("apps.openFailed"));
					const presentation = await fetchAppPresentation(
						workspacePath,
						refreshed.instance_id,
					);
					onOpen(refreshed, presentation);
				}
			} catch (decisionError) {
				setPermissionError(
					decisionError instanceof Error
						? decisionError.message
						: t("apps.publishFailed"),
				);
			} finally {
				setBusyAppId(null);
			}
		},
		[permissionRequest, workspacePath, load, onOpen, t],
	);

	const catalogEntries = useMemo(
		() => buildAppCatalogEntries(candidates, instances),
		[candidates, instances],
	);

	const openCatalogEntry = useCallback(
		(entry: (typeof catalogEntries)[number]) => {
			if (entry.candidate?.state === "publishable") {
				void publishCandidate(entry.candidate);
				return;
			}
			if (entry.instance) void openInstance(entry.instance);
		},
		[publishCandidate, openInstance],
	);

	return (
		<div className="h-full overflow-y-auto p-3 space-y-4">
			<AppPermissionDialog
				request={permissionRequest}
				busy={
					permissionRequest !== null && busyAppId === permissionRequest.app_id
				}
				error={permissionError}
				onAllow={() => void decidePermission("allow")}
				onNotNow={() => {
					setPermissionRequest(null);
					setPermissionError(null);
				}}
				onClose={() => {
					setPermissionRequest(null);
					setPermissionError(null);
				}}
			/>
			<div className="flex items-center justify-between gap-3">
				<div>
					<h2 className="text-sm font-semibold">{t("apps.title")}</h2>
					<p className="text-xs text-muted-foreground">
						{t("apps.description")}
					</p>
				</div>
				<button
					type="button"
					onClick={() => void load()}
					className="min-h-11 min-w-11 inline-flex items-center justify-center border border-border text-muted-foreground hover:text-foreground"
					aria-label={t("common.refresh")}
				>
					<RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
				</button>
			</div>

			{error && (
				<div className="border border-destructive/50 bg-destructive/10 p-2 text-xs text-destructive">
					{error}
				</div>
			)}

			<section className="space-y-2" aria-label={t("apps.available")}>
				{!loading && catalogEntries.length === 0 && (
					<p className="text-xs text-muted-foreground">{t("apps.noneFound")}</p>
				)}
				{catalogEntries.map((entry) => {
					const app = entry.candidate ?? entry.instance;
					if (!app) return null;
					const canOpen =
						entry.candidate?.state === "publishable" ||
						entry.instance !== undefined;
					return (
						<div
							key={entry.appId}
							className="flex items-center justify-between gap-3 border border-border bg-card/30 p-3"
						>
							<div className="min-w-0">
								<div className="flex items-center gap-2">
									<AppWindow
										className="size-4 shrink-0 text-primary"
										aria-hidden="true"
									/>
									<div className="truncate text-sm font-medium">
										{app.title.en}
									</div>
								</div>
								{entry.candidate?.description && (
									<p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
										{entry.candidate.description}
									</p>
								)}
								<div className="mt-1 text-[11px] text-muted-foreground">
									{entry.instance?.status === "awaiting_permission"
										? t("apps.accessNeeded")
										: t("apps.availableInWorkspace")}
									<span aria-hidden="true"> · </span>
									<span className="font-mono">v{app.version}</span>
								</div>
							</div>
							<button
								type="button"
								onClick={() => openCatalogEntry(entry)}
								disabled={!canOpen || busyAppId === entry.appId}
								className="min-h-11 shrink-0 border border-primary bg-primary px-4 text-sm font-medium text-primary-foreground disabled:opacity-50"
							>
								{entry.instance?.status === "awaiting_permission"
									? t("apps.reviewAndOpen")
									: t("common.open")}
							</button>
							{entry.candidate?.rejection && (
								<p className="text-xs text-destructive">
									{entry.candidate.rejection.message}
								</p>
							)}
						</div>
					);
				})}
			</section>
		</div>
	);
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
	onOpenOqtoApp,
	className,
	onExpand,
	onCollapse,
	isExpanded,
}: AppViewProps) {
	const { t } = useTranslation();
	const iframeRef = useRef<HTMLIFrameElement>(null);
	const { resolvedTheme } = useTheme();
	const theme = resolvedTheme === "dark" ? "dark" : "light";
	const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;
	const [loading, setLoading] = useState(false);

	// Load file content for a tab
	const loadTab = useCallback(
		async (tab: LegacyHtmlAppTab) => {
			if (!workspacePath) return;
			setLoading(true);
			try {
				const result = await readFileMux(workspacePath, tab.filePath);
				const text = new TextDecoder().decode(result.data);
				// Extract <title> if present
				const titleMatch = text.match(/<title>([^<]+)<\/title>/i);
				const title = titleMatch
					? titleMatch[1].trim()
					: titleFromPath(tab.filePath);
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
		if (activeTab?.kind === "legacy-html" && !activeTab.content) {
			void loadTab(activeTab);
		}
	}, [activeTab, loadTab]);

	// Handle postMessage from iframe
	useEffect(() => {
		const handler = async (e: MessageEvent) => {
			if (activeTab?.kind !== "legacy-html") return;
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
	}, [workspacePath, theme, activeTab?.kind]);

	// Push theme changes only to the deprecated srcdoc runtime. Runtime-discovered
	// Apps negotiate theme through the exact-origin SDK Bridge in a later slice.
	useEffect(() => {
		if (activeTab?.kind !== "legacy-html") return;
		const iframe = iframeRef.current;
		if (!iframe?.contentWindow) return;
		const vars = theme === "dark" ? DARK_THEME_VARS : LIGHT_THEME_VARS;
		iframe.contentWindow.postMessage(
			{ source: "oqto-host", type: "theme_change", theme, vars },
			"*",
		);
	}, [theme, activeTab?.kind]);

	const handleRefresh = useCallback(() => {
		if (activeTab?.kind === "legacy-html") {
			onUpdateTab(activeTab.id, { content: "" });
			void loadTab(activeTab);
		}
	}, [activeTab, onUpdateTab, loadTab]);

	const preparedDocument = useMemo(() => {
		if (activeTab?.kind !== "legacy-html" || !activeTab.content) return null;
		const withHostShim = injectApphost(activeTab.content, theme);
		return prepareVisualRuntimeDocument({
			html: withHostShim,
			mode: VISUAL_RUNTIME_MODE_DEFAULT,
		});
	}, [activeTab, theme]);

	const srcdoc = preparedDocument?.html;
	const runtimeDiagnostics = preparedDocument?.diagnostics ?? [];
	const errorDiagnostics = runtimeDiagnostics.filter(
		(d) => d.level === "error",
	);
	const warnDiagnostics = runtimeDiagnostics.filter((d) => d.level === "warn");

	return (
		<div className={cn("flex flex-col h-full", className)}>
			{/* Tab bar */}
			<div className="flex items-center gap-0.5 px-1 py-1 border-b border-border bg-muted/30 min-h-[36px]">
				<div className="flex-1 flex items-center gap-0.5 overflow-x-auto scrollbar-none [scrollbar-width:none] [-ms-overflow-style:none] [&::-webkit-scrollbar]:hidden">
					<button
						type="button"
						onClick={() => onSetActiveTab(null)}
						className={cn(
							"flex items-center gap-1.5 px-2.5 py-1 text-xs transition-colors",
							activeTabId === null
								? "bg-background text-foreground"
								: "text-muted-foreground hover:text-foreground",
						)}
					>
						<AppWindow className="w-3 h-3" />
						<span>{t("apps.catalog")}</span>
					</button>
					{tabs.map((tab) => (
						<div
							key={tab.id}
							className={cn(
								"flex items-stretch text-xs max-w-[200px]",
								tab.id === activeTabId
									? "bg-background text-foreground shadow-sm"
									: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
							)}
						>
							<button
								type="button"
								onClick={() => onSetActiveTab(tab.id)}
								className="flex min-w-0 items-center gap-1.5 px-2.5 py-1"
							>
								<AppWindow className="w-3 h-3 flex-shrink-0" />
								<span className="truncate">{tab.title}</span>
							</button>
							{!tab.pinned && (
								<button
									type="button"
									onClick={() => onCloseTab(tab.id)}
									className="flex size-11 flex-shrink-0 items-center justify-center text-muted-foreground transition-colors hover:bg-muted hover:text-foreground md:size-7"
									aria-label={`Close ${tab.title}`}
									title={`Close ${tab.title}`}
								>
									<X className="w-3.5 h-3.5" />
								</button>
							)}
						</div>
					))}
				</div>
				<div className="flex items-center gap-0.5 flex-shrink-0">
					{activeTab?.kind === "legacy-html" && (
						<button
							type="button"
							onClick={handleRefresh}
							className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
							title="Refresh"
						>
							<RefreshCw
								className={cn("w-3.5 h-3.5", loading && "animate-spin")}
							/>
						</button>
					)}
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

			{runtimeDiagnostics.length > 0 && (
				<div className="border-b border-border/60 bg-muted/20 px-2 py-1 text-[11px] text-muted-foreground flex items-center gap-2">
					<span>
						Visual runtime diagnostics: {errorDiagnostics.length} error(s),{" "}
						{warnDiagnostics.length} warning(s)
					</span>
					<span
						className="truncate"
						title={runtimeDiagnostics
							.map((d) => `[${d.level}] ${d.message}`)
							.join("\n")}
					>
						{runtimeDiagnostics[runtimeDiagnostics.length - 1]?.message}
					</span>
				</div>
			)}

			{/* Iframe */}
			<div className="flex-1 min-h-0 relative">
				{activeTab === null && workspacePath && (
					<RuntimeAppCatalog
						workspacePath={workspacePath}
						onOpen={onOpenOqtoApp}
					/>
				)}
				{activeTab === null && !workspacePath && (
					<div className="h-full flex items-center justify-center text-xs text-muted-foreground">
						{t("apps.noWorkspace")}
					</div>
				)}
				{loading && activeTab?.kind === "legacy-html" && (
					<div className="absolute inset-0 flex items-center justify-center bg-background/50 z-10">
						<RefreshCw className="w-5 h-5 animate-spin text-muted-foreground" />
					</div>
				)}
				{activeTab?.kind === "oqto-app" && (
					<RuntimeOqtoAppFrame
						key={activeTab.id}
						instanceId={activeTab.instanceId}
						installationId={activeTab.installationId}
						definitionId={activeTab.definitionId}
						html={activeTab.html}
						title={activeTab.title}
						workspacePath={workspacePath ?? ""}
					/>
				)}
				{activeTab?.kind === "legacy-html" && srcdoc && (
					<iframe
						ref={iframeRef}
						key={activeTab.id}
						srcDoc={srcdoc}
						sandbox="allow-scripts allow-forms allow-modals allow-popups"
						className="w-full h-full border-0"
						title={activeTab.title}
					/>
				)}
			</div>
		</div>
	);
});
