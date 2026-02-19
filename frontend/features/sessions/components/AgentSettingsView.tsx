"use client";

import { ProviderIcon } from "@/components/data-display";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useApp } from "@/hooks/use-app";
import { fetchAgents } from "@/lib/agent-client";
import {
	type PermissionAction,
	type PermissionConfig,
	type ShareMode,
	type WorkspaceConfig,
	getGlobalAgentConfig,
	getWorkspaceConfig,
	restartWorkspaceSession,
	saveWorkspaceConfig,
} from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import {
	AlertCircle,
	Check,
	Info,
	Loader2,
	RefreshCw,
	RotateCcw,
	Save,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

interface ModelOption {
	value: string;
	label: string;
}

interface AgentSettingsViewProps {
	className?: string;
	/** Available models for runtime selection */
	modelOptions?: ModelOption[];
	/** Currently selected model ref (provider/model) */
	selectedModelRef?: string | null;
	/** Callback to change the selected model */
	onModelChange?: (modelRef: string) => void;
	/** Whether model options are loading */
	isModelLoading?: boolean;
}

interface AgentInfo {
	id: string;
	name: string;
	description?: string;
}

export function AgentSettingsView({
	className,
	modelOptions = [],
	selectedModelRef,
	onModelChange,
	isModelLoading = false,
}: AgentSettingsViewProps) {
	const {
		selectedWorkspaceSession,
		agentBaseUrl,
		agentDirectory,
		busySessions,
		refreshWorkspaceSessions,
	} = useApp();
	const sessionId = selectedWorkspaceSession?.id;

	// Global config (legacy) - read-only reference
	const [globalConfig, setGlobalConfig] = useState<WorkspaceConfig | null>(
		null,
	);
	// Local workspace config - editable
	const [localConfig, setLocalConfig] = useState<WorkspaceConfig | null>(null);
	const [agents, setAgents] = useState<AgentInfo[]>([]);
	const [loading, setLoading] = useState(true);
	const [saving, setSaving] = useState(false);
	const [restarting, setRestarting] = useState(false);
	const [waitingForIdle, setWaitingForIdle] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [success, setSuccess] = useState(false);
	const [pendingChanges, setPendingChanges] = useState<
		Partial<WorkspaceConfig>
	>({});

	// Check if the current chat session is busy
	// We need to find the chat session ID associated with this workspace session
	const isBusy = sessionId ? busySessions.has(sessionId) : false;

	// Load config and agents
	// clearPending: only clear pending changes on explicit reload, not on dependency changes
	const loadData = useCallback(
		async (clearPending = false) => {
			if (!sessionId) return;

			setLoading(true);
			setError(null);
			try {
				// Fetch both global and local configs in parallel
				const [globalData, localData] = await Promise.all([
					getGlobalAgentConfig(),
					getWorkspaceConfig(sessionId),
				]);

				// Fetch agents from agent API (requires running instance)
				let agentsList: AgentInfo[] = [];
				if (agentBaseUrl) {
					try {
						const agentsData = await fetchAgents(agentBaseUrl, {
							directory: agentDirectory,
						});
						// fetchAgents returns an array of AgentInfo objects
						agentsList = (agentsData || []).map((agent) => ({
							id: agent.id,
							name: agent.name || agent.id,
							description: agent.description,
						}));
					} catch {
						// Agents API not available, continue without agents
					}
				}

				setGlobalConfig(globalData || {});
				setLocalConfig(localData || {});
				setAgents(agentsList);
				// Only clear pending changes when explicitly requested (reload button)
				if (clearPending) {
					setPendingChanges({});
				}
			} catch (err) {
				setError(
					err instanceof Error ? err.message : "Failed to load settings",
				);
			} finally {
				setLoading(false);
			}
		},
		[sessionId, agentBaseUrl, agentDirectory],
	);

	// For save operations, we work with local config only
	const config = localConfig;

	// Track the previous sessionId to detect session changes
	const prevSessionIdRef = useRef<string | undefined>(undefined);

	useEffect(() => {
		// Clear pending changes only when session changes, not on other dependency changes
		const sessionChanged = prevSessionIdRef.current !== sessionId;
		prevSessionIdRef.current = sessionId;
		loadData(sessionChanged);
	}, [loadData, sessionId]);

	// Save changes
	const handleSave = useCallback(async () => {
		if (!sessionId || Object.keys(pendingChanges).length === 0) return;

		setSaving(true);
		setError(null);
		setSuccess(false);
		try {
			const newConfig = { ...localConfig, ...pendingChanges };
			await saveWorkspaceConfig(sessionId, newConfig);
			setLocalConfig(newConfig);
			setPendingChanges({});
			setSuccess(true);
			setTimeout(() => setSuccess(false), 2000);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to save settings");
		} finally {
			setSaving(false);
		}
	}, [sessionId, localConfig, pendingChanges]);

	// Restart session - waits for agent to finish if busy
	const handleRestart = useCallback(async () => {
		if (!sessionId) return;

		// If agent is busy, mark that we're waiting
		if (isBusy) {
			setWaitingForIdle(true);
			return;
		}

		setRestarting(true);
		setError(null);
		try {
			await restartWorkspaceSession(sessionId);
			// Refresh sessions list to get updated status
			await refreshWorkspaceSessions();
		} catch (err) {
			setError(
				err instanceof Error ? err.message : "Failed to restart session",
			);
		} finally {
			setRestarting(false);
			setWaitingForIdle(false);
		}
	}, [sessionId, isBusy, refreshWorkspaceSessions]);

	// When waiting for idle and agent becomes idle, trigger restart
	useEffect(() => {
		if (waitingForIdle && !isBusy && sessionId) {
			handleRestart();
		}
	}, [waitingForIdle, isBusy, sessionId, handleRestart]);

	// Update a value
	const handleChange = useCallback(
		<K extends keyof WorkspaceConfig>(key: K, value: WorkspaceConfig[K]) => {
			setPendingChanges((prev) => ({ ...prev, [key]: value }));
		},
		[],
	);

	// Get effective value: pending changes > local config > global config
	const getValue = <K extends keyof WorkspaceConfig>(
		key: K,
	): WorkspaceConfig[K] | undefined => {
		if (key in pendingChanges) return pendingChanges[key] as WorkspaceConfig[K];
		if (localConfig?.[key] !== undefined) return localConfig[key];
		return globalConfig?.[key];
	};

	// Get the source of a value: "pending" | "local" | "global" | "default"
	const getValueSource = (
		key: keyof WorkspaceConfig,
	): "pending" | "local" | "global" | "default" => {
		if (key in pendingChanges) return "pending";
		if (localConfig?.[key] !== undefined) return "local";
		if (globalConfig?.[key] !== undefined) return "global";
		return "default";
	};

	// Check if explicitly set in local config (not counting pending)
	const isSetInLocal = (key: keyof WorkspaceConfig): boolean => {
		return localConfig?.[key] !== undefined;
	};

	// Check if explicitly set in global config
	const isSetInGlobal = (key: keyof WorkspaceConfig): boolean => {
		return globalConfig?.[key] !== undefined;
	};

	// Check if a field is modified (has pending changes)
	const isModified = (key: keyof WorkspaceConfig): boolean => {
		return key in pendingChanges;
	};

	// Reset a field
	const handleReset = useCallback((key: keyof WorkspaceConfig) => {
		setPendingChanges((prev) => {
			const next = { ...prev };
			delete next[key];
			return next;
		});
	}, []);

	// Model search/filter state - must be before early returns (Rules of Hooks)
	const [modelQuery, setModelQuery] = useState("");
	const filteredModelOptions = useMemo(() => {
		const query = modelQuery.trim().toLowerCase();
		if (!query) return modelOptions;
		return modelOptions.filter(
			(opt) =>
				opt.value.toLowerCase().includes(query) ||
				opt.label.toLowerCase().includes(query),
		);
	}, [modelOptions, modelQuery]);

	if (!sessionId) {
		return (
			<div
				className={cn("flex items-center justify-center h-full p-4", className)}
			>
				<p className="text-sm text-muted-foreground">No session selected</p>
			</div>
		);
	}

	if (loading) {
		return (
			<div
				className={cn("flex items-center justify-center h-full p-4", className)}
			>
				<Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
			</div>
		);
	}

	const hasChanges = Object.keys(pendingChanges).length > 0;

	return (
		<div className={cn("flex flex-col h-full", className)}>
			{/* Header */}
			<div className="flex items-center justify-between p-3 border-b border-border">
				<span className="text-sm font-medium">Agent Settings</span>
				<div className="flex items-center gap-1">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={() => loadData(true)}
						disabled={loading}
						className="h-7 w-7 p-0"
						title="Reload"
					>
						<RefreshCw
							className={cn("h-3.5 w-3.5", loading && "animate-spin")}
						/>
					</Button>
					<Button
						type="button"
						size="sm"
						onClick={handleSave}
						disabled={!hasChanges || saving}
						className="h-7 px-2"
					>
						{saving ? (
							<Loader2 className="h-3.5 w-3.5 animate-spin" />
						) : success ? (
							<Check className="h-3.5 w-3.5" />
						) : (
							<Save className="h-3.5 w-3.5" />
						)}
						<span className="ml-1.5 text-xs">
							{saving ? "Saving" : success ? "Saved" : "Save"}
						</span>
					</Button>
				</div>
			</div>

			{/* Error message */}
			{error && (
				<div className="flex items-center gap-2 p-3 m-3 bg-destructive/10 text-destructive rounded-md">
					<AlertCircle className="h-4 w-4 flex-shrink-0" />
					<span className="text-xs">{error}</span>
				</div>
			)}

			{/* Settings form */}
			<div className="flex-1 overflow-y-auto overflow-x-hidden p-3 space-y-4">
				{/* Runtime Model Selector - Live section */}
				{onModelChange && (
					<div className="p-3 bg-primary/5 border border-primary/20 rounded-lg space-y-2 overflow-hidden">
						<div className="flex items-center gap-2">
							<div className="w-2 h-2 rounded-full bg-green-500 animate-pulse flex-shrink-0" />
							<Label className="text-xs font-medium">
								Live Model Selection
							</Label>
						</div>
						<Select
							value={selectedModelRef ?? undefined}
							onValueChange={onModelChange}
							onOpenChange={(open) => {
								if (open) setModelQuery("");
							}}
							disabled={isModelLoading}
						>
							<SelectTrigger className="h-8 text-xs w-full">
								<SelectValue
									placeholder={
										isModelLoading ? "Loading models..." : "Select model"
									}
								/>
							</SelectTrigger>
							<SelectContent>
								<div
									className="sticky top-0 z-10 bg-popover p-2 border-b border-border"
									onPointerDown={(e) => e.stopPropagation()}
									onKeyDown={(e) => e.stopPropagation()}
								>
									<Input
										value={modelQuery}
										onChange={(e) => setModelQuery(e.target.value)}
										placeholder="Search models..."
										aria-label="Search models"
										className="h-8 text-xs"
									/>
								</div>
								{modelOptions.length === 0 ? (
									<SelectItem value="__none__" disabled>
										{isModelLoading
											? "Loading..."
											: "Start a session to select models"}
									</SelectItem>
								) : filteredModelOptions.length === 0 ? (
									<SelectItem value="__no_results__" disabled>
										No matches
									</SelectItem>
								) : (
									filteredModelOptions.map((option, index) => {
										const provider = option.value.split("/")[0];
										return (
											<SelectItem
												key={`${option.value}-${index}`}
												value={option.value}
												textValue={option.label}
												className="text-xs"
											>
												<span className="flex items-center gap-2 max-w-[250px]">
													<ProviderIcon
														provider={provider}
														className="w-4 h-4 flex-shrink-0"
													/>
													<span className="truncate">{option.label}</span>
												</span>
											</SelectItem>
										);
									})
								)}
							</SelectContent>
						</Select>
						<p className="text-[10px] text-muted-foreground">
							Changes take effect immediately for this session
						</p>
					</div>
				)}

				{/* Agent Config Section */}
				<div className="border border-border rounded-lg overflow-hidden">
					<div className="flex items-center justify-between px-3 py-2 bg-muted/50 border-b border-border">
						<Label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
							Agent Configuration
						</Label>
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={handleRestart}
							disabled={restarting || waitingForIdle}
							className="h-6 px-2 text-[10px]"
							title={
								waitingForIdle
									? "Waiting for agent to finish..."
									: "Restart session to apply changes"
							}
						>
							{restarting ? (
								<Loader2 className="h-3 w-3 animate-spin" />
							) : waitingForIdle ? (
								<Loader2 className="h-3 w-3 animate-spin" />
							) : (
								<RotateCcw className="h-3 w-3" />
							)}
							<span className="ml-1">
								{restarting
									? "Restarting"
									: waitingForIdle
										? "Waiting..."
										: "Restart"}
							</span>
						</Button>
					</div>

					{/* Model */}
					<SettingField
						label="Model"
						description="Provider/model (e.g., anthropic/claude-sonnet-4-20250514)"
						modified={isModified("model")}
						source={getValueSource("model")}
						setInLocal={isSetInLocal("model")}
						setInGlobal={isSetInGlobal("model")}
						odd
					>
						<Input
							value={getValue("model") || ""}
							onChange={(e) =>
								handleChange("model", e.target.value || undefined)
							}
							placeholder="anthropic/claude-sonnet-4-20250514"
							className={cn(
								"h-8 text-xs bg-background",
								isModified("model") && "border-amber-500",
								getValueSource("model") === "global" && "border-dashed",
							)}
						/>
					</SettingField>

					{/* Default Agent */}
					<SettingField
						label="Default Agent"
						description={`Agent to use for new sessions (${agents.length} available)`}
						modified={isModified("default_agent")}
						source={getValueSource("default_agent")}
						setInLocal={isSetInLocal("default_agent")}
						setInGlobal={isSetInGlobal("default_agent")}
					>
						<Select
							value={getValue("default_agent") || "__none__"}
							onValueChange={(v) =>
								handleChange("default_agent", v === "__none__" ? undefined : v)
							}
						>
							<SelectTrigger
								className={cn(
									"h-8 text-xs bg-background",
									isModified("default_agent") && "border-amber-500",
									getValueSource("default_agent") === "global" &&
										"border-dashed",
								)}
							>
								<SelectValue placeholder="Select agent..." />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="__none__">Default</SelectItem>
								{agents.length === 0 ? (
									<SelectItem value="__no_agents__" disabled>
										No custom agents configured
									</SelectItem>
								) : (
									agents
										.filter((agent) => agent.id && agent.id !== "__none__")
										.map((agent) => (
											<SelectItem key={agent.id} value={agent.id}>
												{agent.name}
											</SelectItem>
										))
								)}
							</SelectContent>
						</Select>
					</SettingField>

					{/* Share Mode */}
					<SettingField
						label="Share Mode"
						description="How to handle session sharing"
						modified={isModified("share")}
						source={getValueSource("share")}
						setInLocal={isSetInLocal("share")}
						setInGlobal={isSetInGlobal("share")}
						odd
					>
						<Select
							value={getValue("share") || "__none__"}
							onValueChange={(v) =>
								handleChange(
									"share",
									v === "__none__" ? undefined : (v as ShareMode),
								)
							}
						>
							<SelectTrigger
								className={cn(
									"h-8 text-xs bg-background",
									isModified("share") && "border-amber-500",
									getValueSource("share") === "global" && "border-dashed",
								)}
							>
								<SelectValue placeholder="Select mode..." />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="__none__">Default</SelectItem>
								<SelectItem value="manual">Manual</SelectItem>
								<SelectItem value="auto">Auto</SelectItem>
								<SelectItem value="disabled">Disabled</SelectItem>
							</SelectContent>
						</Select>
					</SettingField>

					{/* Compaction Settings */}
					<div className="p-3 space-y-2 border-b border-border">
						<Label className="text-xs font-medium">Compaction</Label>
						<div className="space-y-2">
							<div className="flex items-center justify-between">
								<Label
									htmlFor="compaction-auto"
									className="text-xs text-muted-foreground"
								>
									Auto compaction
								</Label>
								<Switch
									id="compaction-auto"
									checked={getValue("compaction")?.auto ?? false}
									onCheckedChange={(checked) =>
										handleChange("compaction", {
											...getValue("compaction"),
											auto: checked,
										})
									}
								/>
							</div>
							<div className="flex items-center justify-between">
								<Label
									htmlFor="compaction-prune"
									className="text-xs text-muted-foreground"
								>
									Prune old messages
								</Label>
								<Switch
									id="compaction-prune"
									checked={getValue("compaction")?.prune ?? false}
									onCheckedChange={(checked) =>
										handleChange("compaction", {
											...getValue("compaction"),
											prune: checked,
										})
									}
								/>
							</div>
						</div>
					</div>

					{/* Instructions */}
					<SettingField
						label="Instructions"
						description="Paths to instruction files (one per line)"
						modified={isModified("instructions")}
						source={getValueSource("instructions")}
						setInLocal={isSetInLocal("instructions")}
						setInGlobal={isSetInGlobal("instructions")}
					>
						<textarea
							value={(getValue("instructions") || []).join("\n")}
							onChange={(e) => {
								const lines = e.target.value
									.split("\n")
									.map((l) => l.trim())
									.filter((l) => l);
								handleChange(
									"instructions",
									lines.length > 0 ? lines : undefined,
								);
							}}
							placeholder="AGENTS.md&#10;.opencode/instructions.md"
							rows={3}
							className={cn(
								"w-full px-3 py-2 text-xs bg-background border rounded-md resize-none",
								"focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
								isModified("instructions") && "border-amber-500",
								getValueSource("instructions") === "global" && "border-dashed",
							)}
						/>
					</SettingField>
				</div>

				{/* Permissions */}
				<PermissionsSection
					permission={getValue("permission")}
					onChange={(p) => handleChange("permission", p)}
					modified={isModified("permission")}
					setInLocal={isSetInLocal("permission")}
					setInGlobal={isSetInGlobal("permission")}
				/>

				{/* Raw JSON view (expandable) */}
				<details className="group">
					<summary className="cursor-pointer text-xs text-muted-foreground hover:text-foreground">
						View raw config (JSON)
					</summary>
					<div className="mt-2 space-y-2">
						<div>
							<p className="text-[10px] text-muted-foreground mb-1">
								Workspace config (editable):
							</p>
							<pre className="p-2 text-[10px] bg-muted/50 border border-border rounded-md overflow-x-auto max-h-32">
								{JSON.stringify({ ...localConfig, ...pendingChanges }, null, 2)}
							</pre>
						</div>
						{globalConfig && Object.keys(globalConfig).length > 0 && (
							<div>
								<p className="text-[10px] text-muted-foreground mb-1">
									Global config (read-only):
								</p>
								<pre className="p-2 text-[10px] bg-muted/50 border border-dashed border-border rounded-md overflow-x-auto max-h-32">
									{JSON.stringify(globalConfig, null, 2)}
								</pre>
							</div>
						)}
					</div>
				</details>
			</div>
		</div>
	);
}

interface SettingFieldProps {
	label: string;
	description?: string;
	/** Has unsaved changes */
	modified?: boolean;
	/** Value source: "pending" | "local" | "global" | "default" */
	source?: "pending" | "local" | "global" | "default";
	/** Explicitly set in local config */
	setInLocal?: boolean;
	/** Explicitly set in global config */
	setInGlobal?: boolean;
	/** Alternating row background */
	odd?: boolean;
	children: React.ReactNode;
}

function SettingField({
	label,
	description,
	modified,
	source = "default",
	setInLocal,
	setInGlobal,
	odd,
	children,
}: SettingFieldProps) {
	return (
		<div
			className={cn(
				"p-3 space-y-1.5 border-b border-border last:border-b-0",
				odd && "bg-muted/30",
			)}
		>
			<div className="flex items-center gap-1.5 flex-wrap">
				<Label className="text-xs font-medium">{label}</Label>
				{modified && (
					<Badge
						variant="default"
						className="text-[9px] px-1 py-0 bg-amber-500 h-4"
					>
						modified
					</Badge>
				)}
				{!modified && setInLocal && (
					<Badge
						variant="default"
						className="text-[9px] px-1 py-0 bg-blue-500 h-4"
					>
						local
					</Badge>
				)}
				{!modified && !setInLocal && setInGlobal && (
					<Badge variant="secondary" className="text-[9px] px-1 py-0 h-4">
						global
					</Badge>
				)}
				{/* Show if also set in global when local is active */}
				{!modified && setInLocal && setInGlobal && (
					<Badge
						variant="outline"
						className="text-[9px] px-1 py-0 h-4 text-muted-foreground"
					>
						overrides global
					</Badge>
				)}
			</div>
			{description && (
				<p className="text-[11px] text-muted-foreground">{description}</p>
			)}
			{children}
		</div>
	);
}

// ============================================================================
// Permissions Section
// ============================================================================

/** Known tools that can have permissions configured */
const KNOWN_TOOLS = [
	{ id: "read", label: "Read", description: "Read files" },
	{ id: "edit", label: "Edit", description: "Edit files" },
	{ id: "bash", label: "Bash", description: "Execute shell commands" },
	{ id: "glob", label: "Glob", description: "Search for files by pattern" },
	{ id: "grep", label: "Grep", description: "Search file contents" },
	{ id: "list", label: "List", description: "List directory contents" },
	{ id: "task", label: "Task", description: "Create sub-agents" },
	{ id: "question", label: "Question", description: "Ask user questions" },
	{ id: "webfetch", label: "Web Fetch", description: "Fetch URLs" },
	{ id: "websearch", label: "Web Search", description: "Search the web" },
	{ id: "codesearch", label: "Code Search", description: "Search code" },
	{ id: "todowrite", label: "Todo Write", description: "Write todos" },
	{ id: "todoread", label: "Todo Read", description: "Read todos" },
	{ id: "lsp", label: "LSP", description: "Language server features" },
	{
		id: "external_directory",
		label: "External Dir",
		description: "Access external directories",
	},
] as const;

interface PermissionsSectionProps {
	permission: PermissionConfig | undefined;
	onChange: (p: PermissionConfig | undefined) => void;
	modified: boolean;
	setInLocal?: boolean;
	setInGlobal?: boolean;
}

function PermissionsSection({
	permission,
	onChange,
	modified,
	setInLocal,
	setInGlobal,
}: PermissionsSectionProps) {
	// Determine if we have a global permission or per-tool permissions
	const isGlobalPermission = typeof permission === "string";
	const globalValue: PermissionAction | "__none__" = isGlobalPermission
		? permission
		: "__none__";
	const toolPermissions: Record<string, PermissionAction> =
		permission && typeof permission === "object"
			? Object.fromEntries(
					Object.entries(permission).map(([k, v]) => [
						k,
						typeof v === "string" ? v : "ask",
					]),
				)
			: {};

	// Handle global permission change
	const handleGlobalChange = (value: string) => {
		if (value === "__none__") {
			// Clear global, keep any tool-specific permissions or clear all
			onChange(undefined);
		} else {
			// Set global permission (removes all tool-specific)
			onChange(value as PermissionAction);
		}
	};

	// Handle individual tool permission change
	const handleToolChange = (toolId: string, value: string) => {
		// Start with existing object permissions or empty object
		const currentObj: Record<string, PermissionAction> =
			permission && typeof permission === "object"
				? { ...toolPermissions }
				: {};

		if (value === "__none__") {
			// Remove this tool's permission
			delete currentObj[toolId];
			// If no permissions left, clear the whole thing
			if (Object.keys(currentObj).length === 0) {
				onChange(undefined);
			} else {
				onChange(currentObj);
			}
		} else {
			// Set or update this tool's permission
			currentObj[toolId] = value as PermissionAction;
			onChange(currentObj);
		}
	};

	// Get the effective permission for a tool
	const getToolPermission = (toolId: string): PermissionAction | "__none__" => {
		if (isGlobalPermission) {
			// Global permission applies to all tools
			return permission;
		}
		return toolPermissions[toolId] || "__none__";
	};

	return (
		<div className="space-y-3 p-3 bg-muted/30 border border-border/50 rounded-md">
			<div className="flex items-center gap-1.5 flex-wrap">
				<Label className="text-xs font-medium">Permissions</Label>
				{modified && (
					<Badge
						variant="default"
						className="text-[9px] px-1 py-0 bg-amber-500 h-4"
					>
						modified
					</Badge>
				)}
				{!modified && setInLocal && (
					<Badge
						variant="default"
						className="text-[9px] px-1 py-0 bg-blue-500 h-4"
					>
						local
					</Badge>
				)}
				{!modified && !setInLocal && setInGlobal && (
					<Badge variant="secondary" className="text-[9px] px-1 py-0 h-4">
						global
					</Badge>
				)}
				{!modified && setInLocal && setInGlobal && (
					<Badge
						variant="outline"
						className="text-[9px] px-1 py-0 h-4 text-muted-foreground"
					>
						overrides global
					</Badge>
				)}
			</div>
			<p className="text-[11px] text-muted-foreground">
				Control which tools require confirmation before use
			</p>

			{/* Global permission override */}
			<div className="flex items-center justify-between py-1.5 border-b border-border/50">
				<div className="flex flex-col">
					<span className="text-xs font-medium">All Tools</span>
					<span className="text-[10px] text-muted-foreground">
						Set permission for all tools
					</span>
				</div>
				<Select value={globalValue} onValueChange={handleGlobalChange}>
					<SelectTrigger className="h-7 w-24 text-xs bg-background">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						<SelectItem value="__none__">Per-tool</SelectItem>
						<SelectItem value="allow">Allow</SelectItem>
						<SelectItem value="ask">Ask</SelectItem>
						<SelectItem value="deny">Deny</SelectItem>
					</SelectContent>
				</Select>
			</div>

			{/* Per-tool permissions (only shown when not using global) */}
			{!isGlobalPermission && (
				<div className="space-y-1 max-h-64 overflow-y-auto">
					{KNOWN_TOOLS.map((tool) => {
						const value = getToolPermission(tool.id);
						return (
							<div
								key={tool.id}
								className="flex items-center justify-between py-1 px-1 rounded hover:bg-muted/50"
							>
								<div className="flex flex-col min-w-0 flex-1 mr-2">
									<span className="text-xs">{tool.label}</span>
									<span className="text-[10px] text-muted-foreground truncate">
										{tool.description}
									</span>
								</div>
								<Select
									value={value}
									onValueChange={(v) => handleToolChange(tool.id, v)}
								>
									<SelectTrigger className="h-6 w-20 text-[10px] bg-background">
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										<SelectItem value="__none__">Default</SelectItem>
										<SelectItem value="allow">Allow</SelectItem>
										<SelectItem value="ask">Ask</SelectItem>
										<SelectItem value="deny">Deny</SelectItem>
									</SelectContent>
								</Select>
							</div>
						);
					})}
				</div>
			)}
		</div>
	);
}
