"use client";

import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	type MainChatAssistantInfo,
	type PiSessionFile,
	createMainChatAssistant,
	deleteMainChatAssistant,
	getMainChatAssistant,
	listMainChatAssistants,
	listMainChatPiSessions,
	updateMainChatAssistant,
} from "@/features/main-chat/api";
import { formatSessionDate, generateReadableId } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	ChevronDown,
	ChevronRight,
	Copy,
	Loader2,
	MessageCircle,
	MessageSquare,
	Plus,
	Settings,
	Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

export interface MainChatEntryProps {
	/** Whether this entry is currently selected */
	isSelected: boolean;
	/** Currently active session ID (for timeline highlighting) */
	activeSessionId?: string | null;
	/** Callback when the entry is clicked */
	onSelect: (assistantName: string, sessionId: string | null) => void;
	/** Callback when a specific session in the timeline is clicked */
	onSessionSelect?: (assistantName: string, sessionId: string) => void;
	/** Callback when the + button is clicked to create a new session */
	onNewSession?: (assistantName: string) => void;
	/** Locale for i18n */
	locale?: "en" | "de";
}

/**
 * Main Chat entry component for the sidebar.
 * Shows a pinned entry for the user's main chat assistant.
 * If no assistant exists, shows a setup prompt.
 */
