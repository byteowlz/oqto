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
	type MainChatSession,
	createMainChatAssistant,
	deleteMainChatAssistant,
	getLatestMainChatSession,
	getMainChatAssistant,
	listMainChatAssistants,
	listMainChatSessions,
	updateMainChatAssistant,
} from "@/lib/control-plane-client";
import { formatSessionDate } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	ChevronDown,
	ChevronRight,
	Loader2,
	MessageCircle,
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
	locale = "en",
}: MainChatEntryProps) {
	const [assistantName, setAssistantName] = useState<string | null>(null);
	const [assistantInfo, setAssistantInfo] =
		useState<MainChatAssistantInfo | null>(null);
	const [sessions, setSessions] = useState<MainChatSession[]>([]);
	const [latestSessionId, setLatestSessionId] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);
	const [expanded, setExpanded] = useState(false);
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

	async function loadAssistant() {
		try {
			setLoading(true);
			const assistants = await listMainChatAssistants();

			if (assistants.length > 0) {
				// Use the first assistant (users typically have one)
				const name = assistants[0];
				setAssistantName(name);

				// Load info, sessions, and latest session
				const [info, sessionList, latestSession] = await Promise.all([
					getMainChatAssistant(name),
					listMainChatSessions(name),
					getLatestMainChatSession(name),
				]);

				setAssistantInfo(info);
				// Sort sessions newest first for the timeline
				setSessions(
					sessionList.sort(
						(a, b) =>
							new Date(b.started_at).getTime() -
							new Date(a.started_at).getTime(),
					),
				);
				setLatestSessionId(latestSession?.session_id ?? null);
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

	// Assistant exists - show entry with timeline
	const hasSessions = sessions.length > 0;

	return (
		<>
			<ContextMenu>
				<ContextMenuTrigger asChild>
					<div>
						<div
							className={cn(
								"w-full px-2 py-2 text-left transition-colors flex items-start gap-1.5 rounded-md",
								isSelected
									? "bg-primary/15 border border-primary text-foreground"
									: "text-muted-foreground hover:bg-sidebar-accent border border-transparent",
							)}
						>
							{hasSessions ? (
								<button
									type="button"
									onClick={toggleExpanded}
									className="mt-0.5 p-0.5 hover:bg-muted rounded flex-shrink-0 cursor-pointer"
								>
									{expanded ? (
										<ChevronDown className="w-3 h-3" />
									) : (
										<ChevronRight className="w-3 h-3" />
									)}
								</button>
							) : (
								<MessageCircle className="w-4 h-4 mt-0.5 flex-shrink-0 text-primary" />
							)}
							<button
								type="button"
								onClick={handleClick}
								className="flex-1 min-w-0 text-left"
							>
								<div className="flex items-center gap-1">
									<span className="text-sm truncate font-medium">
										{assistantName}
									</span>
								</div>
								<div className="text-xs text-muted-foreground/50 mt-0.5">
									{locale === "de" ? "Hauptchat" : "Main Chat"}
									{sessions.length > 0 && (
										<span className="opacity-60">
											{" "}
											{sessions.length}{" "}
											{locale === "de" ? "Sitzungen" : "sessions"}
										</span>
									)}
								</div>
							</button>
						</div>

						{/* Timeline - shown when expanded */}
						{expanded && hasSessions && (
							<div className="ml-4 mt-1 mb-2">
								<SessionTimeline
									sessions={sessions}
									activeSessionId={activeSessionId ?? latestSessionId}
									onSessionClick={handleTimelineSessionClick}
									locale={locale}
								/>
							</div>
						)}
					</div>
				</ContextMenuTrigger>
				<ContextMenuContent>
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
 */
function SessionTimeline({
	sessions,
	activeSessionId,
	onSessionClick,
	locale,
}: {
	sessions: MainChatSession[];
	activeSessionId: string | null;
	onSessionClick: (sessionId: string) => void;
	locale: "en" | "de";
}) {
	return (
		<div className="relative pl-2">
			{/* Vertical line */}
			<div className="absolute left-[5px] top-1 bottom-1 w-0.5 bg-border" />

			{/* Session dots */}
			<div className="flex flex-col gap-0.5">
				{sessions.map((session) => {
					const isActive = session.session_id === activeSessionId;
					const formattedDate = formatSessionDate(
						new Date(session.started_at).getTime(),
					);

					return (
						<button
							key={session.session_id}
							type="button"
							onClick={() => onSessionClick(session.session_id)}
							className={cn(
								"relative flex items-center gap-2 py-1 text-left group",
								"hover:bg-muted/50 rounded-sm transition-colors",
							)}
						>
							{/* Dot */}
							<div
								className={cn(
									"relative z-10 w-2.5 h-2.5 rounded-full flex-shrink-0 transition-all",
									"border border-background",
									isActive
										? "bg-primary scale-110 ring-2 ring-primary/30"
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
