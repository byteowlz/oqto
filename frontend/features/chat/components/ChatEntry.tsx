"use client";

import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
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
	type DefaultChatAssistantInfo,
	type PiSessionFile,
	createDefaultChatAssistant,
	deleteDefaultChatAssistant,
	deleteDefaultChatPiSession,
	getDefaultChatAssistant,
	listDefaultChatAssistants,
	listDefaultChatPiSessions,
	renamePiSession,
	updateDefaultChatAssistant,
} from "@/features/chat/api";
import {
	formatSessionDate,
	formatTempId,
	getDisplayPiTitle,
	getTempIdFromSession,
} from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	ChevronDown,
	ChevronRight,
	Copy,
	CornerDownRight,
	Loader2,
	MessageCircle,
	MessageSquare,
	Pencil,
	Plus,
	Settings,
	Trash2,
	X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export interface DefaultChatEntryProps {
	/** Whether this entry is currently selected */
	isSelected: boolean;
	/** Currently active session ID (for timeline highlighting) */
	activeSessionId?: string | null;
	/** Trigger value that changes when a new session is requested */
	newSessionTrigger?: number;
	/** Trigger value that changes when session has activity (message sent) */
	sessionActivityTrigger?: number;
	/** Callback when the entry is clicked */
	onSelect: (assistantName: string, sessionId: string | null) => void;
	/** Callback when a specific session in the timeline is clicked */
	onSessionSelect?: (assistantName: string, sessionId: string) => void;
	/** Callback when the + button is clicked to create a new session */
	onNewSession?: (assistantName: string) => void;
	/** Locale for i18n */
	locale?: "en" | "de";
	/** Optional filter query from sidebar search */
	filterQuery?: string;
	/** Optional callback when filtered session count changes */
	onFilterCountChange?: (count: number) => void;
	/** Optional callback when total session count changes */
	onTotalCountChange?: (count: number) => void;
}

/**
 * Default Chat entry component for the sidebar.
 * Shows a pinned entry for the user's default chat assistant.
 * If no assistant exists, shows a setup prompt.
 */