export function MainChatEntry({
	isSelected,
	activeSessionId,
	onSelect,
	onSessionSelect,
	onNewSession,
	locale = "en",
}: MainChatEntryProps) {
	const [assistantName, setAssistantName] = useState<string | null>(null);
	const [assistantInfo, setAssistantInfo] =
		useState<MainChatAssistantInfo | null>(null);
	const [sessions, setSessions] = useState<PiSessionFile[]>([]);
	const [latestSessionId, setLatestSessionId] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);
	const [expanded, setExpanded] = useState(false);

	// Auto-expand when Main Chat is selected so sessions are visible.
	useEffect(() => {
		if (isSelected && sessions.length > 0) {
			setExpanded(true);
		}
	}, [isSelected, sessions.length]);
	const [showCreateDialog, setShowCreateDialog] = useState(false);
	const [newName, setNewName] = useState("");
	const [creating, setCreating] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [showResetDialog, setShowResetDialog] = useState(false);
	const [resetName, setResetName] = useState("");
	const [resetting, setResetting] = useState(false);
	const [resetError, setResetError] = useState<string | null>(null);
	const resetNameIsValid = useMemo(() => {
		return Boolean(resetName.trim().match(/^[A-Za-z0-9_-]+$/));
	}, [resetName]);

	// Load assistant on mount
	useEffect(() => {
		loadAssistant();
	}, []);

	// When selection changes (e.g. /new), refresh sessions list.
	useEffect(() => {
		if (!assistantName) return;
		// Only do this while visible/selected to keep it cheap.
		if (!isSelected) return;
		listMainChatPiSessions()
			.then((sessionList) => {
				const sorted = [...sessionList].sort(
					(a, b) => b.modified_at - a.modified_at,
				);
				setSessions(sorted);
				setLatestSessionId(sorted[0]?.id ?? null);
				writeCachedSessions(assistantName, sorted);
			})
			.catch(() => {
				// ignore
			});
	}, [assistantName, isSelected]);

	function cacheKeySessions(name: string) {
		return `octo:mainChatPi:${name}:sessions:v1`;
	}

	function readCachedSessions(name: string): PiSessionFile[] {
		if (typeof window === "undefined") return [];
		try {
			const raw = localStorage.getItem(cacheKeySessions(name));
			if (!raw) return [];
			const parsed = JSON.parse(raw) as PiSessionFile[];
			return Array.isArray(parsed) ? parsed : [];
		} catch {
			return [];
		}
	}

	function writeCachedSessions(name: string, sessions: PiSessionFile[]) {
		if (typeof window === "undefined") return;
		try {
			localStorage.setItem(cacheKeySessions(name), JSON.stringify(sessions));
		} catch {
			// ignore
		}
	}

	async function loadAssistant() {
		try {
			setLoading(true);
			const assistants = await listMainChatAssistants();

			if (assistants.length > 0) {
				// Use the first assistant (users typically have one)
				const name = assistants[0];
				setAssistantName(name);

				// Use cached sessions instantly, then refresh in background
				const cached = readCachedSessions(name);
				if (cached.length > 0) {
					setSessions(cached);
					setLatestSessionId(cached[0]?.id ?? null);
				}

				const [info, sessionList] = await Promise.all([
					getMainChatAssistant(name),
					listMainChatPiSessions(),
				]);

			setAssistantInfo(info);
			// Sort sessions by most recently active first
			const sorted = [...sessionList].sort(
				(a, b) => b.modified_at - a.modified_at,
			);
			setSessions(sorted);
				setLatestSessionId(sorted[0]?.id ?? null);
				writeCachedSessions(name, sorted);
			}
		} catch (err) {
			console.error("Failed to load main chat assistant:", err);
		} finally {
			setLoading(false);
		}
	}

	async function handleCreate() {
		if (!newName.trim()) return;

		try {
			setCreating(true);
			setError(null);
			const info = assistantName
				? await updateMainChatAssistant(newName.trim())
				: await createMainChatAssistant(newName.trim());
			setAssistantName(info.name);
			setAssistantInfo(info);
			setShowCreateDialog(false);
			setNewName("");
		} catch (err) {
			console.error("Failed to create assistant:", err);
			const message = err instanceof Error ? err.message : "Failed to create";
			if (message.includes("already exists")) {
				await loadAssistant();
				setShowCreateDialog(false);
				setNewName("");
				return;
			}
			setError(message);
		} finally {
			setCreating(false);
		}
	}

	async function handleReset() {
		if (!resetName.trim()) return;
		if (!resetNameIsValid) {
			setResetError(
				locale === "de"
					? "Nur Buchstaben, Zahlen, Bindestriche und Unterstriche sind erlaubt."
					: "Name can only contain letters, numbers, hyphens, and underscores.",
			);
			return;
		}

		try {
			setResetting(true);
			setResetError(null);
			await deleteMainChatAssistant(resetName.trim());
			const info = await createMainChatAssistant(resetName.trim());
			setAssistantName(info.name);
			setAssistantInfo(info);
			setSessions([]);
			setLatestSessionId(null);
			setShowResetDialog(false);
			setResetName("");
			await loadAssistant();
		} catch (err) {
			console.error("Failed to reset main chat:", err);
			const message = err instanceof Error ? err.message : "Failed to reset";
			setResetError(message);
		} finally {
			setResetting(false);
		}
	}

	const handleClick = useCallback(() => {
		if (assistantName) {
			onSelect(assistantName, latestSessionId);
		} else {
			setShowCreateDialog(true);
		}
	}, [assistantName, latestSessionId, onSelect]);

	const handleTimelineSessionClick = useCallback(
		(sessionId: string) => {
			if (assistantName && onSessionSelect) {
				onSessionSelect(assistantName, sessionId);
			}
		},
		[assistantName, onSessionSelect],
	);

	const toggleExpanded = useCallback((e: React.MouseEvent) => {
		e.stopPropagation();
		setExpanded((prev) => !prev);
	}, []);

	const handleNewSessionClick = useCallback(
		(e: React.MouseEvent) => {
			e.stopPropagation();
			if (assistantName && onNewSession) {
				onNewSession(assistantName);
			}
		},
		[assistantName, onNewSession],
	);

	// Loading state - show placeholder
	if (loading) {
		return (
			<div className="px-2 py-2 flex items-center gap-2 text-muted-foreground/50">
				<Loader2 className="w-4 h-4 animate-spin" />
				<span className="text-sm">Loading...</span>
			</div>
		);
	}

	// No assistant yet - show setup prompt
	if (!assistantName) {
		return (
			<>
				<button
					type="button"
					onClick={() => setShowCreateDialog(true)}
					className={cn(
						"w-full px-2 py-2 text-left transition-colors flex items-center gap-2",
						"text-muted-foreground hover:bg-sidebar-accent border border-dashed border-muted-foreground/30 rounded-md",
					)}
				>
					<Plus className="w-4 h-4" />
					<span className="text-sm">
						{locale === "de" ? "Hauptchat einrichten" : "Set up Main Chat"}
					</span>
				</button>

				<CreateAssistantDialog
					open={showCreateDialog}
					onOpenChange={setShowCreateDialog}
					name={newName}
					onNameChange={setNewName}
					onSubmit={handleCreate}
					loading={creating}
					error={error}
					locale={locale}
					isRename={false}
				/>
			</>
		);
	}

	// Assistant exists - show entry styled like workspace project entries
	const hasSessions = sessions.length > 0;

	return (
		<>
			<div className="border-b border-sidebar-border/50">
				<ContextMenu>
					<ContextMenuTrigger className="contents">
						{/* Main Chat header - styled like workspace project headers */}
						<div className="flex items-center gap-1 px-1 py-1.5 group">
							<button
								type="button"
								onClick={hasSessions ? toggleExpanded : handleClick}
								className="flex-1 flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1"
							>
								{hasSessions ? (
									expanded ? (
										<ChevronDown className="w-3 h-3 text-muted-foreground flex-shrink-0" />
									) : (
										<ChevronRight className="w-3 h-3 text-muted-foreground flex-shrink-0" />
									)
								) : (
									<ChevronRight className="w-3 h-3 text-muted-foreground/30 flex-shrink-0" />
								)}
								<MessageCircle className="w-3.5 h-3.5 text-primary/70 flex-shrink-0" />
								<span className="text-xs font-medium text-foreground truncate">
									{assistantName}
								</span>
								<span className="text-[10px] text-muted-foreground">
									({sessions.length})
								</span>
							</button>
							{onNewSession && (
								<button
									type="button"
									onClick={handleNewSessionClick}
									className="p-1 text-muted-foreground hover:text-primary hover:bg-sidebar-accent opacity-0 group-hover:opacity-100 transition-opacity"
									title={
										locale === "de"
											? "Neue Sitzung"
											: "New session"
									}
								>
									<Plus className="w-3 h-3" />
								</button>
							)}
						</div>
					</ContextMenuTrigger>
					<ContextMenuContent>
						{onNewSession && (
							<>
								<ContextMenuItem
									onClick={() => {
										if (assistantName) {
											onNewSession(assistantName);
										}
									}}
								>
									<Plus className="w-4 h-4 mr-2" />
									{locale === "de" ? "Neue Sitzung" : "New Session"}
								</ContextMenuItem>
								<div className="h-px bg-border my-1" />
							</>
						)}
						<ContextMenuItem
							onClick={() => {
								setNewName(assistantName ?? "");
								setShowCreateDialog(true);
							}}
						>
							<Settings className="w-4 h-4 mr-2" />
							{locale === "de" ? "Einstellungen" : "Settings"}
						</ContextMenuItem>
						<ContextMenuItem
							onClick={() => {
								setResetName(assistantName ?? "");
								setShowResetDialog(true);
							}}
						>
							<Trash2 className="w-4 h-4 mr-2" />
							{locale === "de" ? "Zurucksetzen" : "Reset"}
						</ContextMenuItem>
					</ContextMenuContent>
				</ContextMenu>

				{/* Session history list - shown when expanded */}
				{expanded && hasSessions && (
					<div className="space-y-0.5 pb-1">
						{sessions.map((session) => {
							const isActive =
								session.id === (activeSessionId ?? latestSessionId);
							const readableId = generateReadableId(session.id);
							const formattedDate = formatSessionDate(
								new Date(session.started_at).getTime(),
							);

							return (
								<ContextMenu key={session.id}>
									<ContextMenuTrigger className="contents">
										<div className="ml-3">
											<button
												type="button"
												onClick={() => handleTimelineSessionClick(session.id)}
												className={cn(
													"w-full px-2 py-1.5 text-left transition-colors flex items-center gap-1.5 rounded-sm",
													isActive
														? "bg-primary/15 text-foreground"
														: "text-muted-foreground hover:bg-sidebar-accent",
												)}
											>
												<MessageSquare className="w-3.5 h-3.5 text-primary/70 flex-shrink-0" />
												<div className="flex-1 min-w-0">
													<div className="text-xs font-medium truncate">
														{session.title || "Untitled"}
													</div>
													<div className="text-[10px] text-muted-foreground/50">
														{formattedDate}
													</div>
												</div>
											</button>
										</div>
									</ContextMenuTrigger>
									<ContextMenuContent>
										<ContextMenuItem
											onClick={() => navigator.clipboard.writeText(readableId)}
										>
											<Copy className="w-4 h-4 mr-2" />
											{readableId}
										</ContextMenuItem>
									</ContextMenuContent>
								</ContextMenu>
							);
						})}
					</div>
				)}
			</div>

			<CreateAssistantDialog
				open={showCreateDialog}
				onOpenChange={setShowCreateDialog}
				name={newName}
				onNameChange={setNewName}
				onSubmit={handleCreate}
				loading={creating}
				error={error}
				locale={locale}
				isRename={Boolean(assistantName)}
			/>
			<ResetAssistantDialog
				open={showResetDialog}
				onOpenChange={setShowResetDialog}
				name={resetName}
				nameIsValid={resetNameIsValid}
				onNameChange={setResetName}
				onSubmit={handleReset}
				loading={resetting}
				error={resetError}
				locale={locale}
			/>
		</>
	);
}

/**
 * Vertical timeline showing sessions as connected dots.
 *
 * Legacy: replaced by OpenCode-style list above.
 */
function SessionTimeline({
	sessions,
	activeSessionId,
	onSessionClick,
	locale,
}: {
	sessions: PiSessionFile[];
	activeSessionId: string | null;
	onSessionClick: (sessionId: string) => void;
	locale: "en" | "de";
}) {
	return (
		<div className="relative">
			{/* Session items */}
			<div className="flex flex-col gap-0.5">
				{sessions.map((session) => {
					const isActive = session.id === activeSessionId;
					const formattedDate = formatSessionDate(
						new Date(session.started_at).getTime(),
					);

						const readableId = generateReadableId(session.id);

						return (
							<button
							key={session.id}
							type="button"
							onClick={() => onSessionClick(session.id)}
							className={cn(
								"relative flex items-center gap-2 py-1 text-left group",
								"hover:bg-muted/50 rounded-sm transition-colors",
							)}
						>
							{/* Dot */}
							<div
								className={cn(
									"relative z-10 w-2 h-2 flex-shrink-0 transition-all",
									isActive
										? "bg-primary"
										: "bg-muted-foreground/40 group-hover:bg-muted-foreground/60",
								)}
							/>

							{/* Label */}
							<div className="flex-1 min-w-0">
								<span
									className={cn(
										"text-xs truncate block",
										isActive
											? "text-foreground font-medium"
											: "text-muted-foreground",
									)}
								>
									{session.title || formattedDate}
									<span className="ml-1 opacity-60">[{readableId}]</span>
								</span>
							</div>
						</button>
					);
				})}
			</div>
		</div>
	);
}