export function ChatEntry({
	isSelected,
	activeSessionId,
	newSessionTrigger,
	sessionActivityTrigger,
	onSelect,
	onSessionSelect,
	onNewSession,
	locale = "en",
	filterQuery,
	onFilterCountChange,
	onTotalCountChange,
}: DefaultChatEntryProps) {
	const [assistantName, setAssistantName] = useState<string | null>(null);
	const [assistantInfo, setAssistantInfo] =
		useState<DefaultChatAssistantInfo | null>(null);
	const [sessions, setSessions] = useState<PiSessionFile[]>([]);
	const [latestSessionId, setLatestSessionId] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);
	const [expanded, setExpanded] = useState(false);
	const filterLower = filterQuery?.toLowerCase().trim() ?? "";
	const filteredSessions = useMemo(() => {
		if (!filterLower) return sessions;
		return sessions.filter((session) => {
			if ((session.title ?? "").toLowerCase().includes(filterLower)) {
				return true;
			}

			const tempId = getTempIdFromSession(session);
			if (tempId?.toLowerCase().includes(filterLower)) return true;

			const dateStr = formatSessionDate(session.modified_at);
			if (dateStr.toLowerCase().includes(filterLower)) return true;
			return false;
		});
	}, [filterLower, sessions]);
	const visibleSessions = filterLower ? filteredSessions : sessions;
	const hasVisibleSessions = visibleSessions.length > 0;
	const flattenedSessions = useMemo(() => {
		if (filterLower) {
			return visibleSessions.map((session) => ({ session, depth: 0 }));
		}

		const childrenByParent = new Map<string, PiSessionFile[]>();
		for (const session of visibleSessions) {
			if (!session.parent_id) continue;
			const bucket = childrenByParent.get(session.parent_id) ?? [];
			bucket.push(session);
			childrenByParent.set(session.parent_id, bucket);
		}

		const result: Array<{ session: PiSessionFile; depth: number }> = [];
		const added = new Set<string>();
		for (const session of visibleSessions) {
			if (session.parent_id) continue;
			result.push({ session, depth: 0 });
			added.add(session.id);
			const children = childrenByParent.get(session.id) ?? [];
			for (const child of children) {
				result.push({ session: child, depth: 1 });
				added.add(child.id);
			}
		}

		for (const session of visibleSessions) {
			if (added.has(session.id)) continue;
			result.push({ session, depth: 0 });
		}

		return result;
	}, [filterLower, visibleSessions]);

	// Auto-expand when Default Chat is selected so sessions are visible.
	useEffect(() => {
		if (isSelected && sessions.length > 0) {
			setExpanded(true);
		}
	}, [isSelected, sessions.length]);
	useEffect(() => {
		setSelectedSessionIds((prev) => {
			if (prev.size === 0) return prev;
			const valid = new Set(sessions.map((s) => s.id));
			const next = new Set<string>();
			for (const id of prev) {
				if (valid.has(id)) next.add(id);
			}
			return next;
		});
		lastSelectedIndexRef.current = null;
	}, [sessions]);
	useEffect(() => {
		if (!filterLower) return;
		setExpanded(filteredSessions.length > 0);
	}, [filterLower, filteredSessions.length]);
	useEffect(() => {
		if (!onFilterCountChange && !onTotalCountChange) return;
		const totalCount = sessions.length;
		const filteredCount = filterLower ? filteredSessions.length : totalCount;
		onFilterCountChange?.(filteredCount);
		onTotalCountChange?.(totalCount);
	}, [
		filterLower,
		filteredSessions.length,
		onFilterCountChange,
		onTotalCountChange,
		sessions.length,
	]);
	const [showCreateDialog, setShowCreateDialog] = useState(false);
	const [newName, setNewName] = useState("");
	const [creating, setCreating] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [showResetDialog, setShowResetDialog] = useState(false);
	const [resetName, setResetName] = useState("");
	const [resetting, setResetting] = useState(false);
	const [resetError, setResetError] = useState<string | null>(null);
	const [selectedSessionIds, setSelectedSessionIds] = useState<Set<string>>(
		() => new Set(),
	);
	const lastSelectedIndexRef = useRef<number | null>(null);
	const [showBulkDeleteDialog, setShowBulkDeleteDialog] = useState(false);
	const [bulkDeleteError, setBulkDeleteError] = useState<string | null>(null);
	const [bulkDeleting, setBulkDeleting] = useState(false);
	const resetNameIsValid = useMemo(() => {
		return Boolean(resetName.trim().match(/^[A-Za-z0-9_-]+$/));
	}, [resetName]);
	// Rename session state
	const [showRenameDialog, setShowRenameDialog] = useState(false);
	const [renameSessionId, setRenameSessionId] = useState<string | null>(null);
	const [renameTitle, setRenameTitle] = useState("");
	const [renaming, setRenaming] = useState(false);
	const [renameError, setRenameError] = useState<string | null>(null);
	const [showDeleteDialog, setShowDeleteDialog] = useState(false);
	const [deleteSessionId, setDeleteSessionId] = useState<string | null>(null);
	const [deleteTitle, setDeleteTitle] = useState("");
	const [deleting, setDeleting] = useState(false);
	const [deleteError, setDeleteError] = useState<string | null>(null);
	const lastNewSessionTriggerRef = useRef(newSessionTrigger);
	const lastActiveSessionIdRef = useRef(activeSessionId);
	const lastSessionActivityTriggerRef = useRef(sessionActivityTrigger);
	// Throttle refresh to avoid excessive API calls
	const lastRefreshTimeRef = useRef(0);
	const pendingRefreshRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const REFRESH_THROTTLE_MS = 5000; // At most one refresh every 5 seconds

	// Unconditional refresh - always fetches sessions regardless of selection state
	// Now with throttling to avoid excessive refreshes
	const refreshSessionsUnconditional = useCallback(
		(force = false) => {
			if (!assistantName) return;

			const now = Date.now();
			const elapsed = now - lastRefreshTimeRef.current;

			// Clear any pending refresh
			if (pendingRefreshRef.current) {
				clearTimeout(pendingRefreshRef.current);
				pendingRefreshRef.current = null;
			}

			const doRefresh = () => {
				lastRefreshTimeRef.current = Date.now();
				listDefaultChatPiSessions()
					.then((sessionList) => {
						const sorted = [...sessionList].sort(
							(a, b) => b.modified_at - a.modified_at,
						);
						// Only update state if data actually changed
						setSessions((prev) => {
							if (prev.length !== sorted.length) {
								writeCachedSessions(assistantName, sorted);
								return sorted;
							}
							// Check if any session changed
							let changed = false;
							for (let i = 0; i < prev.length; i++) {
								if (
									prev[i].id !== sorted[i].id ||
									prev[i].modified_at !== sorted[i].modified_at ||
									prev[i].title !== sorted[i].title
								) {
									changed = true;
									break;
								}
							}
							if (changed) {
								writeCachedSessions(assistantName, sorted);
								return sorted;
							}
							return prev;
						});
						setLatestSessionId((prev) => {
							const newLatest = sorted[0]?.id ?? null;
							return prev === newLatest ? prev : newLatest;
						});
					})
					.catch(() => {
						// ignore
					});
			};

			if (force || elapsed >= REFRESH_THROTTLE_MS) {
				// Enough time has passed, refresh immediately
				doRefresh();
			} else {
				// Schedule refresh after throttle interval
				const delay = REFRESH_THROTTLE_MS - elapsed;
				pendingRefreshRef.current = setTimeout(doRefresh, delay);
			}
		},
		[assistantName],
	);

	const refreshSessions = useCallback(() => {
		if (!isSelected) return;
		refreshSessionsUnconditional();
	}, [isSelected, refreshSessionsUnconditional]);

	// Load assistant on mount
	useEffect(() => {
		loadAssistant();
	}, []);

	// When selection changes (e.g. /new), refresh sessions list.
	useEffect(() => {
		refreshSessions();
	}, [refreshSessions]);

	useEffect(() => {
		if (!assistantName || !isSelected) return;
		if (lastActiveSessionIdRef.current === activeSessionId) return;
		lastActiveSessionIdRef.current = activeSessionId;
		refreshSessions();
	}, [activeSessionId, assistantName, isSelected, refreshSessions]);

	useEffect(() => {
		if (!assistantName || !isSelected) return;
		if (newSessionTrigger === undefined) return;
		if (lastNewSessionTriggerRef.current === undefined) {
			lastNewSessionTriggerRef.current = newSessionTrigger;
			return;
		}
		if (newSessionTrigger !== lastNewSessionTriggerRef.current) {
			lastNewSessionTriggerRef.current = newSessionTrigger;
			refreshSessionsUnconditional(true);
		}
	}, [
		assistantName,
		isSelected,
		newSessionTrigger,
		refreshSessionsUnconditional,
	]);

	// Refresh when session activity trigger changes (message sent) - always refresh
	useEffect(() => {
		if (!assistantName) return;
		if (sessionActivityTrigger === undefined) return;
		if (lastSessionActivityTriggerRef.current === undefined) {
			lastSessionActivityTriggerRef.current = sessionActivityTrigger;
			return;
		}
		if (sessionActivityTrigger !== lastSessionActivityTriggerRef.current) {
			lastSessionActivityTriggerRef.current = sessionActivityTrigger;
			refreshSessionsUnconditional(true);
		}
	}, [assistantName, sessionActivityTrigger, refreshSessionsUnconditional]);

	function cacheKeySessions(name: string) {
		return `oqto:defaultChatPi:${name}:sessions:v1`;
	}
	const SESSION_LIST_CACHE_MAX_CHARS = 1_000_000;

	function readCachedSessions(name: string): PiSessionFile[] {
		if (typeof window === "undefined") return [];
		try {
			const raw = localStorage.getItem(cacheKeySessions(name));
			if (!raw) return [];
			if (raw.length > SESSION_LIST_CACHE_MAX_CHARS) {
				localStorage.removeItem(cacheKeySessions(name));
				return [];
			}
			const parsed = JSON.parse(raw) as PiSessionFile[];
			return Array.isArray(parsed) ? parsed : [];
		} catch {
			return [];
		}
	}

	function writeCachedSessions(name: string, sessions: PiSessionFile[]) {
		if (typeof window === "undefined") return;
		try {
			const encoded = JSON.stringify(sessions);
			if (encoded.length > SESSION_LIST_CACHE_MAX_CHARS) {
				localStorage.removeItem(cacheKeySessions(name));
				return;
			}
			localStorage.setItem(cacheKeySessions(name), encoded);
		} catch {
			// ignore
		}
	}

	async function loadAssistant() {
		try {
			setLoading(true);
			const assistants = await listDefaultChatAssistants();

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
					getDefaultChatAssistant(name),
					listDefaultChatPiSessions(),
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
			console.error("Failed to load default chat assistant:", err);
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
				? await updateDefaultChatAssistant(newName.trim())
				: await createDefaultChatAssistant(newName.trim());
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
			await deleteDefaultChatAssistant(resetName.trim());
			const info = await createDefaultChatAssistant(resetName.trim());
			setAssistantName(info.name);
			setAssistantInfo(info);
			setSessions([]);
			setLatestSessionId(null);
			setShowResetDialog(false);
			setResetName("");
			await loadAssistant();
		} catch (err) {
			console.error("Failed to reset default chat:", err);
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

	const handleRenameSession = useCallback((session: PiSessionFile) => {
		setRenameSessionId(session.id);
		setRenameTitle(session.title || "");
		setRenameError(null);
		setShowRenameDialog(true);
	}, []);

	const handleDeleteSession = useCallback((session: PiSessionFile) => {
		const display = getDisplayPiTitle(session);
		setDeleteSessionId(session.id);
		setDeleteTitle(display || session.id);
		setDeleteError(null);
		setShowDeleteDialog(true);
	}, []);

	async function handleConfirmRename() {
		if (!renameSessionId || !renameTitle.trim()) return;

		try {
			setRenaming(true);
			setRenameError(null);
			const updated = await renamePiSession(
				renameSessionId,
				renameTitle.trim(),
			);
			// Update local state
			setSessions((prev) =>
				prev.map((s) => (s.id === updated.id ? updated : s)),
			);
			setShowRenameDialog(false);
			setRenameSessionId(null);
			setRenameTitle("");
		} catch (err) {
			console.error("Failed to rename session:", err);
			const message = err instanceof Error ? err.message : "Failed to rename";
			setRenameError(message);
		} finally {
			setRenaming(false);
		}
	}

	async function handleConfirmDelete() {
		if (!deleteSessionId) return;

		const previous = sessions;
		const remaining = previous.filter((s) => s.id !== deleteSessionId);
		setSessions(remaining);
		setLatestSessionId(remaining[0]?.id ?? null);
		setSelectedSessionIds((prev) => {
			if (!prev.has(deleteSessionId)) return prev;
			const next = new Set(prev);
			next.delete(deleteSessionId);
			return next;
		});
		if (activeSessionId === deleteSessionId && assistantName) {
			const nextSession = remaining[0]?.id ?? null;
			onSelect(assistantName, nextSession);
		}
		setShowDeleteDialog(false);
		setDeleteSessionId(null);
		setDeleteTitle("");
		try {
			setDeleting(true);
			setDeleteError(null);
			await deleteDefaultChatPiSession(deleteSessionId);
		} catch (err) {
			console.error("Failed to delete session:", err);
			const message = err instanceof Error ? err.message : "Failed to delete";
			setDeleteError(message);
			setSessions(previous);
			setLatestSessionId(previous[0]?.id ?? null);
		} finally {
			setDeleting(false);
		}
	}

	async function handleConfirmBulkDelete() {
		if (selectedSessionIds.size === 0) return;

		const ids = Array.from(selectedSessionIds);
		const previous = sessions;
		const remaining = previous.filter((s) => !selectedSessionIds.has(s.id));
		setSessions(remaining);
		setLatestSessionId(remaining[0]?.id ?? null);
		setSelectedSessionIds(new Set());
		setShowBulkDeleteDialog(false);
		setBulkDeleteError(null);

		if (
			activeSessionId &&
			selectedSessionIds.has(activeSessionId) &&
			assistantName
		) {
			onSelect(assistantName, remaining[0]?.id ?? null);
		}

		try {
			setBulkDeleting(true);
			const results = await Promise.allSettled(
				ids.map((id) => deleteDefaultChatPiSession(id)),
			);
			const failures = results.filter((r) => r.status === "rejected");
			if (failures.length > 0) {
				console.error("Failed to delete some sessions:", failures);
				setBulkDeleteError(
					locale === "de"
						? "Einige Sitzungen konnten nicht geloscht werden."
						: "Some sessions failed to delete.",
				);
				setSessions(previous);
				setLatestSessionId(previous[0]?.id ?? null);
			}
		} finally {
			setBulkDeleting(false);
		}
	}

	const handleSessionRowClick = useCallback(
		(
			e: React.MouseEvent,
			sessionId: string,
			index: number,
			selectableIds: string[],
		) => {
			const hasRange = e.shiftKey && lastSelectedIndexRef.current !== null;
			const isToggle = e.metaKey || e.ctrlKey;
			if (hasRange) {
				const lastIndex = lastSelectedIndexRef.current ?? index;
				const start = Math.min(lastIndex, index);
				const end = Math.max(lastIndex, index);
				const rangeIds = selectableIds.slice(start, end + 1);
				setSelectedSessionIds((prev) => {
					const next = new Set(prev);
					for (const id of rangeIds) next.add(id);
					return next;
				});
			} else if (isToggle) {
				setSelectedSessionIds((prev) => {
					const next = new Set(prev);
					if (next.has(sessionId)) {
						next.delete(sessionId);
					} else {
						next.add(sessionId);
					}
					return next;
				});
			} else {
				handleTimelineSessionClick(sessionId);
			}

			lastSelectedIndexRef.current = index;
		},
		[handleTimelineSessionClick],
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
		if (filterLower) {
			return null;
		}
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
						{locale === "de"
							? "Standardchat einrichten"
							: "Set up Default Chat"}
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
	if (filterLower && filteredSessions.length === 0) {
		return null;
	}
	const hasSessions = hasVisibleSessions;
	const displayCount = filterLower ? filteredSessions.length : sessions.length;

	return (
		<>
			<div className="border-b border-sidebar-border/50">
				<ContextMenu>
					<ContextMenuTrigger className="contents">
						{/* Default Chat header - styled like workspace project headers */}
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
									({displayCount})
								</span>
							</button>
							{onNewSession && (
								<button
									type="button"
									onClick={handleNewSessionClick}
									className="p-1 text-muted-foreground/60 hover:text-primary hover:bg-sidebar-accent transition-colors"
									title={locale === "de" ? "Neue Sitzung" : "New session"}
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
						{(() => {
							const selectableIds = flattenedSessions.map(
								(entry) => entry.session.id,
							);
							return (
								<>
									{selectedSessionIds.size > 0 && (
										<div className="flex items-center gap-2 bg-primary/10 border border-primary/20 rounded px-2 py-1 mx-3">
											<span className="text-xs font-medium text-primary">
												{selectedSessionIds.size}
											</span>
											<div className="flex-1 mr-1" />
											<Button
												type="button"
												variant="ghost"
												size="sm"
												onClick={() => setShowBulkDeleteDialog(true)}
												className="h-6 px-2 text-xs text-destructive hover:text-destructive"
											>
												<Trash2 className="w-3 h-3 mr-1" />
												{locale === "de" ? "Loschen" : "Delete"}
											</Button>
											<Button
												type="button"
												variant="ghost"
												size="sm"
												onClick={() => setSelectedSessionIds(new Set())}
												className="h-6 w-6 p-0"
												title={
													locale === "de"
														? "Auswahl loschen"
														: "Clear selection"
												}
											>
												<X className="w-3 h-3" />
											</Button>
										</div>
									)}
									{flattenedSessions.map(({ session, depth }, index) => {
										const isActive =
											session.id === (activeSessionId ?? latestSessionId);
										const isSelectedRow = selectedSessionIds.has(session.id);

										const emptyTitle =
											locale === "de" ? "Neue Sitzung" : "New Session";
										const displayTitle =
											session.title || emptyTitle;

										const tempId = getTempIdFromSession(session);
									const tempIdLabel = formatTempId(tempId);

										const formattedDate = formatSessionDate(
											new Date(session.started_at).getTime(),
										);

										const isChild = depth > 0;

										return (
											<ContextMenu key={session.id}>
												<ContextMenuTrigger className="contents">
													<div className={cn("ml-3", isChild && "ml-6")}>
														<button
															type="button"
															onClick={(e) =>
																handleSessionRowClick(
																	e,
																	session.id,
																	index,
																	selectableIds,
																)
															}
															className={cn(
																"w-full px-2 py-1 text-left transition-colors flex items-start gap-1.5 rounded-sm border",
																isActive
																	? "bg-primary/15 border-primary text-foreground"
																	: isSelectedRow
																		? "bg-primary/10 border-primary/50 text-foreground"
																		: "text-muted-foreground hover:bg-sidebar-accent border-transparent",
															)}
														>
															{isChild ? (
																<CornerDownRight className="w-3 h-3 mt-0.5 text-muted-foreground/70 flex-shrink-0" />
															) : (
																<MessageSquare className="w-3 h-3 mt-0.5 text-primary/70 flex-shrink-0" />
															)}
															<div className="flex-1 min-w-0">
																<div className="text-xs font-medium truncate">
																	{displayTitle}
																</div>
																<div className="text-[9px] text-muted-foreground/50 mt-0.5">
																	{formattedDate}
																</div>
															</div>
														</button>
													</div>
												</ContextMenuTrigger>
												<ContextMenuContent>
													<ContextMenuItem
														onClick={() =>
															tempId && navigator.clipboard.writeText(tempId)
														}
													>
														<Copy className="w-4 h-4 mr-2" />
														{locale === "de" ? "Temp-ID kopieren" : "Copy Temp ID"}
													</ContextMenuItem>
													<ContextMenuSeparator />
													<ContextMenuItem
														onClick={() => handleRenameSession(session)}
													>
														<Pencil className="w-4 h-4 mr-2" />
														{locale === "de" ? "Umbenennen" : "Rename"}
													</ContextMenuItem>
													<ContextMenuSeparator />
													<ContextMenuItem
														onClick={() => handleDeleteSession(session)}
														className="text-destructive focus:text-destructive"
													>
														<Trash2 className="w-4 h-4 mr-2" />
														{locale === "de" ? "Loschen" : "Delete"}
													</ContextMenuItem>
												</ContextMenuContent>
											</ContextMenu>
										);
									})}
								</>
							);
						})()}
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
			<RenameSessionDialog
				open={showRenameDialog}
				onOpenChange={setShowRenameDialog}
				title={renameTitle}
				onTitleChange={setRenameTitle}
				onSubmit={handleConfirmRename}
				loading={renaming}
				error={renameError}
				locale={locale}
			/>
			<DeleteSessionDialog
				open={showDeleteDialog}
				onOpenChange={setShowDeleteDialog}
				title={deleteTitle}
				onSubmit={handleConfirmDelete}
				loading={deleting}
				error={deleteError}
				locale={locale}
			/>
			<Dialog
				open={showBulkDeleteDialog}
				onOpenChange={setShowBulkDeleteDialog}
			>
				<DialogContent>
					<DialogHeader>
						<DialogTitle>
							{locale === "de"
								? "Mehrere Sitzungen loschen"
								: "Delete multiple sessions"}
						</DialogTitle>
						<DialogDescription>
							{locale === "de"
								? `Mochtest du ${selectedSessionIds.size} Sitzungen loschen?`
								: `Delete ${selectedSessionIds.size} sessions?`}
						</DialogDescription>
					</DialogHeader>
					{bulkDeleteError && (
						<div className="text-sm text-destructive">{bulkDeleteError}</div>
					)}
					<DialogFooter>
						<Button
							type="button"
							variant="ghost"
							onClick={() => setShowBulkDeleteDialog(false)}
						>
							{locale === "de" ? "Abbrechen" : "Cancel"}
						</Button>
						<Button
							type="button"
							variant="destructive"
							onClick={handleConfirmBulkDelete}
							disabled={bulkDeleting}
						>
							{bulkDeleting ? (
								<Loader2 className="w-4 h-4 mr-2 animate-spin" />
							) : (
								<Trash2 className="w-4 h-4 mr-2" />
							)}
							{locale === "de" ? "Loschen" : "Delete"}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</>
	);
}

/**
 * Vertical timeline showing sessions as connected dots.
 *
 * Legacy: replaced by canonical-style list above.
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

					const displayTitle = session.title || formattedDate;
					const tempId = getTempIdFromSession(session);
					const tempIdLabel = formatTempId(tempId);

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
									{displayTitle}
									{tempIdLabel && <span className="ml-1 opacity-60">[{tempIdLabel}]</span>}
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
						{locale === "de"
							? "Standardchat zurucksetzen"
							: "Reset Default Chat"}
					</DialogTitle>
					<DialogDescription>
						{locale === "de"
							? "Dies loscht alle Standardchat-Daten und startet frisch. Geben Sie einen neuen Namen ein."
							: "This deletes all Default Chat data and starts fresh. Enter a new name."}
					</DialogDescription>
				</DialogHeader>

				<div className="grid gap-4 py-4">
					<div className="grid gap-2">
						<Label htmlFor="default-chat-reset-name">
							{locale === "de" ? "Neuer Name" : "New Name"}
						</Label>
						<Input
							id="default-chat-reset-name"
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

function RenameSessionDialog({
	open,
	onOpenChange,
	title,
	onTitleChange,
	onSubmit,
	loading,
	error,
	locale,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
	onTitleChange: (title: string) => void;
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
						{locale === "de" ? "Sitzung umbenennen" : "Rename Session"}
					</DialogTitle>
					<DialogDescription>
						{locale === "de"
							? "Geben Sie einen neuen Titel fur diese Sitzung ein."
							: "Enter a new title for this session."}
					</DialogDescription>
				</DialogHeader>

				<div className="grid gap-4 py-4">
					<div className="grid gap-2">
						<Label htmlFor="session-title">
							{locale === "de" ? "Titel" : "Title"}
						</Label>
						<Input
							id="session-title"
							placeholder={locale === "de" ? "Sitzungstitel" : "Session title"}
							value={title}
							onChange={(e) => onTitleChange(e.target.value)}
							onKeyDown={(e) => {
								if (e.key === "Enter" && !loading) {
									onSubmit();
								}
							}}
							disabled={loading}
						/>
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
					<Button onClick={onSubmit} disabled={loading || !title.trim()}>
						{loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
						{locale === "de" ? "Speichern" : "Save"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

function DeleteSessionDialog({
	open,
	onOpenChange,
	title,
	onSubmit,
	loading,
	error,
	locale,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: string;
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
						{locale === "de" ? "Sitzung loschen" : "Delete Session"}
					</DialogTitle>
					<DialogDescription>
						{locale === "de"
							? `Diese Sitzung wird entfernt: ${title}`
							: `This will remove the session: ${title}`}
					</DialogDescription>
				</DialogHeader>
				{error && <div className="text-sm text-destructive">{error}</div>}
				<DialogFooter>
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						{locale === "de" ? "Abbrechen" : "Cancel"}
					</Button>
					<Button variant="destructive" onClick={onSubmit} disabled={loading}>
						{loading
							? locale === "de"
								? "Loschen..."
								: "Deleting..."
							: locale === "de"
								? "Loschen"
								: "Delete"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