function CreateAssistantDialog({
	open,
	onOpenChange,
	name,
	onNameChange,
	onSubmit,
	loading,
	error,
	locale,
	isRename,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	name: string;
	onNameChange: (name: string) => void;
	onSubmit: () => void;
	loading: boolean;
	error: string | null;
	locale: "en" | "de";
	isRename: boolean;
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>
						{locale === "de"
							? "Benennen Sie Ihren Assistenten"
							: "Name Your Assistant"}
					</DialogTitle>
					<DialogDescription>
						{isRename
							? locale === "de"
								? "Aktualisieren Sie den Namen Ihres Assistenten."
								: "Update your assistant name."
							: locale === "de"
								? "Geben Sie Ihrem KI-Assistenten einen Namen. Dieser wird verwendet, um Ihren persistenten Chat zu identifizieren."
								: "Give your AI assistant a name. This will be used to identify your persistent chat across sessions."}
					</DialogDescription>
				</DialogHeader>

				<div className="grid gap-4 py-4">
					<div className="grid gap-2">
						<Label htmlFor="assistant-name">
							{locale === "de" ? "Assistentenname" : "Assistant Name"}
						</Label>
						<Input
							id="assistant-name"
							placeholder={
								locale === "de"
									? "z.B. jarvis, govnr, friday"
									: "e.g., jarvis, govnr, friday"
							}
							value={name}
							onChange={(e) => onNameChange(e.target.value)}
							onKeyDown={(e) => {
								if (e.key === "Enter" && !loading) {
									onSubmit();
								}
							}}
							disabled={loading}
						/>
						<p className="text-xs text-muted-foreground">
							{locale === "de"
								? "Verwenden Sie Kleinbuchstaben, Zahlen, Bindestriche oder Unterstriche."
								: "Use lowercase letters, numbers, hyphens, or underscores."}
						</p>
					</div>

					{error && <p className="text-sm text-destructive">{error}</p>}
				</div>

				<DialogFooter>
					<Button
						variant="outline"
						onClick={() => onOpenChange(false)}
						disabled={loading}
					>
						{locale === "de" ? "Abbrechen" : "Cancel"}
					</Button>
					<Button onClick={onSubmit} disabled={loading || !name.trim()}>
						{loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
						{isRename
							? locale === "de"
								? "Speichern"
								: "Save"
							: locale === "de"
								? "Erstellen"
								: "Create"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

function ResetAssistantDialog({
	open,
	onOpenChange,
	name,
	nameIsValid,
	onNameChange,
	onSubmit,
	loading,
	error,
	locale,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	name: string;
	nameIsValid: boolean;
	onNameChange: (name: string) => void;
	onSubmit: () => void;
	loading: boolean;
	error: string | null;
	locale: "en" | "de";
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>
						{locale === "de" ? "Hauptchat zurucksetzen" : "Reset Main Chat"}
					</DialogTitle>
					<DialogDescription>
						{locale === "de"
							? "Dies loscht alle Main-Chat-Daten und startet frisch. Geben Sie einen neuen Namen ein."
							: "This deletes all Main Chat data and starts fresh. Enter a new name."}
					</DialogDescription>
				</DialogHeader>

				<div className="grid gap-4 py-4">
					<div className="grid gap-2">
						<Label htmlFor="main-chat-reset-name">
							{locale === "de" ? "Neuer Name" : "New Name"}
						</Label>
						<Input
							id="main-chat-reset-name"
							value={name}
							onChange={(e) => onNameChange(e.target.value)}
							placeholder={locale === "de" ? "Name" : "Name"}
						/>
					</div>
					<p className="text-xs text-muted-foreground">
						{locale === "de"
							? "Erlaubt: a-z, A-Z, 0-9, -, _"
							: "Allowed: a-z, A-Z, 0-9, -, _"}
					</p>
					{error && <div className="text-sm text-destructive">{error}</div>}
				</div>

				<DialogFooter>
					<Button variant="ghost" onClick={() => onOpenChange(false)}>
						{locale === "de" ? "Abbrechen" : "Cancel"}
					</Button>
					<Button
						variant="destructive"
						onClick={onSubmit}
						disabled={loading || !name.trim() || !nameIsValid}
					>
						{loading
							? locale === "de"
								? "Zurucksetzen..."
								: "Resetting..."
							: locale === "de"
								? "Zurucksetzen"
								: "Reset"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
